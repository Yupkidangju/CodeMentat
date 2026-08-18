pub mod db;

pub use db::SqliteStorage;

#[cfg(test)]
mod tests {
    use super::*;
    use mentat_core::models::{
        RepositoryProfile, RepositorySnapshot, RepositoryType, SnapshotStatus,
    };
    use mentat_inference::{BackendProfile, ProviderKind};
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
}
