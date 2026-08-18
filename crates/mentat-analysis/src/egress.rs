use crate::detector::ProjectStructureSummary;
use mentat_core::error::MentatError;
use mentat_core::models::FileRecord;
use mentat_core::ports::RepositoryReader;
use mentat_inference::{BackendProfile, InferenceRequest};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
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
    pub snapshot_id: Uuid,
    pub redacted_user_question: String,
    pub included_file_texts: HashMap<PathBuf, String>,
}

impl EgressPacket {
    fn update_field(hasher: &mut Sha256, name: &str, value: &[u8]) {
        hasher.update((name.len() as u64).to_be_bytes());
        hasher.update(name.as_bytes());
        hasher.update((value.len() as u64).to_be_bytes());
        hasher.update(value);
    }

    fn provider_name(provider: &mentat_inference::ProviderKind) -> &'static str {
        use mentat_inference::ProviderKind;
        match provider {
            ProviderKind::GoogleGemini => "GoogleGemini",
            ProviderKind::OpenRouter => "OpenRouter",
            ProviderKind::OpenAi => "OpenAi",
            ProviderKind::OpenAICompatible => "OpenAICompatible",
            ProviderKind::CustomCompatible => "CustomCompatible",
            ProviderKind::LocalMock => "LocalMock",
        }
    }

    /// 승인 UI와 실제 outbound/citation 판정에 영향을 주는 모든 값을 하나의 digest로 결속한다.
    pub fn canonical_digest(&self, profile: &BackendProfile) -> String {
        let mut hasher = Sha256::new();
        Self::update_field(&mut hasher, "seal_version", b"code-mentat-egress-v1");
        Self::update_field(&mut hasher, "packet_id", self.packet_id.as_bytes());
        Self::update_field(&mut hasher, "snapshot_id", self.snapshot_id.as_bytes());
        Self::update_field(
            &mut hasher,
            "prompt_context",
            self.prompt_context.as_bytes(),
        );
        Self::update_field(
            &mut hasher,
            "redacted_user_question",
            self.redacted_user_question.as_bytes(),
        );
        Self::update_field(
            &mut hasher,
            "redacted_secret_occurrences",
            &(self.redacted_secret_occurrences as u64).to_be_bytes(),
        );
        Self::update_field(
            &mut hasher,
            "estimated_tokens",
            &(self.estimated_tokens as u64).to_be_bytes(),
        );

        let mut included_files: Vec<_> = self
            .included_files
            .iter()
            .map(|path| path.to_string_lossy().into_owned())
            .collect();
        included_files.sort();
        for path in included_files {
            Self::update_field(&mut hasher, "included_file", path.as_bytes());
        }

        let mut excluded_files: Vec<_> = self
            .excluded_sensitive_files
            .iter()
            .map(|path| path.to_string_lossy().into_owned())
            .collect();
        excluded_files.sort();
        for path in excluded_files {
            Self::update_field(&mut hasher, "excluded_file", path.as_bytes());
        }

        let mut refs = self.included_file_refs.clone();
        refs.sort_by(|left, right| {
            left.relative_path
                .cmp(&right.relative_path)
                .then(left.line_start.cmp(&right.line_start))
                .then(left.line_end.cmp(&right.line_end))
                .then(left.line_count.cmp(&right.line_count))
        });
        for reference in refs {
            Self::update_field(
                &mut hasher,
                "ref_path",
                reference.relative_path.to_string_lossy().as_bytes(),
            );
            Self::update_field(
                &mut hasher,
                "ref_line_start",
                &(reference.line_start as u64).to_be_bytes(),
            );
            Self::update_field(
                &mut hasher,
                "ref_line_end",
                &(reference.line_end as u64).to_be_bytes(),
            );
            Self::update_field(
                &mut hasher,
                "ref_line_count",
                &(reference.line_count as u64).to_be_bytes(),
            );
        }

        let mut validation_texts: Vec<_> = self.included_file_texts.iter().collect();
        validation_texts.sort_by_key(|(path, _)| *path);
        for (path, text) in validation_texts {
            let text_digest = Sha256::digest(text.as_bytes());
            Self::update_field(
                &mut hasher,
                "validation_path",
                path.to_string_lossy().as_bytes(),
            );
            Self::update_field(&mut hasher, "validation_text_sha256", &text_digest);
        }

        Self::update_field(&mut hasher, "profile_id", profile.id.as_bytes());
        Self::update_field(
            &mut hasher,
            "provider",
            Self::provider_name(&profile.provider).as_bytes(),
        );
        Self::update_field(
            &mut hasher,
            "base_url",
            profile.base_url.trim_end_matches('/').as_bytes(),
        );
        Self::update_field(&mut hasher, "model", profile.model.as_bytes());
        Self::update_field(
            &mut hasher,
            "timeout_secs",
            &profile.timeout_secs.to_be_bytes(),
        );
        format!("{:x}", hasher.finalize())
    }

    pub fn seal_for_profile(&mut self, profile: &BackendProfile) {
        self.packet_hash = self.canonical_digest(profile);
    }

    fn has_consistent_validation_sources(&self) -> bool {
        let mut files = self.included_files.clone();
        files.sort();
        files.dedup();
        let mut ref_paths: Vec<_> = self
            .included_file_refs
            .iter()
            .map(|reference| reference.relative_path.clone())
            .collect();
        ref_paths.sort();
        ref_paths.dedup();
        let mut text_paths: Vec<_> = self.included_file_texts.keys().cloned().collect();
        text_paths.sort();
        text_paths.dedup();
        files == ref_paths && files == text_paths
    }
}

impl EgressReceipt {
    pub fn issue(packet: &EgressPacket, profile: &BackendProfile) -> Self {
        Self {
            receipt_id: Uuid::new_v4(),
            packet_hash: packet.canonical_digest(profile),
            snapshot_id: packet.snapshot_id,
            token_count: packet.estimated_tokens,
            file_count: packet.included_files.len(),
            granted_at: chrono::Utc::now().to_rfc3339(),
        }
    }
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
        let redacted_question = EgressFilter::scan_and_redact_secrets(&user_question).0;
        if redacted_question != packet.redacted_user_question {
            return Err(MentatError::EgressViolation(
                "승인된 질문과 EgressPacket 질문이 일치하지 않습니다.".to_string(),
            ));
        }
        if packet.snapshot_id != snapshot_id || receipt.snapshot_id != packet.snapshot_id {
            return Err(MentatError::EgressViolation(
                "승인 receipt, packet, 현재 snapshot ID가 일치하지 않습니다.".to_string(),
            ));
        }
        if !packet.has_consistent_validation_sources() {
            return Err(MentatError::EgressViolation(
                "포함 파일, 행 참조, citation validation text 집합이 일치하지 않습니다."
                    .to_string(),
            ));
        }

        let computed_packet_hash = packet.canonical_digest(&approved_profile);

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

        let approved_digest = computed_packet_hash;

        Ok(Self {
            receipt,
            packet,
            user_question: redacted_question,
            snapshot_id,
            approved_profile,
            approved_digest,
        })
    }

    pub fn verify_integrity(&self) -> bool {
        let computed_packet_hash = self.packet.canonical_digest(&self.approved_profile);

        self.approved_digest == computed_packet_hash
            && self.receipt.packet_hash == computed_packet_hash
            && self.packet.packet_hash == computed_packet_hash
            && self.packet.snapshot_id == self.snapshot_id
            && self.receipt.snapshot_id == self.snapshot_id
            && self.packet.redacted_user_question == self.user_question
            && self.packet.has_consistent_validation_sources()
    }

    pub fn citation_file_texts(&self) -> &HashMap<PathBuf, String> {
        &self.packet.included_file_texts
    }

    /// [SEC-F001] Consume-once API: Consumes `self` by value to generate the final sealed `InferenceRequest`
    pub fn into_inference_request(self) -> Result<InferenceRequest, MentatError> {
        if !self.verify_integrity() {
            return Err(MentatError::EgressViolation(
                "승인된 요청의 무결성 검증에 실패했습니다.".to_string(),
            ));
        }

        let question = if self.packet.redacted_user_question.is_empty() {
            EgressFilter::scan_and_redact_secrets(&self.user_question).0
        } else {
            self.packet.redacted_user_question.clone()
        };

        Ok(InferenceRequest {
            request_id: self.receipt.receipt_id,
            system_contract: crate::AnswerBundleNormalizer::system_contract(self.snapshot_id),
            prompt_context: self.packet.prompt_context,
            user_question: question,
            profile: self.approved_profile,
        })
    }
}

pub const MIN_CONTENT_RELEVANCE_SCORE: usize = 3;
pub const HIGH_ENTROPY_MIN_LEN: usize = 24;
pub const HIGH_ENTROPY_THRESHOLD: f64 = 4.0;

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

        let (entropy_redacted, entropy_count) = Self::redact_high_entropy_tokens(&output);
        (entropy_redacted, redaction_count + entropy_count)
    }

    /// [SEC-F002] Generic detector for unknown credential-shaped high-entropy tokens.
    pub fn redact_high_entropy_tokens(text: &str) -> (String, usize) {
        let mut count = 0;
        let mut out = String::with_capacity(text.len());
        let mut rest = text;
        while let Some(idx) = rest.find(|c: char| is_secret_token_char(c)) {
            out.push_str(&rest[..idx]);
            let tail = &rest[idx..];
            let end = tail
                .find(|c: char| !is_secret_token_char(c))
                .unwrap_or(tail.len());
            let token = &tail[..end];
            if token.len() >= HIGH_ENTROPY_MIN_LEN
                && shannon_entropy(token) >= HIGH_ENTROPY_THRESHOLD
                && looks_secret_like(token)
            {
                out.push_str("[REDACTED_HIGH_ENTROPY]");
                count += 1;
            } else {
                out.push_str(token);
            }
            rest = &tail[end..];
        }
        out.push_str(rest);
        (out, count)
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
                    Self::update_earliest(
                        &mut earliest_match,
                        pos,
                        end,
                        "[REDACTED_ANTHROPIC_KEY]",
                    );
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
        snapshot_id: Uuid,
        profile: &BackendProfile,
    ) -> Result<EgressPacket, MentatError> {
        Self::assemble_packet_with_user_exclusions(
            reader,
            files,
            summary,
            user_question,
            &[],
            snapshot_id,
            profile,
        )
        .await
    }

    /// [SEC-F002] Query-aware context assembly with per-request user exclusions and exact file/line preview
    pub async fn assemble_packet_with_user_exclusions(
        reader: &(impl RepositoryReader + ?Sized),
        files: &[FileRecord],
        summary: &ProjectStructureSummary,
        user_question: &str,
        user_excluded_files: &[std::path::PathBuf],
        snapshot_id: Uuid,
        profile: &BackendProfile,
    ) -> Result<EgressPacket, MentatError> {
        let mut included_files = Vec::new();
        let mut included_file_refs = Vec::new();
        let mut included_file_texts = HashMap::new();
        let mut excluded_sensitive_files = Vec::new();
        let mut total_redactions = 0;
        let mut context_buffer = String::new();

        let (redacted_user_question, question_redactions) =
            Self::scan_and_redact_secrets(user_question);
        total_redactions += question_redactions;

        context_buffer.push_str(&format!(
            "# Repository Context Summary\nSnapshot ID: {}\nPrimary Language: {}\nTotal Files: {}\n\n",
            snapshot_id,
            summary.primary_language.as_deref().unwrap_or("Unknown"),
            files.len()
        ));
        context_buffer.push_str("## Citation Catalog\n");
        context_buffer.push_str("| path | content_hash | allowed_lines |\n|---|---|---|\n");

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

        for (file, score) in scored_files {
            if user_excluded_files.contains(&file.relative_path) {
                excluded_sensitive_files.push(file.relative_path.clone());
                continue;
            }

            if Self::is_sensitive_filename(&file.relative_path) {
                excluded_sensitive_files.push(file.relative_path.clone());
                continue;
            }

            // [SEC-F002] Do not retrieve content for files below the relevance threshold.
            if score < MIN_CONTENT_RELEVANCE_SCORE {
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
                let content = reader
                    .read_file_content(&file.relative_path)
                    .await
                    .map_err(|_| {
                        MentatError::EgressViolation(format!(
                            "스캔 이후 파일을 다시 읽을 수 없어 egress를 차단했습니다: {}",
                            file.relative_path.display()
                        ))
                    })?;
                let live_hash = format!("{:x}", Sha256::digest(content.as_bytes()));
                if live_hash != file.content_hash {
                    return Err(MentatError::EgressViolation(format!(
                        "스캔 이후 파일 내용이 변경되어 egress를 차단했습니다: {}",
                        file.relative_path.display()
                    )));
                }
                let lines: Vec<&str> = content.lines().take(60).collect();
                let line_count = lines.len();
                let truncated = lines.join("\n");
                let (redacted, count) = Self::scan_and_redact_secrets(&truncated);
                total_redactions += count;

                context_buffer.push_str(&format!(
                    "| {} | {} | 1-{} |\n",
                    file.relative_path.display(),
                    file.content_hash,
                    line_count
                ));
                context_buffer.push_str(&format!(
                    "### File: {}\nhash: {}\nallowed_lines: 1-{}\n```\n{}\n```\n\n",
                    file.relative_path.display(),
                    file.content_hash,
                    line_count,
                    redacted
                ));

                included_files.push(file.relative_path.clone());
                included_file_refs.push(IncludedFileRef {
                    relative_path: file.relative_path.clone(),
                    line_start: 1,
                    line_end: line_count,
                    line_count,
                });
                included_file_texts.insert(file.relative_path.clone(), redacted);
            }
        }

        // Question lives only in redacted_user_question; not duplicated here.
        let estimated_tokens = (context_buffer.len() + redacted_user_question.len()).div_ceil(4);

        let mut packet = EgressPacket {
            packet_id: Uuid::new_v4(),
            packet_hash: String::new(),
            included_files,
            included_file_refs,
            excluded_sensitive_files,
            redacted_secret_occurrences: total_redactions,
            estimated_tokens,
            prompt_context: context_buffer,
            snapshot_id,
            redacted_user_question,
            included_file_texts,
        };
        packet.seal_for_profile(profile);
        Ok(packet)
    }
}

fn is_secret_token_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '+' | '/' | '=' | '_' | '-')
}

fn looks_secret_like(token: &str) -> bool {
    let has_digit = token.chars().any(|c| c.is_ascii_digit());
    let has_lower = token.chars().any(|c| c.is_ascii_lowercase());
    let has_upper = token.chars().any(|c| c.is_ascii_uppercase());
    has_digit && (has_lower || has_upper)
}

fn shannon_entropy(token: &str) -> f64 {
    let mut freq = [0u32; 256];
    let bytes = token.as_bytes();
    for &b in bytes {
        freq[b as usize] += 1;
    }
    let len = bytes.len() as f64;
    freq.iter()
        .filter(|&&c| c > 0)
        .map(|&c| {
            let p = f64::from(c) / len;
            -p * p.log2()
        })
        .sum()
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
        let question = EgressFilter::scan_and_redact_secrets("Explain structure").0;
        let profile = BackendProfile {
            id: Uuid::new_v4(),
            name: "Gemini Test".to_string(),
            provider: ProviderKind::GoogleGemini,
            base_url: ProviderKind::GoogleGemini.default_base_url().to_string(),
            model: "fixture-gemini".to_string(),
            api_key: Some("dummy".to_string()),
            timeout_secs: 30,
        };
        let mut validation_texts = HashMap::new();
        validation_texts.insert(PathBuf::from("Cargo.toml"), "[workspace]".to_string());
        let mut packet = EgressPacket {
            packet_id: Uuid::new_v4(),
            packet_hash: String::new(),
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
            snapshot_id: snap_id,
            redacted_user_question: question.clone(),
            included_file_texts: validation_texts,
        };
        packet.seal_for_profile(&profile);
        let receipt = EgressReceipt::issue(&packet, &profile);

        let approved = ApprovedInferenceRequest::new(
            receipt.clone(),
            packet.clone(),
            question.clone(),
            snap_id,
            profile.clone(),
        )
        .expect("Approved request should build");

        assert!(approved.verify_integrity());

        // Consume-once execution
        let inference_req = approved
            .into_inference_request()
            .expect("Should consume approved request");
        assert_eq!(inference_req.user_question, question);
        assert_eq!(inference_req.prompt_context, prompt_context);
        assert_eq!(inference_req.profile.model, "fixture-gemini");

        let assert_tamper_rejected =
            |packet: EgressPacket, profile: BackendProfile, question: &str, snapshot_id: Uuid| {
                assert!(ApprovedInferenceRequest::new(
                    receipt.clone(),
                    packet,
                    question.to_string(),
                    snapshot_id,
                    profile,
                )
                .is_err());
            };

        let mut question_swap = packet.clone();
        question_swap.redacted_user_question = "Explain structurE".to_string();
        assert_tamper_rejected(question_swap, profile.clone(), &question, snap_id);

        let mut validation_swap = packet.clone();
        validation_swap
            .included_file_texts
            .insert(PathBuf::from("Cargo.toml"), "[package]".to_string());
        assert_tamper_rejected(validation_swap, profile.clone(), &question, snap_id);

        let mut snapshot_swap = packet.clone();
        snapshot_swap.snapshot_id = Uuid::new_v4();
        assert_tamper_rejected(snapshot_swap, profile.clone(), &question, snap_id);

        let mut ref_swap = packet.clone();
        ref_swap.included_file_refs[0].line_end = 19;
        assert_tamper_rejected(ref_swap, profile.clone(), &question, snap_id);

        let mut endpoint_swap = profile.clone();
        endpoint_swap.base_url = "https://example.invalid".to_string();
        assert_tamper_rejected(packet.clone(), endpoint_swap, &question, snap_id);

        let mut model_swap = profile;
        model_swap.model = "other-model".to_string();
        assert_tamper_rejected(packet, model_swap, &question, snap_id);
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

    #[test]
    fn test_sec_f002_high_entropy_redaction() {
        let secret = "xK9mP2vQ8nL4wR7tY1uI0oE3aS6dF5gH";
        let sample = format!("opaque provider blob {secret} in notes\nplain hello world\n");
        let (redacted, count) = EgressFilter::scan_and_redact_secrets(&sample);
        assert!(count >= 1);
        assert!(!redacted.contains(secret));
        assert!(redacted.contains("[REDACTED_HIGH_ENTROPY]"));
        assert!(redacted.contains("hello world"));
    }

    #[test]
    fn test_sec_f002_outbound_question_zero_leak_and_single_copy() {
        let secret = "sk-ant-api03-abcdef1234567890abcdef";
        let entropy = "xK9mP2vQ8nL4wR7tY1uI0oE3aS6dF5gH";
        let unicode = "한글앞AIzaSyD-1234567890abcdef1234567890abcde한글뒤";
        let question = format!("use {secret} and {entropy} and {unicode}");
        let (redacted, count) = EgressFilter::scan_and_redact_secrets(&question);
        assert!(count >= 2);
        assert!(!redacted.contains("sk-ant-api03-abcdef1234567890abcdef"));
        assert!(!redacted.contains("AIzaSyD-1234567890abcdef1234567890abcde"));
        assert!(!redacted.contains(entropy));
        assert!(
            redacted.contains("[REDACTED_ANTHROPIC_KEY]")
                || redacted.contains("[REDACTED_HIGH_ENTROPY]")
        );
        assert!(
            !redacted.contains("## User Question\n")
                || redacted.matches("## User Question").count() <= 1
        );
    }

    #[tokio::test]
    async fn test_sec_f002_zero_score_file_excluded_from_content() {
        use async_trait::async_trait;
        use mentat_core::models::{
            FileKind, RepositoryProfile, RepositorySnapshot, RepositoryType, SnapshotStatus,
        };
        use mentat_core::ports::RepositoryReader;
        use std::collections::HashMap;

        struct MapReader {
            files: HashMap<PathBuf, String>,
        }

        #[async_trait]
        impl RepositoryReader for MapReader {
            fn root_path(&self) -> &std::path::Path {
                std::path::Path::new(".")
            }
            fn profile(&self) -> &RepositoryProfile {
                use std::sync::OnceLock;
                static PROFILE: OnceLock<RepositoryProfile> = OnceLock::new();
                PROFILE.get_or_init(|| RepositoryProfile {
                    id: Uuid::new_v4(),
                    display_name: "t".into(),
                    root_path: PathBuf::from("."),
                    repo_type: RepositoryType::Directory,
                    consent_policy: false,
                })
            }
            async fn scan_files(&self) -> Result<Vec<FileRecord>, MentatError> {
                Ok(vec![])
            }
            async fn read_file_content(
                &self,
                relative_path: &std::path::Path,
            ) -> Result<String, MentatError> {
                self.files
                    .get(relative_path)
                    .cloned()
                    .ok_or_else(|| MentatError::IoError("missing".into()))
            }
            async fn read_file_lines(
                &self,
                relative_path: &std::path::Path,
                _s: usize,
                _e: usize,
            ) -> Result<String, MentatError> {
                self.read_file_content(relative_path).await
            }
            async fn create_snapshot(&self) -> Result<RepositorySnapshot, MentatError> {
                Ok(RepositorySnapshot {
                    id: Uuid::new_v4(),
                    repo_id: Uuid::new_v4(),
                    created_at: chrono::Utc::now(),
                    tree_digest: "x".into(),
                    status: SnapshotStatus::Ready,
                    file_count: 0,
                    total_bytes: 0,
                })
            }
        }

        let unrelated = PathBuf::from("docs/unrelated_notes.md");
        let entry = PathBuf::from("src/main.rs");
        let entry_content = "fn main() { authentication(); }";
        let entry_hash = format!("{:x}", Sha256::digest(entry_content.as_bytes()));
        let reader = MapReader {
            files: HashMap::from([
                (
                    unrelated.clone(),
                    "totally unrelated gardening notes".into(),
                ),
                (entry.clone(), entry_content.into()),
            ]),
        };
        let files = vec![
            FileRecord {
                relative_path: unrelated.clone(),
                kind: FileKind::Documentation,
                size_bytes: 20,
                content_hash: "a".into(),
                is_text: true,
                line_count: Some(1),
                text_preview: None,
            },
            FileRecord {
                relative_path: entry.clone(),
                kind: FileKind::SourceCode,
                size_bytes: 30,
                content_hash: entry_hash,
                is_text: true,
                line_count: Some(1),
                text_preview: None,
            },
        ];
        let summary = crate::ProjectDetector::summarize(&files);
        let packet = EgressFilter::assemble_packet(
            &reader,
            &files,
            &summary,
            "authentication middleware",
            Uuid::new_v4(),
            &BackendProfile::default(),
        )
        .await
        .unwrap();

        assert!(!packet.included_files.contains(&unrelated));
        assert!(packet.excluded_sensitive_files.contains(&unrelated));
        assert!(packet.included_files.contains(&entry));
        assert!(!packet.prompt_context.contains("gardening"));

        let mut stale_files = files;
        stale_files[1].content_hash = "stale-scan-hash".to_string();
        let stale_summary = crate::ProjectDetector::summarize(&stale_files);
        let stale_result = EgressFilter::assemble_packet(
            &reader,
            &stale_files,
            &stale_summary,
            "authentication middleware",
            Uuid::new_v4(),
            &BackendProfile::default(),
        )
        .await;
        assert!(matches!(stale_result, Err(MentatError::EgressViolation(_))));
    }
}
