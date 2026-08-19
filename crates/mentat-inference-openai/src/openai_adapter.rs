use crate::agent_wire::{openai_body, parse_repository_tool_call, request_has_tool_results};
use futures_util::stream::BoxStream;
use futures_util::StreamExt;
use mentat_core::{MentatError, ToolEgressStatus};
use mentat_inference::{
    AgentRequest, AvailableModel, BackendProfile, HealthStatus, InferenceEvent, InferenceRequest,
    InferenceRoundEvent, ModelCatalog, ModelVerification, ProviderBodyEgressGate, ProviderKind,
};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
use serde_json::json;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;
use tokio_util::sync::CancellationToken;

pub struct OpenAiAdapter {
    client: Option<reqwest::Client>,
}

#[derive(Default)]
struct PendingOpenAiToolCall {
    id: String,
    name: String,
    arguments: String,
}

impl OpenAiAdapter {
    async fn parse_bounded_json(
        response: reqwest::Response,
        limit: usize,
        too_large_code: &str,
        read_error_code: &str,
        invalid_json_code: &str,
    ) -> Result<serde_json::Value, MentatError> {
        if response
            .content_length()
            .is_some_and(|length| length > limit as u64)
        {
            return Err(MentatError::BackendError {
                code: too_large_code.to_string(),
                message: format!("응답이 {limit}바이트 제한을 초과했습니다."),
            });
        }
        let mut body = Vec::new();
        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| MentatError::BackendError {
                code: read_error_code.to_string(),
                message: e.to_string(),
            })?;
            if body.len().saturating_add(chunk.len()) > limit {
                return Err(MentatError::BackendError {
                    code: too_large_code.to_string(),
                    message: format!("응답이 {limit}바이트 제한을 초과했습니다."),
                });
            }
            body.extend_from_slice(&chunk);
        }
        serde_json::from_slice(&body).map_err(|e| MentatError::BackendError {
            code: invalid_json_code.to_string(),
            message: e.to_string(),
        })
    }

    pub fn new() -> Self {
        Self {
            client: reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .ok(),
        }
    }

    fn client(&self) -> Result<&reqwest::Client, MentatError> {
        self.client
            .as_ref()
            .ok_or_else(|| MentatError::BackendError {
                code: "OPENAI_CLIENT_INIT_FAILED".to_string(),
                message: "OpenAI 호환 보안 HTTP client를 초기화하지 못했습니다.".to_string(),
            })
    }
}

impl Default for OpenAiAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl OpenAiAdapter {
    fn authorization_headers(profile: &BackendProfile) -> Result<HeaderMap, MentatError> {
        if profile.provider == ProviderKind::LocalMock {
            return Err(MentatError::BackendError {
                code: "LOCAL_RUNTIME_UNAVAILABLE".to_string(),
                message: "내장 로컬 추론 런타임과 설치 모델을 찾을 수 없습니다.".to_string(),
            });
        }

        let api_key = profile.api_key.as_deref().unwrap_or("");
        if profile.provider.requires_api_key() && api_key.is_empty() {
            return Err(MentatError::BackendError {
                code: "MISSING_API_KEY".to_string(),
                message: "선택한 공급자의 API 키가 설정되지 않았습니다.".to_string(),
            });
        }

        let mut headers = HeaderMap::new();
        if !api_key.is_empty() {
            headers.insert(
                AUTHORIZATION,
                HeaderValue::from_str(&format!("Bearer {}", api_key)).map_err(|e| {
                    MentatError::BackendError {
                        code: "INVALID_HEADER".to_string(),
                        message: e.to_string(),
                    }
                })?,
            );
        }
        Ok(headers)
    }

    fn models_url(base_url: &str) -> String {
        let base = base_url.trim_end_matches('/');
        if let Some(prefix) = base.strip_suffix("/chat/completions") {
            format!("{prefix}/models")
        } else {
            format!("{base}/models")
        }
    }

    fn chat_completions_url(base_url: &str) -> String {
        let base = base_url.trim_end_matches('/');
        if base.ends_with("/chat/completions") {
            base.to_string()
        } else {
            format!("{base}/chat/completions")
        }
    }

    pub fn agent_endpoint(profile: &BackendProfile) -> String {
        Self::chat_completions_url(&profile.base_url)
    }

    pub async fn discover_models(
        &self,
        profile: &BackendProfile,
    ) -> Result<ModelCatalog, MentatError> {
        profile.validate_url()?;
        let start = Instant::now();
        let response = self
            .client()?
            .get(Self::models_url(&profile.base_url))
            .headers(Self::authorization_headers(profile)?)
            .timeout(std::time::Duration::from_secs(
                profile.timeout_secs.clamp(5, 300),
            ))
            .send()
            .await
            .map_err(|e| MentatError::BackendError {
                code: "MODEL_DISCOVERY_NETWORK_ERROR".to_string(),
                message: e.to_string(),
            })?;

        if !response.status().is_success() {
            return Err(MentatError::BackendError {
                code: format!("MODEL_DISCOVERY_HTTP_{}", response.status().as_u16()),
                message: "모델 목록 요청이 거부되었습니다.".to_string(),
            });
        }
        let value = Self::parse_bounded_json(
            response,
            4 * 1024 * 1024,
            "MODEL_DISCOVERY_RESPONSE_TOO_LARGE",
            "MODEL_DISCOVERY_READ_ERROR",
            "MODEL_DISCOVERY_INVALID_JSON",
        )
        .await?;
        let models = value
            .get("data")
            .and_then(|data| data.as_array())
            .ok_or_else(|| MentatError::BackendError {
                code: "MODEL_DISCOVERY_INVALID_SCHEMA".to_string(),
                message: "모델 목록 응답에 data 배열이 없습니다.".to_string(),
            })?
            .iter()
            .filter_map(|item| item.get("id").and_then(|id| id.as_str()))
            .map(|id| AvailableModel::new(id, id))
            .collect();

        let catalog =
            ModelCatalog::from_untrusted(models).with_latency(start.elapsed().as_millis() as u64);
        if catalog.models.is_empty() {
            return Err(MentatError::BackendError {
                code: "MODEL_DISCOVERY_EMPTY".to_string(),
                message: "현재 자격 증명으로 사용할 수 있는 모델이 없습니다.".to_string(),
            });
        }
        Ok(catalog)
    }

    pub async fn verify_model(
        &self,
        profile: &BackendProfile,
    ) -> Result<ModelVerification, MentatError> {
        profile.validate_url()?;
        if profile.model.trim().is_empty() {
            return Err(MentatError::BackendError {
                code: "MODEL_NOT_SELECTED".to_string(),
                message: "검증할 모델을 선택해야 합니다.".to_string(),
            });
        }
        let start = Instant::now();
        let response = self
            .client()?
            .post(Self::chat_completions_url(&profile.base_url))
            .headers(Self::authorization_headers(profile)?)
            .timeout(std::time::Duration::from_secs(
                profile.timeout_secs.clamp(5, 300),
            ))
            .json(&json!({
                "model": profile.model,
                "messages": [{"role": "user", "content": "Reply with OK."}],
                "max_tokens": 1,
                "stream": false
            }))
            .send()
            .await
            .map_err(|e| MentatError::BackendError {
                code: "MODEL_VERIFY_NETWORK_ERROR".to_string(),
                message: e.to_string(),
            })?;
        if !response.status().is_success() {
            return Err(MentatError::BackendError {
                code: format!("MODEL_VERIFY_HTTP_{}", response.status().as_u16()),
                message: "선택 모델의 생성 요청이 거부되었습니다.".to_string(),
            });
        }
        let value = Self::parse_bounded_json(
            response,
            1024 * 1024,
            "MODEL_VERIFY_RESPONSE_TOO_LARGE",
            "MODEL_VERIFY_READ_ERROR",
            "MODEL_VERIFY_INVALID_JSON",
        )
        .await?;
        let compatible = value
            .pointer("/choices/0/message/content")
            .and_then(|content| content.as_str())
            .is_some_and(|content| !content.trim().is_empty());
        Ok(ModelVerification {
            compatible,
            message: if compatible {
                "선택 모델이 생성 요청에 정상 응답했습니다.".to_string()
            } else {
                "응답에 텍스트 생성 결과가 없습니다.".to_string()
            },
            latency_ms: Some(start.elapsed().as_millis() as u64),
        })
    }

    pub async fn health_check(
        &self,
        profile: &BackendProfile,
    ) -> Result<HealthStatus, MentatError> {
        let start = Instant::now();
        let api_key = profile.api_key.as_deref().unwrap_or("");
        if api_key.is_empty() {
            return Ok(HealthStatus {
                healthy: false,
                message: "API 키가 설정되지 않았습니다.".to_string(),
                latency_ms: None,
            });
        }

        profile.validate_url()?;

        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", api_key)).map_err(|e| {
                MentatError::BackendError {
                    code: "INVALID_HEADER".to_string(),
                    message: e.to_string(),
                }
            })?,
        );

        let url = if profile.base_url.ends_with("/chat/completions") {
            profile.base_url.clone()
        } else {
            format!("{}/models", profile.base_url.trim_end_matches('/'))
        };

        let timeout = std::time::Duration::from_secs(profile.timeout_secs.clamp(5, 300));
        let res = self
            .client()?
            .get(&url)
            .headers(headers)
            .timeout(timeout)
            .send()
            .await;
        let latency = start.elapsed().as_millis() as u64;

        match res {
            Ok(resp) => {
                if resp.status().is_success() {
                    Ok(HealthStatus {
                        healthy: true,
                        message: "연결 성공".to_string(),
                        latency_ms: Some(latency),
                    })
                } else {
                    Ok(HealthStatus {
                        healthy: false,
                        message: format!("HTTP {}", resp.status()),
                        latency_ms: Some(latency),
                    })
                }
            }
            Err(e) => Ok(HealthStatus {
                healthy: false,
                message: format!("네트워크 오류: {}", e),
                latency_ms: None,
            }),
        }
    }

    pub async fn infer_stream(
        &self,
        request: InferenceRequest,
        cancel_token: CancellationToken,
    ) -> Result<BoxStream<'static, InferenceEvent>, MentatError> {
        let profile = &request.profile;
        let api_key = profile.api_key.as_deref().unwrap_or("");

        if api_key.is_empty() {
            return Err(MentatError::BackendError {
                code: "MISSING_API_KEY".to_string(),
                message: "OpenAI/OpenRouter API 키가 설정되지 않았습니다.".to_string(),
            });
        }

        if profile.model.trim().is_empty() {
            return Err(MentatError::BackendError {
                code: "MODEL_NOT_SELECTED".to_string(),
                message: "검증되어 활성화된 모델이 없습니다.".to_string(),
            });
        }

        profile.validate_url()?;

        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", api_key)).map_err(|e| {
                MentatError::BackendError {
                    code: "INVALID_HEADER".to_string(),
                    message: e.to_string(),
                }
            })?,
        );

        if profile.provider == ProviderKind::OpenRouter {
            headers.insert(
                HeaderName::from_static("http-referer"),
                HeaderValue::from_static("https://github.com/Yupkidangju/CodeMentat"),
            );
            headers.insert(
                HeaderName::from_static("x-title"),
                HeaderValue::from_static("Code Mentat"),
            );
        }

        let endpoint = if profile.base_url.ends_with("/chat/completions") {
            profile.base_url.clone()
        } else {
            format!(
                "{}/chat/completions",
                profile.base_url.trim_end_matches('/')
            )
        };

        let body = json!({
            "model": profile.model,
            "stream": true,
            "messages": [
                {
                    "role": "system",
                    "content": request.system_contract
                },
                {
                    "role": "user",
                    "content": if request.user_question.is_empty() {
                        request.prompt_context.clone()
                    } else {
                        format!(
                            "{}\n\n## User Question\n{}",
                            request.prompt_context, request.user_question
                        )
                    }
                }
            ]
        });

        let timeout = std::time::Duration::from_secs(profile.timeout_secs.clamp(5, 300));
        let send_future = self
            .client()?
            .post(&endpoint)
            .headers(headers)
            .timeout(timeout)
            .json(&body)
            .send();

        // SEC-F005: Pre-response cancellation
        let response = tokio::select! {
            _ = cancel_token.cancelled() => {
                return Err(MentatError::Cancelled);
            }
            res = send_future => {
                res.map_err(|e| MentatError::BackendError {
                    code: "HTTP_SEND_ERROR".to_string(),
                    message: e.to_string(),
                })?
            }
        };

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(MentatError::BackendError {
                code: format!("HTTP_{}", status.as_u16()),
                message: body,
            });
        }

        let mut byte_stream = response.bytes_stream();
        let req_id = request.request_id;

        let stream = async_stream::stream! {
            yield InferenceEvent::Started { request_id: req_id };
            let mut full_text = String::new();
            let mut byte_buffer = Vec::new();

            loop {
                tokio::select! {
                    _ = cancel_token.cancelled() => {
                        yield InferenceEvent::Cancelled;
                        return;
                    }
                    chunk_opt = byte_stream.next() => {
                        match chunk_opt {
                            Some(Ok(bytes)) => {
                                byte_buffer.extend_from_slice(&bytes);
                                while let Some(pos) = byte_buffer.iter().position(|&b| b == b'\n') {
                                    let line_bytes = &byte_buffer[..pos];
                                    let line = String::from_utf8_lossy(line_bytes).trim().to_string();
                                    byte_buffer.drain(..pos + 1);

                                    if line.is_empty() || line.starts_with(':') {
                                        continue;
                                    }

                                    if let Some(data) = line.strip_prefix("data: ") {
                                        if data.trim() == "[DONE]" {
                                            yield InferenceEvent::Completed { full_text: full_text.clone() };
                                            return;
                                        }

                                        if let Ok(json) = serde_json::from_str::<serde_json::Value>(data) {
                                            if let Some(delta) = json
                                                .get("choices")
                                                .and_then(|c| c.get(0))
                                                .and_then(|c| c.get("delta"))
                                                .and_then(|d| d.get("content"))
                                                .and_then(|c| c.as_str())
                                            {
                                                full_text.push_str(delta);
                                                yield InferenceEvent::TextDelta(delta.to_string());
                                            }
                                        }
                                    }
                                }
                            }
                            Some(Err(e)) => {
                                yield InferenceEvent::Failed {
                                    error_code: "STREAM_READ_ERROR".to_string(),
                                    message: e.to_string(),
                                };
                                return;
                            }
                            None => {
                                yield InferenceEvent::Completed { full_text };
                                return;
                            }
                        }
                    }
                }
            }
        };

        Ok(Box::pin(stream))
    }

    pub async fn infer_agent_round(
        &self,
        request: AgentRequest,
        cancel_token: CancellationToken,
        egress_gate: Option<Arc<dyn ProviderBodyEgressGate>>,
    ) -> Result<BoxStream<'static, InferenceRoundEvent>, MentatError> {
        let profile = &request.profile;
        if profile.api_key.as_deref().unwrap_or("").is_empty() {
            return Err(MentatError::BackendError {
                code: "MISSING_API_KEY".to_string(),
                message: "OpenAI/OpenRouter API 키가 설정되지 않았습니다.".to_string(),
            });
        }
        if profile.model.trim().is_empty() {
            return Err(MentatError::BackendError {
                code: "MODEL_NOT_SELECTED".to_string(),
                message: "검증되어 활성화된 모델이 없습니다.".to_string(),
            });
        }
        profile.validate_url()?;
        let endpoint = Self::agent_endpoint(profile);
        let exact_body = serde_json::to_vec(&openai_body(&request)?).map_err(|error| {
            MentatError::BackendError {
                code: "AGENT_BODY_ENCODE_FAILED".to_string(),
                message: error.to_string(),
            }
        })?;
        let receipt_ids = if request_has_tool_results(&request) {
            let gate = egress_gate
                .as_ref()
                .ok_or_else(|| MentatError::BackendError {
                    code: "TOOL_EGRESS_GATE_REQUIRED".to_string(),
                    message: "외부 provider tool result 전송 승인이 없습니다.".to_string(),
                })?;
            gate.authorize_exact_body(&request, &endpoint, &exact_body)?
        } else {
            Vec::new()
        };
        let mut headers = Self::authorization_headers(profile)?;
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        if profile.provider == ProviderKind::OpenRouter {
            headers.insert(
                HeaderName::from_static("http-referer"),
                HeaderValue::from_static("https://github.com/Yupkidangju/CodeMentat"),
            );
            headers.insert(
                HeaderName::from_static("x-title"),
                HeaderValue::from_static("Code Mentat"),
            );
        }
        let send_future = self
            .client()?
            .post(&endpoint)
            .headers(headers)
            .timeout(std::time::Duration::from_secs(
                profile.timeout_secs.clamp(5, 300),
            ))
            .body(exact_body)
            .send();
        let response = tokio::select! {
            _ = cancel_token.cancelled() => {
                if let Some(gate) = &egress_gate {
                    gate.finish(&receipt_ids, ToolEgressStatus::OutcomeUnknown)?;
                }
                return Err(MentatError::Cancelled);
            }
            result = send_future => match result {
                Ok(response) => response,
                Err(error) => {
                    if let Some(gate) = &egress_gate {
                        gate.finish(&receipt_ids, ToolEgressStatus::OutcomeUnknown)?;
                    }
                    return Err(MentatError::BackendError {
                        code: "HTTP_SEND_ERROR".to_string(),
                        message: error.to_string(),
                    });
                }
            }
        };
        if let Some(gate) = &egress_gate {
            gate.finish(&receipt_ids, ToolEgressStatus::Sent)?;
        }
        if !response.status().is_success() {
            let status = response.status();
            return Err(MentatError::BackendError {
                code: format!("HTTP_{}", status.as_u16()),
                message: "provider가 agent round 요청을 거부했습니다.".to_string(),
            });
        }
        let mut byte_stream = response.bytes_stream();
        let request_id = request.request_id;
        let snapshot_id = request
            .repository_context
            .as_ref()
            .map(|context| context.snapshot_id);
        let stream = async_stream::stream! {
            yield InferenceRoundEvent::Started { request_id };
            let mut full_text = String::new();
            let mut byte_buffer = Vec::new();
            let mut pending = BTreeMap::<u64, PendingOpenAiToolCall>::new();
            loop {
                tokio::select! {
                    _ = cancel_token.cancelled() => {
                        yield InferenceRoundEvent::Failed {
                            error_code: "CANCELLED".to_string(),
                            safe_message: "요청이 취소되었습니다.".to_string(),
                        };
                        return;
                    }
                    chunk = byte_stream.next() => match chunk {
                        Some(Ok(bytes)) => {
                            byte_buffer.extend_from_slice(&bytes);
                            while let Some(position) = byte_buffer.iter().position(|byte| *byte == b'\n') {
                                let line = String::from_utf8_lossy(&byte_buffer[..position]).trim().to_string();
                                byte_buffer.drain(..position + 1);
                                let Some(data) = line.strip_prefix("data: ") else { continue; };
                                if data.trim() == "[DONE]" {
                                    if !pending.is_empty() {
                                        let Some(snapshot_id) = snapshot_id else {
                                            yield InferenceRoundEvent::Failed {
                                                error_code: "AGENT_TOOL_CONTEXT_MISSING".to_string(),
                                                safe_message: "tool call에 repository snapshot이 없습니다.".to_string(),
                                            };
                                            return;
                                        };
                                        let parsed = pending.values().map(|item| {
                                            let args = serde_json::from_str::<serde_json::Value>(&item.arguments)
                                                .map_err(|error| MentatError::BackendError {
                                                    code: "AGENT_TOOL_SCHEMA_INVALID".to_string(),
                                                    message: error.to_string(),
                                                })?;
                                            parse_repository_tool_call(&item.name, &args, Some(&item.id), snapshot_id)
                                        }).collect::<Result<Vec<_>, MentatError>>();
                                        match parsed {
                                            Ok(calls) => yield InferenceRoundEvent::ToolCallsRequested { round: 0, calls },
                                            Err(_) => yield InferenceRoundEvent::Failed {
                                                error_code: "AGENT_TOOL_SCHEMA_INVALID".to_string(),
                                                safe_message: "provider tool call 형식이 유효하지 않습니다.".to_string(),
                                            },
                                        }
                                    } else {
                                        yield InferenceRoundEvent::RawCompleted { full_text };
                                    }
                                    return;
                                }
                                let Ok(value) = serde_json::from_str::<serde_json::Value>(data) else { continue; };
                                let Some(choice) = value.pointer("/choices/0") else { continue; };
                                if let Some(delta) = choice.pointer("/delta/content").and_then(|value| value.as_str()) {
                                    full_text.push_str(delta);
                                    yield InferenceRoundEvent::TextDelta(delta.to_string());
                                }
                                if let Some(tool_deltas) = choice.pointer("/delta/tool_calls").and_then(|value| value.as_array()) {
                                    for tool_delta in tool_deltas {
                                        let index = tool_delta.get("index").and_then(|value| value.as_u64()).unwrap_or(0);
                                        let entry = pending.entry(index).or_default();
                                        if let Some(id) = tool_delta.get("id").and_then(|value| value.as_str()) {
                                            entry.id.push_str(id);
                                        }
                                        if let Some(name) = tool_delta.pointer("/function/name").and_then(|value| value.as_str()) {
                                            entry.name.push_str(name);
                                        }
                                        if let Some(arguments) = tool_delta.pointer("/function/arguments").and_then(|value| value.as_str()) {
                                            entry.arguments.push_str(arguments);
                                        }
                                    }
                                }
                            }
                        }
                        Some(Err(_)) => {
                            yield InferenceRoundEvent::Failed {
                                error_code: "STREAM_READ_ERROR".to_string(),
                                safe_message: "provider stream을 읽지 못했습니다.".to_string(),
                            };
                            return;
                        }
                        None => {
                            if pending.is_empty() {
                                yield InferenceRoundEvent::RawCompleted { full_text };
                            } else {
                                yield InferenceRoundEvent::Failed {
                                    error_code: "AGENT_TOOL_STREAM_INCOMPLETE".to_string(),
                                    safe_message: "provider tool call stream이 완결되지 않았습니다.".to_string(),
                                };
                            }
                            return;
                        }
                    }
                }
            }
        };
        Ok(Box::pin(stream))
    }
}
