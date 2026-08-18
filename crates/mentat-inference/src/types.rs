use mentat_core::error::MentatError;
use serde::{Deserialize, Serialize};
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

    pub fn default_models(&self) -> &'static [&'static str] {
        match self {
            ProviderKind::GoogleGemini => {
                &["gemini-2.5-flash", "gemini-2.5-pro", "gemini-1.5-flash"]
            }
            ProviderKind::OpenRouter => &[
                "anthropic/claude-3.7-sonnet",
                "deepseek/deepseek-r1",
                "meta-llama/llama-3.3-70b-instruct",
            ],
            ProviderKind::OpenAi | ProviderKind::OpenAICompatible => {
                &["gpt-4o", "gpt-4o-mini", "o3-mini"]
            }
            ProviderKind::CustomCompatible | ProviderKind::LocalMock => {
                &["local-model", "mock-model"]
            }
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
}
