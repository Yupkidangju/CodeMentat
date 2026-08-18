pub mod announcer;
pub mod persona;

pub use announcer::{AnnouncementPolicy, AnnouncerEvent, AnnouncerLevel};
pub use persona::{PersonaDefinition, PersonaKind, PersonaRenderer};

#[cfg(test)]
mod tests {
    use super::*;
    use mentat_core::models::{
        AnswerBundle, Claim, ClaimClassification, ConflictItem, EvidenceRef, Recommendation,
        RecommendationBasis,
    };
    use std::path::PathBuf;
    use uuid::Uuid;

    #[test]
    fn test_persona_rendering_preserves_facts_and_evidence() {
        let snap_id = Uuid::new_v4();
        let claim_id = Uuid::new_v4();
        let ev_id = Uuid::new_v4();

        let original_bundle = AnswerBundle {
            request_id: Uuid::new_v4(),
            snapshot_id: snap_id,
            direct_answer: "이 파일은 프로젝트의 메인 진입점입니다.".to_string(),
            claims: vec![Claim {
                id: claim_id,
                classification: ClaimClassification::Observed,
                statement: "src/main.rs 에 fn main() 정의됨".to_string(),
                confidence: 1.0,
                evidence_ids: vec![ev_id],
                rationale: Some("코드 관찰".to_string()),
            }],
            evidence_map: vec![EvidenceRef {
                id: ev_id,
                snapshot_id: snap_id,
                relative_path: PathBuf::from("src/main.rs"),
                line_start: 1,
                line_end: 5,
                content_hash: "hash123".to_string(),
                excerpt: "fn main() {}".to_string(),
            }],
            recommendations: vec![Recommendation {
                id: Uuid::new_v4(),
                basis: RecommendationBasis::ProjectIntentAligned,
                impact: "High".to_string(),
                rationale: "Rationale".to_string(),
                evidence_ids: vec![],
                decision_required: false,
            }],
            conflicts: vec![ConflictItem {
                id: Uuid::new_v4(),
                side_a: "Doc A".to_string(),
                side_b: "Code B".to_string(),
                evidence_ids: vec![],
                impact: "Impact".to_string(),
                unresolved_question: "Question".to_string(),
            }],
            raw_model_response: None,
        };

        // Render with Default Analyst
        let rendered_default =
            PersonaRenderer::render(&original_bundle, PersonaKind::DefaultAnalyst);
        // Render with Mesugaki
        let rendered_mesugaki =
            PersonaRenderer::render(&original_bundle, PersonaKind::MesugakiAnnouncer);
        // Render with Concise Auditor
        let rendered_auditor =
            PersonaRenderer::render(&original_bundle, PersonaKind::ConciseAuditor);

        // Verification of Invariants across all personas:
        for bundle in [&rendered_default, &rendered_mesugaki, &rendered_auditor] {
            assert_eq!(bundle.claims.len(), original_bundle.claims.len());
            assert_eq!(bundle.claims[0].id, claim_id);
            assert_eq!(
                bundle.claims[0].classification,
                ClaimClassification::Observed
            );
            assert_eq!(
                bundle.claims[0].statement,
                "src/main.rs 에 fn main() 정의됨"
            );
            assert_eq!(bundle.claims[0].confidence, 1.0);
            assert_eq!(bundle.evidence_map[0].id, ev_id);
            assert_eq!(bundle.evidence_map[0].excerpt, "fn main() {}");
            assert_eq!(bundle.conflicts.len(), 1);
            assert_eq!(bundle.recommendations.len(), 1);
        }

        // Verify that only the direct answer stylistic wrapping changed
        assert!(rendered_mesugaki.direct_answer.contains("허접~"));
        assert!(rendered_auditor.direct_answer.contains("[감사 핵심 요약]"));
    }

    #[test]
    fn test_announcement_policy_levels() {
        assert!(!AnnouncementPolicy::should_interrupt_user(
            AnnouncerLevel::Trace
        ));
        assert!(!AnnouncementPolicy::should_interrupt_user(
            AnnouncerLevel::Notice
        ));
        assert!(!AnnouncementPolicy::should_interrupt_user(
            AnnouncerLevel::Warning
        ));
        assert!(AnnouncementPolicy::should_interrupt_user(
            AnnouncerLevel::CriticalConfirmation
        ));
    }
}
