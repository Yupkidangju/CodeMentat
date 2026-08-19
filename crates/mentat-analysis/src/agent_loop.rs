use crate::repository_tools::RepositoryToolGateway;
use futures_util::StreamExt;
use mentat_core::{
    GroundingFreshness, GroundingTrace, MentatError, RepositoryToolCallRecord,
    RepositoryToolCallStatus,
};
use mentat_inference::{
    AgentEvent, AgentMessage, AgentMessageContent, AgentRequest, CancelledPayload,
    CompletedPayload, InferenceBackend, InferenceRoundEvent, ProviderKind,
};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

pub struct AgentLoop<B> {
    backend: Arc<B>,
    gateway: Option<Arc<RepositoryToolGateway>>,
}

pub struct AgentLoopOutcome {
    pub events: Vec<AgentEvent>,
    pub grounding_trace: Option<GroundingTrace>,
}

impl<B: InferenceBackend> AgentLoop<B> {
    pub fn new(backend: Arc<B>, gateway: Option<Arc<RepositoryToolGateway>>) -> Self {
        Self { backend, gateway }
    }

    pub async fn run(
        &self,
        request: AgentRequest,
        cancel: CancellationToken,
    ) -> Result<AgentLoopOutcome, MentatError> {
        let timeout = std::time::Duration::from_secs(request.limits.timeout_secs.min(300));
        tokio::time::timeout(timeout, self.run_bounded(request, cancel))
            .await
            .map_err(|_| loop_error("AGENT_LOOP_LIMIT_REACHED", "AgentLoop 시간 한도 초과"))?
    }

    async fn run_bounded(
        &self,
        mut request: AgentRequest,
        cancel: CancellationToken,
    ) -> Result<AgentLoopOutcome, MentatError> {
        let mut events = vec![AgentEvent::Started {
            request_id: request.request_id,
        }];
        let mut trace = request
            .repository_context
            .as_ref()
            .map(|context| GroundingTrace {
                id: Uuid::new_v4(),
                conversation_id: request.conversation_id,
                turn_id: request.turn_id,
                snapshot_id: Some(context.snapshot_id),
                tool_calls: Vec::new(),
                source_refs: Vec::new(),
                egress_receipt_ids: Vec::new(),
                freshness: GroundingFreshness::FreshAtSend,
            });
        let mut total_calls = 0u16;
        let mut total_result_bytes = 0u32;
        let mut fingerprints: HashMap<String, u8> = HashMap::new();

        for round in 1..=request.limits.max_rounds {
            if cancel.is_cancelled() {
                events.push(AgentEvent::Cancelled {
                    payload: cancellation_payload(&request.response_contract, String::new()),
                });
                return Ok(AgentLoopOutcome {
                    events,
                    grounding_trace: trace,
                });
            }
            let mut stream = self
                .backend
                .infer_round_stream(request.clone(), cancel.clone())
                .await?;
            let mut round_text = String::new();
            let mut tool_calls = None;
            let mut terminal = None;
            while let Some(event) = stream.next().await {
                match event {
                    InferenceRoundEvent::Started { .. } => {}
                    InferenceRoundEvent::ThinkingDelta(delta) => {
                        events.push(AgentEvent::ThinkingDelta(delta));
                    }
                    InferenceRoundEvent::TextDelta(delta) => {
                        round_text.push_str(&delta);
                        if matches!(
                            request.response_contract,
                            mentat_core::ResponseContract::AdvisorMarkdown
                        ) {
                            events.push(AgentEvent::TextDelta(delta));
                        }
                    }
                    InferenceRoundEvent::UsageUpdate {
                        prompt_tokens,
                        completion_tokens,
                    } => events.push(AgentEvent::UsageUpdate {
                        prompt_tokens,
                        completion_tokens,
                    }),
                    InferenceRoundEvent::ToolCallsRequested { calls, .. } => {
                        tool_calls = Some(calls);
                        break;
                    }
                    InferenceRoundEvent::RawCompleted { full_text } => {
                        terminal = Some(Ok(full_text));
                        break;
                    }
                    InferenceRoundEvent::Failed {
                        error_code,
                        safe_message,
                    } => {
                        terminal = Some(Err((error_code, safe_message)));
                        break;
                    }
                }
            }

            if let Some(result) = terminal {
                match result {
                    Ok(full_text) => match request.response_contract {
                        mentat_core::ResponseContract::AdvisorMarkdown => {
                            events.push(AgentEvent::Completed {
                                payload: CompletedPayload::AdvisorMarkdown(full_text),
                                trace_id: trace.as_ref().map(|trace| trace.id),
                            });
                        }
                        mentat_core::ResponseContract::AuditAnswerBundle { .. } => {
                            match validate_audit_bundle(
                                &full_text,
                                request.request_id,
                                trace.as_ref(),
                            ) {
                                Ok(bundle) => events.push(AgentEvent::Completed {
                                    payload: CompletedPayload::ValidatedAuditBundle(bundle),
                                    trace_id: trace.as_ref().map(|trace| trace.id),
                                }),
                                Err(_) => events.push(AgentEvent::Failed {
                                    error_code: "AUDIT_RESPONSE_INVALID".to_string(),
                                    safe_message:
                                        "Audit 응답의 schema/evidence 검증에 실패했습니다."
                                            .to_string(),
                                }),
                            }
                        }
                    },
                    Err((error_code, _safe_message)) if error_code == "CANCELLED" => {
                        events.push(AgentEvent::Cancelled {
                            payload: cancellation_payload(&request.response_contract, round_text),
                        });
                    }
                    Err((error_code, safe_message)) => {
                        events.push(AgentEvent::Failed {
                            error_code,
                            safe_message,
                        });
                    }
                }
                return Ok(AgentLoopOutcome {
                    events,
                    grounding_trace: trace,
                });
            }

            let calls = tool_calls.ok_or_else(|| {
                loop_error(
                    "AGENT_ROUND_TERMINAL_MISSING",
                    "provider round가 terminal event 없이 종료되었습니다.",
                )
            })?;
            if request.profile.provider != ProviderKind::LocalMock {
                return Err(loop_error(
                    "TOOL_EGRESS_CONSENT_REQUIRED",
                    "외부 provider tool result는 durable consent/receipt 없이 전송할 수 없습니다.",
                ));
            }
            let gateway = self.gateway.as_ref().ok_or_else(|| {
                loop_error(
                    "REPOSITORY_TOOL_UNAVAILABLE",
                    "repository tool gateway가 없습니다.",
                )
            })?;
            total_calls = total_calls
                .checked_add(u16::try_from(calls.len()).unwrap_or(u16::MAX))
                .ok_or_else(|| loop_error("AGENT_LOOP_LIMIT_REACHED", "tool call 수 overflow"))?;
            if total_calls > request.limits.max_tool_calls {
                return Err(loop_error(
                    "AGENT_LOOP_LIMIT_REACHED",
                    "turn tool call 한도를 초과했습니다.",
                ));
            }
            request.messages.push(AgentMessage {
                role: mentat_inference::AgentRole::Assistant,
                content: AgentMessageContent::ToolCalls(calls.clone()),
            });
            let call_count = u16::try_from(calls.len()).unwrap_or(u16::MAX);
            for (index, call) in calls.into_iter().enumerate() {
                let fingerprint = call_fingerprint(&call)?;
                let repeated = fingerprints.entry(fingerprint.clone()).or_insert(0);
                *repeated += 1;
                if *repeated >= 3 {
                    return Err(loop_error(
                        "AGENT_LOOP_REPEATED_CALL",
                        "동일 tool/arguments/snapshot 호출이 3회 반복되었습니다.",
                    ));
                }
                let result = gateway.execute(call.clone(), cancel.clone()).await?;
                total_result_bytes = total_result_bytes
                    .checked_add(result.content_bytes)
                    .ok_or_else(|| {
                        loop_error("TOOL_RESULT_LIMIT_REACHED", "tool result byte overflow")
                    })?;
                if total_result_bytes > request.limits.max_tool_result_bytes {
                    return Err(loop_error(
                        "TOOL_RESULT_LIMIT_REACHED",
                        "turn tool result byte 한도를 초과했습니다.",
                    ));
                }
                if let Some(trace) = trace.as_mut() {
                    let source_ref_ids =
                        result.source_refs.iter().map(|source| source.id).collect();
                    trace.source_refs.extend(result.source_refs.clone());
                    trace.tool_calls.push(RepositoryToolCallRecord {
                        trace_id: trace.id,
                        call_id: call.call_id,
                        round,
                        name: call.name,
                        canonical_arguments_digest: fingerprint,
                        result_digest: Some(sha256_hex(result.content.as_bytes())),
                        content_bytes: result.content_bytes,
                        source_ref_ids,
                        status: RepositoryToolCallStatus::Completed,
                    });
                }
                request.messages.push(AgentMessage {
                    role: mentat_inference::AgentRole::Tool,
                    content: AgentMessageContent::ToolResult(result),
                });
                events.push(AgentEvent::ToolProgress {
                    round,
                    completed_calls: u16::try_from(index + 1).unwrap_or(u16::MAX),
                    total_calls: call_count,
                });
            }
        }
        Err(loop_error(
            "AGENT_LOOP_LIMIT_REACHED",
            "AgentLoop round 한도를 초과했습니다.",
        ))
    }
}

fn cancellation_payload(
    contract: &mentat_core::ResponseContract,
    partial: String,
) -> CancelledPayload {
    match contract {
        mentat_core::ResponseContract::AdvisorMarkdown => {
            CancelledPayload::AdvisorPartialMarkdown(partial)
        }
        mentat_core::ResponseContract::AuditAnswerBundle { .. } => CancelledPayload::AuditNoContent,
    }
}

pub fn validate_audit_bundle(
    raw: &str,
    request_id: Uuid,
    trace: Option<&GroundingTrace>,
) -> Result<mentat_core::AnswerBundle, MentatError> {
    let trace = trace.ok_or_else(|| {
        loop_error(
            "AUDIT_RESPONSE_INVALID",
            "Audit response에 결속할 GroundingTrace가 없습니다.",
        )
    })?;
    let snapshot_id = trace.snapshot_id.ok_or_else(|| {
        loop_error(
            "AUDIT_RESPONSE_INVALID",
            "Audit GroundingTrace에 snapshot이 없습니다.",
        )
    })?;
    let mut bundle: mentat_core::AnswerBundle = serde_json::from_str(raw)
        .map_err(|error| loop_error("AUDIT_RESPONSE_INVALID", &error.to_string()))?;
    if bundle.snapshot_id != snapshot_id || bundle.direct_answer.trim().is_empty() {
        return Err(loop_error(
            "AUDIT_RESPONSE_INVALID",
            "Audit snapshot/direct_answer contract가 일치하지 않습니다.",
        ));
    }
    let catalog: HashMap<Uuid, &mentat_core::SourceRef> = trace
        .source_refs
        .iter()
        .map(|source| (source.id, source))
        .collect();
    let mut seen_evidence = std::collections::HashSet::new();
    for evidence in &bundle.evidence_map {
        if !seen_evidence.insert(evidence.id) {
            return Err(loop_error(
                "AUDIT_RESPONSE_INVALID",
                "Audit evidence ID가 중복되었습니다.",
            ));
        }
        let source = catalog.get(&evidence.id).ok_or_else(|| {
            loop_error(
                "AUDIT_RESPONSE_INVALID",
                "Tool Gateway catalog에 없는 evidence ID입니다.",
            )
        })?;
        if evidence.snapshot_id != source.snapshot_id
            || evidence.relative_path != source.relative_path
            || evidence.line_start != source.line_start
            || evidence.line_end != source.line_end
            || evidence.content_hash != source.content_hash
            || evidence.excerpt != source.excerpt
        {
            return Err(loop_error(
                "AUDIT_RESPONSE_INVALID",
                "Audit evidence가 canonical SourceRef와 일치하지 않습니다.",
            ));
        }
    }
    for claim in &bundle.claims {
        let evidence_required = claim.classification != mentat_core::ClaimClassification::Unknown;
        if claim.statement.trim().is_empty()
            || !claim.confidence.is_finite()
            || !(0.0..=1.0).contains(&claim.confidence)
            || (evidence_required && claim.evidence_ids.is_empty())
            || claim
                .evidence_ids
                .iter()
                .any(|id| !seen_evidence.contains(id))
        {
            return Err(loop_error(
                "AUDIT_RESPONSE_INVALID",
                "Audit claim invariant가 유효하지 않습니다.",
            ));
        }
    }
    for conflict in &bundle.conflicts {
        let unique: std::collections::HashSet<_> = conflict.evidence_ids.iter().collect();
        if conflict.evidence_ids.is_empty()
            || unique.len() != conflict.evidence_ids.len()
            || conflict
                .evidence_ids
                .iter()
                .any(|id| !seen_evidence.contains(id))
        {
            return Err(loop_error(
                "AUDIT_RESPONSE_INVALID",
                "Audit conflict evidence invariant가 유효하지 않습니다.",
            ));
        }
    }
    bundle.request_id = request_id;
    bundle.raw_model_response = None;
    Ok(bundle)
}

fn call_fingerprint(call: &mentat_core::RepositoryToolCall) -> Result<String, MentatError> {
    let encoded = serde_json::to_vec(&(call.name, &call.arguments, call.snapshot_id))
        .map_err(|error| loop_error("AGENT_TOOL_SCHEMA_INVALID", &error.to_string()))?;
    Ok(sha256_hex(&encoded))
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn loop_error(code: &str, message: &str) -> MentatError {
    MentatError::BackendError {
        code: code.to_string(),
        message: message.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mentat_core::{
        RepositoryReader, RepositoryToolArguments, RepositoryToolCall, RepositoryToolName,
    };
    use mentat_inference::{
        AgentLimits, AgentMessage, BackendProfile, FakeInferenceBackend, RepositoryContext,
    };
    use mentat_repository::ReadOnlySession;
    use std::collections::VecDeque;
    use tempfile::tempdir;

    #[tokio::test]
    async fn local_agent_loop_executes_tool_then_returns_grounded_markdown() {
        let dir = tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        std::fs::write(dir.path().join("src/lib.rs"), "pub fn mentor() {}\n").unwrap();
        let session = Arc::new(ReadOnlySession::open(dir.path()).unwrap());
        let files = session.scan_files().await.unwrap();
        let snapshot = session.create_snapshot_from_files(&files);
        let call = RepositoryToolCall {
            call_id: Uuid::new_v4(),
            snapshot_id: snapshot.id,
            name: RepositoryToolName::SearchText,
            arguments: RepositoryToolArguments::SearchText {
                query: "mentor".to_string(),
                path_filter: None,
                limit: 10,
            },
        };
        let backend = Arc::new(FakeInferenceBackend {
            scripted_rounds: Arc::new(std::sync::Mutex::new(VecDeque::from([
                vec![InferenceRoundEvent::ToolCallsRequested {
                    round: 1,
                    calls: vec![call],
                }],
                vec![
                    InferenceRoundEvent::TextDelta("`mentor`를 확인했습니다.".to_string()),
                    InferenceRoundEvent::RawCompleted {
                        full_text: "`mentor`를 확인했습니다.".to_string(),
                    },
                ],
            ]))),
            ..Default::default()
        });
        let gateway = Arc::new(RepositoryToolGateway::new(session, snapshot.clone(), files));
        let profile = BackendProfile {
            provider: ProviderKind::LocalMock,
            model: "fixture-local".to_string(),
            ..Default::default()
        };
        let request = AgentRequest {
            request_id: Uuid::new_v4(),
            conversation_id: Uuid::new_v4(),
            turn_id: Uuid::new_v4(),
            profile,
            effective_system_prompt: "system".to_string(),
            messages: vec![AgentMessage::user("mentor 구현을 찾아줘")],
            tools: Vec::new(),
            repository_context: Some(RepositoryContext {
                repository_id: snapshot.repo_id,
                snapshot_id: snapshot.id,
                snapshot_status: snapshot.status,
                tools_available: true,
                display_name: "fixture".to_string(),
            }),
            response_contract: mentat_core::ResponseContract::AdvisorMarkdown,
            limits: AgentLimits::default(),
        };

        let outcome = AgentLoop::new(backend, Some(gateway))
            .run(request, CancellationToken::new())
            .await
            .unwrap();

        assert!(matches!(
            outcome.events.last(),
            Some(AgentEvent::Completed {
                payload: CompletedPayload::AdvisorMarkdown(text),
                ..
            }) if text.contains("mentor")
        ));
        assert_eq!(outcome.grounding_trace.unwrap().source_refs.len(), 1);
    }

    #[test]
    fn audit_validator_accepts_only_gateway_catalog_evidence() {
        let snapshot_id = Uuid::new_v4();
        let evidence_id = Uuid::new_v4();
        let source = mentat_core::SourceRef {
            id: evidence_id,
            snapshot_id,
            relative_path: std::path::PathBuf::from("src/lib.rs"),
            line_start: 1,
            line_end: 1,
            content_hash: "hash".to_string(),
            excerpt: "pub fn mentor() {}".to_string(),
        };
        let trace = GroundingTrace {
            id: Uuid::new_v4(),
            conversation_id: Uuid::new_v4(),
            turn_id: Uuid::new_v4(),
            snapshot_id: Some(snapshot_id),
            tool_calls: Vec::new(),
            source_refs: vec![source],
            egress_receipt_ids: Vec::new(),
            freshness: GroundingFreshness::FreshAtSend,
        };
        let raw = serde_json::json!({
            "request_id": Uuid::new_v4(),
            "snapshot_id": snapshot_id,
            "direct_answer": "확인됨",
            "claims": [{
                "id": Uuid::new_v4(),
                "classification": "Observed",
                "statement": "mentor 함수가 있다",
                "confidence": 1.0,
                "evidence_ids": [evidence_id],
                "rationale": null
            }],
            "evidence_map": [{
                "id": evidence_id,
                "snapshot_id": snapshot_id,
                "relative_path": "src/lib.rs",
                "line_start": 1,
                "line_end": 1,
                "content_hash": "hash",
                "excerpt": "pub fn mentor() {}"
            }],
            "recommendations": [],
            "conflicts": [],
            "raw_model_response": "must be removed"
        })
        .to_string();

        let validated = validate_audit_bundle(&raw, Uuid::new_v4(), Some(&trace)).unwrap();
        assert_eq!(validated.claims.len(), 1);
        assert!(validated.raw_model_response.is_none());

        let tampered = raw.replacen("\"content_hash\":\"hash\"", "\"content_hash\":\"wrong\"", 1);
        assert!(validate_audit_bundle(&tampered, Uuid::new_v4(), Some(&trace)).is_err());

        let missing_evidence = serde_json::json!({
            "request_id": Uuid::new_v4(),
            "snapshot_id": snapshot_id,
            "direct_answer": "근거 없음",
            "claims": [{
                "id": Uuid::new_v4(),
                "classification": "Observed",
                "statement": "근거 없는 관찰",
                "confidence": 1.0,
                "evidence_ids": [],
                "rationale": null
            }],
            "evidence_map": [],
            "recommendations": [],
            "conflicts": [],
            "raw_model_response": null
        })
        .to_string();
        assert!(validate_audit_bundle(&missing_evidence, Uuid::new_v4(), Some(&trace)).is_err());
    }
}
