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
    openai: OpenAiAdapter,
    gemini: GeminiAdapter,
}

impl MultiProviderAdapter {
    pub fn new() -> Self {
        Self {
            openai: OpenAiAdapter::new(),
            gemini: GeminiAdapter::new(),
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
}
