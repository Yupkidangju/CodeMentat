pub mod gemini_adapter;
pub mod openai_adapter;

pub use gemini_adapter::GeminiAdapter;
pub use openai_adapter::OpenAiAdapter;

use async_trait::async_trait;
use futures_util::stream::BoxStream;
use mentat_core::error::MentatError;
use mentat_inference::{
    BackendProfile, HealthStatus, InferenceBackend, InferenceEvent, InferenceRequest, ModelCatalog,
    ModelVerification, ProviderKind,
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

    async fn discover_models(&self, profile: &BackendProfile) -> Result<ModelCatalog, MentatError> {
        match profile.provider {
            ProviderKind::GoogleGemini => self.gemini.discover_models(profile).await,
            ProviderKind::OpenRouter
            | ProviderKind::OpenAi
            | ProviderKind::OpenAICompatible
            | ProviderKind::CustomCompatible
            | ProviderKind::LocalMock => self.openai.discover_models(profile).await,
        }
    }

    async fn verify_model(
        &self,
        profile: &BackendProfile,
    ) -> Result<ModelVerification, MentatError> {
        match profile.provider {
            ProviderKind::GoogleGemini => self.gemini.verify_model(profile).await,
            ProviderKind::OpenRouter
            | ProviderKind::OpenAi
            | ProviderKind::OpenAICompatible
            | ProviderKind::CustomCompatible
            | ProviderKind::LocalMock => self.openai.verify_model(profile).await,
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
    async fn inference_never_falls_back_to_a_hardcoded_model() {
        let profile = BackendProfile {
            provider: ProviderKind::OpenAICompatible,
            base_url: "http://127.0.0.1:9/v1".to_string(),
            model: String::new(),
            api_key: Some("test-key".to_string()),
            ..Default::default()
        };
        let request = InferenceRequest {
            request_id: Uuid::new_v4(),
            system_contract: "sys".to_string(),
            prompt_context: "ctx".to_string(),
            user_question: "q".to_string(),
            profile,
        };
        let result = OpenAiAdapter::new()
            .infer_stream(request, CancellationToken::new())
            .await;
        match result {
            Err(MentatError::BackendError { code, .. }) => {
                assert_eq!(code, "MODEL_NOT_SELECTED")
            }
            Err(other) => panic!("unexpected error: {other:?}"),
            Ok(_) => panic!("empty model must fail before network access"),
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
            model: "fixture-model".to_string(),
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

    async fn bind_listener() -> (tokio::net::TcpListener, u16) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind loopback");
        let port = listener.local_addr().unwrap().port();
        (listener, port)
    }

    async fn read_http_request(stream: &mut tokio::net::TcpStream) -> Vec<u8> {
        use tokio::io::AsyncReadExt;
        let mut buf = Vec::new();
        let mut tmp = [0u8; 1024];
        loop {
            let n = match stream.read(&mut tmp).await {
                Ok(0) | Err(_) => break,
                Ok(n) => n,
            };
            buf.extend_from_slice(&tmp[..n]);
            if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                let headers = String::from_utf8_lossy(&buf[..pos]);
                let content_len = headers
                    .lines()
                    .find_map(|line| {
                        let (k, v) = line.split_once(':')?;
                        if k.eq_ignore_ascii_case("content-length") {
                            v.trim().parse::<usize>().ok()
                        } else {
                            None
                        }
                    })
                    .unwrap_or(0);
                let body_start = pos + 4;
                while buf.len() < body_start + content_len {
                    match stream.read(&mut tmp).await {
                        Ok(0) | Err(_) => break,
                        Ok(n) => buf.extend_from_slice(&tmp[..n]),
                    }
                }
                break;
            }
        }
        buf
    }

    fn openai_profile(port: u16) -> BackendProfile {
        BackendProfile {
            id: Uuid::new_v4(),
            name: "Wire".to_string(),
            provider: ProviderKind::OpenAICompatible,
            base_url: format!("http://127.0.0.1:{}/v1", port),
            model: "mock".to_string(),
            api_key: Some("test-key".to_string()),
            timeout_secs: 5,
        }
    }

    fn sample_request(profile: BackendProfile) -> InferenceRequest {
        InferenceRequest {
            request_id: Uuid::new_v4(),
            system_contract: "sys".to_string(),
            prompt_context: "ctx".to_string(),
            user_question: "q".to_string(),
            profile,
        }
    }

    #[tokio::test]
    async fn openai_model_discovery_uses_provider_response_without_presets() {
        use tokio::io::AsyncWriteExt;
        let (listener, port) = bind_listener().await;
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.expect("accept");
            let request = read_http_request(&mut stream).await;
            assert!(String::from_utf8_lossy(&request).starts_with("GET /v1/models "));
            let body = r#"{"data":[{"id":"dynamic-alpha"},{"id":"dynamic-beta"},{"id":"dynamic-alpha"},{"id":""}]}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(), body
            );
            stream.write_all(response.as_bytes()).await.expect("write");
        });

        let adapter = OpenAiAdapter::new();
        let catalog = adapter
            .discover_models(&openai_profile(port))
            .await
            .expect("catalog");
        assert_eq!(
            catalog
                .models
                .iter()
                .map(|m| m.id.as_str())
                .collect::<Vec<_>>(),
            vec!["dynamic-alpha", "dynamic-beta"]
        );
        server.await.expect("server");
    }

    #[tokio::test]
    async fn gemini_model_discovery_keeps_only_generate_content_models() {
        use tokio::io::AsyncWriteExt;
        let (listener, port) = bind_listener().await;
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.expect("accept");
            let request = read_http_request(&mut stream).await;
            assert!(String::from_utf8_lossy(&request).starts_with("GET /v1beta/models "));
            let body = r#"{"models":[{"name":"models/dynamic-gemini","displayName":"Dynamic Gemini","supportedGenerationMethods":["generateContent"]},{"name":"models/embed-only","supportedGenerationMethods":["embedContent"]}]}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(), body
            );
            stream.write_all(response.as_bytes()).await.expect("write");
        });

        let adapter = GeminiAdapter::new();
        let profile = BackendProfile {
            provider: ProviderKind::GoogleGemini,
            base_url: format!("http://127.0.0.1:{port}"),
            api_key: Some("test-key".to_string()),
            ..Default::default()
        };
        let catalog = adapter.discover_models(&profile).await.expect("catalog");
        assert_eq!(catalog.models.len(), 1);
        assert_eq!(catalog.models[0].id, "dynamic-gemini");
        assert_eq!(catalog.models[0].display_name, "Dynamic Gemini");
        server.await.expect("server");
    }

    #[tokio::test]
    async fn selected_openai_model_must_pass_a_real_generation_probe() {
        use tokio::io::AsyncWriteExt;
        let (listener, port) = bind_listener().await;
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.expect("accept");
            let request = read_http_request(&mut stream).await;
            let request_text = String::from_utf8_lossy(&request);
            assert!(request_text.starts_with("POST /v1/chat/completions "));
            assert!(request_text.contains("dynamic-alpha"));
            let body = r#"{"choices":[{"message":{"content":"ok"}}]}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(), body
            );
            stream.write_all(response.as_bytes()).await.expect("write");
        });

        let adapter = OpenAiAdapter::new();
        let mut profile = openai_profile(port);
        profile.model = "dynamic-alpha".to_string();
        let verification = adapter.verify_model(&profile).await.expect("verification");
        assert!(verification.compatible);
        server.await.expect("server");
    }

    #[tokio::test]
    async fn selected_gemini_model_must_pass_a_real_generation_probe() {
        use tokio::io::AsyncWriteExt;
        let (listener, port) = bind_listener().await;
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.expect("accept");
            let request = read_http_request(&mut stream).await;
            let request_text = String::from_utf8_lossy(&request);
            assert!(request_text.starts_with("POST /v1beta/models/dynamic-gemini:generateContent "));
            assert!(!request_text.contains("\"maxOutputTokens\":1"));
            let body = r#"{"candidates":[{"content":{"parts":[{"thought":true,"text":""}]}},{"content":{"parts":[{"thought":true,"text":"internal"},{"text":"ok"}]},"finishReason":"STOP"}],"usageMetadata":{"thoughtsTokenCount":7}}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(), body
            );
            stream.write_all(response.as_bytes()).await.expect("write");
        });

        let adapter = GeminiAdapter::new();
        let profile = BackendProfile {
            provider: ProviderKind::GoogleGemini,
            base_url: format!("http://127.0.0.1:{port}"),
            model: "dynamic-gemini".to_string(),
            api_key: Some("test-key".to_string()),
            ..Default::default()
        };
        let verification = adapter.verify_model(&profile).await.expect("verification");
        assert!(verification.compatible);
        server.await.expect("server");
    }

    #[tokio::test]
    async fn gemini_verification_reports_finish_reason_when_visible_text_is_missing() {
        use tokio::io::AsyncWriteExt;
        let (listener, port) = bind_listener().await;
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.expect("accept");
            let _ = read_http_request(&mut stream).await;
            let body = r#"{"candidates":[{"content":{"parts":[]},"finishReason":"MAX_TOKENS","finishMessage":"Output limit reached"}],"usageMetadata":{"thoughtsTokenCount":128}}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(), body
            );
            stream.write_all(response.as_bytes()).await.expect("write");
        });

        let adapter = GeminiAdapter::new();
        let profile = BackendProfile {
            provider: ProviderKind::GoogleGemini,
            base_url: format!("http://127.0.0.1:{port}"),
            model: "dynamic-gemini".to_string(),
            api_key: Some("test-key".to_string()),
            ..Default::default()
        };
        let verification = adapter.verify_model(&profile).await.expect("verification");
        assert!(!verification.compatible);
        assert!(verification.message.contains("MAX_TOKENS"));
        assert!(verification.message.contains("thinking tokens: 128"));
        server.await.expect("server");
    }

    #[tokio::test]
    async fn builtin_local_without_runtime_fails_closed_without_requesting_a_key() {
        let adapter = MultiProviderAdapter::new();
        let profile = BackendProfile {
            provider: ProviderKind::LocalMock,
            base_url: ProviderKind::LocalMock.default_base_url().to_string(),
            api_key: None,
            ..Default::default()
        };
        let result = adapter.discover_models(&profile).await;
        match result {
            Err(MentatError::BackendError { code, .. }) => {
                assert_eq!(code, "LOCAL_RUNTIME_UNAVAILABLE")
            }
            Err(other) => panic!("unexpected error: {other:?}"),
            Ok(_) => panic!("내장 로컬 런타임이 없으면 활성화 자료를 만들 수 없습니다"),
        }
    }

    #[tokio::test]
    async fn model_verification_rejects_oversized_untrusted_response() {
        use tokio::io::AsyncWriteExt;
        let (listener, port) = bind_listener().await;
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.expect("accept");
            let _ = read_http_request(&mut stream).await;
            let body = vec![b'x'; 1024 * 1024 + 1];
            let headers = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            stream.write_all(headers.as_bytes()).await.expect("headers");
            stream.write_all(&body).await.expect("body");
        });

        let adapter = OpenAiAdapter::new();
        let mut profile = openai_profile(port);
        profile.model = "dynamic-alpha".to_string();
        let result = adapter.verify_model(&profile).await;
        match result {
            Err(MentatError::BackendError { code, .. }) => {
                assert_eq!(code, "MODEL_VERIFY_RESPONSE_TOO_LARGE")
            }
            Err(other) => panic!("unexpected error: {other:?}"),
            Ok(_) => panic!("oversized verification response must fail closed"),
        }
        server.await.expect("server");
    }

    #[tokio::test]
    async fn gemini_cross_origin_redirect_never_receives_api_key() {
        use tokio::io::AsyncWriteExt;
        let (redirect_listener, redirect_port) = bind_listener().await;
        let (target_listener, target_port) = bind_listener().await;

        let redirect_server = tokio::spawn(async move {
            let (mut stream, _) = redirect_listener.accept().await.expect("accept redirect");
            let _ = read_http_request(&mut stream).await;
            let response = format!(
                "HTTP/1.1 302 Found\r\nLocation: http://127.0.0.1:{target_port}/stolen\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            );
            stream
                .write_all(response.as_bytes())
                .await
                .expect("write redirect");
        });
        let target_server = tokio::spawn(async move {
            tokio::time::timeout(
                std::time::Duration::from_millis(500),
                target_listener.accept(),
            )
            .await
            .is_ok()
        });

        let profile = BackendProfile {
            provider: ProviderKind::GoogleGemini,
            base_url: format!("http://127.0.0.1:{redirect_port}"),
            api_key: Some("redirect-test-key".to_string()),
            ..Default::default()
        };
        let result = GeminiAdapter::new().discover_models(&profile).await;
        assert!(matches!(
            result,
            Err(MentatError::BackendError { ref code, .. }) if code == "MODEL_DISCOVERY_HTTP_302"
        ));
        redirect_server.await.expect("redirect server");
        assert!(!target_server.await.expect("target server"));
    }

    #[tokio::test]
    async fn gemini_secure_client_build_failure_blocks_every_network_operation() {
        let adapter = GeminiAdapter::with_client_build_failure_for_test();
        let profile = BackendProfile {
            provider: ProviderKind::GoogleGemini,
            base_url: "http://127.0.0.1:9".to_string(),
            model: "fixture-model".to_string(),
            api_key: Some("fixture-key".to_string()),
            ..Default::default()
        };
        let request = sample_request(profile.clone());

        let errors = [
            adapter.discover_models(&profile).await.err(),
            adapter.verify_model(&profile).await.err(),
            adapter.health_check(&profile).await.err(),
            adapter
                .infer_stream(request, CancellationToken::new())
                .await
                .err(),
        ];
        assert!(errors.into_iter().all(|error| matches!(
            error,
            Some(MentatError::BackendError { ref code, .. }) if code == "GEMINI_CLIENT_INIT_FAILED"
        )));
    }

    #[tokio::test]
    async fn test_openai_wire_http_error_codes() {
        use tokio::io::AsyncWriteExt;

        for status in [401_u16, 429, 503] {
            let (listener, port) = bind_listener().await;
            let server = tokio::spawn(async move {
                if let Ok((mut stream, _)) = listener.accept().await {
                    let _ = read_http_request(&mut stream).await;
                    let body = format!("error {status}");
                    let resp = format!(
                        "HTTP/1.1 {status} ERR\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    let _ = stream.write_all(resp.as_bytes()).await;
                }
            });

            let adapter = OpenAiAdapter::new();
            let result = adapter
                .infer_stream(
                    sample_request(openai_profile(port)),
                    CancellationToken::new(),
                )
                .await;
            match result {
                Err(MentatError::BackendError { code, .. }) => {
                    assert_eq!(code, format!("HTTP_{status}"));
                }
                Err(other) => panic!("unexpected error: {other:?}"),
                Ok(_) => panic!("HTTP error must fail closed"),
            }
            let _ = server.await;
        }
    }

    #[tokio::test]
    async fn test_openai_wire_cancel_during_send() {
        let (listener, port) = bind_listener().await;
        let server = tokio::spawn(async move {
            if let Ok((mut stream, _)) = listener.accept().await {
                let _ = read_http_request(&mut stream).await;
                tokio::time::sleep(std::time::Duration::from_secs(30)).await;
            }
        });

        let adapter = OpenAiAdapter::new();
        let cancel = CancellationToken::new();
        let cancel_clone = cancel.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(80)).await;
            cancel_clone.cancel();
        });

        let result = adapter
            .infer_stream(sample_request(openai_profile(port)), cancel)
            .await;
        match result {
            Err(MentatError::Cancelled) => {}
            Err(other) => panic!("unexpected error: {other:?}"),
            Ok(_) => panic!("cancel during send must fail"),
        }
        server.abort();
    }

    #[tokio::test]
    async fn test_openai_wire_split_sse_chunks() {
        use futures_util::StreamExt;
        use tokio::io::AsyncWriteExt;

        let (listener, port) = bind_listener().await;
        let server = tokio::spawn(async move {
            if let Ok((mut stream, _)) = listener.accept().await {
                let _ = read_http_request(&mut stream).await;
                let part1 = b"data: {\"choices\":[{\"delta\":{\"content\":\"Hel\"}}]}";
                let part2 =
                    b"\ndata: {\"choices\":[{\"delta\":{\"content\":\"lo\"}}]}\ndata: [DONE]\n";
                let total = part1.len() + part2.len();
                let headers = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {total}\r\nConnection: close\r\n\r\n"
                );
                let _ = stream.write_all(headers.as_bytes()).await;
                let _ = stream.write_all(part1).await;
                tokio::time::sleep(std::time::Duration::from_millis(30)).await;
                let _ = stream.write_all(part2).await;
            }
        });

        let adapter = OpenAiAdapter::new();
        let mut stream = adapter
            .infer_stream(
                sample_request(openai_profile(port)),
                CancellationToken::new(),
            )
            .await
            .expect("sse stream");

        let mut deltas = Vec::new();
        let mut completed = None;
        while let Some(event) = stream.next().await {
            match event {
                InferenceEvent::TextDelta(text) => deltas.push(text),
                InferenceEvent::Completed { full_text } => {
                    completed = Some(full_text);
                    break;
                }
                InferenceEvent::Failed {
                    error_code,
                    message,
                } => {
                    panic!("stream failed: {error_code} {message}");
                }
                _ => {}
            }
        }

        assert_eq!(deltas, vec!["Hel".to_string(), "lo".to_string()]);
        assert_eq!(completed.as_deref(), Some("Hello"));
        let _ = server.await;
    }

    #[tokio::test]
    async fn test_sec_f002_wire_question_zero_leak() {
        use tokio::io::AsyncWriteExt;
        let secret = "sk-ant-api03-abcdef1234567890abcdef";
        let (listener, port) = bind_listener().await;
        let server = tokio::spawn(async move {
            if let Ok((mut stream, _)) = listener.accept().await {
                let req = read_http_request(&mut stream).await;
                let body = String::from_utf8_lossy(&req);
                assert!(
                    !body.contains(secret),
                    "raw secret must not appear on the wire"
                );
                assert_eq!(body.matches("## User Question").count(), 1);
                let payload =
                    "data: {\"choices\":[{\"delta\":{\"content\":\"ok\"}}]}\ndata: [DONE]\n";
                let headers = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    payload.len()
                );
                let _ = stream.write_all(headers.as_bytes()).await;
                let _ = stream.write_all(payload.as_bytes()).await;
            }
        });

        let (redacted, _) =
            mentat_analysis::EgressFilter::scan_and_redact_secrets(&format!("please use {secret}"));
        let adapter = OpenAiAdapter::new();
        let req = InferenceRequest {
            request_id: Uuid::new_v4(),
            system_contract: "sys".to_string(),
            prompt_context: "repo files only".to_string(),
            user_question: redacted,
            profile: openai_profile(port),
        };
        let _ = adapter
            .infer_stream(req, CancellationToken::new())
            .await
            .expect("stream");
        let _ = server.await;
    }

    #[tokio::test]
    async fn test_imp_f004_adapter_loopback_valid_and_invalid_citations() {
        use futures_util::StreamExt;
        use mentat_analysis::AnswerBundleNormalizer;
        use mentat_core::models::{ClaimClassification, FileKind, FileRecord};
        use std::collections::HashMap;
        use std::path::PathBuf;
        use tokio::io::AsyncWriteExt;

        let snap = Uuid::new_v4();
        let ev = Uuid::new_v4();
        let preview = "fn main() {\n}\n";
        let valid = format!(
            r#"{{"request_id":"{req}","snapshot_id":"{snap}","direct_answer":"entry","claims":[{{"id":"{claim}","classification":"Observed","statement":"main exists","confidence":1.0,"evidence_ids":["{ev}"],"rationale":null}}],"evidence_map":[{{"id":"{ev}","snapshot_id":"{snap}","relative_path":"src/main.rs","line_start":1,"line_end":2,"content_hash":"hash-main","excerpt":"fn main() {{"}}],"recommendations":[],"conflicts":[],"raw_model_response":null}}"#,
            req = Uuid::new_v4(),
            claim = Uuid::new_v4()
        );
        let invalid = valid.replace("hash-main", "wrong-hash");

        async fn serve_json(json: String) -> u16 {
            let (listener, port) = bind_listener().await;
            tokio::spawn(async move {
                if let Ok((mut stream, _)) = listener.accept().await {
                    let _ = read_http_request(&mut stream).await;
                    let payload = format!(
                        "data: {{\"choices\":[{{\"delta\":{{\"content\":{content}}}}}]}}\ndata: [DONE]\n",
                        content = serde_json::to_string(&json).unwrap()
                    );
                    let headers = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        payload.len()
                    );
                    let _ = stream.write_all(headers.as_bytes()).await;
                    let _ = stream.write_all(payload.as_bytes()).await;
                }
            });
            port
        }

        async fn collect_text(port: u16) -> String {
            let adapter = OpenAiAdapter::new();
            let mut stream = adapter
                .infer_stream(
                    sample_request(openai_profile(port)),
                    CancellationToken::new(),
                )
                .await
                .expect("stream");
            let mut full = String::new();
            while let Some(ev) = stream.next().await {
                if let InferenceEvent::Completed { full_text } = ev {
                    full = full_text;
                    break;
                }
            }
            full
        }

        let files = vec![FileRecord {
            relative_path: PathBuf::from("src/main.rs"),
            kind: FileKind::SourceCode,
            size_bytes: 16,
            content_hash: "hash-main".into(),
            is_text: true,
            line_count: Some(2),
            text_preview: Some(preview.into()),
        }];
        let mut texts = HashMap::new();
        texts.insert(PathBuf::from("src/main.rs"), preview.to_string());

        let valid_port = serve_json(valid).await;
        let valid_text = collect_text(valid_port).await;
        let valid_bundle = AnswerBundleNormalizer::from_model_text_with_contents(
            Uuid::new_v4(),
            snap,
            &valid_text,
            &files,
            &texts,
        );
        assert_eq!(
            valid_bundle.claims[0].classification,
            ClaimClassification::Observed
        );

        let invalid_port = serve_json(invalid).await;
        let invalid_text = collect_text(invalid_port).await;
        let invalid_bundle = AnswerBundleNormalizer::from_model_text_with_contents(
            Uuid::new_v4(),
            snap,
            &invalid_text,
            &files,
            &texts,
        );
        assert_eq!(
            invalid_bundle.claims[0].classification,
            ClaimClassification::Unknown
        );
    }
}
