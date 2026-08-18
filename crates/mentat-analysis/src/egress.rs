use crate::detector::ProjectStructureSummary;
use mentat_core::error::MentatError;
use mentat_core::models::FileRecord;
use mentat_core::ports::RepositoryReader;
use mentat_inference::{BackendProfile, InferenceRequest};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IncludedFileRef {
    pub relative_path: PathBuf,
    pub line_start: usize,
    pub line_end: usize,
    pub line_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EgressReceipt {
    pub receipt_id: Uuid,
    pub packet_hash: String,
    pub snapshot_id: Uuid,
    pub token_count: usize,
    pub file_count: usize,
    pub granted_at: String,
}

#[derive(Debug, Clone)]
pub struct EgressPacket {
    pub packet_id: Uuid,
    pub packet_hash: String,
    pub included_files: Vec<PathBuf>,
    pub included_file_refs: Vec<IncludedFileRef>,
    pub excluded_sensitive_files: Vec<PathBuf>,
    pub redacted_secret_occurrences: usize,
    pub estimated_tokens: usize,
    pub prompt_context: String,
}

/// [SEC-F001] Cryptographically sealed and consume-once approved request
#[derive(Debug)]
pub struct ApprovedInferenceRequest {
    receipt: EgressReceipt,
    packet: EgressPacket,
    user_question: String,
    snapshot_id: Uuid,
    approved_profile: BackendProfile,
    approved_digest: String,
}

impl ApprovedInferenceRequest {
    pub fn new(
        receipt: EgressReceipt,
        packet: EgressPacket,
        user_question: String,
        snapshot_id: Uuid,
        approved_profile: BackendProfile,
    ) -> Result<Self, MentatError> {
        // [SEC-F001] Re-hash actual prompt_context bytes directly to prevent in-memory tampering
        let mut context_hasher = Sha256::new();
        context_hasher.update(packet.prompt_context.as_bytes());
        let computed_packet_hash = format!("{:x}", context_hasher.finalize());

        if computed_packet_hash != packet.packet_hash {
            return Err(MentatError::EgressViolation(
                "EgressPacket의 프롬프트 내용과 저장된 패킷 해시가 일치하지 않습니다 (변조 감지)."
                    .to_string(),
            ));
        }

        if receipt.packet_hash != computed_packet_hash {
            return Err(MentatError::EgressViolation(
                "EgressReceipt의 패킷 해시와 EgressPacket의 실제 해시가 일치하지 않습니다."
                    .to_string(),
            ));
        }

        if receipt.snapshot_id != snapshot_id {
            return Err(MentatError::EgressViolation(
                "EgressReceipt의 스냅샷 ID와 현재 스냅샷 ID가 일치하지 않습니다.".to_string(),
            ));
        }

        if receipt.file_count != packet.included_files.len() {
            return Err(MentatError::EgressViolation(
                "EgressReceipt의 파일 수와 실제 포함된 파일 수가 일치하지 않습니다.".to_string(),
            ));
        }

        if receipt.token_count != packet.estimated_tokens {
            return Err(MentatError::EgressViolation(
                "EgressReceipt의 예상 토큰 수와 실제 패킷의 토큰 수가 일치하지 않습니다."
                    .to_string(),
            ));
        }

        let approved_digest = Self::calculate_digest_internal(
            &computed_packet_hash,
            &snapshot_id,
            &approved_profile.provider,
            &approved_profile.model,
            &user_question,
        );

        Ok(Self {
            receipt,
            packet,
            user_question,
            snapshot_id,
            approved_profile,
            approved_digest,
        })
    }

    fn calculate_digest_internal(
        packet_hash: &str,
        snapshot_id: &Uuid,
        provider: &mentat_inference::ProviderKind,
        model_name: &str,
        user_question: &str,
    ) -> String {
        let mut hasher = Sha256::new();
        hasher.update(packet_hash.as_bytes());
        hasher.update(snapshot_id.as_bytes());
        hasher.update(format!("{:?}", provider).as_bytes());
        hasher.update(model_name.as_bytes());
        hasher.update(user_question.as_bytes());
        format!("{:x}", hasher.finalize())
    }

    pub fn verify_integrity(&self) -> bool {
        let mut context_hasher = Sha256::new();
        context_hasher.update(self.packet.prompt_context.as_bytes());
        let computed_packet_hash = format!("{:x}", context_hasher.finalize());

        let expected_digest = Self::calculate_digest_internal(
            &computed_packet_hash,
            &self.snapshot_id,
            &self.approved_profile.provider,
            &self.approved_profile.model,
            &self.user_question,
        );

        self.approved_digest == expected_digest
            && self.receipt.packet_hash == computed_packet_hash
            && self.packet.packet_hash == computed_packet_hash
    }

    /// [SEC-F001] Consume-once API: Consumes `self` by value to generate the final sealed `InferenceRequest`
    pub fn into_inference_request(self) -> Result<InferenceRequest, MentatError> {
        if !self.verify_integrity() {
            return Err(MentatError::EgressViolation(
                "승인된 요청의 무결성 검증에 실패했습니다.".to_string(),
            ));
        }

        Ok(InferenceRequest {
            request_id: self.receipt.receipt_id,
            system_contract: "You are Code Mentat, a strict read-only repository advisor. Provide evidence-based explanations distinguishing OBSERVED, INFERRED, and CONFLICT.".to_string(),
            prompt_context: self.packet.prompt_context,
            user_question: self.user_question,
            profile: self.approved_profile,
        })
    }
}

pub struct EgressFilter;

impl EgressFilter {
    pub fn is_sensitive_filename(path: &Path) -> bool {
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_lowercase();

        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        // Exact blacklist and extensions
        if name == ".env" || name.starts_with(".env.") || name.contains(".env") {
            return true;
        }

        if ext == "pem"
            || ext == "key"
            || ext == "pfx"
            || ext == "p12"
            || ext == "pkcs12"
            || ext == "crt"
            || ext == "cer"
            || ext == "kdbx"
        {
            return true;
        }

        // Sensitive name keywords
        let sensitive_keywords = [
            "id_rsa",
            "id_ed25519",
            "id_dsa",
            "id_ecdsa",
            "token",
            "tokens",
            "credential",
            "credentials",
            "secret",
            "secrets",
            "password",
            "passwords",
            "private_key",
            "privkey",
            "auth_token",
            "apikey",
            "aws_access_key",
            "jwt_secret",
        ];

        for kw in sensitive_keywords {
            if name.contains(kw) {
                return true;
            }
        }

        false
    }

    /// Unicode-safe and state-aware multi-line secret scanner and redactor
    pub fn scan_and_redact_secrets(text: &str) -> (String, usize) {
        let mut redaction_count = 0;
        let mut output = String::with_capacity(text.len());
        let mut in_pem_block = false;

        for line in text.lines() {
            // 1. Multi-line PEM block state machine
            if line.contains("-----BEGIN ") && line.contains("PRIVATE KEY-----") {
                in_pem_block = true;
                output.push_str("[REDACTED_PRIVATE_KEY_BLOCK]\n");
                redaction_count += 1;
                continue;
            }

            if in_pem_block {
                if line.contains("-----END ") && line.contains("PRIVATE KEY-----") {
                    in_pem_block = false;
                }
                // Silently omit all base64 contents and end marker
                continue;
            }

            // 2. Process individual line for token-based secrets and assignments
            let (redacted_line, count) = Self::redact_line_secrets(line);
            redaction_count += count;
            output.push_str(&redacted_line);
            output.push('\n');
        }

        (output, redaction_count)
    }

    /// Redacts all secret patterns within a single line using char-level Unicode boundary checks
    fn redact_line_secrets(line: &str) -> (String, usize) {
        let mut redaction_count = 0;

        // [SEC-F010] Process line for assigned JSON/YAML/env secrets using exact original byte offsets
        let (processed_line, assign_count) = Self::redact_assignments(line);
        redaction_count += assign_count;

        let mut current = processed_line;
        let mut buf = String::with_capacity(current.len());

        while !current.is_empty() {
            let mut earliest_match: Option<(usize, usize, &'static str)> = None; // (start_byte, end_byte, replacement)

            // 1. Google API Key (AIza + 35 valid chars, total 39)
            if let Some(pos) = current.find("AIza") {
                if let Some(end) = Self::find_token_end(&current, pos, 39) {
                    Self::update_earliest(
                        &mut earliest_match,
                        pos,
                        end,
                        "[REDACTED_GOOGLE_API_KEY]",
                    );
                }
            }

            // 2. Anthropic API Key (sk-ant-...)
            if let Some(pos) = current.find("sk-ant-") {
                if let Some(end) = Self::find_token_end(&current, pos, 20) {
                    Self::update_earliest(&mut earliest_match, pos, end, "[REDACTED_ANTHROPIC_KEY]");
                }
            }

            // 3. OpenAI / OpenRouter Key (sk- + valid token chars)
            if let Some(pos) = current.find("sk-") {
                if let Some(end) = Self::find_token_end(&current, pos, 20) {
                    Self::update_earliest(&mut earliest_match, pos, end, "[REDACTED_OPENAI_KEY]");
                }
            }

            // 4. GitHub fine-grained PAT (github_pat_ + valid token chars)
            if let Some(pos) = current.find("github_pat_") {
                if let Some(end) = Self::find_token_end(&current, pos, 20) {
                    Self::update_earliest(&mut earliest_match, pos, end, "[REDACTED_GITHUB_PAT]");
                }
            }

            // 5. GitHub classic token (ghp_ + 36 chars)
            if let Some(pos) = current.find("ghp_") {
                if let Some(end) = Self::find_token_end(&current, pos, 40) {
                    Self::update_earliest(&mut earliest_match, pos, end, "[REDACTED_GITHUB_TOKEN]");
                }
            }

            // 6. AWS Access Key ID (AKIA + 16 uppercase alphanumeric chars)
            if let Some(pos) = current.find("AKIA") {
                if let Some(end) = Self::find_token_end(&current, pos, 20) {
                    Self::update_earliest(
                        &mut earliest_match,
                        pos,
                        end,
                        "[REDACTED_AWS_ACCESS_KEY]",
                    );
                }
            }

            // 7. JWT Token (eyJ...)
            if let Some(pos) = current.find("eyJ") {
                if let Some(end) = Self::find_jwt_end(&current, pos) {
                    Self::update_earliest(&mut earliest_match, pos, end, "[REDACTED_JWT_TOKEN]");
                }
            }

            // 8. Generic Bearer Token (Bearer <token>)
            if let Some(pos) = current.find("Bearer ") {
                let token_start = pos + 7;
                if let Some(end) = Self::find_token_end(&current, token_start, 20) {
                    Self::update_earliest(
                        &mut earliest_match,
                        pos,
                        end,
                        "Bearer [REDACTED_BEARER_TOKEN]",
                    );
                }
            }

            // 9. HuggingFace Token (hf_...)
            if let Some(pos) = current.find("hf_") {
                if let Some(end) = Self::find_token_end(&current, pos, 30) {
                    Self::update_earliest(&mut earliest_match, pos, end, "[REDACTED_HF_TOKEN]");
                }
            }

            // 10. Slack Token (xoxb- or xoxp-)
            if let Some(pos) = current.find("xoxb-").or_else(|| current.find("xoxp-")) {
                if let Some(end) = Self::find_token_end(&current, pos, 20) {
                    Self::update_earliest(&mut earliest_match, pos, end, "[REDACTED_SLACK_TOKEN]");
                }
            }

            if let Some((start, end, replacement)) = earliest_match {
                buf.push_str(&current[..start]);
                buf.push_str(replacement);
                redaction_count += 1;
                current = current[end..].to_string();
            } else {
                buf.push_str(&current);
                break;
            }
        }

        (buf, redaction_count)
    }

    fn update_earliest(
        earliest: &mut Option<(usize, usize, &'static str)>,
        start: usize,
        end: usize,
        replacement: &'static str,
    ) {
        match earliest {
            Some((cur_start, _, _)) => {
                if start < *cur_start {
                    *earliest = Some((start, end, replacement));
                }
            }
            None => {
                *earliest = Some((start, end, replacement));
            }
        }
    }

    /// Finds valid ASCII token end index consuming all contiguous valid characters
    fn find_token_end(text: &str, start_byte: usize, min_len: usize) -> Option<usize> {
        let remainder = &text[start_byte..];
        let mut char_count = 0;
        let mut byte_len = 0;

        for (idx, ch) in remainder.char_indices() {
            if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
                char_count += 1;
                byte_len = idx + ch.len_utf8();
            } else {
                break;
            }
        }

        if char_count >= min_len {
            Some(start_byte + byte_len)
        } else {
            None
        }
    }

    /// Finds JWT token end index (three dot-separated base64 segments)
    fn find_jwt_end(text: &str, start_byte: usize) -> Option<usize> {
        let remainder = &text[start_byte..];
        let mut dot_count = 0;
        let mut byte_len = 0;

        for (idx, ch) in remainder.char_indices() {
            if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
                byte_len = idx + ch.len_utf8();
            } else if ch == '.' {
                dot_count += 1;
                byte_len = idx + ch.len_utf8();
                if dot_count > 2 {
                    break;
                }
            } else {
                break;
            }
        }

        if dot_count == 2 && byte_len >= 30 {
            Some(start_byte + byte_len)
        } else {
            None
        }
    }

    /// [SEC-F010] Searches for an ASCII keyword within `text` case-insensitively without creating a new String,
    /// guaranteeing exact byte offset alignment on the original UTF-8 string.
    fn find_ascii_case_insensitive(text: &str, needle: &str) -> Option<(usize, usize)> {
        let needle_bytes = needle.as_bytes();
        let needle_len = needle_bytes.len();

        for (byte_idx, _) in text.char_indices() {
            if byte_idx + needle_len <= text.len() {
                let candidate = &text.as_bytes()[byte_idx..byte_idx + needle_len];
                if candidate.eq_ignore_ascii_case(needle_bytes) {
                    return Some((byte_idx, byte_idx + needle_len));
                }
            }
        }
        None
    }

    /// [SEC-F010 & SEC-F002] Redacts JSON/YAML/env key-value assignments with escape-aware quote matching
    fn redact_assignments(line: &str) -> (String, usize) {
        let mut count = 0;
        let mut result = String::with_capacity(line.len());
        let sensitive_keys = [
            "password",
            "secret",
            "api_key",
            "token",
            "auth_token",
            "private_key",
            "bearer",
        ];

        let mut current = line;

        'outer: while !current.is_empty() {
            let mut earliest_key: Option<(usize, usize)> = None;

            for key in &sensitive_keys {
                if let Some((k_start, k_end)) = Self::find_ascii_case_insensitive(current, key) {
                    match earliest_key {
                        Some((cur_pos, _)) if k_start < cur_pos => {
                            earliest_key = Some((k_start, k_end));
                        }
                        None => {
                            earliest_key = Some((k_start, k_end));
                        }
                        _ => {}
                    }
                }
            }

            if let Some((_, k_end)) = earliest_key {
                let after_key = &current[k_end..];
                if let Some(sep_pos) = after_key.find([':', '=']) {
                    let after_sep = &after_key[sep_pos + 1..];
                    let val_start_rel = after_sep.find(|c: char| !c.is_whitespace());
                    if let Some(val_start) = val_start_rel {
                        let val_part = &after_sep[val_start..];
                        if !val_part.starts_with("[REDACTED") {
                            let (val_end, is_quoted) = if let Some(stripped) =
                                val_part.strip_prefix('"')
                            {
                                // Escape-aware quote matching for double quotes
                                let mut escaped = false;
                                let mut end_pos = stripped.len();
                                for (idx, c) in stripped.char_indices() {
                                    if c == '\\' {
                                        escaped = !escaped;
                                    } else if c == '"' && !escaped {
                                        end_pos = idx + 1;
                                        break;
                                    } else {
                                        escaped = false;
                                    }
                                }
                                (end_pos + 1, true)
                            } else if let Some(stripped) = val_part.strip_prefix('\'') {
                                // Escape-aware quote matching for single quotes
                                let mut escaped = false;
                                let mut end_pos = stripped.len();
                                for (idx, c) in stripped.char_indices() {
                                    if c == '\\' {
                                        escaped = !escaped;
                                    } else if c == '\'' && !escaped {
                                        end_pos = idx + 1;
                                        break;
                                    } else {
                                        escaped = false;
                                    }
                                }
                                (end_pos + 1, true)
                            } else {
                                (
                                    val_part
                                        .find(|c: char| {
                                            c.is_whitespace() || c == ',' || c == ';' || c == '}'
                                        })
                                        .unwrap_or(val_part.len()),
                                    false,
                                )
                            };

                            let prefix_len = k_end + sep_pos + 1 + val_start;
                            result.push_str(&current[..prefix_len]);
                            if is_quoted {
                                result.push_str("\"[REDACTED_ASSIGNED_SECRET]\"");
                            } else {
                                result.push_str("[REDACTED_ASSIGNED_SECRET]");
                            }
                            count += 1;
                            current = &current[prefix_len + val_end..];
                            continue 'outer;
                        }
                    }
                }

                result.push_str(&current[..k_end]);
                current = &current[k_end..];
            } else {
                result.push_str(current);
                break;
            }
        }

        (result, count)
    }

    /// [SEC-F002] Query-aware context assembly with exact file and line references preview
    pub async fn assemble_packet(
        reader: &(impl RepositoryReader + ?Sized),
        files: &[FileRecord],
        summary: &ProjectStructureSummary,
        user_question: &str,
    ) -> Result<EgressPacket, MentatError> {
        Self::assemble_packet_with_user_exclusions(reader, files, summary, user_question, &[]).await
    }

    /// [SEC-F002] Query-aware context assembly with per-request user exclusions and exact file/line preview
    pub async fn assemble_packet_with_user_exclusions(
        reader: &(impl RepositoryReader + ?Sized),
        files: &[FileRecord],
        summary: &ProjectStructureSummary,
        user_question: &str,
        user_excluded_files: &[std::path::PathBuf],
    ) -> Result<EgressPacket, MentatError> {
        let mut included_files = Vec::new();
        let mut included_file_refs = Vec::new();
        let mut excluded_sensitive_files = Vec::new();
        let mut total_redactions = 0;
        let mut context_buffer = String::new();

        context_buffer.push_str(&format!(
            "# Repository Context Summary\nPrimary Language: {}\nTotal Files: {}\n\n",
            summary.primary_language.as_deref().unwrap_or("Unknown"),
            files.len()
        ));

        context_buffer.push_str("## Project Manifests & Key Documents\n");

        // Query keyword relevance scoring
        let query_keywords: Vec<String> = user_question
            .split_whitespace()
            .map(|w| {
                w.trim_matches(|c: char| !c.is_alphanumeric())
                    .to_lowercase()
            })
            .filter(|w| w.len() >= 3)
            .collect();

        // Sort files by relevance to user query, prioritizing matching filenames/paths, manifests, and entry points
        let mut scored_files: Vec<(&FileRecord, usize)> = files
            .iter()
            .map(|f| {
                let mut score = 0;
                let path_str = f.relative_path.to_string_lossy().to_lowercase();
                for kw in &query_keywords {
                    if path_str.contains(kw) {
                        score += 10;
                    }
                }
                if summary.manifests.contains(&f.relative_path) {
                    score += 5;
                }
                if summary.entry_points.contains(&f.relative_path) {
                    score += 3;
                }
                if f.relative_path
                    .extension()
                    .map(|e| e == "md")
                    .unwrap_or(false)
                {
                    score += 2;
                }
                (f, score)
            })
            .collect();

        scored_files.sort_by_key(|b| std::cmp::Reverse(b.1));

        for (file, _) in scored_files {
            if user_excluded_files.contains(&file.relative_path) {
                excluded_sensitive_files.push(file.relative_path.clone());
                continue;
            }

            if Self::is_sensitive_filename(&file.relative_path) {
                excluded_sensitive_files.push(file.relative_path.clone());
                continue;
            }

            let is_doc = file
                .relative_path
                .extension()
                .map(|e| e == "md" || e == "toml" || e == "json" || e == "yaml" || e == "yml")
                .unwrap_or(false);
            if (is_doc || summary.entry_points.contains(&file.relative_path))
                && included_files.len() < 8
            {
                if let Ok(content) = reader.read_file_content(&file.relative_path).await {
                    let lines: Vec<&str> = content.lines().take(60).collect();
                    let line_count = lines.len();
                    let truncated = lines.join("\n");
                    let (redacted, count) = Self::scan_and_redact_secrets(&truncated);
                    total_redactions += count;

                    context_buffer.push_str(&format!(
                        "### File: {}\n```\n{}\n```\n\n",
                        file.relative_path.display(),
                        redacted
                    ));

                    included_files.push(file.relative_path.clone());
                    included_file_refs.push(IncludedFileRef {
                        relative_path: file.relative_path.clone(),
                        line_start: 1,
                        line_end: line_count,
                        line_count,
                    });
                }
            }
        }

        context_buffer.push_str(&format!("## User Question\n{}\n", user_question));

        let estimated_tokens = context_buffer.len().div_ceil(4);

        // Cryptographic Hash of the Exact Packet
        let mut hasher = Sha256::new();
        hasher.update(context_buffer.as_bytes());
        let packet_hash = format!("{:x}", hasher.finalize());

        Ok(EgressPacket {
            packet_id: Uuid::new_v4(),
            packet_hash,
            included_files,
            included_file_refs,
            excluded_sensitive_files,
            redacted_secret_occurrences: total_redactions,
            estimated_tokens,
            prompt_context: context_buffer,
        })
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use mentat_inference::{BackendProfile, ProviderKind};
    use std::path::{Path, PathBuf};

    #[test]
    fn test_sensitive_filtering_comprehensive() {
        assert!(EgressFilter::is_sensitive_filename(Path::new(".env")));
        assert!(EgressFilter::is_sensitive_filename(Path::new(".env.local")));
        assert!(EgressFilter::is_sensitive_filename(Path::new("server.key")));
        assert!(EgressFilter::is_sensitive_filename(Path::new("id_rsa")));
        assert!(EgressFilter::is_sensitive_filename(Path::new("token.txt")));
        assert!(EgressFilter::is_sensitive_filename(Path::new(
            "auth_tokens.json"
        )));
        assert!(EgressFilter::is_sensitive_filename(Path::new(
            "api_credentials.yaml"
        )));
        assert!(EgressFilter::is_sensitive_filename(Path::new(
            "secret_password.txt"
        )));

        assert!(!EgressFilter::is_sensitive_filename(Path::new(
            "src/main.rs"
        )));
        assert!(!EgressFilter::is_sensitive_filename(Path::new("README.md")));
        assert!(!EgressFilter::is_sensitive_filename(Path::new(
            "Cargo.toml"
        )));
    }

    #[test]
    fn test_complete_pem_block_redaction_zero_raw_leak() {
        let pem_fixture = "Header info\n-----BEGIN RSA PRIVATE KEY-----\nMIIEowIBAAKCAQEA0Y3v1234567890abcdef==\nMIIEowIBAAKCAQEA0Y3v1234567890abcdef==\n-----END RSA PRIVATE KEY-----\nFooter info";
        let (redacted, count) = EgressFilter::scan_and_redact_secrets(pem_fixture);
        assert!(count >= 1);
        assert!(redacted.contains("[REDACTED_PRIVATE_KEY_BLOCK]"));
        assert!(!redacted.contains("MIIEowIBAAKCAQEA"));
        assert!(!redacted.contains("-----END RSA PRIVATE KEY-----"));
    }

    #[test]
    fn test_multiple_secrets_on_single_line() {
        let multi_secret_line = "let k1 = \"AIzaSyD-1234567890abcdef1234567890abcde\"; let k2 = \"sk-1234567890abcdef1234567890abcdef\";";
        let (redacted, count) = EgressFilter::scan_and_redact_secrets(multi_secret_line);
        assert_eq!(count, 2);
        assert!(redacted.contains("[REDACTED_GOOGLE_API_KEY]"));
        assert!(redacted.contains("[REDACTED_OPENAI_KEY]"));
        assert!(!redacted.contains("AIzaSyD"));
        assert!(!redacted.contains("sk-12345"));
    }

    #[test]
    fn test_aws_key_and_jwt_redaction() {
        let aws_line = "export AWS_ACCESS_KEY_ID=AKIAIOSFODNN7EXAMPLE1";
        let (redacted, count) = EgressFilter::scan_and_redact_secrets(aws_line);
        assert_eq!(count, 1);
        assert!(redacted.contains("[REDACTED_AWS_ACCESS_KEY]"));
        assert!(!redacted.contains("AKIAIOSFOD"));

        let jwt_line = "token: eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIiwibmFtZSI6IkpvaG4gRG9lIn0.SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV_adQssw5c";
        let (redacted2, count2) = EgressFilter::scan_and_redact_secrets(jwt_line);
        assert!(count2 >= 1);
        assert!(
            redacted2.contains("[REDACTED_ASSIGNED_SECRET]")
                || redacted2.contains("[REDACTED_JWT_TOKEN]")
        );
        assert!(!redacted2.contains("eyJhbGciOi"));
    }

    #[test]
    fn test_escaped_quote_json_assignment_redaction() {
        let escaped_json = "{\"secret\": \"val_with_\\\"escaped_quote\\\"_inside\"}";
        let (redacted, count) = EgressFilter::scan_and_redact_secrets(escaped_json);
        assert_eq!(count, 1);
        assert!(redacted.contains("[REDACTED_ASSIGNED_SECRET]"));
        assert!(!redacted.contains("escaped_quote"));
    }

    #[test]
    fn test_github_pat_and_classic_token_redaction() {
        let pat_line = "export GITHUB_AUTH=\"github_pat_11AAAAAAA01234567890_abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ1234567890\"";
        let (redacted, count) = EgressFilter::scan_and_redact_secrets(pat_line);
        assert_eq!(count, 1);
        assert!(redacted.contains("[REDACTED_GITHUB_PAT]"));
        assert!(!redacted.contains("github_pat_11AAA"));

        let classic_line = "let gh_classic = \"ghp_123456789012345678901234567890123456\";";
        let (redacted2, count2) = EgressFilter::scan_and_redact_secrets(classic_line);
        assert_eq!(count2, 1);
        assert!(redacted2.contains("[REDACTED_GITHUB_TOKEN]"));
        assert!(!redacted2.contains("ghp_123456"));
    }

    #[test]
    fn test_json_yaml_assigned_secrets_redaction() {
        let json_fixture =
            "{\n  \"api_key\": \"my_super_secret_value_123\",\n  \"password\": \"P@ssw0rd123!\"\n}";
        let (redacted, count) = EgressFilter::scan_and_redact_secrets(json_fixture);
        assert_eq!(count, 2);
        assert!(redacted.contains("[REDACTED_ASSIGNED_SECRET]"));
        assert!(!redacted.contains("my_super_secret_value_123"));
        assert!(!redacted.contains("P@ssw0rd123!"));
    }

    #[test]
    fn test_sec_f010_unicode_casing_expansion_exact_byte_offsets() {
        let turkish_assignment = "İ password=\"super_secret_val_123\"";
        let (redacted, count) = EgressFilter::scan_and_redact_secrets(turkish_assignment);
        assert_eq!(count, 1);
        assert!(redacted.contains("[REDACTED_ASSIGNED_SECRET]"));
        assert!(!redacted.contains("super_secret_val_123"));

        let emoji_assignment = "🔥 secret: 'api_val_123' 🔥";
        let (redacted2, count2) = EgressFilter::scan_and_redact_secrets(emoji_assignment);
        assert_eq!(count2, 1);
        assert!(redacted2.contains("[REDACTED_ASSIGNED_SECRET]"));
        assert!(!redacted2.contains("api_val_123"));
    }

    #[test]
    fn test_unicode_adjacent_secrets_zero_panic_and_safe_redaction() {
        let unicode_line1 = "한글앞AIzaSyD-1234567890abcdef1234567890abcde한글뒤";
        let (redacted1, count1) = EgressFilter::scan_and_redact_secrets(unicode_line1);
        assert_eq!(count1, 1);
        assert!(redacted1.contains("한글앞[REDACTED_GOOGLE_API_KEY]한글뒤"));

        let unicode_line2 = "🔥AIza🔥malformed_short";
        let (redacted2, count2) = EgressFilter::scan_and_redact_secrets(unicode_line2);
        assert_eq!(count2, 0);
        assert!(redacted2.contains("🔥AIza🔥malformed_short"));
    }

    #[test]
    fn test_approved_inference_request_binding_integrity_and_consume_once() {
        let snap_id = Uuid::new_v4();
        let prompt_context = "context content here".to_string();
        let mut hasher = Sha256::new();
        hasher.update(prompt_context.as_bytes());
        let exact_hash = format!("{:x}", hasher.finalize());

        let packet = EgressPacket {
            packet_id: Uuid::new_v4(),
            packet_hash: exact_hash.clone(),
            included_files: vec![PathBuf::from("Cargo.toml")],
            included_file_refs: vec![IncludedFileRef {
                relative_path: PathBuf::from("Cargo.toml"),
                line_start: 1,
                line_end: 20,
                line_count: 20,
            }],
            excluded_sensitive_files: vec![],
            redacted_secret_occurrences: 0,
            estimated_tokens: 5,
            prompt_context: prompt_context.clone(),
        };

        let receipt = EgressReceipt {
            receipt_id: Uuid::new_v4(),
            packet_hash: exact_hash,
            snapshot_id: snap_id,
            token_count: 5,
            file_count: 1,
            granted_at: chrono::Utc::now().to_rfc3339(),
        };

        let profile = BackendProfile {
            id: Uuid::new_v4(),
            name: "Gemini Test".to_string(),
            provider: ProviderKind::GoogleGemini,
            base_url: ProviderKind::GoogleGemini.default_base_url().to_string(),
            model: "gemini-2.5-flash".to_string(),
            api_key: Some("dummy".to_string()),
            timeout_secs: 30,
        };

        let approved = ApprovedInferenceRequest::new(
            receipt.clone(),
            packet.clone(),
            "Explain structure".to_string(),
            snap_id,
            profile.clone(),
        )
        .expect("Approved request should build");

        assert!(approved.verify_integrity());

        // Consume-once execution
        let inference_req = approved
            .into_inference_request()
            .expect("Should consume approved request");
        assert_eq!(inference_req.user_question, "Explain structure");
        assert_eq!(inference_req.prompt_context, prompt_context);
        assert_eq!(inference_req.profile.model, "gemini-2.5-flash");

        // Tampered prompt context in packet must fail constructor
        let mut tampered_packet = packet;
        tampered_packet.prompt_context = "tampered content".to_string();
        let err = ApprovedInferenceRequest::new(
            receipt,
            tampered_packet,
            "Explain structure".to_string(),
            snap_id,
            profile,
        );
        assert!(err.is_err());
    }

    #[test]
    fn test_bearer_token_redaction() {
        let sample = "Authorization: Bearer my_super_secret_token_1234567890abcdef\n";
        let (redacted, count) = EgressFilter::scan_and_redact_secrets(sample);
        assert_eq!(count, 1);
        assert!(!redacted.contains("my_super_secret_token_1234567890abcdef"));
        assert!(redacted.contains("Bearer [REDACTED_BEARER_TOKEN]"));
    }

    #[test]
    fn test_extended_tokens_redaction() {
        let sample = "anthropic = \"sk-ant-api03-abcdef1234567890abcdef\"\nhf = \"hf_abcdefghijklmnopqrstuvwxyz123456\"\nslack = \"xoxb-1234567890-abcdef\"\n";
        let (redacted, count) = EgressFilter::scan_and_redact_secrets(sample);
        assert_eq!(count, 3);
        assert!(redacted.contains("[REDACTED_ANTHROPIC_KEY]"));
        assert!(redacted.contains("[REDACTED_HF_TOKEN]"));
        assert!(redacted.contains("[REDACTED_SLACK_TOKEN]"));
    }
}
