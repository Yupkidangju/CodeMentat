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
        let response = self
            .client
            .post(&endpoint)
            .headers(headers)
            .timeout(timeout)
            .json(&body)
            .send()
            .await
            .map_err(|e| MentatError::BackendError {
                code: "HTTP_SEND_ERROR".to_string(),
                message: e.to_string(),
            })?;

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
            let mut buffer = String::new();

            loop {
                tokio::select! {
                    _ = cancel_token.cancelled() => {
                        yield InferenceEvent::Cancelled;
                        return;
                    }
                    chunk_opt = byte_stream.next() => {
                        match chunk_opt {
                            Some(Ok(bytes)) => {
                                if let Ok(s) = std::str::from_utf8(&bytes) {
                                    buffer.push_str(s);
                                    while let Some(pos) = buffer.find('\n') {
                                        let line = buffer[..pos].trim().to_string();
                                        buffer = buffer[pos + 1..].to_string();

                                        if line.is_empty() || line.starts_with(':') {
                                            continue;
                                        }

                                        if let Some(data) = line.strip_prefix("data: ") {
                                            if data == "[DONE]" {
                                                yield InferenceEvent::Completed { full_text: full_text.clone() };
                                                return;
                                            }

                                            if let Ok(val) = serde_json::from_str::<serde_json::Value>(data) {
                                                if let Some(content) = val["choices"][0]["delta"]["content"].as_str() {
                                                    full_text.push_str(content);
                                                    yield InferenceEvent::TextDelta(content.to_string());
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
