use crate::db::{parse_datetime, parse_uuid, storage_error, SqliteStorage};
use mentat_core::{
    GroundingFreshness, GroundingTrace, MentatError, ProviderBinding, RepositoryConsentKind,
    RepositoryConsentScope, RepositoryToolCallRecord, RepositoryToolCallStatus, RepositoryToolName,
    SourceRef, ToolEgressReceipt, ToolEgressStatus,
};
use rusqlite::{params, OptionalExtension, Transaction, TransactionBehavior};
use uuid::Uuid;

impl SqliteStorage {
    pub fn prepare_grounding_trace(&self, trace: &GroundingTrace) -> Result<(), MentatError> {
        let mut conn = self.lock_conn()?;
        let transaction = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| storage_error("GROUNDING_TRACE_BEGIN_FAILED", &error.to_string()))?;
        write_grounding_trace_in_transaction(&transaction, trace)?;
        transaction
            .commit()
            .map_err(|error| storage_error("GROUNDING_TRACE_COMMIT_FAILED", &error.to_string()))?;
        Ok(())
    }

    pub fn load_grounding_trace(&self, id: Uuid) -> Result<Option<GroundingTrace>, MentatError> {
        let conn = self.lock_conn()?;
        let header = conn
            .query_row(
                "SELECT conversation_id, turn_id, snapshot_id, freshness
                 FROM grounding_traces WHERE id = ?1",
                [id.to_string()],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Option<String>>(2)?,
                        row.get::<_, String>(3)?,
                    ))
                },
            )
            .optional()
            .map_err(|error| storage_error("GROUNDING_TRACE_READ_FAILED", &error.to_string()))?;
        let Some((conversation_id, turn_id, snapshot_id, freshness)) = header else {
            return Ok(None);
        };
        let mut record_statement = conn
            .prepare(
                "SELECT call_id, round, tool_name, canonical_arguments_digest,
                        result_digest, content_bytes, source_ref_ids, status
                 FROM tool_call_records WHERE trace_id = ?1 ORDER BY round, call_id",
            )
            .map_err(|error| storage_error("GROUNDING_TOOL_READ_FAILED", &error.to_string()))?;
        let record_rows = record_statement
            .query_map([id.to_string()], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, i64>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, String>(7)?,
                ))
            })
            .map_err(|error| storage_error("GROUNDING_TOOL_READ_FAILED", &error.to_string()))?;
        let mut tool_calls = Vec::new();
        for row in record_rows {
            let row = row
                .map_err(|error| storage_error("GROUNDING_TOOL_READ_FAILED", &error.to_string()))?;
            tool_calls.push(RepositoryToolCallRecord {
                trace_id: id,
                call_id: parse_uuid(&row.0, "tool_call_records.call_id")?,
                round: u8::try_from(row.1).map_err(|_| {
                    storage_error(
                        "STORAGE_DECODE_INTEGER",
                        "tool call round가 유효하지 않습니다.",
                    )
                })?,
                name: parse_tool_name(&row.2)?,
                canonical_arguments_digest: row.3,
                result_digest: row.4,
                content_bytes: u32::try_from(row.5).map_err(|_| {
                    storage_error(
                        "STORAGE_DECODE_INTEGER",
                        "tool result bytes가 유효하지 않습니다.",
                    )
                })?,
                source_ref_ids: serde_json::from_str(&row.6).map_err(|error| {
                    storage_error("GROUNDING_SOURCE_IDS_DECODE_FAILED", &error.to_string())
                })?,
                status: parse_tool_call_status(&row.7)?,
            });
        }
        drop(record_statement);

        let mut source_statement = conn
            .prepare(
                "SELECT id, snapshot_id, relative_path, line_start, line_end, content_hash, excerpt
                 FROM source_refs WHERE trace_id = ?1 ORDER BY ordinal",
            )
            .map_err(|error| storage_error("GROUNDING_SOURCE_READ_FAILED", &error.to_string()))?;
        let source_rows = source_statement
            .query_map([id.to_string()], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                ))
            })
            .map_err(|error| storage_error("GROUNDING_SOURCE_READ_FAILED", &error.to_string()))?;
        let mut source_refs = Vec::new();
        for row in source_rows {
            let row = row.map_err(|error| {
                storage_error("GROUNDING_SOURCE_READ_FAILED", &error.to_string())
            })?;
            source_refs.push(SourceRef {
                id: parse_uuid(&row.0, "source_refs.id")?,
                snapshot_id: parse_uuid(&row.1, "source_refs.snapshot_id")?,
                relative_path: std::path::PathBuf::from(row.2),
                line_start: usize::try_from(row.3).map_err(|_| {
                    storage_error(
                        "STORAGE_DECODE_INTEGER",
                        "source line_start가 유효하지 않습니다.",
                    )
                })?,
                line_end: usize::try_from(row.4).map_err(|_| {
                    storage_error(
                        "STORAGE_DECODE_INTEGER",
                        "source line_end가 유효하지 않습니다.",
                    )
                })?,
                content_hash: row.5,
                excerpt: row.6,
            });
        }
        drop(source_statement);

        let mut receipt_statement = conn
            .prepare("SELECT id FROM tool_egress_receipts WHERE trace_id = ?1 ORDER BY prepared_at")
            .map_err(|error| storage_error("GROUNDING_RECEIPT_READ_FAILED", &error.to_string()))?;
        let receipt_rows = receipt_statement
            .query_map([id.to_string()], |row| row.get::<_, String>(0))
            .map_err(|error| storage_error("GROUNDING_RECEIPT_READ_FAILED", &error.to_string()))?;
        let mut egress_receipt_ids = Vec::new();
        for row in receipt_rows {
            egress_receipt_ids.push(parse_uuid(
                &row.map_err(|error| {
                    storage_error("GROUNDING_RECEIPT_READ_FAILED", &error.to_string())
                })?,
                "tool_egress_receipts.id",
            )?);
        }
        Ok(Some(GroundingTrace {
            id,
            conversation_id: parse_uuid(&conversation_id, "grounding_traces.conversation_id")?,
            turn_id: parse_uuid(&turn_id, "grounding_traces.turn_id")?,
            snapshot_id: snapshot_id
                .as_deref()
                .map(|value| parse_uuid(value, "grounding_traces.snapshot_id"))
                .transpose()?,
            tool_calls,
            source_refs,
            egress_receipt_ids,
            freshness: serde_json::from_str::<GroundingFreshness>(&freshness).map_err(|error| {
                storage_error("GROUNDING_FRESHNESS_DECODE_FAILED", &error.to_string())
            })?,
        }))
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
                exact_provider_body_digest, canonical_digest, status, prepared_at, updated_at,
                runtime_owner_id
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12,
                       ?13, ?14, ?15, 'Prepared', ?16, ?17, ?18)",
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
                self.runtime_owner_id().to_string(),
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
        self.compare_and_set_tool_egress_status_batch(&[id], expected, next)
    }

    pub fn compare_and_set_tool_egress_status_batch(
        &self,
        ids: &[Uuid],
        expected: ToolEgressStatus,
        next: ToolEgressStatus,
    ) -> Result<(), MentatError> {
        self.compare_and_set_tool_egress_status_batch_internal(ids, expected, next, None)
    }

    #[cfg(test)]
    pub(crate) fn compare_and_set_tool_egress_status_batch_killpoint(
        &self,
        ids: &[Uuid],
        expected: ToolEgressStatus,
        next: ToolEgressStatus,
        fail_before_index: usize,
    ) -> Result<(), MentatError> {
        self.compare_and_set_tool_egress_status_batch_internal(
            ids,
            expected,
            next,
            Some(fail_before_index),
        )
    }

    fn compare_and_set_tool_egress_status_batch_internal(
        &self,
        ids: &[Uuid],
        expected: ToolEgressStatus,
        next: ToolEgressStatus,
        fail_before_index: Option<usize>,
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
        if ids.is_empty() {
            return Err(storage_error(
                "TOOL_EGRESS_BATCH_EMPTY",
                "receipt batch는 비어 있을 수 없습니다.",
            ));
        }
        let unique = ids
            .iter()
            .copied()
            .collect::<std::collections::HashSet<_>>();
        if unique.len() != ids.len() {
            return Err(storage_error(
                "TOOL_EGRESS_BATCH_DUPLICATE",
                "receipt batch에 중복 ID가 있습니다.",
            ));
        }
        let mut conn = self.lock_conn()?;
        let transaction = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| storage_error("TOOL_EGRESS_BATCH_BEGIN_FAILED", &error.to_string()))?;
        let mut exact_body_digest: Option<String> = None;
        for id in ids {
            let receipt: Option<(String, String)> = transaction
                .query_row(
                    "SELECT status, exact_provider_body_digest
                       FROM tool_egress_receipts WHERE id = ?1",
                    [id.to_string()],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()
                .map_err(|error| {
                    storage_error("TOOL_EGRESS_STATUS_READ_FAILED", &error.to_string())
                })?;
            let Some((status, body_digest)) = receipt else {
                return Err(storage_error(
                    "TOOL_EGRESS_STATUS_CONFLICT",
                    "receipt batch에 존재하지 않는 ID가 있습니다.",
                ));
            };
            if status != egress_status_text(expected) {
                return Err(storage_error(
                    "TOOL_EGRESS_STATUS_CONFLICT",
                    "receipt batch expected 상태와 다릅니다.",
                ));
            }
            if exact_body_digest
                .as_ref()
                .is_some_and(|expected_digest| expected_digest != &body_digest)
            {
                return Err(storage_error(
                    "TOOL_EGRESS_BATCH_BODY_MISMATCH",
                    "receipt batch가 서로 다른 exact provider body를 가리킵니다.",
                ));
            }
            exact_body_digest.get_or_insert(body_digest);
        }
        let updated_at = chrono::Utc::now().to_rfc3339();
        for (index, id) in ids.iter().enumerate() {
            if fail_before_index == Some(index) {
                return Err(storage_error(
                    "TOOL_EGRESS_BATCH_KILLPOINT",
                    "receipt batch commit 전 killpoint가 실행되었습니다.",
                ));
            }
            let changed = transaction
                .execute(
                    "UPDATE tool_egress_receipts SET status = ?1, updated_at = ?2
                 WHERE id = ?3 AND status = ?4",
                    params![
                        egress_status_text(next),
                        updated_at,
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
                    "receipt batch 갱신 수가 예상과 다릅니다.",
                ));
            }
        }
        transaction.commit().map_err(|error| {
            storage_error("TOOL_EGRESS_BATCH_COMMIT_FAILED", &error.to_string())
        })?;
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

pub(crate) fn write_grounding_trace_in_transaction(
    transaction: &Transaction<'_>,
    trace: &GroundingTrace,
) -> Result<(), MentatError> {
    transaction
        .execute(
            "INSERT INTO grounding_traces (
                id, conversation_id, turn_id, snapshot_id, freshness, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(id) DO UPDATE SET
                conversation_id = excluded.conversation_id,
                turn_id = excluded.turn_id,
                snapshot_id = excluded.snapshot_id,
                freshness = excluded.freshness",
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
    transaction
        .execute(
            "DELETE FROM tool_call_records WHERE trace_id = ?1",
            [trace.id.to_string()],
        )
        .map_err(|error| storage_error("GROUNDING_TOOL_RECORD_RESET_FAILED", &error.to_string()))?;
    transaction
        .execute(
            "DELETE FROM source_refs WHERE trace_id = ?1",
            [trace.id.to_string()],
        )
        .map_err(|error| storage_error("GROUNDING_SOURCE_RESET_FAILED", &error.to_string()))?;
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
            .map_err(|error| storage_error("GROUNDING_SOURCE_INSERT_FAILED", &error.to_string()))?;
    }
    Ok(())
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

fn parse_tool_call_status(value: &str) -> Result<RepositoryToolCallStatus, MentatError> {
    match value {
        "Pending" => Ok(RepositoryToolCallStatus::Pending),
        "Completed" => Ok(RepositoryToolCallStatus::Completed),
        "Omitted" => Ok(RepositoryToolCallStatus::Omitted),
        "Failed" => Ok(RepositoryToolCallStatus::Failed),
        _ => Err(storage_error(
            "STORAGE_DECODE_ENUM",
            "tool_call_records.status 값이 유효하지 않습니다.",
        )),
    }
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

    fn two_prepared_receipts_for_same_body() -> (SqliteStorage, Uuid, Uuid) {
        let directory = tempdir().unwrap().keep();
        let storage = SqliteStorage::open(directory.join("mentat.db")).unwrap();
        let profile_id = Uuid::new_v4();
        let profile = storage
            .seed_factory_prompt_profile(&FactoryPromptSeed {
                profile_id,
                profile_name: "batch".to_string(),
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
                user_message: ChatMessage::new(
                    conversation.id,
                    turn_id,
                    ChatRole::User,
                    0,
                    "질문",
                    MessageStatus::Completed,
                ),
                assistant_placeholder: ChatMessage::new(
                    conversation.id,
                    turn_id,
                    ChatRole::Assistant,
                    1,
                    "",
                    MessageStatus::Pending,
                ),
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
            "OpenAICompatible",
            "https://api.example.com/v1/chat/completions",
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
                kind: RepositoryConsentKind::RepositorySession,
                granted_at: chrono::Utc::now(),
                revoked_at: None,
            })
            .unwrap();
        let first = Uuid::new_v4();
        let second = Uuid::new_v4();
        for id in [first, second] {
            let now = chrono::Utc::now();
            storage
                .prepare_tool_egress_receipt(&ToolEgressReceipt {
                    id,
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
                    provider_binding: binding.clone(),
                    semantic_payload_digest: format!("semantic-{id}"),
                    exact_provider_body_digest: "same-body".to_string(),
                    canonical_digest: format!("canonical-{id}"),
                    status: ToolEgressStatus::Prepared,
                    prepared_at: now,
                    updated_at: now,
                })
                .unwrap();
        }
        (storage, first, second)
    }

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
        let restored = storage.load_grounding_trace(trace_id).unwrap().unwrap();
        assert_eq!(restored.id, trace_id);
        assert_eq!(restored.snapshot_id, Some(snapshot_id));
        assert_eq!(restored.egress_receipt_ids, vec![receipt_id]);
    }

    #[test]
    fn second_receipt_update_failure_rolls_back_entire_body_batch() {
        let (storage, first, second) = two_prepared_receipts_for_same_body();

        assert!(storage
            .compare_and_set_tool_egress_status_batch_killpoint(
                &[first, second],
                ToolEgressStatus::Prepared,
                ToolEgressStatus::Sent,
                1,
            )
            .is_err());
        assert_eq!(
            storage
                .load_tool_egress_receipt(first)
                .unwrap()
                .unwrap()
                .status,
            ToolEgressStatus::Prepared
        );
        assert_eq!(
            storage
                .load_tool_egress_receipt(second)
                .unwrap()
                .unwrap()
                .status,
            ToolEgressStatus::Prepared
        );

        storage
            .compare_and_set_tool_egress_status_batch(
                &[first, second],
                ToolEgressStatus::Prepared,
                ToolEgressStatus::Sent,
            )
            .unwrap();
        assert_eq!(
            storage
                .load_tool_egress_receipt(first)
                .unwrap()
                .unwrap()
                .status,
            ToolEgressStatus::Sent
        );
        assert_eq!(
            storage
                .load_tool_egress_receipt(second)
                .unwrap()
                .unwrap()
                .status,
            ToolEgressStatus::Sent
        );
    }

    #[test]
    fn true_stale_owner_reopen_reconciles_prepared_body_batch_to_outcome_unknown() {
        let (storage, first, second) = two_prepared_receipts_for_same_body();
        let db_path = storage.db_path().to_path_buf();
        let stale_owner_id = storage.runtime_owner_id();
        drop(storage);
        let connection = rusqlite::Connection::open(&db_path).unwrap();
        let stale_at = chrono::Utc::now() - chrono::Duration::seconds(120);
        connection
            .execute(
                "INSERT INTO runtime_ownership (
                    id, owner_id, process_id, acquired_at, heartbeat_at
                 ) VALUES (1, ?1, 999999, ?2, ?2)",
                params![stale_owner_id.to_string(), stale_at.to_rfc3339()],
            )
            .unwrap();
        drop(connection);

        let reopened = SqliteStorage::open(db_path).unwrap();
        for id in [first, second] {
            assert_eq!(
                reopened
                    .load_tool_egress_receipt(id)
                    .unwrap()
                    .unwrap()
                    .status,
                ToolEgressStatus::OutcomeUnknown
            );
        }
    }

    #[test]
    fn receipt_batch_rejects_mixed_exact_body_digests_without_partial_update() {
        let (storage, first, second) = two_prepared_receipts_for_same_body();
        storage
            .lock_conn()
            .unwrap()
            .execute(
                "UPDATE tool_egress_receipts SET exact_provider_body_digest = 'other-body'
                 WHERE id = ?1",
                [second.to_string()],
            )
            .unwrap();

        assert!(storage
            .compare_and_set_tool_egress_status_batch(
                &[first, second],
                ToolEgressStatus::Prepared,
                ToolEgressStatus::Sent,
            )
            .is_err());
        for id in [first, second] {
            assert_eq!(
                storage
                    .load_tool_egress_receipt(id)
                    .unwrap()
                    .unwrap()
                    .status,
                ToolEgressStatus::Prepared
            );
        }
    }

    #[test]
    fn second_live_handle_is_rejected_without_touching_live_prepared_receipts() {
        let (storage, first, second) = two_prepared_receipts_for_same_body();
        let second_open = SqliteStorage::open(storage.db_path());

        assert!(matches!(
            second_open,
            Err(MentatError::StorageError { code, .. }) if code == "STORAGE_RUNTIME_OWNED"
        ));
        for id in [first, second] {
            assert_eq!(
                storage
                    .load_tool_egress_receipt(id)
                    .unwrap()
                    .unwrap()
                    .status,
                ToolEgressStatus::Prepared
            );
        }
    }
}
