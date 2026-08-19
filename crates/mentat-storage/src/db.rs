use mentat_core::error::MentatError;
use mentat_core::models::{RepositoryProfile, RepositorySnapshot, RepositoryType, SnapshotStatus};
use mentat_inference::{BackendProfile, ProviderKind};
use rusqlite::{params, Connection, DatabaseName, OptionalExtension, TransactionBehavior};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};
use uuid::Uuid;

const CURRENT_SCHEMA_VERSION: u32 = 5;

#[derive(Clone)]
pub struct SqliteStorage {
    pub(crate) conn: Arc<Mutex<Connection>>,
    db_path: PathBuf,
    migration_backup_path: Option<PathBuf>,
    recovery_quarantine_path: Option<PathBuf>,
}

impl SqliteStorage {
    pub fn open(db_path: impl AsRef<Path>) -> Result<Self, MentatError> {
        let path = db_path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| MentatError::IoError(format!("DB 디렉터리 생성 실패: {}", e)))?;
        }

        let existed_nonempty = std::fs::metadata(&path)
            .map(|metadata| metadata.len() > 0)
            .unwrap_or(false);
        let (conn, migration_backup_path, recovery_quarantine_path) =
            match open_and_migrate(&path, existed_nonempty) {
                Ok((conn, backup)) => (conn, backup, None),
                Err(error) if existed_nonempty && should_quarantine(&error) => {
                    let quarantine = quarantine_database_files(&path)?;
                    let (conn, backup) = open_and_migrate(&path, false).map_err(|fresh_error| {
                        storage_error(
                            "STORAGE_RECOVERY_FAILED",
                            &format!(
                                "손상 DB 격리 후 새 DB 생성 실패: {fresh_error}; 원인: {error}"
                            ),
                        )
                    })?;
                    (conn, backup, Some(quarantine))
                }
                Err(error) => return Err(error),
            };

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
            db_path: path,
            migration_backup_path,
            recovery_quarantine_path,
        })
    }

    pub(crate) fn lock_conn(&self) -> Result<MutexGuard<'_, Connection>, MentatError> {
        self.conn.lock().map_err(|_| {
            storage_error(
                "STORAGE_LOCK_POISONED",
                "SQLite 연결 잠금이 손상되어 쓰기를 중단했습니다.",
            )
        })
    }

    pub fn schema_version(&self) -> Result<u32, MentatError> {
        let conn = self.lock_conn()?;
        schema_version_of(&conn)
    }

    pub fn migration_backup_path(&self) -> Option<&Path> {
        self.migration_backup_path.as_deref()
    }

    pub fn recovery_quarantine_path(&self) -> Option<&Path> {
        self.recovery_quarantine_path.as_deref()
    }

    pub fn save_recent_repo(&self, repo: &RepositoryProfile) -> Result<(), MentatError> {
        let conn = self.lock_conn()?;
        let repo_type_str = match repo.repo_type {
            RepositoryType::Git => "Git",
            RepositoryType::Directory => "Directory",
        };
        let now = chrono::Utc::now().to_rfc3339();
        let root_path = repo
            .root_path
            .canonicalize()
            .unwrap_or_else(|_| repo.root_path.clone());

        conn.execute(
            "INSERT INTO recent_repositories (id, display_name, root_path, repo_type, last_opened_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(id) DO UPDATE SET
                display_name = excluded.display_name,
                root_path = excluded.root_path,
                repo_type = excluded.repo_type,
                last_opened_at = excluded.last_opened_at",
            params![
                repo.id.to_string(),
                repo.display_name,
                root_path.to_string_lossy().to_string(),
                repo_type_str,
                now,
            ],
        )
        .map_err(|e| MentatError::IoError(format!("최근 저장소 저장 실패: {}", e)))?;

        Ok(())
    }

    pub fn list_recent_repos(&self) -> Result<Vec<RepositoryProfile>, MentatError> {
        let conn = self.lock_conn()?;
        let mut stmt = conn
            .prepare("SELECT id, display_name, root_path, repo_type FROM recent_repositories ORDER BY last_opened_at DESC LIMIT 10")
            .map_err(|error| storage_error("RECENT_REPOSITORY_READ_FAILED", &error.to_string()))?;
        let mut rows = stmt
            .query([])
            .map_err(|error| storage_error("RECENT_REPOSITORY_READ_FAILED", &error.to_string()))?;
        let mut repos = Vec::new();
        while let Some(row) = rows
            .next()
            .map_err(|error| storage_error("RECENT_REPOSITORY_READ_FAILED", &error.to_string()))?
        {
            let id_str: String = row.get(0).map_err(|error| {
                storage_error("RECENT_REPOSITORY_READ_FAILED", &error.to_string())
            })?;
            let display_name: String = row.get(1).map_err(|error| {
                storage_error("RECENT_REPOSITORY_READ_FAILED", &error.to_string())
            })?;
            let root_path_str: String = row.get(2).map_err(|error| {
                storage_error("RECENT_REPOSITORY_READ_FAILED", &error.to_string())
            })?;
            let repo_type_str: String = row.get(3).map_err(|error| {
                storage_error("RECENT_REPOSITORY_READ_FAILED", &error.to_string())
            })?;
            let repo_type = match repo_type_str.as_str() {
                "Git" => RepositoryType::Git,
                "Directory" => RepositoryType::Directory,
                _ => {
                    return Err(storage_error(
                        "STORAGE_DECODE_ENUM",
                        "recent_repositories.repo_type 값이 유효하지 않습니다.",
                    ))
                }
            };
            repos.push(RepositoryProfile {
                id: parse_uuid(&id_str, "recent_repositories.id")?,
                display_name,
                root_path: PathBuf::from(root_path_str),
                repo_type,
                consent_policy: false,
            });
        }

        Ok(repos)
    }

    /// [IMP-F005] Saves backend profile configuration (CON-007: API keys are never written to plaintext DB)
    pub fn save_backend_profile(&self, profile: &BackendProfile) -> Result<(), MentatError> {
        let conn = self.lock_conn()?;
        let provider_str = match profile.provider {
            ProviderKind::GoogleGemini => "GoogleGemini",
            ProviderKind::OpenRouter => "OpenRouter",
            ProviderKind::OpenAi => "OpenAi",
            ProviderKind::OpenAICompatible => "OpenAICompatible",
            ProviderKind::CustomCompatible => "CustomCompatible",
            ProviderKind::LocalMock => "LocalMock",
        };
        let now = chrono::Utc::now().to_rfc3339();

        conn.execute(
            "INSERT INTO saved_profiles (id, name, provider, base_url, model, timeout_secs, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                provider = excluded.provider,
                base_url = excluded.base_url,
                model = excluded.model,
                timeout_secs = excluded.timeout_secs,
                updated_at = excluded.updated_at",
            params![
                profile.id.to_string(),
                profile.name,
                provider_str,
                profile.base_url,
                profile.model,
                profile.timeout_secs,
                now,
            ],
        )
        .map_err(|e| MentatError::IoError(format!("백엔드 프로필 저장 실패: {}", e)))?;

        Ok(())
    }

    /// [IMP-F005] Loads active backend profile configuration
    pub fn load_backend_profile(&self) -> Result<Option<BackendProfile>, MentatError> {
        let conn = self.lock_conn()?;
        let values = conn
            .query_row(
                "SELECT id, name, provider, base_url, model, timeout_secs
                 FROM saved_profiles ORDER BY updated_at DESC LIMIT 1",
                [],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, i64>(5)?,
                    ))
                },
            )
            .optional()
            .map_err(|error| storage_error("BACKEND_PROFILE_READ_FAILED", &error.to_string()))?;
        let Some((id, name, provider, base_url, model, timeout_secs)) = values else {
            return Ok(None);
        };
        let provider = match provider.as_str() {
            "GoogleGemini" => ProviderKind::GoogleGemini,
            "OpenRouter" => ProviderKind::OpenRouter,
            "OpenAi" => ProviderKind::OpenAi,
            "OpenAICompatible" => ProviderKind::OpenAICompatible,
            "CustomCompatible" => ProviderKind::CustomCompatible,
            "LocalMock" => ProviderKind::LocalMock,
            _ => {
                return Err(storage_error(
                    "STORAGE_DECODE_ENUM",
                    "saved_profiles.provider 값이 유효하지 않습니다.",
                ))
            }
        };
        let timeout_secs = u64::try_from(timeout_secs).map_err(|_| {
            storage_error(
                "STORAGE_DECODE_INTEGER",
                "saved_profiles.timeout_secs 값이 유효하지 않습니다.",
            )
        })?;
        Ok(Some(BackendProfile {
            id: parse_uuid(&id, "saved_profiles.id")?,
            name,
            provider,
            base_url,
            model,
            api_key: None,
            timeout_secs,
        }))
    }

    /// [IMP-F005] Looks up a previously opened repository by canonical root path.
    pub fn find_repo_by_root(&self, root: &Path) -> Result<Option<RepositoryProfile>, MentatError> {
        let target = normalize_root_key(root);
        let repos = self.list_recent_repos()?;
        Ok(repos
            .into_iter()
            .find(|repo| normalize_root_key(&repo.root_path) == target))
    }

    /// [IMP-F005] Saves repository snapshot record to history
    pub fn save_snapshot_meta(&self, snapshot: &RepositorySnapshot) -> Result<(), MentatError> {
        let conn = self.lock_conn()?;
        let status_str = match snapshot.status {
            SnapshotStatus::Ready => "Ready",
            SnapshotStatus::Stale => "Stale",
            SnapshotStatus::Indexing => "Indexing",
            SnapshotStatus::Incomplete => "Incomplete",
        };

        conn.execute(
            "INSERT INTO snapshot_history (id, repo_id, created_at, tree_digest, status, file_count, total_bytes)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(id) DO UPDATE SET
                repo_id = excluded.repo_id,
                created_at = excluded.created_at,
                tree_digest = excluded.tree_digest,
                status = excluded.status,
                file_count = excluded.file_count,
                total_bytes = excluded.total_bytes",
            params![
                snapshot.id.to_string(),
                snapshot.repo_id.to_string(),
                snapshot.created_at.to_rfc3339(),
                snapshot.tree_digest,
                status_str,
                snapshot.file_count as i64,
                snapshot.total_bytes as i64,
            ],
        )
        .map_err(|e| MentatError::IoError(format!("스냅샷 저장 실패: {}", e)))?;

        Ok(())
    }

    /// [IMP-F005] Loads latest snapshot record for a repository
    pub fn load_latest_snapshot(
        &self,
        repo_id: Uuid,
    ) -> Result<Option<RepositorySnapshot>, MentatError> {
        let conn = self.lock_conn()?;
        let values = conn
            .query_row(
                "SELECT id, repo_id, created_at, tree_digest, status, file_count, total_bytes
                 FROM snapshot_history WHERE repo_id = ?1 ORDER BY created_at DESC LIMIT 1",
                [repo_id.to_string()],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, i64>(5)?,
                        row.get::<_, i64>(6)?,
                    ))
                },
            )
            .optional()
            .map_err(|error| storage_error("SNAPSHOT_READ_FAILED", &error.to_string()))?;
        let Some((id, stored_repo_id, created_at, tree_digest, status, file_count, total_bytes)) =
            values
        else {
            return Ok(None);
        };
        let status = match status.as_str() {
            "Ready" => SnapshotStatus::Ready,
            "Stale" => SnapshotStatus::Stale,
            "Indexing" => SnapshotStatus::Indexing,
            "Incomplete" => SnapshotStatus::Incomplete,
            _ => {
                return Err(storage_error(
                    "STORAGE_DECODE_ENUM",
                    "snapshot_history.status 값이 유효하지 않습니다.",
                ))
            }
        };
        Ok(Some(RepositorySnapshot {
            id: parse_uuid(&id, "snapshot_history.id")?,
            repo_id: parse_uuid(&stored_repo_id, "snapshot_history.repo_id")?,
            created_at: parse_datetime(&created_at, "snapshot_history.created_at")?,
            tree_digest,
            status,
            file_count: usize::try_from(file_count).map_err(|_| {
                storage_error(
                    "STORAGE_DECODE_INTEGER",
                    "snapshot_history.file_count 값이 유효하지 않습니다.",
                )
            })?,
            total_bytes: u64::try_from(total_bytes).map_err(|_| {
                storage_error(
                    "STORAGE_DECODE_INTEGER",
                    "snapshot_history.total_bytes 값이 유효하지 않습니다.",
                )
            })?,
        }))
    }

    pub fn db_path(&self) -> &Path {
        &self.db_path
    }
}

fn open_and_migrate(
    path: &Path,
    existed_nonempty: bool,
) -> Result<(Connection, Option<PathBuf>), MentatError> {
    let mut conn = Connection::open(path)
        .map_err(|error| MentatError::IoError(format!("SQLite 연결 실패: {error}")))?;
    configure_connection(&conn)?;
    let version = schema_version_of(&conn)?;
    if version > CURRENT_SCHEMA_VERSION {
        return Err(storage_error(
            "STORAGE_SCHEMA_FUTURE",
            &format!(
                "DB schema v{version}은 현재 지원 버전 v{CURRENT_SCHEMA_VERSION}보다 높습니다."
            ),
        ));
    }
    let backup = if existed_nonempty && version < CURRENT_SCHEMA_VERSION {
        Some(create_online_backup(&conn, path)?)
    } else {
        None
    };
    run_migrations(&mut conn, version)?;
    verify_database(&conn)?;
    Ok((conn, backup))
}

fn should_quarantine(error: &MentatError) -> bool {
    matches!(
        error,
        MentatError::StorageError { code, .. }
            if !matches!(code.as_str(), "STORAGE_SCHEMA_FUTURE" | "STORAGE_BACKUP_FAILED")
    )
}

fn quarantine_database_files(db_path: &Path) -> Result<PathBuf, MentatError> {
    let file_name = db_path.file_name().ok_or_else(|| {
        storage_error(
            "STORAGE_QUARANTINE_PATH_INVALID",
            "DB 파일 이름을 확인할 수 없습니다.",
        )
    })?;
    let timestamp = chrono::Utc::now().format("%Y%m%dT%H%M%S%.9fZ");
    let quarantine = db_path.with_file_name(format!(
        "{}.quarantine-{}",
        file_name.to_string_lossy(),
        timestamp
    ));
    std::fs::create_dir(&quarantine)
        .map_err(|error| storage_error("STORAGE_QUARANTINE_CREATE_FAILED", &error.to_string()))?;
    let sidecars = [
        db_path.to_path_buf(),
        PathBuf::from(format!("{}-wal", db_path.to_string_lossy())),
        PathBuf::from(format!("{}-shm", db_path.to_string_lossy())),
    ];
    for source in sidecars {
        if !source.exists() {
            continue;
        }
        let destination = quarantine.join(source.file_name().ok_or_else(|| {
            storage_error(
                "STORAGE_QUARANTINE_PATH_INVALID",
                "격리할 DB artifact 파일 이름을 확인할 수 없습니다.",
            )
        })?);
        std::fs::rename(&source, &destination).map_err(|error| {
            storage_error(
                "STORAGE_QUARANTINE_MOVE_FAILED",
                &format!("DB artifact 격리 실패: {error}"),
            )
        })?;
    }
    Ok(quarantine)
}

fn configure_connection(conn: &Connection) -> Result<(), MentatError> {
    conn.pragma_update(None, "foreign_keys", true)
        .map_err(|error| storage_error("STORAGE_CONFIG_FAILED", &error.to_string()))?;
    conn.busy_timeout(std::time::Duration::from_secs(5))
        .map_err(|error| storage_error("STORAGE_CONFIG_FAILED", &error.to_string()))?;
    Ok(())
}

fn schema_version_of(conn: &Connection) -> Result<u32, MentatError> {
    conn.pragma_query_value(None, "user_version", |row| row.get(0))
        .map_err(|error| storage_error("STORAGE_SCHEMA_READ_FAILED", &error.to_string()))
}

fn create_online_backup(conn: &Connection, db_path: &Path) -> Result<PathBuf, MentatError> {
    let timestamp = chrono::Utc::now().format("%Y%m%dT%H%M%S%.9fZ");
    let backup_path = PathBuf::from(format!(
        "{}.pre-cr-ux-001-{}.sqlite",
        db_path.to_string_lossy(),
        timestamp
    ));
    conn.backup(DatabaseName::Main, &backup_path, None)
        .map_err(|error| storage_error("STORAGE_BACKUP_FAILED", &error.to_string()))?;
    Ok(backup_path)
}

fn run_migrations(conn: &mut Connection, starting_version: u32) -> Result<(), MentatError> {
    let mut version = starting_version;
    if version == 0 {
        let transaction = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| storage_error("STORAGE_MIGRATION_BEGIN_FAILED", &error.to_string()))?;
        transaction
            .execute_batch(
                "CREATE TABLE IF NOT EXISTS schema_migrations (
                    version INTEGER PRIMARY KEY,
                    applied_at TEXT NOT NULL
                );
                CREATE TABLE IF NOT EXISTS recent_repositories (
                    id TEXT PRIMARY KEY,
                    display_name TEXT NOT NULL,
                    root_path TEXT NOT NULL UNIQUE,
                    repo_type TEXT NOT NULL CHECK (repo_type IN ('Git', 'Directory')),
                    last_opened_at TEXT NOT NULL
                );
                CREATE TABLE IF NOT EXISTS saved_profiles (
                    id TEXT PRIMARY KEY,
                    name TEXT NOT NULL,
                    provider TEXT NOT NULL,
                    base_url TEXT NOT NULL,
                    model TEXT NOT NULL,
                    timeout_secs INTEGER NOT NULL DEFAULT 30 CHECK (timeout_secs > 0),
                    updated_at TEXT NOT NULL
                );
                CREATE TABLE IF NOT EXISTS snapshot_history (
                    id TEXT PRIMARY KEY,
                    repo_id TEXT NOT NULL,
                    created_at TEXT NOT NULL,
                    tree_digest TEXT NOT NULL,
                    status TEXT NOT NULL CHECK (status IN ('Ready', 'Stale', 'Indexing', 'Incomplete')),
                    file_count INTEGER NOT NULL CHECK (file_count >= 0),
                    total_bytes INTEGER NOT NULL CHECK (total_bytes >= 0)
                );",
            )
            .map_err(|error| storage_error("STORAGE_MIGRATION_V1_FAILED", &error.to_string()))?;
        transaction
            .execute(
                "INSERT INTO schema_migrations (version, applied_at) VALUES (1, ?1)
                 ON CONFLICT(version) DO NOTHING",
                [chrono::Utc::now().to_rfc3339()],
            )
            .map_err(|error| storage_error("STORAGE_MIGRATION_V1_FAILED", &error.to_string()))?;
        transaction
            .pragma_update(None, "user_version", 1)
            .map_err(|error| storage_error("STORAGE_MIGRATION_V1_FAILED", &error.to_string()))?;
        transaction
            .commit()
            .map_err(|error| storage_error("STORAGE_MIGRATION_V1_FAILED", &error.to_string()))?;
        version = 1;
    }

    if version == 1 {
        let transaction = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| storage_error("STORAGE_MIGRATION_BEGIN_FAILED", &error.to_string()))?;
        transaction
            .execute_batch(
                "CREATE TABLE IF NOT EXISTS schema_migrations (
                    version INTEGER PRIMARY KEY,
                    applied_at TEXT NOT NULL
                );
                CREATE TABLE prompt_profiles (
                    id TEXT PRIMARY KEY,
                    name TEXT NOT NULL,
                    experience_preset TEXT NOT NULL,
                    base_system_preset TEXT NOT NULL,
                    active_revision_id TEXT NOT NULL,
                    factory_system_version TEXT NOT NULL,
                    factory_persona_version TEXT NOT NULL,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL
                );
                CREATE TABLE prompt_content_versions (
                    id TEXT PRIMARY KEY,
                    profile_id TEXT NOT NULL REFERENCES prompt_profiles(id) ON DELETE CASCADE,
                    layer TEXT NOT NULL CHECK (layer IN ('System', 'Persona')),
                    version INTEGER NOT NULL CHECK (version > 0),
                    source_kind TEXT NOT NULL CHECK (source_kind IN ('FactoryRef', 'UserText', 'RestoredText')),
                    resource_key TEXT,
                    resource_version TEXT,
                    content TEXT,
                    checksum TEXT NOT NULL,
                    restored_from TEXT,
                    parent_version_id TEXT,
                    created_at TEXT NOT NULL,
                    UNIQUE(profile_id, layer, version),
                    CHECK (
                        (source_kind = 'FactoryRef' AND resource_key IS NOT NULL AND resource_version IS NOT NULL AND content IS NULL)
                        OR (source_kind = 'UserText' AND resource_key IS NULL AND resource_version IS NULL AND content IS NOT NULL)
                        OR (source_kind = 'RestoredText' AND resource_key IS NULL AND resource_version IS NULL AND content IS NOT NULL AND restored_from IS NOT NULL)
                    )
                );
                CREATE TABLE prompt_profile_revisions (
                    id TEXT PRIMARY KEY,
                    profile_id TEXT NOT NULL REFERENCES prompt_profiles(id) ON DELETE CASCADE,
                    revision INTEGER NOT NULL CHECK (revision > 0),
                    system_version_id TEXT REFERENCES prompt_content_versions(id) ON DELETE SET NULL,
                    persona_version_id TEXT REFERENCES prompt_content_versions(id) ON DELETE SET NULL,
                    system_checksum TEXT NOT NULL,
                    persona_checksum TEXT NOT NULL,
                    content_deleted INTEGER NOT NULL DEFAULT 0 CHECK (content_deleted IN (0, 1)),
                    expected_previous_revision_id TEXT,
                    created_at TEXT NOT NULL,
                    UNIQUE(profile_id, revision)
                );
                CREATE TABLE conversations (
                    id TEXT PRIMARY KEY,
                    repository_id TEXT,
                    active_snapshot_id TEXT,
                    prompt_profile_id TEXT NOT NULL REFERENCES prompt_profiles(id),
                    compact_summary TEXT,
                    persistence TEXT NOT NULL CHECK (persistence IN ('Durable', 'Ephemeral')),
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL
                );
                CREATE TABLE conversation_turns (
                    id TEXT PRIMARY KEY,
                    conversation_id TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
                    sequence INTEGER NOT NULL CHECK (sequence > 0),
                    prompt_profile_id TEXT NOT NULL,
                    prompt_profile_revision_id TEXT NOT NULL,
                    kernel_version TEXT NOT NULL,
                    kernel_digest TEXT NOT NULL,
                    snapshot_id TEXT,
                    response_contract TEXT NOT NULL,
                    audit_result_id TEXT,
                    started_at TEXT NOT NULL,
                    completed_at TEXT,
                    UNIQUE(conversation_id, sequence)
                );
                CREATE TABLE chat_messages (
                    id TEXT PRIMARY KEY,
                    conversation_id TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
                    turn_id TEXT NOT NULL REFERENCES conversation_turns(id) ON DELETE CASCADE,
                    role TEXT NOT NULL CHECK (role IN ('User', 'Assistant')),
                    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
                    markdown TEXT NOT NULL,
                    status TEXT NOT NULL,
                    error_code TEXT,
                    grounding_trace_id TEXT,
                    grounding_freshness TEXT,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL,
                    UNIQUE(conversation_id, ordinal)
                );
                CREATE TABLE ui_preferences (
                    id INTEGER PRIMARY KEY CHECK (id = 1),
                    width_points REAL NOT NULL,
                    height_points REAL NOT NULL,
                    submit_mode TEXT NOT NULL CHECK (submit_mode IN ('EnterSend', 'CtrlEnterSend')),
                    updated_at TEXT NOT NULL
                );",
            )
            .map_err(|error| storage_error("STORAGE_MIGRATION_V2_FAILED", &error.to_string()))?;
        transaction
            .execute(
                "INSERT INTO schema_migrations (version, applied_at) VALUES (2, ?1)
                 ON CONFLICT(version) DO NOTHING",
                [chrono::Utc::now().to_rfc3339()],
            )
            .map_err(|error| storage_error("STORAGE_MIGRATION_V2_FAILED", &error.to_string()))?;
        transaction
            .pragma_update(None, "user_version", 2)
            .map_err(|error| storage_error("STORAGE_MIGRATION_V2_FAILED", &error.to_string()))?;
        transaction
            .commit()
            .map_err(|error| storage_error("STORAGE_MIGRATION_V2_FAILED", &error.to_string()))?;
        version = 2;
    }

    if version == 2 {
        let transaction = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| storage_error("STORAGE_MIGRATION_BEGIN_FAILED", &error.to_string()))?;
        transaction
            .execute_batch(
                "CREATE TABLE IF NOT EXISTS schema_migrations (
                    version INTEGER PRIMARY KEY,
                    applied_at TEXT NOT NULL
                );
                CREATE TABLE grounding_traces (
                    id TEXT PRIMARY KEY,
                    conversation_id TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
                    turn_id TEXT NOT NULL REFERENCES conversation_turns(id) ON DELETE CASCADE,
                    snapshot_id TEXT,
                    freshness TEXT NOT NULL,
                    created_at TEXT NOT NULL
                );
                CREATE TABLE tool_call_records (
                    trace_id TEXT NOT NULL REFERENCES grounding_traces(id) ON DELETE CASCADE,
                    call_id TEXT PRIMARY KEY,
                    round INTEGER NOT NULL,
                    tool_name TEXT NOT NULL,
                    canonical_arguments_digest TEXT NOT NULL,
                    result_digest TEXT,
                    content_bytes INTEGER NOT NULL,
                    source_ref_ids TEXT NOT NULL,
                    status TEXT NOT NULL
                );
                CREATE TABLE source_refs (
                    id TEXT PRIMARY KEY,
                    trace_id TEXT NOT NULL REFERENCES grounding_traces(id) ON DELETE CASCADE,
                    ordinal INTEGER NOT NULL,
                    snapshot_id TEXT NOT NULL,
                    relative_path TEXT NOT NULL,
                    line_start INTEGER NOT NULL,
                    line_end INTEGER NOT NULL,
                    content_hash TEXT NOT NULL,
                    excerpt TEXT NOT NULL,
                    UNIQUE(trace_id, ordinal)
                );
                CREATE TABLE audit_turn_results (
                    id TEXT PRIMARY KEY,
                    turn_id TEXT NOT NULL UNIQUE REFERENCES conversation_turns(id) ON DELETE CASCADE,
                    schema_version TEXT NOT NULL,
                    validated_bundle TEXT NOT NULL,
                    created_at TEXT NOT NULL
                );",
            )
            .map_err(|error| storage_error("STORAGE_MIGRATION_V3_FAILED", &error.to_string()))?;
        transaction
            .execute(
                "INSERT INTO schema_migrations (version, applied_at) VALUES (3, ?1)
                 ON CONFLICT(version) DO NOTHING",
                [chrono::Utc::now().to_rfc3339()],
            )
            .map_err(|error| storage_error("STORAGE_MIGRATION_V3_FAILED", &error.to_string()))?;
        transaction
            .pragma_update(None, "user_version", 3)
            .map_err(|error| storage_error("STORAGE_MIGRATION_V3_FAILED", &error.to_string()))?;
        transaction
            .commit()
            .map_err(|error| storage_error("STORAGE_MIGRATION_V3_FAILED", &error.to_string()))?;
        version = 3;
    }

    if version == 3 {
        let transaction = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| storage_error("STORAGE_MIGRATION_BEGIN_FAILED", &error.to_string()))?;
        transaction
            .execute_batch(
                "CREATE TABLE IF NOT EXISTS schema_migrations (
                    version INTEGER PRIMARY KEY,
                    applied_at TEXT NOT NULL
                );
                CREATE TABLE repository_consent_scopes (
                    id TEXT PRIMARY KEY,
                    conversation_id TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
                    repository_id TEXT NOT NULL,
                    snapshot_id TEXT NOT NULL,
                    provider_binding TEXT NOT NULL,
                    consent_kind TEXT NOT NULL,
                    request_once_turn_id TEXT,
                    granted_at TEXT NOT NULL,
                    revoked_at TEXT
                );
                CREATE TABLE tool_egress_receipts (
                    id TEXT PRIMARY KEY,
                    seal_version TEXT NOT NULL,
                    trace_id TEXT NOT NULL REFERENCES grounding_traces(id) ON DELETE CASCADE,
                    consent_scope_id TEXT NOT NULL REFERENCES repository_consent_scopes(id) ON DELETE CASCADE,
                    conversation_id TEXT NOT NULL,
                    turn_id TEXT NOT NULL,
                    tool_call_id TEXT NOT NULL,
                    repository_id TEXT NOT NULL,
                    snapshot_id TEXT NOT NULL,
                    tool_name TEXT NOT NULL,
                    canonical_refs TEXT NOT NULL,
                    provider_binding TEXT NOT NULL,
                    semantic_payload_digest TEXT NOT NULL,
                    exact_provider_body_digest TEXT NOT NULL,
                    canonical_digest TEXT NOT NULL,
                    status TEXT NOT NULL,
                    prepared_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL
                );",
            )
            .map_err(|error| storage_error("STORAGE_MIGRATION_V4_FAILED", &error.to_string()))?;
        transaction
            .execute(
                "INSERT INTO schema_migrations (version, applied_at) VALUES (4, ?1)
                 ON CONFLICT(version) DO NOTHING",
                [chrono::Utc::now().to_rfc3339()],
            )
            .map_err(|error| storage_error("STORAGE_MIGRATION_V4_FAILED", &error.to_string()))?;
        transaction
            .pragma_update(None, "user_version", 4)
            .map_err(|error| storage_error("STORAGE_MIGRATION_V4_FAILED", &error.to_string()))?;
        transaction
            .commit()
            .map_err(|error| storage_error("STORAGE_MIGRATION_V4_FAILED", &error.to_string()))?;
        version = 4;
    }

    if version == 4 {
        let transaction = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| storage_error("STORAGE_MIGRATION_BEGIN_FAILED", &error.to_string()))?;
        transaction
            .execute_batch(
                "CREATE TABLE IF NOT EXISTS schema_migrations (
                    version INTEGER PRIMARY KEY,
                    applied_at TEXT NOT NULL
                );
                ALTER TABLE ui_preferences
                    ADD COLUMN always_on_top INTEGER NOT NULL DEFAULT 1
                    CHECK (always_on_top IN (0, 1));
                ALTER TABLE ui_preferences
                    ADD COLUMN layout_revision INTEGER NOT NULL DEFAULT 1
                    CHECK (layout_revision > 0);
                CREATE TABLE provider_secret_preferences (
                    profile_id TEXT PRIMARY KEY,
                    credential_ref TEXT NOT NULL UNIQUE,
                    remember_api_key INTEGER NOT NULL CHECK (remember_api_key IN (0, 1)),
                    updated_at TEXT NOT NULL
                );
                UPDATE ui_preferences
                   SET width_points = 312.5,
                       height_points = 660.0
                 WHERE ABS(width_points - 250.0) <= 1.0
                   AND ABS(height_points - 600.0) <= 1.0
                   AND layout_revision = 1;
                UPDATE ui_preferences SET layout_revision = 2 WHERE layout_revision = 1;",
            )
            .map_err(|error| storage_error("STORAGE_MIGRATION_V5_FAILED", &error.to_string()))?;
        transaction
            .execute(
                "INSERT INTO schema_migrations (version, applied_at) VALUES (5, ?1)
                 ON CONFLICT(version) DO NOTHING",
                [chrono::Utc::now().to_rfc3339()],
            )
            .map_err(|error| storage_error("STORAGE_MIGRATION_V5_FAILED", &error.to_string()))?;
        transaction
            .pragma_update(None, "user_version", 5)
            .map_err(|error| storage_error("STORAGE_MIGRATION_V5_FAILED", &error.to_string()))?;
        transaction
            .commit()
            .map_err(|error| storage_error("STORAGE_MIGRATION_V5_FAILED", &error.to_string()))?;
    }
    Ok(())
}

fn verify_database(conn: &Connection) -> Result<(), MentatError> {
    let integrity: String = conn
        .query_row("PRAGMA integrity_check", [], |row| row.get(0))
        .map_err(|error| storage_error("STORAGE_INTEGRITY_FAILED", &error.to_string()))?;
    if integrity != "ok" {
        return Err(storage_error("STORAGE_INTEGRITY_FAILED", &integrity));
    }
    let foreign_key_error_count: u64 = conn
        .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
            row.get(0)
        })
        .map_err(|error| storage_error("STORAGE_FOREIGN_KEY_FAILED", &error.to_string()))?;
    if foreign_key_error_count != 0 {
        return Err(storage_error(
            "STORAGE_FOREIGN_KEY_FAILED",
            &format!("foreign key 위반 {foreign_key_error_count}건"),
        ));
    }
    Ok(())
}

pub(crate) fn parse_uuid(value: &str, field: &str) -> Result<Uuid, MentatError> {
    Uuid::parse_str(value).map_err(|_| {
        storage_error(
            "STORAGE_DECODE_UUID",
            &format!("{field} UUID가 유효하지 않습니다."),
        )
    })
}

pub(crate) fn parse_datetime(
    value: &str,
    field: &str,
) -> Result<chrono::DateTime<chrono::Utc>, MentatError> {
    chrono::DateTime::parse_from_rfc3339(value)
        .map(|datetime| datetime.with_timezone(&chrono::Utc))
        .map_err(|_| {
            storage_error(
                "STORAGE_DECODE_DATETIME",
                &format!("{field} timestamp가 RFC3339가 아닙니다."),
            )
        })
}

pub(crate) fn storage_error(code: &str, message: &str) -> MentatError {
    MentatError::StorageError {
        code: code.to_string(),
        message: message.to_string(),
    }
}

/// Windows-safe comparison key for the same repository root across runs.
pub fn normalize_root_key(path: &Path) -> String {
    let raw = path
        .canonicalize()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| path.to_string_lossy().into_owned());
    let trimmed = raw
        .strip_prefix(r"\\?\")
        .or_else(|| raw.strip_prefix("//?/"))
        .unwrap_or(&raw);
    trimmed
        .replace('/', "\\")
        .trim_end_matches('\\')
        .to_lowercase()
}
