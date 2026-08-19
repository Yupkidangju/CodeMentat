mod conversation;
pub mod db;
mod grounding_store;
mod ports;
mod prompt_store;
mod secret_preferences;

pub use db::SqliteStorage;
pub use prompt_store::FactoryPromptSeed;
pub use secret_preferences::ProviderSecretPreference;

#[cfg(test)]
mod tests {
    use super::*;
    use mentat_core::models::{
        AnswerBundle, ChatMessage, ChatRole, ConversationPersistence, ConversationTurn,
        ExperiencePreset, GroundingFreshness, GroundingTrace, MessageStatus, NewConversation,
        PromptContentSource, PromptDraft, PromptLayerDraft, ProviderBinding, RepositoryConsentKind,
        RepositoryConsentScope, RepositoryProfile, RepositorySnapshot, RepositoryToolCallRecord,
        RepositoryToolCallStatus, RepositoryToolName, RepositoryType, ResponseContract,
        SnapshotStatus, SourceRef, SystemPreset, ToolEgressReceipt, ToolEgressStatus, TurnStart,
        TurnTerminalUpdate, UiPreferences,
    };
    use mentat_core::MentatError;
    use mentat_inference::{BackendProfile, ProviderKind};
    use rusqlite::Connection;
    use std::path::PathBuf;
    use tempfile::tempdir;
    use uuid::Uuid;

    #[test]
    fn test_sqlite_storage_save_and_list_recent_repos() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("mentat.db");

        let storage = SqliteStorage::open(&db_path).expect("DB should open");

        let profile1 = RepositoryProfile {
            id: Uuid::new_v4(),
            display_name: "Repo Alpha".to_string(),
            root_path: PathBuf::from("C:/alpha"),
            repo_type: RepositoryType::Git,
            consent_policy: true,
        };

        let profile2 = RepositoryProfile {
            id: Uuid::new_v4(),
            display_name: "Repo Beta".to_string(),
            root_path: PathBuf::from("C:/beta"),
            repo_type: RepositoryType::Directory,
            consent_policy: false,
        };

        storage.save_recent_repo(&profile1).unwrap();
        storage.save_recent_repo(&profile2).unwrap();

        let list = storage.list_recent_repos().unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].display_name, "Repo Beta"); // Most recent first
        assert_eq!(list[1].display_name, "Repo Alpha");
    }

    #[test]
    fn test_sqlite_storage_save_and_load_backend_profile() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("mentat.db");

        let storage = SqliteStorage::open(&db_path).expect("DB should open");

        let profile = BackendProfile {
            id: Uuid::new_v4(),
            name: "Gemini Pro".to_string(),
            provider: ProviderKind::GoogleGemini,
            base_url: "https://generativelanguage.googleapis.com".to_string(),
            model: "gemini-2.5-pro".to_string(),
            api_key: Some("secret_key_should_not_persist".to_string()),
            timeout_secs: 45,
        };

        storage
            .save_backend_profile(&profile)
            .expect("Save profile should succeed");

        let loaded = storage
            .load_backend_profile()
            .expect("Load profile query should succeed")
            .expect("Profile should exist");

        assert_eq!(loaded.name, "Gemini Pro");
        assert_eq!(loaded.provider, ProviderKind::GoogleGemini);
        assert_eq!(loaded.model, "gemini-2.5-pro");
        assert_eq!(loaded.timeout_secs, 45);
        // CON-007: Plaintext API key is NEVER stored in database
        assert_eq!(loaded.api_key, None);
    }

    #[test]
    fn provider_secret_preference_round_trips_reference_without_secret_bytes() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("mentat.db");
        let storage = SqliteStorage::open(&db_path).unwrap();
        let profile_id = Uuid::new_v4();
        let credential_ref = format!("provider:{profile_id}");

        storage
            .save_provider_secret_preference(&ProviderSecretPreference {
                profile_id,
                credential_ref: credential_ref.clone(),
                remember_api_key: true,
            })
            .unwrap();
        drop(storage);

        let reopened = SqliteStorage::open(&db_path).unwrap();
        let loaded = reopened
            .load_provider_secret_preference(profile_id)
            .unwrap()
            .unwrap();
        assert_eq!(loaded.credential_ref, credential_ref);
        assert!(loaded.remember_api_key);
        let bytes = std::fs::read(&db_path).unwrap();
        assert!(!String::from_utf8_lossy(&bytes).contains("never-store-this-api-secret"));
    }

    #[test]
    fn test_sqlite_storage_save_and_load_snapshot_meta() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("mentat.db");

        let storage = SqliteStorage::open(&db_path).expect("DB should open");
        let repo_id = Uuid::new_v4();

        let snap = RepositorySnapshot {
            id: Uuid::new_v4(),
            repo_id,
            created_at: chrono::Utc::now(),
            tree_digest: "abcd1234efgh5678".to_string(),
            status: SnapshotStatus::Ready,
            file_count: 42,
            total_bytes: 1048576,
        };

        storage
            .save_snapshot_meta(&snap)
            .expect("Save snapshot should succeed");

        let loaded = storage
            .load_latest_snapshot(repo_id)
            .expect("Load snapshot query should succeed")
            .expect("Snapshot should exist");

        assert_eq!(loaded.repo_id, repo_id);
        assert_eq!(loaded.tree_digest, "abcd1234efgh5678");
        assert_eq!(loaded.file_count, 42);
        assert_eq!(loaded.total_bytes, 1048576);
        assert_eq!(loaded.status, SnapshotStatus::Ready);
    }

    #[test]
    fn test_imp_f005_find_repo_by_canonical_root() {
        let dir = tempdir().unwrap();
        let repo_root = dir.path().join("sample_repo");
        std::fs::create_dir_all(&repo_root).unwrap();
        let db_path = dir.path().join("mentat.db");
        let storage = SqliteStorage::open(&db_path).expect("DB should open");

        let id = Uuid::new_v4();
        let profile = RepositoryProfile {
            id,
            display_name: "Sample".to_string(),
            root_path: repo_root.clone(),
            repo_type: RepositoryType::Directory,
            consent_policy: false,
        };
        storage.save_recent_repo(&profile).unwrap();

        let found = storage
            .find_repo_by_root(&repo_root)
            .expect("lookup")
            .expect("repo should exist");
        assert_eq!(found.id, id);

        let again = storage
            .find_repo_by_root(&repo_root)
            .unwrap()
            .expect("stable lookup");
        assert_eq!(again.id, id);
    }

    fn factory_seed() -> FactoryPromptSeed {
        FactoryPromptSeed {
            profile_id: Uuid::new_v4(),
            profile_name: "기본 멘토".to_string(),
            experience_preset: ExperiencePreset::Intermediate,
            base_system_preset: SystemPreset::Intermediate,
            system_resource_key: "system.intermediate.v1".to_string(),
            system_resource_version: "cr-ux-001.1".to_string(),
            system_checksum: "system-checksum".to_string(),
            persona_resource_key: "persona.default_analyst.v1".to_string(),
            persona_resource_version: "cr-ux-001.1".to_string(),
            persona_checksum: "persona-checksum".to_string(),
        }
    }

    fn prepare_force_kill_state(storage: &SqliteStorage) -> (Uuid, Uuid) {
        let profile = storage
            .seed_factory_prompt_profile(&factory_seed())
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
                user_message: ChatMessage::new(
                    conversation.id,
                    turn_id,
                    ChatRole::User,
                    0,
                    "질문",
                    MessageStatus::Completed,
                ),
                assistant_placeholder: assistant.clone(),
            })
            .unwrap();
        storage
            .append_assistant_delta(assistant.id, "streaming")
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
        let scope = RepositoryConsentScope {
            id: Uuid::new_v4(),
            conversation_id: conversation.id,
            repository_id,
            snapshot_id,
            provider_binding: binding.clone(),
            kind: RepositoryConsentKind::RepositorySession,
            granted_at: chrono::Utc::now(),
            revoked_at: None,
        };
        storage.save_repository_consent_scope(&scope).unwrap();
        let receipt_id = Uuid::new_v4();
        let now = chrono::Utc::now();
        storage
            .prepare_tool_egress_receipt(&ToolEgressReceipt {
                id: receipt_id,
                seal_version: "CM_TOOL_EGRESS_V1".to_string(),
                trace_id,
                consent_scope_id: scope.id,
                conversation_id: conversation.id,
                turn_id,
                tool_call_id: Uuid::new_v4(),
                repository_id,
                snapshot_id,
                tool_name: RepositoryToolName::RepoStatus,
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
        (conversation.id, receipt_id)
    }

    #[test]
    fn legacy_database_migrates_to_v2_and_keeps_rows_with_backup() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("mentat.db");
        let legacy_repo_id = Uuid::new_v4();
        {
            let conn = Connection::open(&db_path).unwrap();
            conn.execute_batch(
                "CREATE TABLE recent_repositories (
                    id TEXT PRIMARY KEY,
                    display_name TEXT NOT NULL,
                    root_path TEXT NOT NULL UNIQUE,
                    repo_type TEXT NOT NULL,
                    last_opened_at TEXT NOT NULL
                );",
            )
            .unwrap();
            conn.execute(
                "INSERT INTO recent_repositories VALUES (?1, 'Legacy', 'C:/legacy', 'Git', '2026-01-01T00:00:00Z')",
                [legacy_repo_id.to_string()],
            )
            .unwrap();
        }

        let storage = SqliteStorage::open(&db_path).expect("legacy DB should migrate");

        assert_eq!(storage.schema_version().unwrap(), 6);
        assert_eq!(storage.list_recent_repos().unwrap()[0].id, legacy_repo_id);
        assert!(storage
            .migration_backup_path()
            .expect("legacy migration should create backup")
            .exists());
    }

    #[test]
    fn registered_v1_without_migration_ledger_upgrades_without_quarantine() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("mentat.db");
        let repository_id = Uuid::new_v4();
        {
            let conn = Connection::open(&db_path).unwrap();
            conn.execute_batch(
                "CREATE TABLE recent_repositories (
                    id TEXT PRIMARY KEY,
                    display_name TEXT NOT NULL,
                    root_path TEXT NOT NULL UNIQUE,
                    repo_type TEXT NOT NULL,
                    last_opened_at TEXT NOT NULL
                 );
                 CREATE TABLE saved_profiles (
                    id TEXT PRIMARY KEY, name TEXT NOT NULL, provider TEXT NOT NULL,
                    base_url TEXT NOT NULL, model TEXT NOT NULL,
                    timeout_secs INTEGER NOT NULL, updated_at TEXT NOT NULL
                 );
                 CREATE TABLE snapshot_history (
                    id TEXT PRIMARY KEY, repo_id TEXT NOT NULL, created_at TEXT NOT NULL,
                    tree_digest TEXT NOT NULL, status TEXT NOT NULL,
                    file_count INTEGER NOT NULL, total_bytes INTEGER NOT NULL
                 );
                 PRAGMA user_version = 1;",
            )
            .unwrap();
            conn.execute(
                "INSERT INTO recent_repositories VALUES (?1, 'V1', 'C:/v1', 'Git', '2026-01-01T00:00:00Z')",
                [repository_id.to_string()],
            )
            .unwrap();
        }

        let storage = SqliteStorage::open(&db_path).unwrap();

        assert_eq!(storage.schema_version().unwrap(), 6);
        assert!(storage.recovery_quarantine_path().is_none());
        assert_eq!(storage.list_recent_repos().unwrap()[0].id, repository_id);
    }

    fn create_v4_ui_preferences(db_path: &std::path::Path, width: f32, height: f32) {
        let conn = Connection::open(db_path).unwrap();
        conn.execute_batch(
            "CREATE TABLE ui_preferences (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                width_points REAL NOT NULL,
                height_points REAL NOT NULL,
                submit_mode TEXT NOT NULL,
                updated_at TEXT NOT NULL
             );
             PRAGMA user_version = 4;",
        )
        .unwrap();
        conn.execute(
            "INSERT INTO ui_preferences VALUES (1, ?1, ?2, 'EnterSend', '2026-01-01T00:00:00Z')",
            rusqlite::params![width, height],
        )
        .unwrap();
    }

    #[test]
    fn v5_migration_expands_only_the_legacy_default_window() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("mentat.db");
        create_v4_ui_preferences(&db_path, 250.0, 600.0);

        let storage = SqliteStorage::open(&db_path).unwrap();
        let preferences = storage.load_ui_preferences().unwrap();

        assert_eq!(storage.schema_version().unwrap(), 6);
        assert_eq!(preferences.width_points, 312.5);
        assert_eq!(preferences.height_points, 660.0);
        assert_eq!(preferences.layout_revision, 2);
        assert!(preferences.always_on_top);
    }

    #[test]
    fn v5_migration_preserves_a_user_resized_window() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("mentat.db");
        create_v4_ui_preferences(&db_path, 600.0, 800.0);

        let storage = SqliteStorage::open(&db_path).unwrap();
        let preferences = storage.load_ui_preferences().unwrap();

        assert_eq!(preferences.width_points, 600.0);
        assert_eq!(preferences.height_points, 800.0);
        assert_eq!(preferences.layout_revision, 2);
        assert!(preferences.always_on_top);
    }

    #[test]
    fn ui_preferences_round_trip_pin_submit_mode_and_layout_revision() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("mentat.db");
        let storage = SqliteStorage::open(&db_path).unwrap();
        storage
            .save_ui_preferences(&UiPreferences {
                width_points: 777.0,
                height_points: 555.0,
                submit_mode: mentat_core::ComposerSubmitMode::CtrlEnterSend,
                always_on_top: false,
                layout_revision: 2,
                updated_at: chrono::Utc::now(),
            })
            .unwrap();
        drop(storage);

        let reopened = SqliteStorage::open(&db_path).unwrap();
        let preferences = reopened.load_ui_preferences().unwrap();
        assert_eq!(preferences.width_points, 777.0);
        assert_eq!(preferences.height_points, 555.0);
        assert_eq!(
            preferences.submit_mode,
            mentat_core::ComposerSubmitMode::CtrlEnterSend
        );
        assert!(!preferences.always_on_top);
        assert_eq!(preferences.layout_revision, 2);
    }

    #[test]
    fn future_schema_is_rejected_without_downgrade() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("mentat.db");
        {
            let conn = Connection::open(&db_path).unwrap();
            conn.pragma_update(None, "user_version", 99).unwrap();
        }

        let error = SqliteStorage::open(&db_path)
            .err()
            .expect("future schema must be rejected");
        assert!(error.to_string().contains("STORAGE_SCHEMA_FUTURE"));
    }

    #[test]
    fn factory_profile_persists_only_resource_references() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("mentat.db");
        let storage = SqliteStorage::open(&db_path).unwrap();
        let seed = factory_seed();

        let profile = storage.seed_factory_prompt_profile(&seed).unwrap();
        let stored = storage
            .load_active_prompt_profile(profile.id)
            .unwrap()
            .expect("seeded profile should load");

        assert!(matches!(
            stored.system_source,
            PromptContentSource::FactoryRef { ref resource_key, .. }
                if resource_key == "system.intermediate.v1"
        ));
        let database_bytes = std::fs::read(&db_path).unwrap();
        assert!(!String::from_utf8_lossy(&database_bytes).contains("Answer clearly and directly"));
    }

    #[test]
    fn repository_free_conversation_and_ui_preferences_survive_reopen() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("mentat.db");
        let conversation_id;
        {
            let storage = SqliteStorage::open(&db_path).unwrap();
            let profile = storage
                .seed_factory_prompt_profile(&factory_seed())
                .unwrap();
            let conversation = storage
                .create_conversation(&NewConversation {
                    repository_id: None,
                    active_snapshot_id: None,
                    prompt_profile_id: profile.id,
                    persistence: ConversationPersistence::Durable,
                })
                .unwrap();
            conversation_id = conversation.id;
            storage
                .save_ui_preferences(&UiPreferences {
                    width_points: 600.0,
                    height_points: 800.0,
                    ..UiPreferences::default()
                })
                .unwrap();
        }

        let reopened = SqliteStorage::open(&db_path).unwrap();
        let conversation = reopened
            .load_conversation(conversation_id)
            .unwrap()
            .expect("conversation should survive reopen");
        let preferences = reopened.load_ui_preferences().unwrap();

        assert_eq!(conversation.repository_id, None);
        assert_eq!(conversation.active_snapshot_id, None);
        assert_eq!(preferences.width_points, 600.0);
        assert_eq!(preferences.height_points, 800.0);
    }

    #[test]
    fn malformed_legacy_uuid_fails_closed_instead_of_becoming_random_id() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("mentat.db");
        let storage = SqliteStorage::open(&db_path).unwrap();
        {
            let conn = Connection::open(&db_path).unwrap();
            conn.execute(
                "INSERT INTO recent_repositories (id, display_name, root_path, repo_type, last_opened_at)
                 VALUES ('not-a-uuid', 'Broken', 'C:/broken', 'Git', '2026-01-01T00:00:00Z')",
                [],
            )
            .unwrap();
        }

        let error = storage
            .list_recent_repos()
            .expect_err("malformed row must fail closed");
        assert!(error.to_string().contains("STORAGE_DECODE_UUID"));
    }

    #[test]
    fn prompt_apply_is_atomic_and_rejects_stale_expected_revision() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("mentat.db");
        let storage = SqliteStorage::open(&db_path).unwrap();
        let profile = storage
            .seed_factory_prompt_profile(&factory_seed())
            .unwrap();
        let active = storage
            .load_active_prompt_profile(profile.id)
            .unwrap()
            .unwrap();

        let applied = storage
            .apply_prompt_draft(
                active.revision.id,
                &PromptDraft {
                    profile_id: profile.id,
                    name: "사용자 멘토".to_string(),
                    experience_preset: ExperiencePreset::Custom,
                    base_system_preset: SystemPreset::Intermediate,
                    system: PromptLayerDraft::UserText(
                        "내 저장소를 간결하게 설명하세요.".to_string(),
                    ),
                    persona: PromptLayerDraft::Preserve,
                },
            )
            .unwrap();
        let stored = storage
            .load_active_prompt_profile(profile.id)
            .unwrap()
            .unwrap();

        assert_eq!(stored.revision.id, applied.id);
        assert!(matches!(
            stored.system_source,
            PromptContentSource::UserText { ref content, .. }
                if content == "내 저장소를 간결하게 설명하세요."
        ));
        assert!(matches!(
            stored.persona_source,
            PromptContentSource::FactoryRef { .. }
        ));

        let stale_error = storage
            .apply_prompt_draft(
                active.revision.id,
                &PromptDraft {
                    profile_id: profile.id,
                    name: "충돌".to_string(),
                    experience_preset: ExperiencePreset::Custom,
                    base_system_preset: SystemPreset::Intermediate,
                    system: PromptLayerDraft::UserText("덮어쓰기".to_string()),
                    persona: PromptLayerDraft::Preserve,
                },
            )
            .expect_err("stale apply must fail");
        assert!(stale_error.to_string().contains("PROMPT_REVISION_CONFLICT"));
    }

    #[test]
    fn turn_messages_preserve_markdown_and_terminal_state_after_reopen() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("mentat.db");
        let conversation_id;
        let expected_markdown = "## 결과\n\n```rust\nfn main() {}\n```\n한글 설명";
        {
            let storage = SqliteStorage::open(&db_path).unwrap();
            let profile = storage
                .seed_factory_prompt_profile(&factory_seed())
                .unwrap();
            let active = storage
                .load_active_prompt_profile(profile.id)
                .unwrap()
                .unwrap();
            let conversation = storage
                .create_conversation(&NewConversation {
                    repository_id: None,
                    active_snapshot_id: None,
                    prompt_profile_id: profile.id,
                    persistence: ConversationPersistence::Durable,
                })
                .unwrap();
            conversation_id = conversation.id;
            let turn_id = Uuid::new_v4();
            let user = ChatMessage::new(
                conversation.id,
                turn_id,
                ChatRole::User,
                0,
                "코드를 설명해 주세요.",
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
                        kernel_digest: "kernel-digest".to_string(),
                        snapshot_id: None,
                        response_contract: ResponseContract::AdvisorMarkdown,
                        audit_result_id: None,
                        started_at: chrono::Utc::now(),
                        completed_at: None,
                    },
                    user_message: user,
                    assistant_placeholder: assistant.clone(),
                })
                .unwrap();
            storage
                .append_assistant_delta(assistant.id, "## 결과\n\n```rust\n")
                .unwrap();
            storage
                .append_assistant_delta(assistant.id, "fn main() {}\n```\n한글 설명")
                .unwrap();
            storage
                .finish_turn(&TurnTerminalUpdate::AdvisorCompleted {
                    turn_id,
                    assistant_message_id: assistant.id,
                    markdown: expected_markdown.to_string(),
                    grounding_trace_id: None,
                    freshness: None,
                    completed_at: chrono::Utc::now(),
                })
                .unwrap();
        }

        let reopened = SqliteStorage::open(&db_path).unwrap();
        let conversation = reopened
            .load_conversation(conversation_id)
            .unwrap()
            .unwrap();

        assert_eq!(conversation.messages.len(), 2);
        assert_eq!(conversation.messages[1].markdown, expected_markdown);
        assert_eq!(conversation.messages[1].status, MessageStatus::Completed);
    }

    #[test]
    fn validated_audit_result_and_grounding_restore_by_turn() {
        let dir = tempdir().unwrap();
        let storage = SqliteStorage::open(dir.path().join("mentat.db")).unwrap();
        let profile = storage
            .seed_factory_prompt_profile(&factory_seed())
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
                    response_contract: ResponseContract::AuditAnswerBundle {
                        schema_version: "answer_bundle.v1".to_string(),
                    },
                    audit_result_id: None,
                    started_at: chrono::Utc::now(),
                    completed_at: None,
                },
                user_message: ChatMessage::new(
                    conversation.id,
                    turn_id,
                    ChatRole::User,
                    0,
                    "감사",
                    MessageStatus::Completed,
                ),
                assistant_placeholder: assistant.clone(),
            })
            .unwrap();
        let trace_id = Uuid::new_v4();
        let trace = GroundingTrace {
            id: trace_id,
            conversation_id: conversation.id,
            turn_id,
            snapshot_id: Some(snapshot_id),
            tool_calls: Vec::new(),
            source_refs: Vec::new(),
            egress_receipt_ids: Vec::new(),
            freshness: GroundingFreshness::FreshAtSend,
        };
        storage.prepare_grounding_trace(&trace).unwrap();
        let result = AnswerBundle {
            request_id: Uuid::new_v4(),
            snapshot_id,
            direct_answer: "검증됨".to_string(),
            claims: Vec::new(),
            evidence_map: Vec::new(),
            recommendations: Vec::new(),
            conflicts: Vec::new(),
            raw_model_response: None,
        };
        storage
            .finish_turn_with_grounding(
                &trace,
                &TurnTerminalUpdate::AuditCompleted {
                    turn_id,
                    assistant_message_id: assistant.id,
                    result: result.clone(),
                    grounding_trace_id: trace_id,
                    freshness: GroundingFreshness::FreshAtSend,
                    completed_at: chrono::Utc::now(),
                },
            )
            .unwrap();

        assert_eq!(
            storage
                .load_audit_result_for_turn(turn_id)
                .unwrap()
                .unwrap()
                .direct_answer,
            result.direct_answer
        );
        assert_eq!(
            storage
                .load_conversation(conversation.id)
                .unwrap()
                .unwrap()
                .messages[1]
                .grounding_trace_id,
            Some(trace_id)
        );
    }

    #[test]
    fn terminal_and_final_grounding_killpoint_roll_back_together() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("mentat.db");
        let storage = SqliteStorage::open(&db_path).unwrap();
        let profile = storage
            .seed_factory_prompt_profile(&factory_seed())
            .unwrap();
        let active = storage
            .load_active_prompt_profile(profile.id)
            .unwrap()
            .unwrap();
        let snapshot_id = Uuid::new_v4();
        let conversation = storage
            .create_conversation(&NewConversation {
                repository_id: Some(Uuid::new_v4()),
                active_snapshot_id: Some(snapshot_id),
                prompt_profile_id: profile.id,
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
                assistant_placeholder: assistant.clone(),
            })
            .unwrap();
        storage
            .append_assistant_delta(assistant.id, "streaming")
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
        let source_id = Uuid::new_v4();
        let call_id = Uuid::new_v4();
        let final_trace = GroundingTrace {
            id: trace_id,
            conversation_id: conversation.id,
            turn_id,
            snapshot_id: Some(snapshot_id),
            tool_calls: vec![RepositoryToolCallRecord {
                trace_id,
                call_id,
                round: 1,
                name: RepositoryToolName::ReadFileLines,
                canonical_arguments_digest: "args".to_string(),
                result_digest: Some("result".to_string()),
                content_bytes: 4,
                source_ref_ids: vec![source_id],
                status: RepositoryToolCallStatus::Completed,
            }],
            source_refs: vec![SourceRef {
                id: source_id,
                snapshot_id,
                relative_path: PathBuf::from("src/lib.rs"),
                line_start: 1,
                line_end: 1,
                content_hash: "hash".to_string(),
                excerpt: "code".to_string(),
            }],
            egress_receipt_ids: Vec::new(),
            freshness: GroundingFreshness::FreshAtSend,
        };
        let update = TurnTerminalUpdate::AdvisorCompleted {
            turn_id,
            assistant_message_id: assistant.id,
            markdown: "완료".to_string(),
            grounding_trace_id: Some(trace_id),
            freshness: Some(GroundingFreshness::FreshAtSend),
            completed_at: chrono::Utc::now(),
        };

        assert!(storage
            .finish_turn_with_grounding_killpoint(&final_trace, &update)
            .is_err());
        drop(storage);
        let storage = SqliteStorage::open(&db_path).unwrap();
        let after_kill = storage.load_conversation(conversation.id).unwrap().unwrap();
        assert_eq!(
            after_kill.messages[1].status,
            MessageStatus::Failed {
                error_code: "INTERRUPTED_BY_RESTART".to_string()
            }
        );
        assert!(storage
            .load_grounding_trace(trace_id)
            .unwrap()
            .unwrap()
            .source_refs
            .is_empty());

        assert!(storage
            .finish_turn_with_grounding(&final_trace, &update)
            .is_err());
    }

    #[test]
    fn corrupt_database_is_quarantined_before_fresh_v2_database_opens() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("mentat.db");
        std::fs::write(&db_path, b"not a sqlite database").unwrap();

        let storage = SqliteStorage::open(&db_path).expect("corrupt DB should recover explicitly");
        let quarantine = storage
            .recovery_quarantine_path()
            .expect("corrupt DB should be quarantined");

        assert_eq!(storage.schema_version().unwrap(), 6);
        assert!(quarantine.is_dir());
        assert!(quarantine.join("mentat.db").exists());
        assert!(db_path.exists());
    }

    #[test]
    fn busy_timeout_preserves_original_database_without_quarantine() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("mentat.db");
        let storage = SqliteStorage::open(&db_path).unwrap();
        let profile = BackendProfile {
            model: "preserved-model".to_string(),
            ..Default::default()
        };
        storage.save_backend_profile(&profile).unwrap();
        drop(storage);

        let lock_connection = Connection::open(&db_path).unwrap();
        lock_connection.execute_batch("BEGIN IMMEDIATE").unwrap();
        let second_open = SqliteStorage::open(&db_path);
        assert!(matches!(
            second_open,
            Err(MentatError::StorageError { code, .. })
                if code == "STORAGE_RUNTIME_OWNER_BEGIN_FAILED"
        ));
        assert!(dir
            .path()
            .read_dir()
            .unwrap()
            .filter_map(Result::ok)
            .all(|entry| !entry.file_name().to_string_lossy().contains(".quarantine-")));
        lock_connection.execute_batch("ROLLBACK").unwrap();
        drop(lock_connection);

        let reopened = SqliteStorage::open(&db_path).unwrap();
        assert_eq!(
            reopened.load_backend_profile().unwrap().unwrap().model,
            "preserved-model"
        );
    }

    #[test]
    fn recovery_transaction_error_preserves_database_without_quarantine() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("mentat.db");
        drop(SqliteStorage::open(&db_path).unwrap());
        let connection = Connection::open(&db_path).unwrap();
        connection
            .execute_batch("DROP TABLE runtime_ownership")
            .unwrap();
        drop(connection);

        let reopened = SqliteStorage::open(&db_path);
        assert!(matches!(
            reopened,
            Err(MentatError::StorageError { code, .. })
                if code == "STORAGE_RUNTIME_OWNER_WRITE_FAILED"
        ));
        assert!(db_path.exists());
        assert!(dir
            .path()
            .read_dir()
            .unwrap()
            .filter_map(Result::ok)
            .all(|entry| !entry.file_name().to_string_lossy().contains(".quarantine-")));
    }

    #[test]
    fn runtime_owner_process_helper() {
        let Ok(db_path) = std::env::var("CODEMENTAT_OWNER_HELPER_DB") else {
            return;
        };
        let ready = PathBuf::from(
            std::env::var("CODEMENTAT_OWNER_HELPER_READY").expect("ready marker path"),
        );
        let release = PathBuf::from(
            std::env::var("CODEMENTAT_OWNER_HELPER_RELEASE").expect("release marker path"),
        );
        let _storage = SqliteStorage::open(db_path).expect("helper acquires runtime owner");
        std::fs::write(&ready, b"ready").unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while !release.exists() && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert!(release.exists(), "parent did not release helper");
    }

    #[test]
    fn runtime_owner_force_kill_helper() {
        let Ok(db_path) = std::env::var("CODEMENTAT_FORCE_KILL_DB") else {
            return;
        };
        let marker =
            PathBuf::from(std::env::var("CODEMENTAT_FORCE_KILL_MARKER").expect("marker path"));
        let storage = SqliteStorage::open(db_path).expect("force-kill helper owns DB");
        let (conversation_id, receipt_id) = prepare_force_kill_state(&storage);
        std::fs::write(&marker, format!("{conversation_id}\n{receipt_id}")).unwrap();
        loop {
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
    }

    #[test]
    fn runtime_stale_contender_helper() {
        let Ok(db_path) = std::env::var("CODEMENTAT_STALE_CONTENDER_DB") else {
            return;
        };
        let marker =
            PathBuf::from(std::env::var("CODEMENTAT_STALE_CONTENDER_MARKER").expect("marker path"));
        let outcome = match SqliteStorage::open(db_path) {
            Err(MentatError::StorageError { code, .. }) if code == "STORAGE_RUNTIME_OWNED" => {
                "rejected"
            }
            Ok(storage) => {
                let profile = BackendProfile {
                    model: "contender-write".to_string(),
                    ..Default::default()
                };
                storage.save_backend_profile(&profile).unwrap();
                "wrote"
            }
            Err(error) => panic!("unexpected contender error: {error}"),
        };
        std::fs::write(marker, outcome).unwrap();
    }

    #[test]
    fn second_process_is_rejected_while_runtime_owner_is_live() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("mentat.db");
        drop(SqliteStorage::open(&db_path).unwrap());
        let ready = dir.path().join("owner-ready");
        let release = dir.path().join("owner-release");
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .arg("--exact")
            .arg("tests::runtime_owner_process_helper")
            .arg("--nocapture")
            .env("CODEMENTAT_OWNER_HELPER_DB", &db_path)
            .env("CODEMENTAT_OWNER_HELPER_READY", &ready)
            .env("CODEMENTAT_OWNER_HELPER_RELEASE", &release)
            .spawn()
            .unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !ready.exists() && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert!(ready.exists(), "child owner did not become ready");

        let second_open = SqliteStorage::open(&db_path);
        assert!(matches!(
            second_open,
            Err(MentatError::StorageError { code, .. }) if code == "STORAGE_RUNTIME_OWNED"
        ));
        std::fs::write(&release, b"release").unwrap();
        assert!(child.wait().unwrap().success());
    }

    #[test]
    fn force_killed_owner_recovers_prepared_and_orphan_on_first_reopen_without_sleep() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("mentat.db");
        drop(SqliteStorage::open(&db_path).unwrap());
        let marker = dir.path().join("force-kill-state");
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .arg("--exact")
            .arg("tests::runtime_owner_force_kill_helper")
            .arg("--nocapture")
            .env("CODEMENTAT_FORCE_KILL_DB", &db_path)
            .env("CODEMENTAT_FORCE_KILL_MARKER", &marker)
            .spawn()
            .unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !marker.exists() && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert!(marker.exists(), "force-kill helper did not become ready");
        let ids = std::fs::read_to_string(&marker).unwrap();
        let mut ids = ids.lines();
        let conversation_id = Uuid::parse_str(ids.next().unwrap()).unwrap();
        let receipt_id = Uuid::parse_str(ids.next().unwrap()).unwrap();
        child.kill().unwrap();
        let _ = child.wait().unwrap();

        let reopened = SqliteStorage::open(&db_path).expect("kernel lock must release on kill");
        let conversation = reopened
            .load_conversation(conversation_id)
            .unwrap()
            .unwrap();
        assert_eq!(
            conversation.messages[1].status,
            MessageStatus::Failed {
                error_code: "INTERRUPTED_BY_RESTART".to_string()
            }
        );
        assert_eq!(
            reopened
                .load_tool_egress_receipt(receipt_id)
                .unwrap()
                .unwrap()
                .status,
            ToolEgressStatus::OutcomeUnknown
        );
    }

    #[test]
    fn stale_timestamp_cannot_create_two_successful_runtime_writers() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("mentat.db");
        let owner = SqliteStorage::open(&db_path).unwrap();
        owner
            .lock_conn()
            .unwrap()
            .execute(
                "UPDATE runtime_ownership SET heartbeat_at = ?1 WHERE id = 1",
                [(chrono::Utc::now() - chrono::Duration::seconds(120)).to_rfc3339()],
            )
            .unwrap();
        let marker = dir.path().join("stale-contender-result");
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .arg("--exact")
            .arg("tests::runtime_stale_contender_helper")
            .arg("--nocapture")
            .env("CODEMENTAT_STALE_CONTENDER_DB", &db_path)
            .env("CODEMENTAT_STALE_CONTENDER_MARKER", &marker)
            .status()
            .unwrap();
        assert!(status.success());
        assert_eq!(std::fs::read_to_string(marker).unwrap(), "rejected");

        let profile = BackendProfile {
            model: "owner-write".to_string(),
            ..Default::default()
        };
        owner.save_backend_profile(&profile).unwrap();
        assert_eq!(
            owner.load_backend_profile().unwrap().unwrap().model,
            "owner-write"
        );
    }
}
