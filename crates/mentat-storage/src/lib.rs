pub mod db;

pub use db::SqliteStorage;

#[cfg(test)]
mod tests {
    use super::*;
    use mentat_core::models::{RepositoryProfile, RepositoryType};
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
}
