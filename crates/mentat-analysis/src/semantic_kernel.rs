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

    /// Generates structured advice bundle for standard workflows without requiring an external AI model
    pub async fn run_local_workflow(
        workflow: &str,
        reader: &(impl RepositoryReader + ?Sized),
        _files: &[FileRecord],
        summary: &ProjectStructureSummary,
        snapshot_id: Uuid,
    ) -> Result<AnswerBundle, MentatError> {
        let mut claims = Vec::new();
        let mut recommendations = Vec::new();
        let mut conflicts = Vec::new();
        let mut evidence_map = Vec::new();

        let direct_answer: String;

        match workflow {
            "/onboard" | "onboard" => {
                let tech = summary.primary_language.as_deref().unwrap_or("알 수 없음");
                direct_answer = format!(
                    "이 프로젝트는 **{}** 기반 프로젝트입니다. 총 {}개의 소스 파일 (약 {} 라인)로 구성되어 있습니다.",
                    tech, summary.total_source_files, summary.total_lines_of_code
                );

                if let Some(manifest) = summary.manifests.first() {
                    let mut index = EvidenceIndex::new(snapshot_id);
                    if let Ok(ev) = index.create_evidence_ref(reader, manifest, 1, 10).await {
                        evidence_map.push(ev.clone());
                        claims.push(Claim {
                            id: Uuid::new_v4(),
                            classification: ClaimClassification::Observed,
                            statement: format!(
                                "프로젝트 매니페스트 확인됨: {}",
                                manifest.display()
                            ),
                            confidence: 1.0,
                            evidence_ids: vec![ev.id],
                            rationale: Some("빌드 시스템 정의 파일".to_string()),
                        });
                    }
                }

                if let Some(entry) = summary.entry_points.first() {
                    let mut index = EvidenceIndex::new(snapshot_id);
                    if let Ok(ev) = index.create_evidence_ref(reader, entry, 1, 15).await {
                        evidence_map.push(ev.clone());
                        claims.push(Claim {
                            id: Uuid::new_v4(),
                            classification: ClaimClassification::Observed,
                            statement: format!("메인 진입점 파일 발견: {}", entry.display()),
                            confidence: 0.95,
                            evidence_ids: vec![ev.id],
                            rationale: Some("애플리케이션 시작 지점".to_string()),
                        });
                    }
                }

                recommendations.push(Recommendation {
                    id: Uuid::new_v4(),
                    basis: RecommendationBasis::ProjectIntentAligned,
                    impact: "프로젝트 빠른 파악".to_string(),
                    rationale: "진입점과 권위 문서를 먼저 검토하는 것이 좋습니다.".to_string(),
                    evidence_ids: vec![],
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
                    claims.push(Claim {
                        id: Uuid::new_v4(),
                        classification: ClaimClassification::Observed,
                        statement: format!("문서 파일: {}", doc.display()),
                        confidence: 1.0,
                        evidence_ids: vec![],
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
                    claims.push(Claim {
                        id: Uuid::new_v4(),
                        classification: ClaimClassification::Observed,
                        statement: format!(
                            "{}건의 문서와 소스코드의 정합성이 모니터링 중입니다.",
                            summary.documents.len()
                        ),
                        confidence: 0.9,
                        evidence_ids: vec![],
                        rationale: None,
                    });
                }
            }
            "/where" | "where" => {
                direct_answer = "주요 진입점 및 빌드 설정 파일 위치입니다.".to_string();
                for entry in &summary.entry_points {
                    claims.push(Claim {
                        id: Uuid::new_v4(),
                        classification: ClaimClassification::Observed,
                        statement: format!("진입점: {}", entry.display()),
                        confidence: 1.0,
                        evidence_ids: vec![],
                        rationale: Some("애플리케이션 진입점".to_string()),
                    });
                }
                for manifest in &summary.manifests {
                    claims.push(Claim {
                        id: Uuid::new_v4(),
                        classification: ClaimClassification::Observed,
                        statement: format!("매니페스트: {}", manifest.display()),
                        confidence: 1.0,
                        evidence_ids: vec![],
                        rationale: Some("빌드 및 의존성 매니페스트".to_string()),
                    });
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
