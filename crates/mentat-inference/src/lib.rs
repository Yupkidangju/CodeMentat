pub mod fake;
pub mod types;

pub use fake::FakeInferenceBackend;
pub use types::*;

use async_trait::async_trait;
use futures_util::stream::BoxStream;
use mentat_core::error::MentatError;
use tokio_util::sync::CancellationToken;

#[async_trait]
pub trait InferenceBackend: Send + Sync {
    async fn health_check(&self, profile: &BackendProfile) -> Result<HealthStatus, MentatError>;

    async fn infer_stream(
        &self,
        request: InferenceRequest,
        cancel_token: CancellationToken,
    ) -> Result<BoxStream<'static, InferenceEvent>, MentatError>;

    fn estimate_tokens(&self, text: &str) -> usize {
        text.len().div_ceil(4)
    }
}
