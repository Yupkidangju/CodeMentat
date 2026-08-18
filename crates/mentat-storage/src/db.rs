use mentat_core::error::MentatError;
use mentat_core::models::{RepositoryProfile, RepositoryType};
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
                updated_at TEXT NOT NULL
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

    pub fn db_path(&self) -> &Path {
        &self.db_path
    }
}
