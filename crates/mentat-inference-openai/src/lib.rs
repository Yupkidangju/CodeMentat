pub mod gemini_adapter;
pub mod openai_adapter;

pub use gemini_adapter::GeminiAdapter;
pub use openai_adapter::OpenAiAdapter;

use async_trait::async_trait;
use futures_util::stream::BoxStream;
use mentat_core::error::MentatError;
use mentat_inference::{
    BackendProfile, HealthStatus, InferenceBackend, InferenceEvent, InferenceRequest, ProviderKind,
};
use tokio_util::sync::CancellationToken;

pub struct MultiProviderAdapter {
    gemini: GeminiAdapter,
    openai: OpenAiAdapter,
}

impl MultiProviderAdapter {
    pub fn new() -> Self {
        Self {
            gemini: GeminiAdapter::new(),
            openai: OpenAiAdapter::new(),
        }
    }
}

impl Default for MultiProviderAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl InferenceBackend for MultiProviderAdapter {
    async fn health_check(&self, profile: &BackendProfile) -> Result<HealthStatus, MentatError> {
        match profile.provider {
            ProviderKind::GoogleGemini => self.gemini.health_check(profile).await,
            ProviderKind::OpenRouter
            | ProviderKind::OpenAi
            | ProviderKind::OpenAICompatible
            | ProviderKind::CustomCompatible
            | ProviderKind::LocalMock => self.openai.health_check(profile).await,
        }
    }

    async fn infer_stream(
        &self,
        request: InferenceRequest,
        cancel_token: CancellationToken,
    ) -> Result<BoxStream<'static, InferenceEvent>, MentatError> {
        match request.profile.provider {
            ProviderKind::GoogleGemini => self.gemini.infer_stream(request, cancel_token).await,
            ProviderKind::OpenRouter
            | ProviderKind::OpenAi
            | ProviderKind::OpenAICompatible
            | ProviderKind::CustomCompatible
            | ProviderKind::LocalMock => self.openai.infer_stream(request, cancel_token).await,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[tokio::test]
    async fn test_multi_provider_adapter_default_initialization() {
        let adapter = MultiProviderAdapter::default();
        let gemini = GeminiAdapter::default();
        let openai = OpenAiAdapter::default();

        // Empty API keys should gracefully return unhealthy status without network panic
        let gemini_status = gemini.health_check(&BackendProfile::default()).await;
        assert!(gemini_status.is_ok());
        assert!(!gemini_status.unwrap().healthy);

        let openai_status = openai.health_check(&BackendProfile::default()).await;
        assert!(openai_status.is_ok());
        assert!(!openai_status.unwrap().healthy);

        let adapter_status = adapter.health_check(&BackendProfile::default()).await;
        assert!(adapter_status.is_ok());
        assert!(!adapter_status.unwrap().healthy);
    }

    #[tokio::test]
    async fn test_gemini_and_openai_missing_key_infer_fail_closed() {
        let gemini = GeminiAdapter::default();
        let openai = OpenAiAdapter::default();
        let cancel_token = CancellationToken::new();

        let req = InferenceRequest {
            request_id: Uuid::new_v4(),
            system_contract: "sys".to_string(),
            prompt_context: "ctx".to_string(),
            user_question: "q".to_string(),
            profile: BackendProfile::default(),
        };

        let gemini_res = gemini.infer_stream(req.clone(), cancel_token.clone()).await;
        assert!(gemini_res.is_err());
        if let Err(MentatError::BackendError { code, .. }) = gemini_res {
            assert_eq!(code, "MISSING_GEMINI_KEY");
        } else {
            panic!("Expected MISSING_GEMINI_KEY BackendError");
        }

        let openai_res = openai.infer_stream(req, cancel_token).await;
        assert!(openai_res.is_err());
        if let Err(MentatError::BackendError { code, .. }) = openai_res {
            assert_eq!(code, "MISSING_API_KEY");
        } else {
            panic!("Expected MISSING_API_KEY BackendError");
        }
    }

    #[tokio::test]
    async fn test_pre_response_cancellation_aborts_immediately() {
        let gemini = GeminiAdapter::default();
        let cancel_token = CancellationToken::new();
        // Cancel token before network call
        cancel_token.cancel();

        let profile = BackendProfile {
            api_key: Some("dummy_key".to_string()),
            ..Default::default()
        };

        let req = InferenceRequest {
            request_id: Uuid::new_v4(),
            system_contract: "sys".to_string(),
            prompt_context: "ctx".to_string(),
            user_question: "q".to_string(),
            profile,
        };

        let res = gemini.infer_stream(req, cancel_token).await;
        assert!(res.is_err());
        match res {
            Err(MentatError::Cancelled) => {}
            Err(e) => panic!("Expected MentatError::Cancelled, got {:?}", e),
            Ok(_) => panic!("Expected Err(MentatError::Cancelled), got Ok(_)"),
        }
    }

    #[tokio::test]
    async fn test_adapter_invalid_url_health_check_fail_closed() {
        let adapter = MultiProviderAdapter::default();
        let profile = BackendProfile {
            api_key: Some("test_key".to_string()),
            base_url: "http://localhost.evil.com/v1".to_string(),
            ..Default::default()
        };

        let status = adapter.health_check(&profile).await;
        assert!(status.is_err());
    }
}
