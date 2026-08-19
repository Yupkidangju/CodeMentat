use mentat_core::error::MentatError;
pub use mentat_core::{RepositoryToolCall, RepositoryToolResult};
use mentat_core::{ResponseContract, SnapshotStatus};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProviderKind {
    GoogleGemini,
    OpenRouter,
    OpenAi,
    OpenAICompatible,
    CustomCompatible,
    LocalMock,
}

impl ProviderKind {
    pub fn default_base_url(&self) -> &'static str {
        match self {
            ProviderKind::GoogleGemini => "https://generativelanguage.googleapis.com",
            ProviderKind::OpenRouter => "https://openrouter.ai/api/v1",
            ProviderKind::OpenAi | ProviderKind::OpenAICompatible => "https://api.openai.com/v1",
            ProviderKind::CustomCompatible | ProviderKind::LocalMock => "http://localhost:8080/v1",
        }
    }

    pub fn requires_api_key(&self) -> bool {
        !matches!(self, ProviderKind::LocalMock)
    }
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackendProfile {
    pub id: Uuid,
    pub name: String,
    pub provider: ProviderKind,
    pub base_url: String,
    pub model: String,
    pub api_key: Option<String>,
    pub timeout_secs: u64,
}

impl std::fmt::Debug for BackendProfile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BackendProfile")
            .field("id", &self.id)
            .field("name", &self.name)
            .field("provider", &self.provider)
            .field("base_url", &self.base_url)
            .field("model", &self.model)
            .field(
                "api_key",
                &self.api_key.as_ref().map(|_| "[REDACTED_API_KEY]"),
            )
            .field("timeout_secs", &self.timeout_secs)
            .finish()
    }
}

impl BackendProfile {
    /// [SEC-F004] Validates endpoint URL for TLS encryption or exact local loopback (localhost/127.0.0.1/[::1]) only.
    /// Rejects userinfo (@), non-loopback HTTP subdomains (e.g. localhost.evil.com), and plain HTTP to remote hosts.
    pub fn validate_url(&self) -> Result<(), MentatError> {
        let parsed = url::Url::parse(&self.base_url).map_err(|e| {
            MentatError::EgressViolation(format!("유효하지 않은 엔드포인트 URL 형식입니다: {}", e))
        })?;

        // 1. Reject userinfo (e.g. http://localhost@evil.com)
        if !parsed.username().is_empty() || parsed.password().is_some() {
            return Err(MentatError::EgressViolation(
                "엔드포인트 URL에 사용자 인증 정보(userinfo)를 포함할 수 없습니다.".to_string(),
            ));
        }

        match parsed.scheme() {
            "https" => Ok(()),
            "http" => {
                if let Some(host) = parsed.host_str() {
                    let host_lower = host.to_lowercase();
                    if host_lower == "localhost"
                        || host_lower == "127.0.0.1"
                        || host_lower == "[::1]"
                        || host_lower == "::1"
                    {
                        Ok(())
                    } else {
                        Err(MentatError::EgressViolation(
                            format!("비보안 HTTP 엔드포인트는 오직 로컬 루프백(localhost/127.0.0.1/[::1])만 허용됩니다. 입력된 호스트: {}", host),
                        ))
                    }
                } else {
                    Err(MentatError::EgressViolation(
                        "엔드포인트 URL에 유효한 호스트명이 없습니다.".to_string(),
                    ))
                }
            }
            scheme => Err(MentatError::EgressViolation(format!(
                "지원되지 않는 프로토콜 스키마입니다: {}. HTTPS 또는 로컬 HTTP만 지원합니다.",
                scheme
            ))),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AvailableModel {
    pub id: String,
    pub display_name: String,
}

impl AvailableModel {
    pub fn new(id: impl Into<String>, display_name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            display_name: display_name.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelCatalog {
    pub models: Vec<AvailableModel>,
    pub latency_ms: Option<u64>,
}

impl ModelCatalog {
    pub fn from_untrusted(models: Vec<AvailableModel>) -> Self {
        let mut seen = HashSet::new();
        let mut normalized = Vec::new();

        for model in models {
            let id = model.id.trim().to_string();
            let valid_id = !id.is_empty()
                && id.len() <= 256
                && !id.starts_with('/')
                && !id.contains("..")
                && id.chars().all(|ch| {
                    ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | '/' | ':')
                });
            if !valid_id || !seen.insert(id.clone()) {
                continue;
            }
            let display_name = if model.display_name.trim().is_empty() {
                id.clone()
            } else {
                model.display_name.trim().to_string()
            };
            normalized.push(AvailableModel { id, display_name });
        }

        Self {
            models: normalized,
            latency_ms: None,
        }
    }

    pub fn with_latency(mut self, latency_ms: u64) -> Self {
        self.latency_ms = Some(latency_ms);
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelVerification {
    pub compatible: bool,
    pub message: String,
    pub latency_ms: Option<u64>,
}

impl Default for BackendProfile {
    fn default() -> Self {
        Self {
            id: Uuid::new_v4(),
            name: "Google Gemini Default".to_string(),
            provider: ProviderKind::GoogleGemini,
            base_url: ProviderKind::GoogleGemini.default_base_url().to_string(),
            model: String::new(),
            api_key: None,
            timeout_secs: 60,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthStatus {
    pub healthy: bool,
    pub message: String,
    pub latency_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceRequest {
    pub request_id: Uuid,
    pub system_contract: String,
    pub prompt_context: String,
    pub user_question: String,
    pub profile: BackendProfile,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum InferenceEvent {
    Started {
        request_id: Uuid,
    },
    TextDelta(String),
    ThinkingDelta(String),
    UsageUpdate {
        prompt_tokens: usize,
        completion_tokens: usize,
    },
    Completed {
        full_text: String,
    },
    Cancelled,
    Failed {
        error_code: String,
        message: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentCapabilities {
    pub chat_capable: bool,
    pub native_tool_capable: bool,
    pub emulated_tool_capable: bool,
    pub repository_advisor_capable: bool,
}

impl AgentCapabilities {
    pub const CHAT_ONLY: Self = Self {
        chat_capable: true,
        native_tool_capable: false,
        emulated_tool_capable: false,
        repository_advisor_capable: false,
    };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentLimits {
    pub max_rounds: u8,
    pub max_tool_calls: u16,
    pub max_tool_result_bytes: u32,
    pub timeout_secs: u64,
}

impl Default for AgentLimits {
    fn default() -> Self {
        Self {
            max_rounds: 8,
            max_tool_calls: 24,
            max_tool_result_bytes: 262_144,
            timeout_secs: 300,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentRole {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AgentMessageContent {
    Text(String),
    ToolCalls(Vec<RepositoryToolCall>),
    ToolResult(RepositoryToolResult),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentMessage {
    pub role: AgentRole,
    pub content: AgentMessageContent,
}

impl AgentMessage {
    pub fn user(text: impl Into<String>) -> Self {
        Self {
            role: AgentRole::User,
            content: AgentMessageContent::Text(text.into()),
        }
    }

    pub fn assistant(text: impl Into<String>) -> Self {
        Self {
            role: AgentRole::Assistant,
            content: AgentMessageContent::Text(text.into()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub schema_version: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepositoryContext {
    pub repository_id: Uuid,
    pub snapshot_id: Uuid,
    pub snapshot_status: SnapshotStatus,
    pub tools_available: bool,
    pub display_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRequest {
    pub request_id: Uuid,
    pub conversation_id: Uuid,
    pub turn_id: Uuid,
    pub profile: BackendProfile,
    pub effective_system_prompt: String,
    pub messages: Vec<AgentMessage>,
    pub tools: Vec<ToolDefinition>,
    pub repository_context: Option<RepositoryContext>,
    pub response_contract: ResponseContract,
    pub limits: AgentLimits,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum InferenceRoundEvent {
    Started {
        request_id: Uuid,
    },
    ThinkingDelta(String),
    TextDelta(String),
    ToolCallsRequested {
        round: u8,
        calls: Vec<RepositoryToolCall>,
    },
    UsageUpdate {
        prompt_tokens: usize,
        completion_tokens: usize,
    },
    RawCompleted {
        full_text: String,
    },
    Failed {
        error_code: String,
        safe_message: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CompletedPayload {
    AdvisorMarkdown(String),
    ValidatedAuditBundle(mentat_core::AnswerBundle),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CancelledPayload {
    AdvisorPartialMarkdown(String),
    AuditNoContent,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AgentEvent {
    Started {
        request_id: Uuid,
    },
    ThinkingDelta(String),
    TextDelta(String),
    ToolProgress {
        round: u8,
        completed_calls: u16,
        total_calls: u16,
    },
    UsageUpdate {
        prompt_tokens: usize,
        completion_tokens: usize,
    },
    Completed {
        payload: CompletedPayload,
        trace_id: Option<Uuid>,
    },
    Cancelled {
        payload: CancelledPayload,
    },
    Failed {
        error_code: String,
        safe_message: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptInspectionReceipt {
    pub receipt_id: Uuid,
    pub snapshot_id: Uuid,
    pub token_estimate: usize,
    pub redacted_count: usize,
    pub granted_at: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sec_f004_parsed_url_loopback_validation() {
        let make_profile = |url: &str| BackendProfile {
            id: Uuid::new_v4(),
            name: "Test".to_string(),
            provider: ProviderKind::OpenAICompatible,
            base_url: url.to_string(),
            model: "gpt-4o".to_string(),
            api_key: None,
            timeout_secs: 30,
        };

        // Allowed URLs
        assert!(make_profile("https://api.openai.com/v1")
            .validate_url()
            .is_ok());
        assert!(make_profile("https://generativelanguage.googleapis.com")
            .validate_url()
            .is_ok());
        assert!(make_profile("http://localhost:8080/v1")
            .validate_url()
            .is_ok());
        assert!(make_profile("http://127.0.0.1:11434")
            .validate_url()
            .is_ok());
        assert!(make_profile("http://[::1]:8080").validate_url().is_ok());

        // Prohibited / Bypassed URLs (Must FAIL)
        assert!(make_profile("http://localhost.evil.com/v1")
            .validate_url()
            .is_err());
        assert!(make_profile("http://localhost@evil.com/v1")
            .validate_url()
            .is_err());
        assert!(make_profile("http://127.0.0.1.evil.com/v1")
            .validate_url()
            .is_err());
        assert!(make_profile("http://remote-server.com/v1")
            .validate_url()
            .is_err());
        assert!(make_profile("ftp://api.openai.com").validate_url().is_err());
    }

    #[test]
    fn test_sec_f004_redacted_debug_formatting() {
        let profile = BackendProfile {
            id: Uuid::new_v4(),
            name: "Test".to_string(),
            provider: ProviderKind::GoogleGemini,
            base_url: "https://api.openai.com".to_string(),
            model: "gemini-2.5-flash".to_string(),
            api_key: Some("super_secret_raw_key_123".to_string()),
            timeout_secs: 30,
        };

        let debug_str = format!("{:?}", profile);
        assert!(!debug_str.contains("super_secret_raw_key_123"));
        assert!(debug_str.contains("[REDACTED_API_KEY]"));
    }

    #[test]
    fn production_profile_does_not_choose_a_hardcoded_model() {
        let profile = BackendProfile::default();
        assert!(profile.model.is_empty());
    }

    #[test]
    fn model_catalog_rejects_empty_ids_and_deduplicates_provider_data() {
        let catalog = ModelCatalog::from_untrusted(vec![
            AvailableModel::new("model-a", "Model A"),
            AvailableModel::new("", "Invalid"),
            AvailableModel::new("model-a", "Duplicate"),
            AvailableModel::new(" model-b ", ""),
            AvailableModel::new("../escape", "Invalid path"),
            AvailableModel::new("bad?query", "Invalid query"),
        ]);

        assert_eq!(catalog.models.len(), 2);
        assert_eq!(catalog.models[0].id, "model-a");
        assert_eq!(catalog.models[1].id, "model-b");
        assert_eq!(catalog.models[1].display_name, "model-b");
    }

    #[test]
    fn only_builtin_local_can_skip_api_key() {
        assert!(!ProviderKind::LocalMock.requires_api_key());
        assert!(ProviderKind::GoogleGemini.requires_api_key());
        assert!(ProviderKind::OpenAi.requires_api_key());
    }
}
