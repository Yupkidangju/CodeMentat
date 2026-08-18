use futures_util::stream::BoxStream;
use futures_util::StreamExt;
use mentat_core::error::MentatError;
use mentat_inference::{
    BackendProfile, HealthStatus, InferenceEvent, InferenceRequest, ProviderKind,
};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
use serde_json::json;
use std::time::Instant;
use tokio_util::sync::CancellationToken;

pub struct OpenAiAdapter {
    client: reqwest::Client,
}

impl OpenAiAdapter {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::builder()
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
        }
    }
}

impl Default for OpenAiAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl OpenAiAdapter {
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

        let model = if profile.model.is_empty() {
            "gpt-4o"
        } else {
            &profile.model
        };

        let body = json!({
            "model": model,
            "stream": true,
            "messages": [
                {
                    "role": "system",
                    "content": request.system_contract
                },
                {
                    "role": "user",
                    "content": format!("{}\n\nQuestion:\n{}", request.prompt_context, request.user_question)
                }
            ]
        });

        let timeout = std::time::Duration::from_secs(profile.timeout_secs.clamp(5, 300));
        let send_future = self
            .client
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
}
