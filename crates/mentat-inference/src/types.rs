use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProviderKind {
    GoogleGemini,
    OpenRouter,
    OpenAi,
    CustomCompatible,
}

impl ProviderKind {
    pub fn default_base_url(&self) -> &'static str {
        match self {
            ProviderKind::GoogleGemini => "https://generativelanguage.googleapis.com",
            ProviderKind::OpenRouter => "https://openrouter.ai/api/v1",
            ProviderKind::OpenAi => "https://api.openai.com/v1",
            ProviderKind::CustomCompatible => "http://localhost:8000/v1",
        }
    }

    pub fn default_models(&self) -> &'static [&'static str] {
        match self {
            ProviderKind::GoogleGemini => {
                &["gemini-2.5-flash", "gemini-2.5-pro", "gemini-1.5-flash"]
            }
            ProviderKind::OpenRouter => &[
                "anthropic/claude-3.7-sonnet",
                "deepseek/deepseek-r1",
                "meta-llama/llama-3.3-70b-instruct",
                "google/gemini-2.5-flash",
            ],
            ProviderKind::OpenAi => &["gpt-4o", "gpt-4o-mini", "o3-mini"],
            ProviderKind::CustomCompatible => &["default-model"],
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
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
    /// Validates endpoint URL for TLS encryption or local loopback only (SEC-F004)
    pub fn validate_url(&self) -> Result<(), mentat_core::error::MentatError> {
        let url_lower = self.base_url.to_lowercase();
        if url_lower.starts_with("https://")
            || url_lower.starts_with("http://localhost")
            || url_lower.starts_with("http://127.0.0.1")
        {
            Ok(())
        } else {
            Err(mentat_core::error::MentatError::EgressViolation(
                "외부 AI 엔드포인트는 HTTPS 또는 로컬 루프백(localhost/127.0.0.1)이어야 합니다."
                    .to_string(),
            ))
        }
    }
}

impl Default for BackendProfile {
    fn default() -> Self {
        Self {
            id: Uuid::new_v4(),
            name: "Google Gemini Default".to_string(),
            provider: ProviderKind::GoogleGemini,
            base_url: ProviderKind::GoogleGemini.default_base_url().to_string(),
            model: "gemini-2.5-flash".to_string(),
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum InferenceEvent {
    Started {
        request_id: Uuid,
    },
    TextDelta(String),
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
