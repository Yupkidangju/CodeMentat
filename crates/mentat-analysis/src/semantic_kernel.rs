use crate::detector::ProjectStructureSummary;
use crate::evidence::EvidenceIndex;
use mentat_core::error::MentatError;
use mentat_core::models::{
    AnswerBundle, Claim, ClaimClassification, ConflictItem, FileRecord, Recommendation,
    RecommendationBasis,
};
use mentat_core::ports::RepositoryReader;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticKernel {
    pub purpose: String,
    pub primary_tech: String,
    pub invariants: Vec<String>,
    pub components: Vec<String>,
    pub uncertainties: Vec<String>,
}

pub struct SemanticKernelBuilder;

impl SemanticKernelBuilder {
    pub fn build(summary: &ProjectStructureSummary) -> SemanticKernel {
        let primary_tech = summary
            .primary_language
            .clone()
            .unwrap_or_else(|| "General Polyglot".to_string());

        let mut components = Vec::new();
        for entry in &summary.entry_points {
            components.push(format!("Entry Point: {}", entry.display()));
        }
        for manifest in &summary.manifests {
            components.push(format!("Manifest: {}", manifest.display()));
        }

        let mut invariants = vec![
            "저장소 파일 수정 불가 (Strict Read-Only Boundary)".to_string(),
            "모든 조언과 주장은 검증 가능한 파일 행 증거(EvidenceRef)에 기반함".to_string(),
        ];

        if summary.primary_language.as_deref() == Some("Rust") {
            invariants.push("Cargo workspace 모듈 경계 및 컴파일 무결성 준수".to_string());
        }

        let mut uncertainties = Vec::new();
        if summary.documents.is_empty() {
            uncertainties.push(
                "명시적 문서(README / spec.md)가 없어 코드 구조에서만 의도를 역추론함".to_string(),
            );
        }
        if summary.test_files.is_empty() {
            uncertainties.push("테스트 코드가 감지되지 않아 회귀 계약 검증이 제한됨".to_string());
        }

        SemanticKernel {
            purpose: format!("{} 기반 소프트웨어 프로젝트", primary_tech),
            primary_tech,
            invariants,
            components,
            uncertainties,
        }
    }

    /// [IMP-F004] Generates structured advice bundle for standard workflows with verified EvidenceRef linkages
    pub async fn run_local_workflow(
        workflow: &str,
        reader: &(impl RepositoryReader + ?Sized),
        files: &[FileRecord],
        summary: &ProjectStructureSummary,
        snapshot_id: Uuid,
    ) -> Result<AnswerBundle, MentatError> {
        let mut claims = Vec::new();
        let mut recommendations = Vec::new();
        let mut conflicts = Vec::new();
        let mut evidence_map = Vec::new();
        let mut index = EvidenceIndex::new(snapshot_id);

        let direct_answer: String;

        match workflow {
            "/onboard" | "onboard" => {
                let tech = summary.primary_language.as_deref().unwrap_or("알 수 없음");
                direct_answer = format!(
                    "이 프로젝트는 **{}** 기반 프로젝트입니다. 총 {}개의 소스 파일 (약 {} 라인)로 구성되어 있습니다.",
                    tech, summary.total_source_files, summary.total_lines_of_code
                );

                if let Some(manifest) = summary.manifests.first() {
                    if let Ok(ev) = index.create_evidence_ref(reader, manifest, 1, 10).await {
                        let ev_id = ev.id;
                        evidence_map.push(ev);
                        claims.push(Claim {
                            id: Uuid::new_v4(),
                            classification: ClaimClassification::Observed,
                            statement: format!(
                                "프로젝트 매니페스트 확인됨: {}",
                                manifest.display()
                            ),
                            confidence: 1.0,
                            evidence_ids: vec![ev_id],
                            rationale: Some("빌드 시스템 정의 파일".to_string()),
                        });
                    }
                }

                if let Some(entry) = summary.entry_points.first() {
                    if let Ok(ev) = index.create_evidence_ref(reader, entry, 1, 15).await {
                        let ev_id = ev.id;
                        evidence_map.push(ev);
                        claims.push(Claim {
                            id: Uuid::new_v4(),
                            classification: ClaimClassification::Observed,
                            statement: format!("메인 진입점 파일 발견: {}", entry.display()),
                            confidence: 0.95,
                            evidence_ids: vec![ev_id],
                            rationale: Some("애플리케이션 시작 지점".to_string()),
                        });
                    }
                }

                recommendations.push(Recommendation {
                    id: Uuid::new_v4(),
                    basis: RecommendationBasis::ProjectIntentAligned,
                    impact: "프로젝트 빠른 파악".to_string(),
                    rationale: "진입점과 권위 문서를 먼저 검토하는 것이 좋습니다.".to_string(),
                    evidence_ids: evidence_map.iter().map(|e| e.id).collect(),
                    decision_required: false,
                });
            }
            "/structure" | "structure" => {
                direct_answer = format!(
                    "언어 분포: {:?}\n매니페스트: {}개, 문서: {}개, 테스트 파일: {}개",
                    summary.languages,
                    summary.manifests.len(),
                    summary.documents.len(),
                    summary.test_files.len()
                );

                for doc in &summary.documents {
                    let mut ev_ids = Vec::new();
                    if let Ok(ev) = index.create_evidence_ref(reader, doc, 1, 5).await {
                        ev_ids.push(ev.id);
                        evidence_map.push(ev);
                    }
                    claims.push(Claim {
                        id: Uuid::new_v4(),
                        classification: ClaimClassification::Observed,
                        statement: format!("문서 파일: {}", doc.display()),
                        confidence: 1.0,
                        evidence_ids: ev_ids,
                        rationale: Some("설계 및 사양 문서".to_string()),
                    });
                }
            }
            "/conflicts" | "conflicts" => {
                direct_answer = "현재 감지된 문서-구현 간 충돌 사항 분석 결과입니다.".to_string();
                if summary.documents.is_empty() {
                    conflicts.push(ConflictItem {
                        id: Uuid::new_v4(),
                        side_a: "사양 문서 부재".to_string(),
                        side_b: "실제 소스코드 구현".to_string(),
                        evidence_ids: vec![],
                        impact: "설계 의도를 코드로만 역추론해야 함".to_string(),
                        unresolved_question: "공식 사양서(spec.md) 작성이 필요합니까?".to_string(),
                    });
                } else {
                    detect_doc_code_conflicts(
                        reader,
                        files,
                        summary,
                        &mut index,
                        &mut claims,
                        &mut conflicts,
                        &mut evidence_map,
                    )
                    .await;
                }
            }
            "/where" | "where" => {
                direct_answer = "주요 진입점 및 빌드 설정 파일 위치입니다.".to_string();
                for entry in &summary.entry_points {
                    let mut ev_ids = Vec::new();
                    if let Ok(ev) = index.create_evidence_ref(reader, entry, 1, 10).await {
                        ev_ids.push(ev.id);
                        evidence_map.push(ev);
                    }
                    claims.push(Claim {
                        id: Uuid::new_v4(),
                        classification: ClaimClassification::Observed,
                        statement: format!("진입점: {}", entry.display()),
                        confidence: 1.0,
                        evidence_ids: ev_ids,
                        rationale: Some("애플리케이션 진입점".to_string()),
                    });
                }
                for manifest in &summary.manifests {
                    let mut ev_ids = Vec::new();
                    if let Ok(ev) = index.create_evidence_ref(reader, manifest, 1, 10).await {
                        ev_ids.push(ev.id);
                        evidence_map.push(ev);
                    }
                    claims.push(Claim {
                        id: Uuid::new_v4(),
                        classification: ClaimClassification::Observed,
                        statement: format!("매니페스트: {}", manifest.display()),
                        confidence: 1.0,
                        evidence_ids: ev_ids,
                        rationale: Some("빌드 및 의존성 매니페스트".to_string()),
                    });
                }
            }
            "/risks" | "risks" => {
                direct_answer = "프로젝트의 위험 요소 및 미확정 결정 분석 결과입니다.".to_string();
                if summary.test_files.is_empty() {
                    claims.push(Claim {
                        id: Uuid::new_v4(),
                        classification: ClaimClassification::Conflict,
                        statement: "자동화 회귀 테스트 스위트 부재".to_string(),
                        confidence: 0.95,
                        evidence_ids: vec![],
                        rationale: Some("소스 파일 대비 테스트 파일 0건 감지".to_string()),
                    });
                    recommendations.push(Recommendation {
                        id: Uuid::new_v4(),
                        basis: RecommendationBasis::NeedsUserDecision,
                        impact: "품질 게이트 및 리팩터링 안정성 확보".to_string(),
                        rationale: "단위 테스트 또는 통합 테스트 픽스처 추가가 권장됩니다."
                            .to_string(),
                        evidence_ids: vec![],
                        decision_required: true,
                    });
                } else {
                    for test_file in summary.test_files.iter().take(3) {
                        let mut ev_ids = Vec::new();
                        if let Ok(ev) = index.create_evidence_ref(reader, test_file, 1, 10).await {
                            ev_ids.push(ev.id);
                            evidence_map.push(ev);
                        }
                        claims.push(Claim {
                            id: Uuid::new_v4(),
                            classification: ClaimClassification::Observed,
                            statement: format!("테스트 파일 검증됨: {}", test_file.display()),
                            confidence: 1.0,
                            evidence_ids: ev_ids,
                            rationale: Some("회귀 테스트 스위트".to_string()),
                        });
                    }
                }
            }
            _ => {
                direct_answer = format!(
                    "질문 '{}'에 대한 저장소 구조 및 키워드 분석 결과입니다.",
                    workflow
                );
                claims.push(Claim {
                    id: Uuid::new_v4(),
                    classification: ClaimClassification::Inferred,
                    statement: "저장소 메타데이터 및 구조 기반 분석 완료".to_string(),
                    confidence: 0.88,
                    evidence_ids: vec![],
                    rationale: None,
                });
            }
        }

        Ok(AnswerBundle {
            request_id: Uuid::new_v4(),
            snapshot_id,
            direct_answer,
            claims,
            evidence_map,
            recommendations,
            conflicts,
            raw_model_response: None,
        })
    }
}

/// [IMP-F004] Compares document-claimed stack/paths against the scanned tree.
async fn detect_doc_code_conflicts(
    reader: &(impl RepositoryReader + ?Sized),
    files: &[FileRecord],
    summary: &ProjectStructureSummary,
    index: &mut EvidenceIndex,
    claims: &mut Vec<Claim>,
    conflicts: &mut Vec<ConflictItem>,
    evidence_map: &mut Vec<mentat_core::models::EvidenceRef>,
) {
    let detected = summary.primary_language.as_deref().unwrap_or("Unknown");

    for doc in &summary.documents {
        let Ok(text) = reader.read_file_content(doc).await else {
            continue;
        };
        let mut ev_ids = Vec::new();
        if let Ok(ev) = index.create_evidence_ref(reader, doc, 1, 8).await {
            ev_ids.push(ev.id);
            evidence_map.push(ev);
        }

        if let Some(claimed) = language_claimed_in_text(&text) {
            if !claimed.eq_ignore_ascii_case(detected) {
                conflicts.push(ConflictItem {
                    id: Uuid::new_v4(),
                    side_a: format!("문서({})가 {}를 주장", doc.display(), claimed),
                    side_b: format!("스캔된 주 언어는 {}", detected),
                    evidence_ids: ev_ids.clone(),
                    impact: "온보딩과 도구 선택이 어긋날 수 있음".to_string(),
                    unresolved_question: "문서와 구현 중 어느 쪽이 현재 진실인가?".to_string(),
                });
                claims.push(Claim {
                    id: Uuid::new_v4(),
                    classification: ClaimClassification::Conflict,
                    statement: format!(
                        "문서가 {}를 주장하지만 저장소는 {}로 감지됨",
                        claimed, detected
                    ),
                    confidence: 0.9,
                    evidence_ids: ev_ids.clone(),
                    rationale: Some("문서 본문과 ProjectDetector 언어 분포 비교".to_string()),
                });
            }
        }

        for missing in referenced_missing_paths(&text, files) {
            conflicts.push(ConflictItem {
                id: Uuid::new_v4(),
                side_a: format!("문서가 {}를 참조", missing),
                side_b: "스캔된 파일 목록에 해당 경로 없음".to_string(),
                evidence_ids: ev_ids.clone(),
                impact: "문서의 작업 위치가 현재 트리와 불일치".to_string(),
                unresolved_question: format!("{}가 이동·삭제되었습니까?", missing),
            });
        }
    }

    if conflicts.is_empty() {
        claims.push(Claim {
            id: Uuid::new_v4(),
            classification: ClaimClassification::Observed,
            statement: format!(
                "{}건의 문서와 소스코드에서 언어/경로 충돌이 감지되지 않았습니다.",
                summary.documents.len()
            ),
            confidence: 0.7,
            evidence_ids: evidence_map.iter().map(|e| e.id).collect(),
            rationale: Some("문서 본문과 스캔 트리 대조".to_string()),
        });
    }
}

fn language_claimed_in_text(text: &str) -> Option<&'static str> {
    let lower = text.to_lowercase();
    if lower.contains("python") || lower.contains("django") || lower.contains("pypi") {
        Some("Python")
    } else if lower.contains("typescript") {
        Some("TypeScript")
    } else if lower.contains("javascript") || lower.contains("node.js") || lower.contains("npm ") {
        Some("JavaScript")
    } else if lower.contains("golang") || lower.contains(" go ") {
        Some("Go")
    } else if lower.contains("rust") || lower.contains("cargo workspace") || lower.contains("crate")
    {
        Some("Rust")
    } else {
        None
    }
}

fn referenced_missing_paths(text: &str, files: &[FileRecord]) -> Vec<String> {
    let mut missing = Vec::new();
    for token in text.split_whitespace() {
        let cleaned = token.trim_matches(|c: char| c == '`' || c == ',' || c == '.' || c == '"');
        let looks_like_path = cleaned.contains('/')
            || cleaned.contains('\\')
            || cleaned.ends_with(".py")
            || cleaned.ends_with(".rs")
            || cleaned.ends_with(".js")
            || cleaned.ends_with(".ts")
            || cleaned.ends_with(".go");
        if !looks_like_path {
            continue;
        }
        let exists = files.iter().any(|f| {
            f.relative_path.as_os_str() == std::ffi::OsStr::new(cleaned)
                || f.relative_path
                    .to_string_lossy()
                    .replace('\\', "/")
                    .ends_with(cleaned.replace('\\', "/").as_str())
        });
        if !exists && !missing.contains(&cleaned.to_string()) {
            missing.push(cleaned.to_string());
        }
    }
    missing
}

#[cfg(test)]
mod tests {
    use super::*;
    use mentat_core::ports::RepositoryReader;
    use mentat_repository::ReadOnlySession;
    use std::fs;

    #[tokio::test]
    async fn test_imp_f004_doc_code_language_conflict_fixture() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::write(
            root.join("README.md"),
            "# Demo\nThis is a Python 3.12 application. See `app.py` for the entry point.\n",
        )
        .unwrap();
        fs::write(root.join("Cargo.toml"), "[package]\nname = \"demo\"\n").unwrap();
        fs::create_dir(root.join("src")).unwrap();
        fs::write(root.join("src/main.rs"), "fn main() {}\n").unwrap();

        let session = ReadOnlySession::open(root).unwrap();
        let files = session.scan_files().await.unwrap();
        let summary = crate::ProjectDetector::summarize(&files);
        assert_eq!(summary.primary_language.as_deref(), Some("Rust"));

        let bundle = SemanticKernelBuilder::run_local_workflow(
            "/conflicts",
            &session,
            &files,
            &summary,
            uuid::Uuid::new_v4(),
        )
        .await
        .unwrap();

        assert!(
            !bundle.conflicts.is_empty(),
            "doc-code language mismatch must produce a conflict"
        );
        assert!(bundle.conflicts.iter().any(|c| c.side_a.contains("Python")));
        assert!(bundle
            .claims
            .iter()
            .any(|c| c.classification == ClaimClassification::Conflict));
        assert!(bundle.conflicts.iter().any(|c| c.side_a.contains("app.py")));
    }
}
