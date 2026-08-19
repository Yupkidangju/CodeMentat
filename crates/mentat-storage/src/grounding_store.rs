use crate::db::{parse_datetime, parse_uuid, storage_error, SqliteStorage};
use mentat_core::{
    GroundingTrace, MentatError, ProviderBinding, RepositoryConsentKind, RepositoryConsentScope,
    RepositoryToolName, ToolEgressReceipt, ToolEgressStatus,
};
use rusqlite::{params, OptionalExtension, TransactionBehavior};
use uuid::Uuid;

impl SqliteStorage {
    pub fn prepare_grounding_trace(&self, trace: &GroundingTrace) -> Result<(), MentatError> {
        let mut conn = self.lock_conn()?;
        let transaction = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| storage_error("GROUNDING_TRACE_BEGIN_FAILED", &error.to_string()))?;
        transaction
            .execute(
                "INSERT INTO grounding_traces (
                    id, conversation_id, turn_id, snapshot_id, freshness, created_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    trace.id.to_string(),
                    trace.conversation_id.to_string(),
                    trace.turn_id.to_string(),
                    trace.snapshot_id.map(|id| id.to_string()),
                    serde_json::to_string(&trace.freshness).map_err(|error| storage_error(
                        "GROUNDING_FRESHNESS_ENCODE_FAILED",
                        &error.to_string()
                    ))?,
                    chrono::Utc::now().to_rfc3339(),
                ],
            )
            .map_err(|error| storage_error("GROUNDING_TRACE_INSERT_FAILED", &error.to_string()))?;
        for record in &trace.tool_calls {
            transaction
                .execute(
                    "INSERT INTO tool_call_records (
                        trace_id, call_id, round, tool_name, canonical_arguments_digest,
                        result_digest, content_bytes, source_ref_ids, status
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                    params![
                        trace.id.to_string(),
                        record.call_id.to_string(),
                        i64::from(record.round),
                        record.name.wire_name(),
                        record.canonical_arguments_digest,
                        record.result_digest,
                        i64::from(record.content_bytes),
                        serde_json::to_string(&record.source_ref_ids).map_err(|error| {
                            storage_error("GROUNDING_SOURCE_IDS_ENCODE_FAILED", &error.to_string())
                        })?,
                        format!("{:?}", record.status),
                    ],
                )
                .map_err(|error| {
                    storage_error("GROUNDING_TOOL_RECORD_INSERT_FAILED", &error.to_string())
                })?;
        }
        for (ordinal, source) in trace.source_refs.iter().enumerate() {
            transaction
                .execute(
                    "INSERT INTO source_refs (
                        id, trace_id, ordinal, snapshot_id, relative_path,
                        line_start, line_end, content_hash, excerpt
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                    params![
                        source.id.to_string(),
                        trace.id.to_string(),
                        i64::try_from(ordinal).map_err(|_| storage_error(
                            "GROUNDING_ORDINAL_OVERFLOW",
                            "SourceRef ordinal이 SQLite 범위를 초과했습니다."
                        ))?,
                        source.snapshot_id.to_string(),
                        source.relative_path.to_string_lossy().replace('\\', "/"),
                        i64::try_from(source.line_start).map_err(|_| storage_error(
                            "GROUNDING_LINE_OVERFLOW",
                            "SourceRef line_start가 SQLite 범위를 초과했습니다."
                        ))?,
                        i64::try_from(source.line_end).map_err(|_| storage_error(
                            "GROUNDING_LINE_OVERFLOW",
                            "SourceRef line_end가 SQLite 범위를 초과했습니다."
                        ))?,
                        source.content_hash,
                        source.excerpt,
                    ],
                )
                .map_err(|error| {
                    storage_error("GROUNDING_SOURCE_INSERT_FAILED", &error.to_string())
                })?;
        }
        transaction
            .commit()
            .map_err(|error| storage_error("GROUNDING_TRACE_COMMIT_FAILED", &error.to_string()))?;
        Ok(())
    }

    pub fn save_repository_consent_scope(
        &self,
        scope: &RepositoryConsentScope,
    ) -> Result<(), MentatError> {
        let (kind, request_once_turn_id) = match scope.kind {
            RepositoryConsentKind::RequestOnce { turn_id } => {
                ("RequestOnce", Some(turn_id.to_string()))
            }
            RepositoryConsentKind::RepositorySession => ("RepositorySession", None),
        };
        let conn = self.lock_conn()?;
        conn.execute(
            "INSERT INTO repository_consent_scopes (
                id, conversation_id, repository_id, snapshot_id, provider_binding,
                consent_kind, request_once_turn_id, granted_at, revoked_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                scope.id.to_string(),
                scope.conversation_id.to_string(),
                scope.repository_id.to_string(),
                scope.snapshot_id.to_string(),
                serde_json::to_string(&scope.provider_binding).map_err(|error| storage_error(
                    "CONSENT_BINDING_ENCODE_FAILED",
                    &error.to_string()
                ))?,
                kind,
                request_once_turn_id,
                scope.granted_at.to_rfc3339(),
                scope.revoked_at.map(|value| value.to_rfc3339()),
            ],
        )
        .map_err(|error| storage_error("CONSENT_SCOPE_INSERT_FAILED", &error.to_string()))?;
        Ok(())
    }

    pub fn prepare_tool_egress_receipt(
        &self,
        receipt: &ToolEgressReceipt,
    ) -> Result<(), MentatError> {
        if receipt.status != ToolEgressStatus::Prepared {
            return Err(storage_error(
                "TOOL_EGRESS_RECEIPT_NOT_PREPARED",
                "새 receipt는 Prepared 상태로만 저장할 수 있습니다.",
            ));
        }
        let conn = self.lock_conn()?;
        conn.execute(
            "INSERT INTO tool_egress_receipts (
                id, seal_version, trace_id, consent_scope_id, conversation_id,
                turn_id, tool_call_id, repository_id, snapshot_id, tool_name,
                canonical_refs, provider_binding, semantic_payload_digest,
                exact_provider_body_digest, canonical_digest, status, prepared_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12,
                       ?13, ?14, ?15, 'Prepared', ?16, ?17)",
            params![
                receipt.id.to_string(),
                receipt.seal_version,
                receipt.trace_id.to_string(),
                receipt.consent_scope_id.to_string(),
                receipt.conversation_id.to_string(),
                receipt.turn_id.to_string(),
                receipt.tool_call_id.to_string(),
                receipt.repository_id.to_string(),
                receipt.snapshot_id.to_string(),
                receipt.tool_name.wire_name(),
                serde_json::to_string(&receipt.canonical_refs).map_err(|error| storage_error(
                    "TOOL_EGRESS_REFS_ENCODE_FAILED",
                    &error.to_string()
                ))?,
                serde_json::to_string(&receipt.provider_binding).map_err(|error| storage_error(
                    "TOOL_EGRESS_BINDING_ENCODE_FAILED",
                    &error.to_string()
                ))?,
                receipt.semantic_payload_digest,
                receipt.exact_provider_body_digest,
                receipt.canonical_digest,
                receipt.prepared_at.to_rfc3339(),
                receipt.updated_at.to_rfc3339(),
            ],
        )
        .map_err(|error| storage_error("TOOL_EGRESS_RECEIPT_INSERT_FAILED", &error.to_string()))?;
        Ok(())
    }

    pub fn compare_and_set_tool_egress_status(
        &self,
        id: Uuid,
        expected: ToolEgressStatus,
        next: ToolEgressStatus,
    ) -> Result<(), MentatError> {
        if expected != ToolEgressStatus::Prepared
            || !matches!(
                next,
                ToolEgressStatus::Sent
                    | ToolEgressStatus::Failed
                    | ToolEgressStatus::OutcomeUnknown
            )
        {
            return Err(storage_error(
                "TOOL_EGRESS_STATUS_TRANSITION_INVALID",
                "receipt status는 Prepared에서 terminal 상태로 한 번만 전이할 수 있습니다.",
            ));
        }
        let conn = self.lock_conn()?;
        let changed = conn
            .execute(
                "UPDATE tool_egress_receipts SET status = ?1, updated_at = ?2
                 WHERE id = ?3 AND status = ?4",
                params![
                    egress_status_text(next),
                    chrono::Utc::now().to_rfc3339(),
                    id.to_string(),
                    egress_status_text(expected),
                ],
            )
            .map_err(|error| {
                storage_error("TOOL_EGRESS_STATUS_UPDATE_FAILED", &error.to_string())
            })?;
        if changed != 1 {
            return Err(storage_error(
                "TOOL_EGRESS_STATUS_CONFLICT",
                "receipt가 없거나 이미 terminal 상태입니다.",
            ));
        }
        Ok(())
    }

    pub fn load_tool_egress_receipt(
        &self,
        id: Uuid,
    ) -> Result<Option<ToolEgressReceipt>, MentatError> {
        let conn = self.lock_conn()?;
        let values = conn
            .query_row(
                "SELECT id, seal_version, trace_id, consent_scope_id, conversation_id,
                        turn_id, tool_call_id, repository_id, snapshot_id, tool_name,
                        canonical_refs, provider_binding, semantic_payload_digest,
                        exact_provider_body_digest, canonical_digest, status, prepared_at, updated_at
                 FROM tool_egress_receipts WHERE id = ?1",
                [id.to_string()],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, String>(6)?,
                        row.get::<_, String>(7)?,
                        row.get::<_, String>(8)?,
                        row.get::<_, String>(9)?,
                        row.get::<_, String>(10)?,
                        row.get::<_, String>(11)?,
                        row.get::<_, String>(12)?,
                        row.get::<_, String>(13)?,
                        row.get::<_, String>(14)?,
                        row.get::<_, String>(15)?,
                        row.get::<_, String>(16)?,
                        row.get::<_, String>(17)?,
                    ))
                },
            )
            .optional()
            .map_err(|error| storage_error("TOOL_EGRESS_RECEIPT_READ_FAILED", &error.to_string()))?;
        let Some(values) = values else {
            return Ok(None);
        };
        Ok(Some(ToolEgressReceipt {
            id: parse_uuid(&values.0, "tool_egress_receipts.id")?,
            seal_version: values.1,
            trace_id: parse_uuid(&values.2, "tool_egress_receipts.trace_id")?,
            consent_scope_id: parse_uuid(&values.3, "tool_egress_receipts.consent_scope_id")?,
            conversation_id: parse_uuid(&values.4, "tool_egress_receipts.conversation_id")?,
            turn_id: parse_uuid(&values.5, "tool_egress_receipts.turn_id")?,
            tool_call_id: parse_uuid(&values.6, "tool_egress_receipts.tool_call_id")?,
            repository_id: parse_uuid(&values.7, "tool_egress_receipts.repository_id")?,
            snapshot_id: parse_uuid(&values.8, "tool_egress_receipts.snapshot_id")?,
            tool_name: parse_tool_name(&values.9)?,
            canonical_refs: serde_json::from_str(&values.10).map_err(|error| {
                storage_error("TOOL_EGRESS_REFS_DECODE_FAILED", &error.to_string())
            })?,
            provider_binding: serde_json::from_str::<ProviderBinding>(&values.11).map_err(
                |error| storage_error("TOOL_EGRESS_BINDING_DECODE_FAILED", &error.to_string()),
            )?,
            semantic_payload_digest: values.12,
            exact_provider_body_digest: values.13,
            canonical_digest: values.14,
            status: parse_egress_status(&values.15)?,
            prepared_at: parse_datetime(&values.16, "tool_egress_receipts.prepared_at")?,
            updated_at: parse_datetime(&values.17, "tool_egress_receipts.updated_at")?,
        }))
    }
}

fn parse_tool_name(value: &str) -> Result<RepositoryToolName, MentatError> {
    RepositoryToolName::ALL
        .into_iter()
        .find(|name| name.wire_name() == value)
        .ok_or_else(|| {
            storage_error(
                "STORAGE_DECODE_ENUM",
                "tool_egress_receipts.tool_name 값이 유효하지 않습니다.",
            )
        })
}

fn egress_status_text(status: ToolEgressStatus) -> &'static str {
    match status {
        ToolEgressStatus::Prepared => "Prepared",
        ToolEgressStatus::Sent => "Sent",
        ToolEgressStatus::Failed => "Failed",
        ToolEgressStatus::OutcomeUnknown => "OutcomeUnknown",
    }
}

fn parse_egress_status(value: &str) -> Result<ToolEgressStatus, MentatError> {
    match value {
        "Prepared" => Ok(ToolEgressStatus::Prepared),
        "Sent" => Ok(ToolEgressStatus::Sent),
        "Failed" => Ok(ToolEgressStatus::Failed),
        "OutcomeUnknown" => Ok(ToolEgressStatus::OutcomeUnknown),
        _ => Err(storage_error(
            "STORAGE_DECODE_ENUM",
            "tool_egress_receipts.status 값이 유효하지 않습니다.",
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FactoryPromptSeed;
    use mentat_core::{
        ChatMessage, ChatRole, ConversationPersistence, ConversationTurn, ExperiencePreset,
        GroundingFreshness, MessageStatus, NewConversation, ResponseContract, SystemPreset,
        TurnStart,
    };
    use tempfile::tempdir;

    #[test]
    fn prepared_receipt_is_durable_and_status_transition_is_compare_and_set() {
        let dir = tempdir().unwrap();
        let storage = SqliteStorage::open(dir.path().join("mentat.db")).unwrap();
        let profile_id = Uuid::new_v4();
        let profile = storage
            .seed_factory_prompt_profile(&FactoryPromptSeed {
                profile_id,
                profile_name: "fixture".to_string(),
                experience_preset: ExperiencePreset::Intermediate,
                base_system_preset: SystemPreset::Intermediate,
                system_resource_key: "system.intermediate.v1".to_string(),
                system_resource_version: "cr-ux-001.1".to_string(),
                system_checksum: "system".to_string(),
                persona_resource_key: "persona.default_analyst.v1".to_string(),
                persona_resource_version: "cr-ux-001.1".to_string(),
                persona_checksum: "persona".to_string(),
            })
            .unwrap();
        let active = storage
            .load_active_prompt_profile(profile.id)
            .unwrap()
            .unwrap();
        let repository_id = Uuid::new_v4();
        let snapshot_id = Uuid::new_v4();
        let conversation = storage
            .create_conversation(&NewConversation {
                repository_id: Some(repository_id),
                active_snapshot_id: Some(snapshot_id),
                prompt_profile_id: profile.id,
                persistence: ConversationPersistence::Durable,
            })
            .unwrap();
        let turn_id = Uuid::new_v4();
        let user = ChatMessage::new(
            conversation.id,
            turn_id,
            ChatRole::User,
            0,
            "질문",
            MessageStatus::Completed,
        );
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
                    prompt_profile_id: profile.id,
                    prompt_profile_revision_id: active.revision.id,
                    kernel_version: "kernel.v1".to_string(),
                    kernel_digest: "kernel".to_string(),
                    snapshot_id: Some(snapshot_id),
                    response_contract: ResponseContract::AdvisorMarkdown,
                    audit_result_id: None,
                    started_at: chrono::Utc::now(),
                    completed_at: None,
                },
                user_message: user,
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
        let binding = ProviderBinding::new(
            Uuid::new_v4(),
            "GoogleGemini",
            "https://generativelanguage.googleapis.com/v1beta",
            "dynamic",
        )
        .unwrap();
        let scope_id = Uuid::new_v4();
        storage
            .save_repository_consent_scope(&RepositoryConsentScope {
                id: scope_id,
                conversation_id: conversation.id,
                repository_id,
                snapshot_id,
                provider_binding: binding.clone(),
                kind: RepositoryConsentKind::RequestOnce { turn_id },
                granted_at: chrono::Utc::now(),
                revoked_at: None,
            })
            .unwrap();
        let receipt_id = Uuid::new_v4();
        let now = chrono::Utc::now();
        storage
            .prepare_tool_egress_receipt(&ToolEgressReceipt {
                id: receipt_id,
                seal_version: "CM_TOOL_EGRESS_V1".to_string(),
                trace_id,
                consent_scope_id: scope_id,
                conversation_id: conversation.id,
                turn_id,
                tool_call_id: Uuid::new_v4(),
                repository_id,
                snapshot_id,
                tool_name: RepositoryToolName::ReadFileLines,
                canonical_refs: Vec::new(),
                provider_binding: binding,
                semantic_payload_digest: "semantic".to_string(),
                exact_provider_body_digest: "body".to_string(),
                canonical_digest: "canonical".to_string(),
                status: ToolEgressStatus::Prepared,
                prepared_at: now,
                updated_at: now,
            })
            .unwrap();

        assert_eq!(
            storage
                .load_tool_egress_receipt(receipt_id)
                .unwrap()
                .unwrap()
                .status,
            ToolEgressStatus::Prepared
        );
        storage
            .compare_and_set_tool_egress_status(
                receipt_id,
                ToolEgressStatus::Prepared,
                ToolEgressStatus::Sent,
            )
            .unwrap();
        assert!(storage
            .compare_and_set_tool_egress_status(
                receipt_id,
                ToolEgressStatus::Prepared,
                ToolEgressStatus::Failed,
            )
            .is_err());
    }
}
