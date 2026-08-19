use mentat_core::models::AnswerBundle;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PersonaKind {
    DefaultAnalyst,
    MesugakiAnnouncer,
    ConciseAuditor,
}

impl PersonaKind {
    pub const ALL: [Self; 3] = [
        Self::DefaultAnalyst,
        Self::MesugakiAnnouncer,
        Self::ConciseAuditor,
    ];

    pub fn resource_key(self) -> &'static str {
        match self {
            Self::DefaultAnalyst => "persona.default_analyst.v1",
            Self::MesugakiAnnouncer => "persona.mesugaki.v1",
            Self::ConciseAuditor => "persona.concise_auditor.v1",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            PersonaKind::DefaultAnalyst => "기본 분석가 (Default Analyst)",
            PersonaKind::MesugakiAnnouncer => "메스카키 아나운서 (Mesugaki)",
            PersonaKind::ConciseAuditor => "간결한 감사자 (Concise Auditor)",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersonaDefinition {
    pub id: Uuid,
    pub kind: PersonaKind,
    pub display_name: String,
    pub description: String,
}

impl Default for PersonaDefinition {
    fn default() -> Self {
        Self {
            id: Uuid::new_v4(),
            kind: PersonaKind::DefaultAnalyst,
            display_name: PersonaKind::DefaultAnalyst.display_name().to_string(),
            description: "객관적이고 신뢰할 수 있는 단정한 분석가 페르소나".to_string(),
        }
    }
}

pub struct PersonaRenderer;

impl PersonaRenderer {
    pub fn render(bundle: &AnswerBundle, persona: PersonaKind) -> AnswerBundle {
        let mut rendered_bundle = bundle.clone();

        // Persona ONLY adjusts the stylistic presentation of the direct answer.
        // It NEVER mutates claims, evidence_map, recommendations, or conflicts (Strict Invariant).
        match persona {
            PersonaKind::DefaultAnalyst => {
                // Keep clean direct answer
                rendered_bundle.direct_answer = bundle.direct_answer.clone();
            }
            PersonaKind::MesugakiAnnouncer => {
                let intro = "허접~ 이런 것도 혼자 못 봐서 멘타트한테 물어보는 거야? 자, 팩트나 똑바로 확인해봐!\n\n";
                let outro = "\n\n흥, 다음엔 스스로 문서랑 코드 비교해보고 오라고~?";
                rendered_bundle.direct_answer =
                    format!("{}{}{}", intro, bundle.direct_answer, outro);
            }
            PersonaKind::ConciseAuditor => {
                let intro = "[감사 핵심 요약]\n";
                rendered_bundle.direct_answer = format!("{}{}", intro, bundle.direct_answer);
            }
        }

        rendered_bundle
    }
}
