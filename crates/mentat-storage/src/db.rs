use mentat_core::error::MentatError;
use mentat_core::models::{RepositoryProfile, RepositorySnapshot, RepositoryType, SnapshotStatus};
use mentat_inference::{BackendProfile, ProviderKind};
use rusqlite::{params, Connection};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use uuid::Uuid;

pub struct SqliteStorage {
    conn: Arc<Mutex<Connection>>,
    db_path: PathBuf,
}

impl SqliteStorage {
    pub fn open(db_path: impl AsRef<Path>) -> Result<Self, MentatError> {
        let path = db_path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| MentatError::IoError(format!("DB 디렉터리 생성 실패: {}", e)))?;
        }

        let conn = Connection::open(&path)
            .map_err(|e| MentatError::IoError(format!("SQLite 연결 실패: {}", e)))?;

        let storage = Self {
            conn: Arc::new(Mutex::new(conn)),
            db_path: path,
        };

        storage.run_migrations()?;
        Ok(storage)
    }

    fn run_migrations(&self) -> Result<(), MentatError> {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS recent_repositories (
                id TEXT PRIMARY KEY,
                display_name TEXT NOT NULL,
                root_path TEXT NOT NULL UNIQUE,
                repo_type TEXT NOT NULL,
                last_opened_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS saved_profiles (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                provider TEXT NOT NULL,
                base_url TEXT NOT NULL,
                model TEXT NOT NULL,
                timeout_secs INTEGER NOT NULL DEFAULT 30,
                updated_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS snapshot_history (
                id TEXT PRIMARY KEY,
                repo_id TEXT NOT NULL,
                created_at TEXT NOT NULL,
                tree_digest TEXT NOT NULL,
                status TEXT NOT NULL,
                file_count INTEGER NOT NULL,
                total_bytes INTEGER NOT NULL
            );",
        )
        .map_err(|e| MentatError::IoError(format!("SQLite 마이그레이션 실패: {}", e)))?;

        Ok(())
    }

    pub fn save_recent_repo(&self, repo: &RepositoryProfile) -> Result<(), MentatError> {
        let conn = self.conn.lock().unwrap();
        let repo_type_str = match repo.repo_type {
            RepositoryType::Git => "Git",
            RepositoryType::Directory => "Directory",
        };
        let now = chrono::Utc::now().to_rfc3339();

        conn.execute(
            "INSERT OR REPLACE INTO recent_repositories (id, display_name, root_path, repo_type, last_opened_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                repo.id.to_string(),
                repo.display_name,
                repo.root_path.to_string_lossy().to_string(),
                repo_type_str,
                now,
            ],
        )
        .map_err(|e| MentatError::IoError(format!("최근 저장소 저장 실패: {}", e)))?;

        Ok(())
    }

    pub fn list_recent_repos(&self) -> Result<Vec<RepositoryProfile>, MentatError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT id, display_name, root_path, repo_type FROM recent_repositories ORDER BY last_opened_at DESC LIMIT 10")
            .map_err(|e| MentatError::IoError(e.to_string()))?;

        let rows = stmt
            .query_map([], |row| {
                let id_str: String = row.get(0)?;
                let display_name: String = row.get(1)?;
                let root_path_str: String = row.get(2)?;
                let repo_type_str: String = row.get(3)?;

                let id = Uuid::parse_str(&id_str).unwrap_or_else(|_| Uuid::new_v4());
                let repo_type = if repo_type_str == "Git" {
                    RepositoryType::Git
                } else {
                    RepositoryType::Directory
                };

                Ok(RepositoryProfile {
                    id,
                    display_name,
                    root_path: PathBuf::from(root_path_str),
                    repo_type,
                    consent_policy: false,
                })
            })
            .map_err(|e| MentatError::IoError(e.to_string()))?;

        let mut repos = Vec::new();
        for row in rows.flatten() {
            repos.push(row);
        }

        Ok(repos)
    }

    /// [IMP-F005] Saves backend profile configuration (CON-007: API keys are never written to plaintext DB)
    pub fn save_backend_profile(&self, profile: &BackendProfile) -> Result<(), MentatError> {
        let conn = self.conn.lock().unwrap();
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
            "INSERT OR REPLACE INTO saved_profiles (id, name, provider, base_url, model, timeout_secs, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
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
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT id, name, provider, base_url, model, timeout_secs FROM saved_profiles ORDER BY updated_at DESC LIMIT 1")
            .map_err(|e| MentatError::IoError(e.to_string()))?;

        let mut rows = stmt
            .query_map([], |row| {
                let id_str: String = row.get(0)?;
                let name: String = row.get(1)?;
                let provider_str: String = row.get(2)?;
                let base_url: String = row.get(3)?;
                let model: String = row.get(4)?;
                let timeout_secs: u64 = row.get(5).unwrap_or(30);

                let id = Uuid::parse_str(&id_str).unwrap_or_else(|_| Uuid::new_v4());
                let provider = match provider_str.as_str() {
                    "GoogleGemini" => ProviderKind::GoogleGemini,
                    "OpenRouter" => ProviderKind::OpenRouter,
                    "OpenAi" => ProviderKind::OpenAi,
                    "OpenAICompatible" => ProviderKind::OpenAICompatible,
                    "LocalMock" => ProviderKind::LocalMock,
                    _ => ProviderKind::CustomCompatible,
                };

                Ok(BackendProfile {
                    id,
                    name,
                    provider,
                    base_url,
                    model,
                    api_key: None, // Stored in-memory session only
                    timeout_secs,
                })
            })
            .map_err(|e| MentatError::IoError(e.to_string()))?;

        if let Some(res) = rows.next() {
            Ok(Some(res.map_err(|e| MentatError::IoError(e.to_string()))?))
        } else {
            Ok(None)
        }
    }

    /// [IMP-F005] Saves repository snapshot record to history
    pub fn save_snapshot_meta(&self, snapshot: &RepositorySnapshot) -> Result<(), MentatError> {
        let conn = self.conn.lock().unwrap();
        let status_str = match snapshot.status {
            SnapshotStatus::Ready => "Ready",
            SnapshotStatus::Stale => "Stale",
            SnapshotStatus::Indexing => "Indexing",
        };

        conn.execute(
            "INSERT OR REPLACE INTO snapshot_history (id, repo_id, created_at, tree_digest, status, file_count, total_bytes)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
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
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT id, repo_id, created_at, tree_digest, status, file_count, total_bytes FROM snapshot_history WHERE repo_id = ?1 ORDER BY created_at DESC LIMIT 1")
            .map_err(|e| MentatError::IoError(e.to_string()))?;

        let mut rows = stmt
            .query_map(params![repo_id.to_string()], |row| {
                let id_str: String = row.get(0)?;
                let repo_id_str: String = row.get(1)?;
                let created_at_str: String = row.get(2)?;
                let tree_digest: String = row.get(3)?;
                let status_str: String = row.get(4)?;
                let file_count: i64 = row.get(5)?;
                let total_bytes: i64 = row.get(6)?;

                let id = Uuid::parse_str(&id_str).unwrap_or_else(|_| Uuid::new_v4());
                let repo_id = Uuid::parse_str(&repo_id_str).unwrap_or_else(|_| Uuid::new_v4());
                let created_at = chrono::DateTime::parse_from_rfc3339(&created_at_str)
                    .map(|dt| dt.with_timezone(&chrono::Utc))
                    .unwrap_or_else(|_| chrono::Utc::now());

                let status = match status_str.as_str() {
                    "Stale" => SnapshotStatus::Stale,
                    "Indexing" => SnapshotStatus::Indexing,
                    _ => SnapshotStatus::Ready,
                };

                Ok(RepositorySnapshot {
                    id,
                    repo_id,
                    created_at,
                    tree_digest,
                    status,
                    file_count: file_count as usize,
                    total_bytes: total_bytes as u64,
                })
            })
            .map_err(|e| MentatError::IoError(e.to_string()))?;

        if let Some(res) = rows.next() {
            Ok(Some(res.map_err(|e| MentatError::IoError(e.to_string()))?))
        } else {
            Ok(None)
        }
    }

    pub fn db_path(&self) -> &Path {
        &self.db_path
    }
}
