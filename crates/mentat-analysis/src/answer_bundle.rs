use mentat_core::models::{AnswerBundle, Claim, ClaimClassification, EvidenceRef, FileRecord};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use uuid::Uuid;

/// [IMP-F004] Parses and validates cloud model output against the current snapshot.
pub struct AnswerBundleNormalizer;

impl AnswerBundleNormalizer {
    pub fn system_contract(snapshot_id: Uuid) -> String {
        format!(
            r#"You are Code Mentat, a strict read-only repository advisor.
Respond with a single JSON AnswerBundle object and no markdown wrapper.
Required schema:
{{
  "request_id": "<uuid>",
  "snapshot_id": "{snapshot_id}",
  "direct_answer": "<string>",
  "claims": [{{
    "id": "<uuid>",
    "classification": "Observed|Inferred|Proposed|Conflict|Unknown",
    "statement": "<string>",
    "confidence": 0.0,
    "evidence_ids": ["<uuid>"],
    "rationale": "<string or null>"
  }}],
  "evidence_map": [{{
    "id": "<uuid>",
    "snapshot_id": "{snapshot_id}",
    "relative_path": "<path from catalog>",
    "line_start": 1,
    "line_end": 1,
    "content_hash": "<exact catalog hash>",
    "excerpt": "<verbatim lines from provided file body>"
  }}],
  "recommendations": [],
  "conflicts": [],
  "raw_model_response": null
}}
Use only catalog paths, the current snapshot_id, and the provided content_hash values.
If evidence is missing, classify the claim as Unknown. Do not invent files or hashes."#
        )
    }

    pub fn from_model_text(
        request_id: Uuid,
        snapshot_id: Uuid,
        full_text: &str,
        snapshot_files: &[FileRecord],
    ) -> AnswerBundle {
        Self::from_model_text_with_contents(
            request_id,
            snapshot_id,
            full_text,
            snapshot_files,
            &HashMap::new(),
        )
    }

    pub fn from_model_text_with_contents(
        request_id: Uuid,
        snapshot_id: Uuid,
        full_text: &str,
        snapshot_files: &[FileRecord],
        file_texts: &HashMap<PathBuf, String>,
    ) -> AnswerBundle {
        if let Some(mut bundle) = Self::try_parse_bundle(full_text) {
            bundle.request_id = request_id;
            bundle.snapshot_id = snapshot_id;
            bundle.raw_model_response = Some(full_text.to_string());
            Self::validate_citations(&mut bundle, snapshot_id, snapshot_files, file_texts);
            bundle.direct_answer = Self::compose_verified_answer(&bundle.claims);
            return bundle;
        }

        AnswerBundle {
            request_id,
            snapshot_id,
            direct_answer: "검증된 근거 기반 답변을 생성할 수 없습니다. 모델 원문은 검증되지 않은 진단 데이터로만 보존되었습니다.".to_string(),
            claims: vec![Claim {
                id: Uuid::new_v4(),
                classification: ClaimClassification::Unknown,
                statement: "모델 응답이 구조화된 AnswerBundle이 아닙니다.".to_string(),
                confidence: 0.0,
                evidence_ids: vec![],
                rationale: Some("UNSTRUCTURED_RESPONSE".to_string()),
            }],
            evidence_map: Vec::new(),
            recommendations: Vec::new(),
            conflicts: Vec::new(),
            raw_model_response: Some(full_text.to_string()),
        }
    }

    fn try_parse_bundle(full_text: &str) -> Option<AnswerBundle> {
        let trimmed = full_text.trim();
        if let Ok(bundle) = serde_json::from_str::<AnswerBundle>(trimmed) {
            return Some(bundle);
        }
        if let Some(json) = extract_json_object(trimmed) {
            if let Ok(bundle) = serde_json::from_str::<AnswerBundle>(json) {
                return Some(bundle);
            }
        }
        None
    }

    pub fn validate_citations(
        bundle: &mut AnswerBundle,
        current_snapshot_id: Uuid,
        snapshot_files: &[FileRecord],
        file_texts: &HashMap<PathBuf, String>,
    ) {
        bundle.snapshot_id = current_snapshot_id;
        let mut invalid_ids = HashSet::new();
        let mut evidence_id_counts = HashMap::<Uuid, usize>::new();
        for evidence in &bundle.evidence_map {
            *evidence_id_counts.entry(evidence.id).or_default() += 1;
        }

        for ev in &mut bundle.evidence_map {
            if evidence_id_counts.get(&ev.id).copied().unwrap_or_default() != 1
                || !citation_is_valid(ev, current_snapshot_id, snapshot_files, file_texts)
            {
                invalid_ids.insert(ev.id);
                if !ev.excerpt.starts_with("[INVALID_CITATION]") {
                    ev.excerpt = format!("[INVALID_CITATION] {}", ev.excerpt);
                }
            }
            ev.snapshot_id = current_snapshot_id;
        }

        for claim in &mut bundle.claims {
            let mut seen = HashSet::new();
            let has_duplicate = !claim.evidence_ids.iter().all(|id| seen.insert(*id));
            let has_invalid = claim.evidence_ids.iter().any(|id| {
                invalid_ids.contains(id) || !bundle.evidence_map.iter().any(|e| e.id == *id)
            });

            claim.evidence_ids.retain(|id| {
                !invalid_ids.contains(id) && bundle.evidence_map.iter().any(|e| e.id == *id)
            });
            claim.evidence_ids.sort_unstable();
            claim.evidence_ids.dedup();

            let evidence_required = matches!(
                claim.classification,
                ClaimClassification::Observed | ClaimClassification::Conflict
            );
            let missing_required_evidence = evidence_required && claim.evidence_ids.is_empty();
            let invalid_confidence =
                !claim.confidence.is_finite() || !(0.0..=1.0).contains(&claim.confidence);

            if has_invalid || has_duplicate || missing_required_evidence || invalid_confidence {
                claim.classification = ClaimClassification::Unknown;
                claim.confidence = 0.0;
                claim.rationale = Some(
                    if invalid_confidence {
                        "INVALID_CONFIDENCE"
                    } else if has_duplicate {
                        "DUPLICATE_EVIDENCE"
                    } else if missing_required_evidence {
                        "EVIDENCE_REQUIRED"
                    } else {
                        "INVALID_CITATION"
                    }
                    .to_string(),
                );
            }
        }
    }

    fn compose_verified_answer(claims: &[Claim]) -> String {
        let lines: Vec<String> = claims
            .iter()
            .filter(|claim| claim.classification != ClaimClassification::Unknown)
            .map(|claim| {
                let classification = match claim.classification {
                    ClaimClassification::Observed => "OBSERVED",
                    ClaimClassification::Inferred => "INFERRED",
                    ClaimClassification::Proposed => "PROPOSED",
                    ClaimClassification::Conflict => "CONFLICT",
                    ClaimClassification::Unknown => unreachable!(),
                };
                format!(
                    "- [{classification}] {} (신뢰도 {:.2}, 근거 {}건)",
                    claim.statement,
                    claim.confidence,
                    claim.evidence_ids.len()
                )
            })
            .collect();

        if lines.is_empty() {
            "검증된 근거 기반 답변을 생성할 수 없습니다. 모델 원문은 검증되지 않은 진단 데이터로만 보존되었습니다.".to_string()
        } else {
            format!("검증된 주장 기반 답변:\n{}", lines.join("\n"))
        }
    }
}

fn citation_is_valid(
    ev: &EvidenceRef,
    current_snapshot_id: Uuid,
    snapshot_files: &[FileRecord],
    file_texts: &HashMap<PathBuf, String>,
) -> bool {
    if ev.snapshot_id != Uuid::nil() && ev.snapshot_id != current_snapshot_id {
        return false;
    }

    let Some(file) = snapshot_files
        .iter()
        .find(|f| f.relative_path == ev.relative_path)
    else {
        return false;
    };

    if ev.content_hash != file.content_hash {
        return false;
    }

    if ev.line_start == 0 || ev.line_end < ev.line_start {
        return false;
    }
    if let Some(lines) = file.line_count {
        if ev.line_start > lines || ev.line_end > lines {
            return false;
        }
    }

    let source = file_texts
        .get(&ev.relative_path)
        .cloned()
        .or_else(|| file.text_preview.clone());
    let Some(source) = source else {
        return false;
    };

    excerpt_matches_range(&source, ev.line_start, ev.line_end, &ev.excerpt)
}

fn excerpt_matches_range(source: &str, line_start: usize, line_end: usize, excerpt: &str) -> bool {
    if excerpt.trim().is_empty() {
        return false;
    }
    let lines: Vec<&str> = source.lines().collect();
    if line_start == 0 || line_start > lines.len() {
        return false;
    }
    let start_idx = line_start - 1;
    let end_idx = line_end.min(lines.len());
    if start_idx >= end_idx {
        return false;
    }
    let ranged = lines[start_idx..end_idx].join("\n");
    let excerpt_norm = excerpt.replace("\r\n", "\n").trim().to_string();
    let ranged_norm = ranged.replace("\r\n", "\n");
    ranged_norm.trim() == excerpt_norm || ranged_norm.contains(&excerpt_norm)
}

fn extract_json_object(text: &str) -> Option<&str> {
    if let Some(start) = text.find("```json") {
        let content_start = start + 7;
        if let Some(rel_end) = text[content_start..].find("```") {
            return Some(text[content_start..content_start + rel_end].trim());
        }
    }
    let start = text.find('{')?;
    let end = text.rfind('}')?;
    if end > start {
        Some(&text[start..=end])
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mentat_core::models::FileKind;

    fn sample_file(path: &str, lines: usize, hash: &str, preview: &str) -> FileRecord {
        FileRecord {
            relative_path: PathBuf::from(path),
            kind: FileKind::SourceCode,
            size_bytes: preview.len() as u64,
            content_hash: hash.to_string(),
            is_text: true,
            line_count: Some(lines),
            text_preview: Some(preview.to_string()),
        }
    }

    struct CitationCase<'a> {
        snap: Uuid,
        claim_class: &'a str,
        ev_id: Uuid,
        ev_snap: Uuid,
        path: &'a str,
        start: usize,
        end: usize,
        hash: &'a str,
        excerpt: &'a str,
        extra_ev: Option<(Uuid, &'a str)>,
    }

    fn bundle_json(case: CitationCase<'_>) -> String {
        let extra = if let Some((id, excerpt2)) = case.extra_ev {
            format!(
                r#",{{
                    "id": "{id}",
                    "snapshot_id": "{ev_snap}",
                    "relative_path": "src/main.rs",
                    "line_start": 1,
                    "line_end": 1,
                    "content_hash": "{hash}",
                    "excerpt": "{excerpt2}"
                }}"#,
                ev_snap = case.ev_snap,
                hash = case.hash
            )
        } else {
            String::new()
        };
        let extra_ids = if let Some((id, _)) = case.extra_ev {
            format!(r#", "{id}""#)
        } else {
            String::new()
        };
        format!(
            r#"{{
                "request_id": "{req}",
                "snapshot_id": "{snap}",
                "direct_answer": "cited",
                "claims": [{{
                    "id": "{claim}",
                    "classification": "{claim_class}",
                    "statement": "statement",
                    "confidence": 0.95,
                    "evidence_ids": ["{ev}"{extra_ids}],
                    "rationale": null
                }}],
                "evidence_map": [{{
                    "id": "{ev}",
                    "snapshot_id": "{ev_snap}",
                    "relative_path": "{path}",
                    "line_start": {start},
                    "line_end": {end},
                    "content_hash": "{hash}",
                    "excerpt": "{excerpt}"
                }}{extra}],
                "recommendations": [],
                "conflicts": [],
                "raw_model_response": null
            }}"#,
            req = Uuid::new_v4(),
            snap = case.snap,
            claim = Uuid::new_v4(),
            claim_class = case.claim_class,
            ev = case.ev_id,
            ev_snap = case.ev_snap,
            path = case.path,
            start = case.start,
            end = case.end,
            hash = case.hash,
            excerpt = case.excerpt
        )
    }

    #[test]
    fn test_imp_f004_unstructured_response_does_not_invent_claims() {
        let text = "# Summary\n- The API key is safe\n- Everything is fine";
        let bundle =
            AnswerBundleNormalizer::from_model_text(Uuid::new_v4(), Uuid::new_v4(), text, &[]);
        assert_eq!(bundle.claims.len(), 1);
        assert_eq!(
            bundle.claims[0].classification,
            ClaimClassification::Unknown
        );
        assert_eq!(
            bundle.claims[0].rationale.as_deref(),
            Some("UNSTRUCTURED_RESPONSE")
        );
        assert!(!bundle.direct_answer.contains("The API key is safe"));
        assert!(bundle
            .direct_answer
            .contains("검증된 근거 기반 답변을 생성할 수 없습니다"));
        assert_eq!(bundle.raw_model_response.as_deref(), Some(text));
    }

    #[test]
    fn test_imp_f004_invalid_citation_is_rejected() {
        let ev_id = Uuid::new_v4();
        let snap = Uuid::new_v4();
        let json = bundle_json(CitationCase {
            snap,
            claim_class: "Inferred",
            ev_id,
            ev_snap: snap,
            path: "missing.rs",
            start: 1,
            end: 10,
            hash: "deadbeef",
            excerpt: "not in snapshot",
            extra_ev: None,
        });
        let files = vec![sample_file("src/main.rs", 20, "abc", "fn main()\n")];
        let bundle = AnswerBundleNormalizer::from_model_text(Uuid::new_v4(), snap, &json, &files);
        assert_eq!(
            bundle.claims[0].classification,
            ClaimClassification::Unknown
        );
        assert!(bundle.evidence_map[0]
            .excerpt
            .starts_with("[INVALID_CITATION]"));
    }

    #[test]
    fn test_imp_f004_wrong_snapshot_hash_excerpt_range_and_mixed_evidence() {
        let current = Uuid::new_v4();
        let other_snap = Uuid::new_v4();
        let preview = "fn main() {\n    println!(\"hi\");\n}\n";
        let files = vec![sample_file("src/main.rs", 3, "filehash", preview)];

        let wrong_snap = bundle_json(CitationCase {
            snap: other_snap,
            claim_class: "Observed",
            ev_id: Uuid::new_v4(),
            ev_snap: other_snap,
            path: "src/main.rs",
            start: 1,
            end: 1,
            hash: "filehash",
            excerpt: "fn main() {",
            extra_ev: None,
        });
        let bundle =
            AnswerBundleNormalizer::from_model_text(Uuid::new_v4(), current, &wrong_snap, &files);
        assert_eq!(bundle.snapshot_id, current);
        assert_eq!(
            bundle.claims[0].classification,
            ClaimClassification::Unknown
        );

        let wrong_hash = bundle_json(CitationCase {
            snap: current,
            claim_class: "Observed",
            ev_id: Uuid::new_v4(),
            ev_snap: current,
            path: "src/main.rs",
            start: 1,
            end: 1,
            hash: "not-the-file-hash",
            excerpt: "fn main() {",
            extra_ev: None,
        });
        let bundle =
            AnswerBundleNormalizer::from_model_text(Uuid::new_v4(), current, &wrong_hash, &files);
        assert_eq!(
            bundle.claims[0].classification,
            ClaimClassification::Unknown
        );

        let wrong_excerpt = bundle_json(CitationCase {
            snap: current,
            claim_class: "Observed",
            ev_id: Uuid::new_v4(),
            ev_snap: current,
            path: "src/main.rs",
            start: 1,
            end: 1,
            hash: "filehash",
            excerpt: "totally fabricated excerpt",
            extra_ev: None,
        });
        let bundle = AnswerBundleNormalizer::from_model_text(
            Uuid::new_v4(),
            current,
            &wrong_excerpt,
            &files,
        );
        assert_eq!(
            bundle.claims[0].classification,
            ClaimClassification::Unknown
        );

        let overflow_range = bundle_json(CitationCase {
            snap: current,
            claim_class: "Observed",
            ev_id: Uuid::new_v4(),
            ev_snap: current,
            path: "src/main.rs",
            start: 1,
            end: 99,
            hash: "filehash",
            excerpt: "fn main() {",
            extra_ev: None,
        });
        let bundle = AnswerBundleNormalizer::from_model_text(
            Uuid::new_v4(),
            current,
            &overflow_range,
            &files,
        );
        assert_eq!(
            bundle.claims[0].classification,
            ClaimClassification::Unknown
        );

        let valid_id = Uuid::new_v4();
        let invalid_id = Uuid::new_v4();
        let mixed = bundle_json(CitationCase {
            snap: current,
            claim_class: "Observed",
            ev_id: valid_id,
            ev_snap: current,
            path: "src/main.rs",
            start: 1,
            end: 1,
            hash: "filehash",
            excerpt: "fn main() {",
            extra_ev: Some((invalid_id, "not in file")),
        });
        let bundle =
            AnswerBundleNormalizer::from_model_text(Uuid::new_v4(), current, &mixed, &files);
        assert_eq!(
            bundle.claims[0].classification,
            ClaimClassification::Unknown
        );
        assert_eq!(
            bundle.claims[0].rationale.as_deref(),
            Some("INVALID_CITATION")
        );
    }

    #[test]
    fn test_imp_f004_valid_json_bundle_keeps_observed_claim() {
        let ev_id = Uuid::new_v4();
        let snap = Uuid::new_v4();
        let preview = "fn main()\nlet x = 1;\nlet y = 2;\nlet z = 3;\n";
        let json = bundle_json(CitationCase {
            snap,
            claim_class: "Observed",
            ev_id,
            ev_snap: snap,
            path: "src/main.rs",
            start: 1,
            end: 4,
            hash: "abc",
            excerpt: "fn main()",
            extra_ev: None,
        });
        let files = vec![sample_file("src/main.rs", 20, "abc", preview)];
        let bundle = AnswerBundleNormalizer::from_model_text(Uuid::new_v4(), snap, &json, &files);
        assert_eq!(
            bundle.claims[0].classification,
            ClaimClassification::Observed
        );
        assert_eq!(bundle.claims[0].evidence_ids, vec![ev_id]);
        assert!(!bundle.direct_answer.contains("cited"));
        assert!(bundle.direct_answer.contains("statement"));
        assert!(bundle.direct_answer.contains("[OBSERVED]"));
        assert!(!bundle.evidence_map[0]
            .excerpt
            .starts_with("[INVALID_CITATION]"));
    }

    #[test]
    fn test_imp_f004_claim_invariants_reject_empty_duplicate_and_invalid_confidence() {
        let snapshot = Uuid::new_v4();
        let evidence_id = Uuid::new_v4();
        let files = vec![sample_file("src/main.rs", 1, "hash", "fn main() {}\n")];
        let evidence = EvidenceRef {
            id: evidence_id,
            snapshot_id: snapshot,
            relative_path: PathBuf::from("src/main.rs"),
            line_start: 1,
            line_end: 1,
            content_hash: "hash".to_string(),
            excerpt: "fn main() {}".to_string(),
        };
        let make_claim = |classification, confidence, evidence_ids| Claim {
            id: Uuid::new_v4(),
            classification,
            statement: "claim".to_string(),
            confidence,
            evidence_ids,
            rationale: None,
        };
        let mut bundle = AnswerBundle {
            request_id: Uuid::new_v4(),
            snapshot_id: snapshot,
            direct_answer: String::new(),
            claims: vec![
                make_claim(ClaimClassification::Observed, 0.9, vec![]),
                make_claim(ClaimClassification::Conflict, 0.8, vec![]),
                make_claim(
                    ClaimClassification::Observed,
                    0.9,
                    vec![evidence_id, evidence_id],
                ),
                make_claim(ClaimClassification::Inferred, 1.5, vec![evidence_id]),
            ],
            evidence_map: vec![evidence],
            recommendations: vec![],
            conflicts: vec![],
            raw_model_response: None,
        };

        AnswerBundleNormalizer::validate_citations(&mut bundle, snapshot, &files, &HashMap::new());

        assert!(bundle
            .claims
            .iter()
            .all(|claim| claim.classification == ClaimClassification::Unknown));
        assert!(bundle.claims.iter().all(|claim| claim.confidence == 0.0));
    }
}
