use crate::types::*;
use crate::InferenceBackend;
use async_trait::async_trait;
use futures_util::stream::BoxStream;
use mentat_core::error::MentatError;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

pub struct FakeInferenceBackend {
    pub simulated_chunks: Vec<String>,
    pub delay_per_chunk: Duration,
    pub should_fail: bool,
}

impl Default for FakeInferenceBackend {
    fn default() -> Self {
        Self {
            simulated_chunks: vec![
                "저장소의 ".to_string(),
                "구조와 문서를 ".to_string(),
                "분석한 결과, ".to_string(),
                "주요 진입점이 확인되었습니다.".to_string(),
            ],
            delay_per_chunk: Duration::from_millis(10),
            should_fail: false,
        }
    }
}

#[async_trait]
impl InferenceBackend for FakeInferenceBackend {
    async fn health_check(&self, _profile: &BackendProfile) -> Result<HealthStatus, MentatError> {
        if self.should_fail {
            Ok(HealthStatus {
                healthy: false,
                message: "가상 연결 실패".to_string(),
                latency_ms: None,
            })
        } else {
            Ok(HealthStatus {
                healthy: true,
                message: "정상 작동 중".to_string(),
                latency_ms: Some(15),
            })
        }
    }

    async fn discover_models(
        &self,
        _profile: &BackendProfile,
    ) -> Result<ModelCatalog, MentatError> {
        if self.should_fail {
            return Err(MentatError::BackendError {
                code: "SIMULATED_DISCOVERY_FAILURE".to_string(),
                message: "가상 모델 검색 실패".to_string(),
            });
        }
        Ok(ModelCatalog::from_untrusted(vec![AvailableModel::new(
            "fake-deterministic",
            "Fake Deterministic",
        )]))
    }

    async fn verify_model(
        &self,
        profile: &BackendProfile,
    ) -> Result<ModelVerification, MentatError> {
        Ok(ModelVerification {
            compatible: !self.should_fail && !profile.model.trim().is_empty(),
            message: if self.should_fail {
                "가상 모델 검증 실패".to_string()
            } else {
                "가상 모델 검증 성공".to_string()
            },
            latency_ms: Some(1),
        })
    }

    async fn infer_stream(
        &self,
        request: InferenceRequest,
        cancel_token: CancellationToken,
    ) -> Result<BoxStream<'static, InferenceEvent>, MentatError> {
        let chunks = self.simulated_chunks.clone();
        let delay = self.delay_per_chunk;
        let should_fail = self.should_fail;

        let stream = async_stream::stream! {
            yield InferenceEvent::Started { request_id: request.request_id };

            if should_fail {
                yield InferenceEvent::Failed {
                    error_code: "SIMULATED_FAILURE".to_string(),
                    message: "테스트 실패가 발생했습니다.".to_string(),
                };
                return;
            }

            let mut full_text = String::new();

            for chunk in chunks {
                tokio::select! {
                    _ = cancel_token.cancelled() => {
                        yield InferenceEvent::Cancelled;
                        return;
                    }
                    _ = tokio::time::sleep(delay) => {
                        full_text.push_str(&chunk);
                        yield InferenceEvent::TextDelta(chunk);
                    }
                }
            }

            yield InferenceEvent::UsageUpdate {
                prompt_tokens: 50,
                completion_tokens: 30,
            };

            yield InferenceEvent::Completed { full_text };
        };

        Ok(Box::pin(stream))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::StreamExt;
    use uuid::Uuid;

    #[tokio::test]
    async fn test_fake_inference_stream_completes() {
        let backend = FakeInferenceBackend::default();
        let cancel = CancellationToken::new();
        let request = InferenceRequest {
            request_id: Uuid::new_v4(),
            system_contract: "system".to_string(),
            prompt_context: "context".to_string(),
            user_question: "question".to_string(),
            profile: BackendProfile::default(),
        };

        let mut stream = backend.infer_stream(request, cancel).await.unwrap();
        let mut events = Vec::new();

        while let Some(event) = stream.next().await {
            events.push(event);
        }

        assert!(matches!(events[0], InferenceEvent::Started { .. }));
        assert!(matches!(
            events.last(),
            Some(InferenceEvent::Completed { .. })
        ));
    }

    #[tokio::test]
    async fn test_fake_inference_cancellation() {
        let backend = FakeInferenceBackend {
            delay_per_chunk: Duration::from_millis(50),
            ..Default::default()
        };
        let cancel = CancellationToken::new();
        let cancel_clone = cancel.clone();

        let request = InferenceRequest {
            request_id: Uuid::new_v4(),
            system_contract: "system".to_string(),
            prompt_context: "context".to_string(),
            user_question: "question".to_string(),
            profile: BackendProfile::default(),
        };

        let mut stream = backend.infer_stream(request, cancel).await.unwrap();

        // Cancel after receiving Started
        let first = stream.next().await.unwrap();
        assert!(matches!(first, InferenceEvent::Started { .. }));

        cancel_clone.cancel();

        let next_event = stream.next().await.unwrap();
        assert_eq!(next_event, InferenceEvent::Cancelled);
    }
}
