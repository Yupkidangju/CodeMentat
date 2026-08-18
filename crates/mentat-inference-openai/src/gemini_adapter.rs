use async_stream::stream;
use futures_util::stream::BoxStream;
use futures_util::StreamExt;
use mentat_core::error::MentatError;
use mentat_inference::{BackendProfile, HealthStatus, InferenceEvent, InferenceRequest};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue, CONTENT_TYPE};
use serde_json::json;
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;

pub struct GeminiAdapter {
    client: reqwest::Client,
}

impl GeminiAdapter {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::builder()
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
        }
    }
}

impl Default for GeminiAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl GeminiAdapter {
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
            .client
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

        profile.validate_url()?;

        let base = profile.base_url.trim_end_matches('/');
        let model = if profile.model.is_empty() {
            "gemini-2.5-flash"
        } else {
            &profile.model
        };

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
            .client
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
}
