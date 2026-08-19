use crate::db::{parse_datetime, parse_uuid, storage_error, SqliteStorage};
use mentat_core::{
    ChatMessage, ChatRole, ComposerSubmitMode, Conversation, ConversationPersistence,
    DeleteReceipt, GroundingFreshness, MentatError, MessageStatus, NewConversation, TurnStart,
    TurnTerminalUpdate, UiPreferences,
};
use rusqlite::{params, OptionalExtension, TransactionBehavior};
use uuid::Uuid;

impl SqliteStorage {
    pub fn create_conversation(
        &self,
        draft: &NewConversation,
    ) -> Result<Conversation, MentatError> {
        let conversation = Conversation::new(
            draft.prompt_profile_id,
            draft.repository_id,
            draft.active_snapshot_id,
        );
        if draft.persistence == ConversationPersistence::Ephemeral {
            return Ok(conversation);
        }
        let conn = self.lock_conn()?;
        conn.execute(
            "INSERT INTO conversations (
                id, repository_id, active_snapshot_id, prompt_profile_id,
                compact_summary, persistence, created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, NULL, 'Durable', ?5, ?5)",
            params![
                conversation.id.to_string(),
                conversation.repository_id.map(|id| id.to_string()),
                conversation.active_snapshot_id.map(|id| id.to_string()),
                conversation.prompt_profile_id.to_string(),
                conversation.created_at.to_rfc3339(),
            ],
        )
        .map_err(|error| storage_error("CONVERSATION_CREATE_FAILED", &error.to_string()))?;
        Ok(conversation)
    }

    pub fn load_conversation(&self, id: Uuid) -> Result<Option<Conversation>, MentatError> {
        let conn = self.lock_conn()?;
        let values = conn
            .query_row(
                "SELECT id, repository_id, active_snapshot_id, prompt_profile_id,
                        compact_summary, created_at, updated_at
                 FROM conversations WHERE id = ?1",
                [id.to_string()],
                |row| {
                    Ok(ConversationValues {
                        id: row.get(0)?,
                        repository_id: row.get(1)?,
                        active_snapshot_id: row.get(2)?,
                        prompt_profile_id: row.get(3)?,
                        compact_summary: row.get(4)?,
                        created_at: row.get(5)?,
                        updated_at: row.get(6)?,
                    })
                },
            )
            .optional()
            .map_err(|error| storage_error("CONVERSATION_READ_FAILED", &error.to_string()))?;
        let Some(values) = values else {
            return Ok(None);
        };
        let mut conversation = values.into_conversation()?;
        let mut statement = conn
            .prepare(
                "SELECT id, conversation_id, turn_id, role, ordinal, markdown, status,
                        error_code, grounding_trace_id, grounding_freshness, created_at, updated_at
                 FROM chat_messages WHERE conversation_id = ?1 ORDER BY ordinal ASC",
            )
            .map_err(|error| storage_error("CHAT_MESSAGE_READ_FAILED", &error.to_string()))?;
        let mut rows = statement
            .query([conversation.id.to_string()])
            .map_err(|error| storage_error("CHAT_MESSAGE_READ_FAILED", &error.to_string()))?;
        while let Some(row) = rows
            .next()
            .map_err(|error| storage_error("CHAT_MESSAGE_READ_FAILED", &error.to_string()))?
        {
            let values = MessageValues {
                id: row.get(0).map_err(sql_decode_error)?,
                conversation_id: row.get(1).map_err(sql_decode_error)?,
                turn_id: row.get(2).map_err(sql_decode_error)?,
                role: row.get(3).map_err(sql_decode_error)?,
                ordinal: row.get(4).map_err(sql_decode_error)?,
                markdown: row.get(5).map_err(sql_decode_error)?,
                status: row.get(6).map_err(sql_decode_error)?,
                error_code: row.get(7).map_err(sql_decode_error)?,
                grounding_trace_id: row.get(8).map_err(sql_decode_error)?,
                grounding_freshness: row.get(9).map_err(sql_decode_error)?,
                created_at: row.get(10).map_err(sql_decode_error)?,
                updated_at: row.get(11).map_err(sql_decode_error)?,
            };
            conversation.messages.push(values.into_message()?);
        }
        Ok(Some(conversation))
    }

    pub fn load_most_recent_conversation(&self) -> Result<Option<Conversation>, MentatError> {
        let id = {
            let conn = self.lock_conn()?;
            conn.query_row(
                "SELECT id FROM conversations ORDER BY updated_at DESC LIMIT 1",
                [],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|error| storage_error("CONVERSATION_READ_FAILED", &error.to_string()))?
        };
        id.map(|value| parse_uuid(&value, "conversations.id"))
            .transpose()?
            .map(|id| self.load_conversation(id))
            .transpose()
            .map(Option::flatten)
    }

    pub fn bind_conversation_repository(
        &self,
        conversation_id: Uuid,
        repository_id: Uuid,
        snapshot_id: Uuid,
    ) -> Result<(), MentatError> {
        let conn = self.lock_conn()?;
        let changed = conn
            .execute(
                "UPDATE conversations SET repository_id = ?1, active_snapshot_id = ?2, updated_at = ?3
                 WHERE id = ?4",
                params![
                    repository_id.to_string(),
                    snapshot_id.to_string(),
                    chrono::Utc::now().to_rfc3339(),
                    conversation_id.to_string(),
                ],
            )
            .map_err(|error| storage_error("CONVERSATION_BIND_FAILED", &error.to_string()))?;
        if changed != 1 {
            return Err(storage_error(
                "CONVERSATION_NOT_FOUND",
                "repository를 결속할 conversation이 없습니다.",
            ));
        }
        Ok(())
    }

    pub fn delete_conversation(&self, id: Uuid) -> Result<DeleteReceipt, MentatError> {
        let mut conn = self.lock_conn()?;
        let transaction = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| {
                storage_error("CONVERSATION_DELETE_BEGIN_FAILED", &error.to_string())
            })?;
        let mut deleted_counts = std::collections::BTreeMap::new();
        for (table, key) in [
            ("conversation_turns", "turns"),
            ("chat_messages", "messages"),
            ("grounding_traces", "grounding_traces"),
            ("tool_egress_receipts", "egress_receipts"),
            ("audit_turn_results", "audit_results"),
        ] {
            let sql = match table {
                "tool_egress_receipts" => {
                    "SELECT COUNT(*) FROM tool_egress_receipts WHERE conversation_id = ?1"
                }
                "audit_turn_results" => {
                    "SELECT COUNT(*) FROM audit_turn_results WHERE turn_id IN (
                        SELECT id FROM conversation_turns WHERE conversation_id = ?1)"
                }
                _ => match table {
                    "conversation_turns" => {
                        "SELECT COUNT(*) FROM conversation_turns WHERE conversation_id = ?1"
                    }
                    "chat_messages" => {
                        "SELECT COUNT(*) FROM chat_messages WHERE conversation_id = ?1"
                    }
                    "grounding_traces" => {
                        "SELECT COUNT(*) FROM grounding_traces WHERE conversation_id = ?1"
                    }
                    _ => unreachable!(),
                },
            };
            let count: u64 = transaction
                .query_row(sql, [id.to_string()], |row| row.get(0))
                .map_err(|error| {
                    storage_error("CONVERSATION_DELETE_COUNT_FAILED", &error.to_string())
                })?;
            deleted_counts.insert(key.to_string(), count);
        }
        let changed = transaction
            .execute("DELETE FROM conversations WHERE id = ?1", [id.to_string()])
            .map_err(|error| storage_error("CONVERSATION_DELETE_FAILED", &error.to_string()))?;
        if changed != 1 {
            return Err(storage_error(
                "CONVERSATION_NOT_FOUND",
                "삭제할 conversation이 없습니다.",
            ));
        }
        transaction.commit().map_err(|error| {
            storage_error("CONVERSATION_DELETE_COMMIT_FAILED", &error.to_string())
        })?;
        deleted_counts.insert("conversations".to_string(), 1);
        Ok(DeleteReceipt {
            operation_id: Uuid::new_v4(),
            deleted_counts,
            removed_artifacts: Vec::new(),
            completed_at: chrono::Utc::now(),
        })
    }

    pub fn begin_turn(&self, start: &TurnStart) -> Result<(), MentatError> {
        validate_turn_start(start)?;
        let mut conn = self.lock_conn()?;
        let transaction = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| storage_error("TURN_BEGIN_FAILED", &error.to_string()))?;
        let response_contract = serde_json::to_string(&start.turn.response_contract)
            .map_err(|error| storage_error("TURN_CONTRACT_ENCODE_FAILED", &error.to_string()))?;
        transaction
            .execute(
                "INSERT INTO conversation_turns (
                    id, conversation_id, sequence, prompt_profile_id,
                    prompt_profile_revision_id, kernel_version, kernel_digest,
                    snapshot_id, response_contract, audit_result_id, started_at, completed_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, NULL, ?10, NULL)",
                params![
                    start.turn.id.to_string(),
                    start.turn.conversation_id.to_string(),
                    i64::try_from(start.turn.sequence).map_err(|_| storage_error(
                        "TURN_SEQUENCE_OVERFLOW",
                        "turn sequence가 SQLite 범위를 초과했습니다."
                    ))?,
                    start.turn.prompt_profile_id.to_string(),
                    start.turn.prompt_profile_revision_id.to_string(),
                    start.turn.kernel_version,
                    start.turn.kernel_digest,
                    start.turn.snapshot_id.map(|id| id.to_string()),
                    response_contract,
                    start.turn.started_at.to_rfc3339(),
                ],
            )
            .map_err(|error| storage_error("TURN_INSERT_FAILED", &error.to_string()))?;
        insert_message(&transaction, &start.user_message)?;
        insert_message(&transaction, &start.assistant_placeholder)?;
        transaction
            .commit()
            .map_err(|error| storage_error("TURN_BEGIN_COMMIT_FAILED", &error.to_string()))?;
        Ok(())
    }

    pub fn append_assistant_delta(&self, message_id: Uuid, delta: &str) -> Result<(), MentatError> {
        let conn = self.lock_conn()?;
        let changed = conn
            .execute(
                "UPDATE chat_messages SET
                    markdown = markdown || ?1,
                    status = 'Streaming',
                    updated_at = ?2
                 WHERE id = ?3 AND role = 'Assistant' AND status IN ('Pending', 'Streaming')",
                params![
                    delta,
                    chrono::Utc::now().to_rfc3339(),
                    message_id.to_string()
                ],
            )
            .map_err(|error| storage_error("CHAT_DELTA_APPEND_FAILED", &error.to_string()))?;
        if changed != 1 {
            return Err(storage_error(
                "CHAT_DELTA_TERMINAL_CONFLICT",
                "assistant message가 없거나 이미 terminal 상태입니다.",
            ));
        }
        Ok(())
    }

    pub fn finish_turn(&self, update: &TurnTerminalUpdate) -> Result<(), MentatError> {
        if let TurnTerminalUpdate::AuditCompleted {
            turn_id,
            assistant_message_id,
            result,
            grounding_trace_id,
            freshness,
            completed_at,
        } = update
        {
            return self.finish_audit_turn(
                *turn_id,
                *assistant_message_id,
                result,
                *grounding_trace_id,
                freshness,
                *completed_at,
            );
        }
        let (turn_id, assistant_message_id, markdown, status, error_code, trace_id, freshness, at) =
            terminal_values(update)?;
        let mut conn = self.lock_conn()?;
        let transaction = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| storage_error("TURN_FINISH_BEGIN_FAILED", &error.to_string()))?;
        let changed_message = transaction
            .execute(
                "UPDATE chat_messages SET
                    markdown = ?1,
                    status = ?2,
                    error_code = ?3,
                    grounding_trace_id = ?4,
                    grounding_freshness = ?5,
                    updated_at = ?6
                 WHERE id = ?7 AND turn_id = ?8 AND role = 'Assistant'
                   AND status IN ('Pending', 'Streaming')",
                params![
                    markdown,
                    status,
                    error_code,
                    trace_id.map(|id| id.to_string()),
                    freshness,
                    at.to_rfc3339(),
                    assistant_message_id.to_string(),
                    turn_id.to_string(),
                ],
            )
            .map_err(|error| storage_error("TURN_MESSAGE_FINISH_FAILED", &error.to_string()))?;
        let changed_turn = transaction
            .execute(
                "UPDATE conversation_turns SET completed_at = ?1
                 WHERE id = ?2 AND completed_at IS NULL",
                params![at.to_rfc3339(), turn_id.to_string()],
            )
            .map_err(|error| storage_error("TURN_FINISH_FAILED", &error.to_string()))?;
        if changed_message != 1 || changed_turn != 1 {
            return Err(storage_error(
                "TURN_TERMINAL_CONFLICT",
                "turn 또는 assistant message가 이미 terminal 상태입니다.",
            ));
        }
        transaction
            .commit()
            .map_err(|error| storage_error("TURN_FINISH_COMMIT_FAILED", &error.to_string()))?;
        Ok(())
    }

    fn finish_audit_turn(
        &self,
        turn_id: Uuid,
        assistant_message_id: Uuid,
        result: &mentat_core::AnswerBundle,
        grounding_trace_id: Uuid,
        freshness: &GroundingFreshness,
        completed_at: chrono::DateTime<chrono::Utc>,
    ) -> Result<(), MentatError> {
        if result.raw_model_response.is_some() {
            return Err(storage_error(
                "AUDIT_RAW_RESPONSE_FORBIDDEN",
                "validated Audit result에는 raw_model_response를 저장할 수 없습니다.",
            ));
        }
        let result_id = Uuid::new_v4();
        let validated_bundle = serde_json::to_string(result)
            .map_err(|error| storage_error("AUDIT_RESULT_ENCODE_FAILED", &error.to_string()))?;
        let freshness = serde_json::to_string(freshness).map_err(|error| {
            storage_error("GROUNDING_FRESHNESS_ENCODE_FAILED", &error.to_string())
        })?;
        let mut conn = self.lock_conn()?;
        let transaction = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| storage_error("AUDIT_FINISH_BEGIN_FAILED", &error.to_string()))?;
        let contract_json: String = transaction
            .query_row(
                "SELECT response_contract FROM conversation_turns WHERE id = ?1",
                [turn_id.to_string()],
                |row| row.get(0),
            )
            .map_err(|error| storage_error("AUDIT_CONTRACT_READ_FAILED", &error.to_string()))?;
        let contract: mentat_core::ResponseContract = serde_json::from_str(&contract_json)
            .map_err(|error| storage_error("AUDIT_CONTRACT_DECODE_FAILED", &error.to_string()))?;
        if !matches!(
            contract,
            mentat_core::ResponseContract::AuditAnswerBundle { ref schema_version }
                if schema_version == "answer_bundle.v1"
        ) {
            return Err(storage_error(
                "AUDIT_TURN_CONTRACT_MISMATCH",
                "Advisor turn에는 Audit result를 저장할 수 없습니다.",
            ));
        }
        transaction
            .execute(
                "INSERT INTO audit_turn_results (id, turn_id, schema_version, validated_bundle, created_at)
                 VALUES (?1, ?2, 'answer_bundle.v1', ?3, ?4)",
                params![
                    result_id.to_string(),
                    turn_id.to_string(),
                    validated_bundle,
                    completed_at.to_rfc3339(),
                ],
            )
            .map_err(|error| storage_error("AUDIT_RESULT_INSERT_FAILED", &error.to_string()))?;
        let changed_message = transaction
            .execute(
                "UPDATE chat_messages SET markdown = '', status = 'Completed', error_code = NULL,
                    grounding_trace_id = ?1, grounding_freshness = ?2, updated_at = ?3
                 WHERE id = ?4 AND turn_id = ?5 AND role = 'Assistant'
                   AND status IN ('Pending', 'Streaming')",
                params![
                    grounding_trace_id.to_string(),
                    freshness,
                    completed_at.to_rfc3339(),
                    assistant_message_id.to_string(),
                    turn_id.to_string(),
                ],
            )
            .map_err(|error| storage_error("AUDIT_MESSAGE_FINISH_FAILED", &error.to_string()))?;
        let changed_turn = transaction
            .execute(
                "UPDATE conversation_turns SET audit_result_id = ?1, completed_at = ?2
                 WHERE id = ?3 AND completed_at IS NULL",
                params![
                    result_id.to_string(),
                    completed_at.to_rfc3339(),
                    turn_id.to_string(),
                ],
            )
            .map_err(|error| storage_error("AUDIT_TURN_FINISH_FAILED", &error.to_string()))?;
        if changed_message != 1 || changed_turn != 1 {
            return Err(storage_error(
                "TURN_TERMINAL_CONFLICT",
                "Audit turn 또는 assistant message가 이미 terminal 상태입니다.",
            ));
        }
        transaction
            .commit()
            .map_err(|error| storage_error("AUDIT_FINISH_COMMIT_FAILED", &error.to_string()))?;
        Ok(())
    }

    pub fn save_ui_preferences(&self, preferences: &UiPreferences) -> Result<(), MentatError> {
        if !preferences.width_points.is_finite()
            || !preferences.height_points.is_finite()
            || preferences.width_points <= 0.0
            || preferences.height_points <= 0.0
        {
            return Err(storage_error(
                "UI_PREFERENCES_INVALID",
                "창 크기는 유한한 양수여야 합니다.",
            ));
        }
        let submit_mode = match preferences.submit_mode {
            ComposerSubmitMode::EnterSend => "EnterSend",
            ComposerSubmitMode::CtrlEnterSend => "CtrlEnterSend",
        };
        let conn = self.lock_conn()?;
        conn.execute(
            "INSERT INTO ui_preferences (
                id, width_points, height_points, submit_mode,
                always_on_top, layout_revision, updated_at
             ) VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(id) DO UPDATE SET
                width_points = excluded.width_points,
                height_points = excluded.height_points,
                submit_mode = excluded.submit_mode,
                always_on_top = excluded.always_on_top,
                layout_revision = excluded.layout_revision,
                updated_at = excluded.updated_at",
            params![
                preferences.width_points,
                preferences.height_points,
                submit_mode,
                preferences.always_on_top,
                preferences.layout_revision,
                preferences.updated_at.to_rfc3339(),
            ],
        )
        .map_err(|error| storage_error("UI_PREFERENCES_SAVE_FAILED", &error.to_string()))?;
        Ok(())
    }

    pub fn load_ui_preferences(&self) -> Result<UiPreferences, MentatError> {
        let conn = self.lock_conn()?;
        let values = conn
            .query_row(
                "SELECT width_points, height_points, submit_mode,
                        always_on_top, layout_revision, updated_at
                 FROM ui_preferences WHERE id = 1",
                [],
                |row| {
                    Ok((
                        row.get::<_, f32>(0)?,
                        row.get::<_, f32>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, i64>(3)?,
                        row.get::<_, i64>(4)?,
                        row.get::<_, String>(5)?,
                    ))
                },
            )
            .optional()
            .map_err(|error| storage_error("UI_PREFERENCES_READ_FAILED", &error.to_string()))?;
        let Some((
            width_points,
            height_points,
            submit_mode,
            always_on_top,
            layout_revision,
            updated_at,
        )) = values
        else {
            return Ok(UiPreferences::default());
        };
        if !width_points.is_finite()
            || !height_points.is_finite()
            || width_points <= 0.0
            || height_points <= 0.0
        {
            return Err(storage_error(
                "UI_PREFERENCES_INVALID",
                "저장된 창 크기가 유한한 양수가 아닙니다.",
            ));
        }
        let submit_mode = match submit_mode.as_str() {
            "EnterSend" => ComposerSubmitMode::EnterSend,
            "CtrlEnterSend" => ComposerSubmitMode::CtrlEnterSend,
            _ => {
                return Err(storage_error(
                    "STORAGE_DECODE_ENUM",
                    "submit_mode 값이 유효하지 않습니다.",
                ))
            }
        };
        let always_on_top = match always_on_top {
            0 => false,
            1 => true,
            _ => {
                return Err(storage_error(
                    "STORAGE_DECODE_BOOL",
                    "always_on_top 값이 0/1이 아닙니다.",
                ))
            }
        };
        let layout_revision = u32::try_from(layout_revision).map_err(|_| {
            storage_error(
                "STORAGE_DECODE_INTEGER",
                "layout_revision 값이 유효하지 않습니다.",
            )
        })?;
        Ok(UiPreferences {
            width_points,
            height_points,
            submit_mode,
            always_on_top,
            layout_revision,
            updated_at: parse_datetime(&updated_at, "ui_preferences.updated_at")?,
        })
    }
}

fn validate_turn_start(start: &TurnStart) -> Result<(), MentatError> {
    let turn = &start.turn;
    let user = &start.user_message;
    let assistant = &start.assistant_placeholder;
    if user.conversation_id != turn.conversation_id
        || assistant.conversation_id != turn.conversation_id
        || user.turn_id != turn.id
        || assistant.turn_id != turn.id
        || user.role != ChatRole::User
        || assistant.role != ChatRole::Assistant
        || user.status != MessageStatus::Completed
        || assistant.status != MessageStatus::Pending
        || user.ordinal >= assistant.ordinal
    {
        return Err(storage_error(
            "TURN_START_INVALID",
            "turn과 user/assistant placeholder 결속이 유효하지 않습니다.",
        ));
    }
    Ok(())
}

fn insert_message(
    transaction: &rusqlite::Transaction<'_>,
    message: &ChatMessage,
) -> Result<(), MentatError> {
    let (status, error_code) = encode_message_status(&message.status);
    let freshness = message
        .grounding_freshness
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .map_err(|error| storage_error("GROUNDING_FRESHNESS_ENCODE_FAILED", &error.to_string()))?;
    transaction
        .execute(
            "INSERT INTO chat_messages (
                id, conversation_id, turn_id, role, ordinal, markdown, status,
                error_code, grounding_trace_id, grounding_freshness, created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                message.id.to_string(),
                message.conversation_id.to_string(),
                message.turn_id.to_string(),
                match message.role {
                    ChatRole::User => "User",
                    ChatRole::Assistant => "Assistant",
                },
                i64::try_from(message.ordinal).map_err(|_| storage_error(
                    "CHAT_ORDINAL_OVERFLOW",
                    "message ordinal이 SQLite 범위를 초과했습니다."
                ))?,
                message.markdown,
                status,
                error_code,
                message.grounding_trace_id.map(|id| id.to_string()),
                freshness,
                message.created_at.to_rfc3339(),
                message.updated_at.to_rfc3339(),
            ],
        )
        .map_err(|error| storage_error("CHAT_MESSAGE_INSERT_FAILED", &error.to_string()))?;
    Ok(())
}

type TerminalValues = (
    Uuid,
    Uuid,
    String,
    &'static str,
    Option<String>,
    Option<Uuid>,
    Option<String>,
    chrono::DateTime<chrono::Utc>,
);

fn terminal_values(update: &TurnTerminalUpdate) -> Result<TerminalValues, MentatError> {
    match update {
        TurnTerminalUpdate::AdvisorCompleted {
            turn_id,
            assistant_message_id,
            markdown,
            grounding_trace_id,
            freshness,
            completed_at,
        } => Ok((
            *turn_id,
            *assistant_message_id,
            markdown.clone(),
            "Completed",
            None,
            *grounding_trace_id,
            freshness
                .as_ref()
                .map(serde_json::to_string)
                .transpose()
                .map_err(|error| {
                    storage_error("GROUNDING_FRESHNESS_ENCODE_FAILED", &error.to_string())
                })?,
            *completed_at,
        )),
        TurnTerminalUpdate::AdvisorCancelled {
            turn_id,
            assistant_message_id,
            partial_markdown,
            completed_at,
        } => Ok((
            *turn_id,
            *assistant_message_id,
            partial_markdown.clone(),
            "Cancelled",
            None,
            None,
            None,
            *completed_at,
        )),
        TurnTerminalUpdate::AuditCancelled {
            turn_id,
            assistant_message_id,
            completed_at,
        } => Ok((
            *turn_id,
            *assistant_message_id,
            String::new(),
            "Cancelled",
            None,
            None,
            None,
            *completed_at,
        )),
        TurnTerminalUpdate::Failed {
            turn_id,
            assistant_message_id,
            error_code,
            safe_message,
            completed_at,
        } => Ok((
            *turn_id,
            *assistant_message_id,
            safe_message.clone(),
            "Failed",
            Some(error_code.clone()),
            None,
            None,
            *completed_at,
        )),
        TurnTerminalUpdate::AuditCompleted { .. } => {
            unreachable!("Audit terminal은 별도 경로에서 처리")
        }
    }
}

fn encode_message_status(status: &MessageStatus) -> (&'static str, Option<&str>) {
    match status {
        MessageStatus::Pending => ("Pending", None),
        MessageStatus::Streaming => ("Streaming", None),
        MessageStatus::Completed => ("Completed", None),
        MessageStatus::Cancelled => ("Cancelled", None),
        MessageStatus::Failed { error_code } => ("Failed", Some(error_code.as_str())),
    }
}

fn sql_decode_error(error: rusqlite::Error) -> MentatError {
    storage_error("CHAT_MESSAGE_READ_FAILED", &error.to_string())
}

struct ConversationValues {
    id: String,
    repository_id: Option<String>,
    active_snapshot_id: Option<String>,
    prompt_profile_id: String,
    compact_summary: Option<String>,
    created_at: String,
    updated_at: String,
}

impl ConversationValues {
    fn into_conversation(self) -> Result<Conversation, MentatError> {
        Ok(Conversation {
            id: parse_uuid(&self.id, "conversations.id")?,
            repository_id: self
                .repository_id
                .as_deref()
                .map(|value| parse_uuid(value, "conversations.repository_id"))
                .transpose()?,
            active_snapshot_id: self
                .active_snapshot_id
                .as_deref()
                .map(|value| parse_uuid(value, "conversations.active_snapshot_id"))
                .transpose()?,
            prompt_profile_id: parse_uuid(
                &self.prompt_profile_id,
                "conversations.prompt_profile_id",
            )?,
            messages: Vec::new(),
            compact_summary: self.compact_summary,
            created_at: parse_datetime(&self.created_at, "conversations.created_at")?,
            updated_at: parse_datetime(&self.updated_at, "conversations.updated_at")?,
        })
    }
}

struct MessageValues {
    id: String,
    conversation_id: String,
    turn_id: String,
    role: String,
    ordinal: i64,
    markdown: String,
    status: String,
    error_code: Option<String>,
    grounding_trace_id: Option<String>,
    grounding_freshness: Option<String>,
    created_at: String,
    updated_at: String,
}

impl MessageValues {
    fn into_message(self) -> Result<ChatMessage, MentatError> {
        let role = match self.role.as_str() {
            "User" => ChatRole::User,
            "Assistant" => ChatRole::Assistant,
            _ => {
                return Err(storage_error(
                    "STORAGE_DECODE_ENUM",
                    "chat_messages.role 값이 유효하지 않습니다.",
                ))
            }
        };
        let error_code = self.error_code;
        let status = match self.status.as_str() {
            "Pending" => MessageStatus::Pending,
            "Streaming" => MessageStatus::Streaming,
            "Completed" => MessageStatus::Completed,
            "Cancelled" => MessageStatus::Cancelled,
            "Failed" => MessageStatus::Failed {
                error_code: error_code.clone().ok_or_else(|| {
                    storage_error(
                        "STORAGE_DECODE_STATUS",
                        "Failed message에 error_code가 없습니다.",
                    )
                })?,
            },
            _ => {
                return Err(storage_error(
                    "STORAGE_DECODE_ENUM",
                    "chat_messages.status 값이 유효하지 않습니다.",
                ))
            }
        };
        if !matches!(status, MessageStatus::Failed { .. }) && error_code.is_some() {
            return Err(storage_error(
                "STORAGE_DECODE_STATUS",
                "Failed가 아닌 message에 error_code가 있습니다.",
            ));
        }
        let grounding_freshness: Option<GroundingFreshness> = self
            .grounding_freshness
            .as_deref()
            .map(serde_json::from_str)
            .transpose()
            .map_err(|error| storage_error("STORAGE_DECODE_GROUNDING", &error.to_string()))?;
        Ok(ChatMessage {
            id: parse_uuid(&self.id, "chat_messages.id")?,
            conversation_id: parse_uuid(&self.conversation_id, "chat_messages.conversation_id")?,
            turn_id: parse_uuid(&self.turn_id, "chat_messages.turn_id")?,
            role,
            ordinal: u64::try_from(self.ordinal).map_err(|_| {
                storage_error(
                    "STORAGE_DECODE_INTEGER",
                    "chat_messages.ordinal 값이 유효하지 않습니다.",
                )
            })?,
            markdown: self.markdown,
            status,
            source_ref_ids: Vec::new(),
            grounding_trace_id: self
                .grounding_trace_id
                .as_deref()
                .map(|value| parse_uuid(value, "chat_messages.grounding_trace_id"))
                .transpose()?,
            grounding_freshness,
            created_at: parse_datetime(&self.created_at, "chat_messages.created_at")?,
            updated_at: parse_datetime(&self.updated_at, "chat_messages.updated_at")?,
        })
    }
}
