use crate::agent_wire::{gemini_body, parse_repository_tool_call, request_has_tool_results};
use async_stream::stream;
use futures_util::stream::BoxStream;
use futures_util::StreamExt;
use mentat_core::{MentatError, ToolEgressStatus};
use mentat_inference::{
    AgentRequest, AvailableModel, BackendProfile, HealthStatus, InferenceEvent, InferenceRequest,
    InferenceRoundEvent, ModelCatalog, ModelVerification, ProviderBodyEgressGate,
};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue, CONTENT_TYPE};
use serde_json::json;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;

pub struct GeminiAdapter {
    client: Option<reqwest::Client>,
}

impl GeminiAdapter {
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
                // Gemini 자격 증명은 custom header이므로 redirect를 자동 추적하지 않는다.
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .ok(),
        }
    }

    fn client(&self) -> Result<&reqwest::Client, MentatError> {
        self.client
            .as_ref()
            .ok_or_else(|| MentatError::BackendError {
                code: "GEMINI_CLIENT_INIT_FAILED".to_string(),
                message: "Gemini 보안 HTTP client를 초기화하지 못했습니다.".to_string(),
            })
    }

    #[cfg(test)]
    pub(crate) fn with_client_build_failure_for_test() -> Self {
        Self { client: None }
    }
}

impl Default for GeminiAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl GeminiAdapter {
    fn generated_text(value: &serde_json::Value) -> Option<&str> {
        value
            .get("candidates")?
            .as_array()?
            .iter()
            .filter_map(|candidate| candidate.pointer("/content/parts")?.as_array())
            .flatten()
            .filter(|part| part.get("thought").and_then(|flag| flag.as_bool()) != Some(true))
            .filter_map(|part| part.get("text").and_then(|text| text.as_str()))
            .find(|text| !text.trim().is_empty())
    }

    fn bounded_metadata(value: &str) -> String {
        const MAX_CHARS: usize = 160;
        let mut text: String = value.chars().take(MAX_CHARS).collect();
        if value.chars().count() > MAX_CHARS {
            text.push('…');
        }
        text
    }

    fn missing_text_message(value: &serde_json::Value) -> String {
        let mut details = Vec::new();
        if let Some(reason) = value
            .pointer("/promptFeedback/blockReason")
            .and_then(|reason| reason.as_str())
        {
            details.push(format!("prompt block: {}", Self::bounded_metadata(reason)));
        }
        if let Some(candidate) = value
            .get("candidates")
            .and_then(|candidates| candidates.as_array())
            .and_then(|candidates| candidates.first())
        {
            if let Some(reason) = candidate
                .get("finishReason")
                .and_then(|reason| reason.as_str())
            {
                details.push(format!("finish reason: {}", Self::bounded_metadata(reason)));
            }
            if let Some(message) = candidate
                .get("finishMessage")
                .and_then(|message| message.as_str())
            {
                details.push(format!(
                    "finish message: {}",
                    Self::bounded_metadata(message)
                ));
            }
        }
        if let Some(tokens) = value
            .pointer("/usageMetadata/thoughtsTokenCount")
            .and_then(|tokens| tokens.as_u64())
        {
            details.push(format!("thinking tokens: {tokens}"));
        }

        if details.is_empty() {
            "Gemini 응답에 visible text 생성 결과가 없습니다.".to_string()
        } else {
            format!(
                "Gemini 응답에 visible text 생성 결과가 없습니다. ({})",
                details.join(", ")
            )
        }
    }

    fn api_key_header(profile: &BackendProfile) -> Result<HeaderMap, MentatError> {
        let api_key = profile.api_key.as_deref().unwrap_or("");
        if api_key.is_empty() {
            return Err(MentatError::BackendError {
                code: "MISSING_GEMINI_KEY".to_string(),
                message: "Gemini API 키가 비어 있습니다.".to_string(),
            });
        }
        let mut headers = HeaderMap::new();
        headers.insert(
            HeaderName::from_static("x-goog-api-key"),
            HeaderValue::from_str(api_key).map_err(|e| MentatError::BackendError {
                code: "INVALID_HEADER".to_string(),
                message: e.to_string(),
            })?,
        );
        Ok(headers)
    }

    pub async fn discover_models(
        &self,
        profile: &BackendProfile,
    ) -> Result<ModelCatalog, MentatError> {
        profile.validate_url()?;
        let start = Instant::now();
        let url = format!("{}/v1beta/models", profile.base_url.trim_end_matches('/'));
        let response = self
            .client()?
            .get(url)
            .headers(Self::api_key_header(profile)?)
            .timeout(Duration::from_secs(profile.timeout_secs.clamp(5, 300)))
            .send()
            .await
            .map_err(|e| MentatError::BackendError {
                code: "MODEL_DISCOVERY_NETWORK_ERROR".to_string(),
                message: e.to_string(),
            })?;
        if !response.status().is_success() {
            return Err(MentatError::BackendError {
                code: format!("MODEL_DISCOVERY_HTTP_{}", response.status().as_u16()),
                message: "Gemini 모델 목록 요청이 거부되었습니다.".to_string(),
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
            .get("models")
            .and_then(|models| models.as_array())
            .ok_or_else(|| MentatError::BackendError {
                code: "MODEL_DISCOVERY_INVALID_SCHEMA".to_string(),
                message: "Gemini 응답에 models 배열이 없습니다.".to_string(),
            })?
            .iter()
            .filter(|item| {
                item.get("supportedGenerationMethods")
                    .and_then(|methods| methods.as_array())
                    .is_some_and(|methods| {
                        methods
                            .iter()
                            .any(|method| method.as_str() == Some("generateContent"))
                    })
            })
            .filter_map(|item| {
                let id = item
                    .get("baseModelId")
                    .and_then(|id| id.as_str())
                    .or_else(|| item.get("name").and_then(|id| id.as_str()))?
                    .trim_start_matches("models/");
                let display = item
                    .get("displayName")
                    .and_then(|name| name.as_str())
                    .unwrap_or(id);
                Some(AvailableModel::new(id, display))
            })
            .collect();
        let catalog =
            ModelCatalog::from_untrusted(models).with_latency(start.elapsed().as_millis() as u64);
        if catalog.models.is_empty() {
            return Err(MentatError::BackendError {
                code: "MODEL_DISCOVERY_EMPTY".to_string(),
                message: "generateContent를 지원하는 Gemini 모델이 없습니다.".to_string(),
            });
        }
        Ok(catalog)
    }

    pub async fn verify_model(
        &self,
        profile: &BackendProfile,
    ) -> Result<ModelVerification, MentatError> {
        profile.validate_url()?;
        let model = profile.model.trim().trim_start_matches("models/");
        if model.is_empty() {
            return Err(MentatError::BackendError {
                code: "MODEL_NOT_SELECTED".to_string(),
                message: "검증할 Gemini 모델을 선택해야 합니다.".to_string(),
            });
        }
        let start = Instant::now();
        let url = format!(
            "{}/v1beta/models/{}:generateContent",
            profile.base_url.trim_end_matches('/'),
            model
        );
        let response = self
            .client()?
            .post(url)
            .headers(Self::api_key_header(profile)?)
            .timeout(Duration::from_secs(profile.timeout_secs.clamp(5, 300)))
            .json(&json!({
                "contents": [{"parts": [{"text": "Reply exactly with OK."}]}]
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
                message: "선택 Gemini 모델의 생성 요청이 거부되었습니다.".to_string(),
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
        let compatible = Self::generated_text(&value).is_some();
        Ok(ModelVerification {
            compatible,
            message: if compatible {
                "선택 Gemini 모델이 생성 요청에 정상 응답했습니다.".to_string()
            } else {
                Self::missing_text_message(&value)
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
                message: "Gemini API 키가 설정되지 않았습니다.".to_string(),
                latency_ms: None,
            });
        }

        profile.validate_url()?;

        let base = profile.base_url.trim_end_matches('/');
        let url = format!("{}/v1beta/models", base);

        let mut headers = HeaderMap::new();
        if let Ok(val) = HeaderValue::from_str(api_key) {
            headers.insert(HeaderName::from_static("x-goog-api-key"), val);
        }

        let timeout = Duration::from_secs(profile.timeout_secs.clamp(5, 300));
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
                        message: "Google Gemini 연결 성공".to_string(),
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
                message: format!("Gemini 네트워크 오류: {}", e),
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
                code: "MISSING_GEMINI_KEY".to_string(),
                message: "Gemini API 키가 비어 있습니다.".to_string(),
            });
        }

        if profile.model.trim().is_empty() {
            return Err(MentatError::BackendError {
                code: "MODEL_NOT_SELECTED".to_string(),
                message: "검증되어 활성화된 Gemini 모델이 없습니다.".to_string(),
            });
        }

        profile.validate_url()?;

        let base = profile.base_url.trim_end_matches('/');
        let model = profile.model.trim().trim_start_matches("models/");

        // SEC-F004: Pass API key via x-goog-api-key header instead of URL parameter
        let endpoint = format!(
            "{}/v1beta/models/{}:streamGenerateContent?alt=sse",
            base, model
        );

        let body = json!({
            "system_instruction": {
                "parts": [{ "text": request.system_contract }]
            },
            "contents": [{
                "parts": [
                    { "text": if request.user_question.is_empty() {
                        request.prompt_context.clone()
                    } else {
                        format!("{}\n\n## User Question\n{}", request.prompt_context, request.user_question)
                    } }
                ]
            }],
            "generationConfig": {
                "temperature": 0.2,
                "maxOutputTokens": 4096
            }
        });

        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        if let Ok(val) = HeaderValue::from_str(api_key) {
            headers.insert(HeaderName::from_static("x-goog-api-key"), val);
        }

        let timeout = Duration::from_secs(profile.timeout_secs.clamp(5, 300));

        // SEC-F005: Pre-response cancellation check during connect and request dispatch
        let send_future = self
            .client()?
            .post(&endpoint)
            .headers(headers)
            .timeout(timeout)
            .json(&body)
            .send();

        let response = tokio::select! {
            _ = cancel_token.cancelled() => {
                return Err(MentatError::Cancelled);
            }
            res = send_future => {
                res.map_err(|e| MentatError::BackendError {
                    code: "GEMINI_NETWORK_ERROR".to_string(),
                    message: e.to_string(),
                })?
            }
        };

        if !response.status().is_success() {
            let status = response.status();
            let err_body = response
                .text()
                .await
                .unwrap_or_else(|_| "No response body".to_string());
            return Err(MentatError::BackendError {
                code: format!("GEMINI_HTTP_{}", status.as_u16()),
                message: err_body,
            });
        }

        let mut byte_stream = response.bytes_stream();

        let req_id = request.request_id;
        let output_stream = stream! {
            yield InferenceEvent::Started {
                request_id: req_id,
            };

            let mut full_accumulated = String::new();
            let mut byte_buffer = Vec::new();

            loop {
                tokio::select! {
                    _ = cancel_token.cancelled() => {
                        yield InferenceEvent::Cancelled;
                        break;
                    }
                    chunk_opt = byte_stream.next() => {
                        match chunk_opt {
                            Some(Ok(bytes)) => {
                                byte_buffer.extend_from_slice(&bytes);

                                while let Some(pos) = byte_buffer.iter().position(|&b| b == b'\n') {
                                    let line_bytes = &byte_buffer[..pos];
                                    let line = String::from_utf8_lossy(line_bytes).trim().to_string();
                                    byte_buffer.drain(..pos + 1);

                                    if let Some(data) = line.strip_prefix("data: ") {
                                        if data.trim() == "[DONE]" {
                                            break;
                                        }

                                        if let Ok(val) = serde_json::from_str::<serde_json::Value>(data) {
                                            if let Some(candidates) = val.get("candidates").and_then(|c| c.as_array()) {
                                                for cand in candidates {
                                                    if let Some(parts) = cand.get("content").and_then(|c| c.get("parts")).and_then(|p| p.as_array()) {
                                                        for part in parts {
                                                            if let Some(text) = part.get("text").and_then(|t| t.as_str()) {
                                                                full_accumulated.push_str(text);
                                                                yield InferenceEvent::TextDelta(text.to_string());
                                                            }
                                                        }
                                                    }
                                                }
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
                                break;
                            }
                            None => {
                                break;
                            }
                        }
                    }
                }
            }

            if !cancel_token.is_cancelled() {
                yield InferenceEvent::Completed {
                    full_text: full_accumulated,
                };
            }
        };

        Ok(Box::pin(output_stream))
    }

    pub fn agent_endpoint(profile: &BackendProfile) -> String {
        let model = profile.model.trim().trim_start_matches("models/");
        format!(
            "{}/v1beta/models/{}:streamGenerateContent",
            profile.base_url.trim_end_matches('/'),
            model
        )
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
                code: "MISSING_GEMINI_KEY".to_string(),
                message: "Gemini API 키가 비어 있습니다.".to_string(),
            });
        }
        if profile.model.trim().is_empty() {
            return Err(MentatError::BackendError {
                code: "MODEL_NOT_SELECTED".to_string(),
                message: "검증되어 활성화된 Gemini 모델이 없습니다.".to_string(),
            });
        }
        profile.validate_url()?;
        let endpoint_identity = Self::agent_endpoint(profile);
        let endpoint = format!("{endpoint_identity}?alt=sse");
        let exact_body = serde_json::to_vec(&gemini_body(&request)?).map_err(|error| {
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
            gate.authorize_exact_body(&request, &endpoint_identity, &exact_body)?
        } else {
            Vec::new()
        };
        let mut headers = Self::api_key_header(profile)?;
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        let send_future = self
            .client()?
            .post(&endpoint)
            .headers(headers)
            .timeout(Duration::from_secs(profile.timeout_secs.clamp(5, 300)))
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
                        code: "GEMINI_NETWORK_ERROR".to_string(),
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
                code: format!("GEMINI_HTTP_{}", status.as_u16()),
                message: "Gemini가 agent round 요청을 거부했습니다.".to_string(),
            });
        }
        let mut byte_stream = response.bytes_stream();
        let request_id = request.request_id;
        let snapshot_id = request
            .repository_context
            .as_ref()
            .map(|context| context.snapshot_id);
        let output_stream = stream! {
            yield InferenceRoundEvent::Started { request_id };
            let mut full_text = String::new();
            let mut byte_buffer = Vec::new();
            let mut tool_calls = Vec::new();
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
                                if data.trim() == "[DONE]" { break; }
                                let Ok(value) = serde_json::from_str::<serde_json::Value>(data) else { continue; };
                                let Some(parts) = value.pointer("/candidates/0/content/parts").and_then(|value| value.as_array()) else { continue; };
                                for part in parts {
                                    if part.get("thought").and_then(|value| value.as_bool()) == Some(true) {
                                        continue;
                                    }
                                    if let Some(text) = part.get("text").and_then(|value| value.as_str()) {
                                        full_text.push_str(text);
                                        yield InferenceRoundEvent::TextDelta(text.to_string());
                                    }
                                    if let Some(function) = part.get("functionCall") {
                                        let Some(name) = function.get("name").and_then(|value| value.as_str()) else { continue; };
                                        let args = function.get("args").cloned().unwrap_or_else(|| serde_json::json!({}));
                                        let Some(snapshot_id) = snapshot_id else {
                                            yield InferenceRoundEvent::Failed {
                                                error_code: "AGENT_TOOL_CONTEXT_MISSING".to_string(),
                                                safe_message: "tool call에 repository snapshot이 없습니다.".to_string(),
                                            };
                                            return;
                                        };
                                        match parse_repository_tool_call(name, &args, None, snapshot_id) {
                                            Ok(call) => tool_calls.push(call),
                                            Err(_) => {
                                                yield InferenceRoundEvent::Failed {
                                                    error_code: "AGENT_TOOL_SCHEMA_INVALID".to_string(),
                                                    safe_message: "Gemini tool call 형식이 유효하지 않습니다.".to_string(),
                                                };
                                                return;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        Some(Err(_)) => {
                            yield InferenceRoundEvent::Failed {
                                error_code: "STREAM_READ_ERROR".to_string(),
                                safe_message: "Gemini stream을 읽지 못했습니다.".to_string(),
                            };
                            return;
                        }
                        None => break,
                    }
                }
            }
            if tool_calls.is_empty() {
                yield InferenceRoundEvent::RawCompleted { full_text };
            } else {
                yield InferenceRoundEvent::ToolCallsRequested { round: 0, calls: tool_calls };
            }
        };
        Ok(Box::pin(output_stream))
    }
}
