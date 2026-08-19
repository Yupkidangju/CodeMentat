pub mod fake;
pub mod types;

pub use fake::FakeInferenceBackend;
pub use types::*;

use async_trait::async_trait;
use futures_util::stream::BoxStream;
use futures_util::StreamExt;
use mentat_core::error::MentatError;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

pub trait ProviderBodyEgressGate: Send + Sync {
    fn authorize_exact_body(
        &self,
        request: &AgentRequest,
        exact_provider_body: &[u8],
    ) -> Result<Vec<uuid::Uuid>, MentatError>;

    fn finish(
        &self,
        receipt_ids: &[uuid::Uuid],
        status: mentat_core::ToolEgressStatus,
    ) -> Result<(), MentatError>;

    fn receipt_ids(&self) -> Result<Vec<uuid::Uuid>, MentatError>;
}

#[async_trait]
pub trait InferenceBackend: Send + Sync {
    async fn health_check(&self, profile: &BackendProfile) -> Result<HealthStatus, MentatError>;

    async fn discover_models(&self, profile: &BackendProfile) -> Result<ModelCatalog, MentatError>;

    async fn verify_model(
        &self,
        profile: &BackendProfile,
    ) -> Result<ModelVerification, MentatError>;

    async fn infer_stream(
        &self,
        request: InferenceRequest,
        cancel_token: CancellationToken,
    ) -> Result<BoxStream<'static, InferenceEvent>, MentatError>;

    async fn verify_capabilities(
        &self,
        profile: &BackendProfile,
    ) -> Result<AgentCapabilities, MentatError> {
        let verification = self.verify_model(profile).await?;
        Ok(AgentCapabilities {
            chat_capable: verification.compatible,
            native_tool_capable: false,
            emulated_tool_capable: false,
            repository_advisor_capable: false,
        })
    }

    async fn infer_round_stream(
        &self,
        request: AgentRequest,
        cancel_token: CancellationToken,
    ) -> Result<BoxStream<'static, InferenceRoundEvent>, MentatError> {
        if !request.tools.is_empty() || request.repository_context.is_some() {
            return Err(MentatError::BackendError {
                code: "AGENT_TOOLS_UNSUPPORTED".to_string(),
                message: "이 backend 경로는 아직 chat-only round만 지원합니다.".to_string(),
            });
        }
        let last_user_index = request
            .messages
            .iter()
            .rposition(|message| message.role == AgentRole::User)
            .ok_or_else(|| MentatError::BackendError {
                code: "AGENT_USER_MESSAGE_REQUIRED".to_string(),
                message: "AgentRequest에 user message가 없습니다.".to_string(),
            })?;
        let user_question = match &request.messages[last_user_index].content {
            AgentMessageContent::Text(text) => text.clone(),
            _ => {
                return Err(MentatError::BackendError {
                    code: "AGENT_USER_MESSAGE_INVALID".to_string(),
                    message: "마지막 user message는 text여야 합니다.".to_string(),
                })
            }
        };
        let history =
            serde_json::to_string(&request.messages[..last_user_index]).map_err(|error| {
                MentatError::BackendError {
                    code: "AGENT_HISTORY_ENCODE_FAILED".to_string(),
                    message: error.to_string(),
                }
            })?;
        let legacy = InferenceRequest {
            request_id: request.request_id,
            system_contract: request.effective_system_prompt,
            prompt_context: history,
            user_question,
            profile: request.profile,
        };
        let stream = self.infer_stream(legacy, cancel_token).await?;
        Ok(Box::pin(stream.map(|event| match event {
            InferenceEvent::Started { request_id } => InferenceRoundEvent::Started { request_id },
            InferenceEvent::TextDelta(delta) => InferenceRoundEvent::TextDelta(delta),
            InferenceEvent::ThinkingDelta(delta) => InferenceRoundEvent::ThinkingDelta(delta),
            InferenceEvent::UsageUpdate {
                prompt_tokens,
                completion_tokens,
            } => InferenceRoundEvent::UsageUpdate {
                prompt_tokens,
                completion_tokens,
            },
            InferenceEvent::Completed { full_text } => {
                InferenceRoundEvent::RawCompleted { full_text }
            }
            InferenceEvent::Cancelled => InferenceRoundEvent::Failed {
                error_code: "CANCELLED".to_string(),
                safe_message: "요청이 취소되었습니다.".to_string(),
            },
            InferenceEvent::Failed {
                error_code,
                message,
            } => InferenceRoundEvent::Failed {
                error_code,
                safe_message: message,
            },
        })))
    }

    async fn infer_round_stream_guarded(
        &self,
        request: AgentRequest,
        cancel_token: CancellationToken,
        egress_gate: Option<Arc<dyn ProviderBodyEgressGate>>,
    ) -> Result<BoxStream<'static, InferenceRoundEvent>, MentatError> {
        let contains_tool_result = request
            .messages
            .iter()
            .any(|message| matches!(message.content, AgentMessageContent::ToolResult(_)));
        if contains_tool_result && request.profile.provider.requires_api_key() {
            return Err(MentatError::BackendError {
                code: if egress_gate.is_some() {
                    "AGENT_TOOLS_UNSUPPORTED".to_string()
                } else {
                    "TOOL_EGRESS_GATE_REQUIRED".to_string()
                },
                message: "외부 provider tool result는 exact-body egress gate를 구현한 adapter만 전송할 수 있습니다.".to_string(),
            });
        }
        self.infer_round_stream(request, cancel_token).await
    }

    fn estimate_tokens(&self, text: &str) -> usize {
        text.len().div_ceil(4)
    }
}
