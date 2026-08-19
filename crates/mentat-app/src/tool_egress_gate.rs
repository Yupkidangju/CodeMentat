use mentat_analysis::tool_egress::{
    RuntimeConsentCapability, ToolEgressEnvelope, ToolEgressSealer,
};
use mentat_core::{CanonicalToolRef, MentatError, ProviderBinding, ToolEgressStatus};
use mentat_inference::{AgentMessageContent, AgentRequest, AgentRole, ProviderBodyEgressGate};
use mentat_storage::SqliteStorage;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::Mutex;
use uuid::Uuid;

pub struct DurableToolEgressGate {
    storage: SqliteStorage,
    capability: RuntimeConsentCapability,
    trace_id: Uuid,
    receipt_ids: Mutex<Vec<Uuid>>,
}

impl DurableToolEgressGate {
    pub fn new(
        storage: SqliteStorage,
        capability: RuntimeConsentCapability,
        trace_id: Uuid,
    ) -> Self {
        Self {
            storage,
            capability,
            trace_id,
            receipt_ids: Mutex::new(Vec::new()),
        }
    }
}

impl ProviderBodyEgressGate for DurableToolEgressGate {
    fn authorize_exact_body(
        &self,
        request: &AgentRequest,
        endpoint_identity: &str,
        exact_provider_body: &[u8],
    ) -> Result<Vec<Uuid>, MentatError> {
        let context = request.repository_context.as_ref().ok_or_else(|| {
            gate_error(
                "TOOL_EGRESS_CONTEXT_REQUIRED",
                "tool egress에 repository context가 없습니다.",
            )
        })?;
        let provider_binding = ProviderBinding::new(
            request.profile.id,
            format!("{:?}", request.profile.provider),
            endpoint_identity,
            request.profile.model.clone(),
        )?;
        let call_names = request
            .messages
            .iter()
            .filter_map(|message| match (&message.role, &message.content) {
                (AgentRole::Assistant, AgentMessageContent::ToolCalls(calls)) => Some(calls),
                _ => None,
            })
            .flatten()
            .map(|call| (call.call_id, call.name))
            .collect::<HashMap<_, _>>();
        let results =
            request
                .messages
                .iter()
                .filter_map(|message| match (&message.role, &message.content) {
                    (AgentRole::Tool, AgentMessageContent::ToolResult(result)) => Some(result),
                    _ => None,
                });
        let mut prepared = Vec::new();
        for result in results {
            if result.snapshot_id != context.snapshot_id {
                self.fail_prepared(&prepared);
                return Err(gate_error(
                    "TOOL_EGRESS_SNAPSHOT_MISMATCH",
                    "tool result snapshot과 request context가 다릅니다.",
                ));
            }
            let tool_name = match call_names.get(&result.call_id).copied() {
                Some(name) => name,
                None => {
                    self.fail_prepared(&prepared);
                    return Err(gate_error(
                        "TOOL_EGRESS_CALL_MISSING",
                        "tool result에 대응하는 허용된 tool call이 없습니다.",
                    ));
                }
            };
            let semantic_payload = match serde_json::to_vec(result) {
                Ok(payload) => payload,
                Err(error) => {
                    self.fail_prepared(&prepared);
                    return Err(gate_error(
                        "TOOL_EGRESS_PAYLOAD_ENCODE_FAILED",
                        &error.to_string(),
                    ));
                }
            };
            let refs = result
                .source_refs
                .iter()
                .map(|source| CanonicalToolRef {
                    relative_path: source.relative_path.clone(),
                    line_start: source.line_start,
                    line_end: source.line_end,
                    content_hash: source.content_hash.clone(),
                    redacted_payload_digest: sha256_hex(source.excerpt.as_bytes()),
                })
                .collect();
            let envelope = ToolEgressEnvelope {
                trace_id: self.trace_id,
                conversation_id: request.conversation_id,
                turn_id: request.turn_id,
                tool_call_id: result.call_id,
                repository_id: context.repository_id,
                snapshot_id: context.snapshot_id,
                tool_name,
                refs,
                provider_binding: provider_binding.clone(),
                semantic_payload,
                exact_provider_body: exact_provider_body.to_vec(),
            };
            let receipt = match ToolEgressSealer::prepare(&self.capability, &envelope) {
                Ok(receipt) => receipt,
                Err(error) => {
                    self.fail_prepared(&prepared);
                    return Err(error);
                }
            };
            if let Err(error) = ToolEgressSealer::verify_exact_body(&receipt, exact_provider_body) {
                self.fail_prepared(&prepared);
                return Err(error);
            }
            if let Err(error) = self.storage.prepare_tool_egress_receipt(&receipt) {
                self.fail_prepared(&prepared);
                return Err(error);
            }
            prepared.push(receipt.id);
        }
        if prepared.is_empty() {
            return Err(gate_error(
                "TOOL_EGRESS_RESULT_REQUIRED",
                "승인할 tool result가 없습니다.",
            ));
        }
        match self.receipt_ids.lock() {
            Ok(mut receipt_ids) => receipt_ids.extend(prepared.iter().copied()),
            Err(_) => {
                self.fail_prepared(&prepared);
                return Err(gate_error(
                    "TOOL_EGRESS_LOCK_FAILED",
                    "receipt 잠금이 손상되었습니다.",
                ));
            }
        }
        Ok(prepared)
    }

    fn finish(&self, receipt_ids: &[Uuid], status: ToolEgressStatus) -> Result<(), MentatError> {
        for id in receipt_ids {
            self.storage.compare_and_set_tool_egress_status(
                *id,
                ToolEgressStatus::Prepared,
                status,
            )?;
        }
        Ok(())
    }

    fn receipt_ids(&self) -> Result<Vec<Uuid>, MentatError> {
        self.receipt_ids
            .lock()
            .map(|ids| ids.clone())
            .map_err(|_| gate_error("TOOL_EGRESS_LOCK_FAILED", "receipt 잠금이 손상되었습니다."))
    }
}

impl DurableToolEgressGate {
    fn fail_prepared(&self, ids: &[Uuid]) {
        for id in ids {
            let _ = self.storage.compare_and_set_tool_egress_status(
                *id,
                ToolEgressStatus::Prepared,
                ToolEgressStatus::Failed,
            );
        }
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn gate_error(code: &str, message: &str) -> MentatError {
    MentatError::BackendError {
        code: code.to_string(),
        message: message.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mentat_core::{
        ChatMessage, ChatRole, ConversationPersistence, ConversationTurn, ExperiencePreset,
        GroundingFreshness, GroundingTrace, MessageStatus, NewConversation, RepositoryConsentKind,
        RepositoryConsentScope, RepositoryToolArguments, RepositoryToolCall, RepositoryToolName,
        RepositoryToolResult, ResponseContract, SystemPreset, TurnStart,
    };
    use mentat_inference::{
        AgentLimits, AgentMessage, AgentMessageContent, AgentRole, BackendProfile, ProviderKind,
        RepositoryContext,
    };
    use mentat_storage::FactoryPromptSeed;
    use std::sync::Arc;
    use tempfile::tempdir;

    #[test]
    fn durable_gate_prepares_verifies_and_finishes_exact_body_receipt() {
        let dir = tempdir().unwrap();
        let storage = SqliteStorage::open(dir.path().join("mentat.db")).unwrap();
        let prompt_profile_id = Uuid::new_v4();
        let prompt = storage
            .seed_factory_prompt_profile(&FactoryPromptSeed {
                profile_id: prompt_profile_id,
                profile_name: "fixture".to_string(),
                experience_preset: ExperiencePreset::Intermediate,
                base_system_preset: SystemPreset::Intermediate,
                system_resource_key: "system.intermediate.v1".to_string(),
                system_resource_version: "v1".to_string(),
                system_checksum: "system".to_string(),
                persona_resource_key: "persona.default_analyst.v1".to_string(),
                persona_resource_version: "v1".to_string(),
                persona_checksum: "persona".to_string(),
            })
            .unwrap();
        let repository_id = Uuid::new_v4();
        let snapshot_id = Uuid::new_v4();
        let conversation = storage
            .create_conversation(&NewConversation {
                repository_id: Some(repository_id),
                active_snapshot_id: Some(snapshot_id),
                prompt_profile_id,
                persistence: ConversationPersistence::Durable,
            })
            .unwrap();
        let turn_id = Uuid::new_v4();
        let assistant = ChatMessage::new(
            conversation.id,
            turn_id,
            ChatRole::Assistant,
            1,
            "",
            MessageStatus::Pending,
        );
        storage
            .begin_turn(&TurnStart {
                turn: ConversationTurn {
                    id: turn_id,
                    conversation_id: conversation.id,
                    sequence: 1,
                    prompt_profile_id,
                    prompt_profile_revision_id: prompt.active_revision_id,
                    kernel_version: "kernel.v1".to_string(),
                    kernel_digest: "kernel".to_string(),
                    snapshot_id: Some(snapshot_id),
                    response_contract: ResponseContract::AdvisorMarkdown,
                    audit_result_id: None,
                    started_at: chrono::Utc::now(),
                    completed_at: None,
                },
                user_message: ChatMessage::new(
                    conversation.id,
                    turn_id,
                    ChatRole::User,
                    0,
                    "status",
                    MessageStatus::Completed,
                ),
                assistant_placeholder: assistant,
            })
            .unwrap();
        let trace_id = Uuid::new_v4();
        storage
            .prepare_grounding_trace(&GroundingTrace {
                id: trace_id,
                conversation_id: conversation.id,
                turn_id,
                snapshot_id: Some(snapshot_id),
                tool_calls: Vec::new(),
                source_refs: Vec::new(),
                egress_receipt_ids: Vec::new(),
                freshness: GroundingFreshness::FreshAtSend,
            })
            .unwrap();
        let profile = BackendProfile {
            id: Uuid::new_v4(),
            provider: ProviderKind::OpenAICompatible,
            base_url: "https://api.example.com/v1".to_string(),
            model: "dynamic".to_string(),
            api_key: Some("fixture-key".to_string()),
            ..Default::default()
        };
        let endpoint = "https://api.example.com/v1/chat/completions";
        let binding = ProviderBinding::new(
            profile.id,
            "OpenAICompatible",
            endpoint,
            profile.model.clone(),
        )
        .unwrap();
        let scope = RepositoryConsentScope {
            id: Uuid::new_v4(),
            conversation_id: conversation.id,
            repository_id,
            snapshot_id,
            provider_binding: binding,
            kind: RepositoryConsentKind::RepositorySession,
            granted_at: chrono::Utc::now(),
            revoked_at: None,
        };
        storage.save_repository_consent_scope(&scope).unwrap();
        let call_id = Uuid::new_v4();
        let request = mentat_inference::AgentRequest {
            request_id: Uuid::new_v4(),
            conversation_id: conversation.id,
            turn_id,
            profile,
            effective_system_prompt: "system".to_string(),
            messages: vec![
                AgentMessage::user("status"),
                AgentMessage {
                    role: AgentRole::Assistant,
                    content: AgentMessageContent::ToolCalls(vec![RepositoryToolCall {
                        call_id,
                        snapshot_id,
                        name: RepositoryToolName::RepoStatus,
                        arguments: RepositoryToolArguments::RepoStatus,
                    }]),
                },
                AgentMessage {
                    role: AgentRole::Tool,
                    content: AgentMessageContent::ToolResult(RepositoryToolResult {
                        call_id,
                        snapshot_id,
                        content: "ready".to_string(),
                        source_refs: Vec::new(),
                        omissions: Vec::new(),
                        content_bytes: 5,
                    }),
                },
            ],
            tools: Vec::new(),
            repository_context: Some(RepositoryContext {
                repository_id,
                snapshot_id,
                snapshot_status: mentat_core::SnapshotStatus::Ready,
                tools_available: true,
                display_name: "fixture".to_string(),
            }),
            response_contract: ResponseContract::AdvisorMarkdown,
            limits: AgentLimits::default(),
        };
        let gate = Arc::new(DurableToolEgressGate::new(
            storage.clone(),
            RuntimeConsentCapability::new(scope),
            trace_id,
        ));
        let body = br#"{"messages":[{"role":"tool","content":"ready"}]}"#;
        let ids = gate.authorize_exact_body(&request, endpoint, body).unwrap();
        assert_eq!(ids.len(), 1);
        let receipt = storage.load_tool_egress_receipt(ids[0]).unwrap().unwrap();
        ToolEgressSealer::verify_exact_body(&receipt, body).unwrap();
        gate.finish(&ids, ToolEgressStatus::Sent).unwrap();
        assert_eq!(
            storage
                .load_tool_egress_receipt(ids[0])
                .unwrap()
                .unwrap()
                .status,
            ToolEgressStatus::Sent
        );
    }
}
