use crate::error::MentatError;
use crate::models::{FileRecord, RepositoryProfile, RepositorySnapshot};
use async_trait::async_trait;
use std::path::{Path, PathBuf};

#[async_trait]
pub trait RepositoryReader: Send + Sync {
    fn root_path(&self) -> &Path;
    fn profile(&self) -> &RepositoryProfile;
    async fn scan_files(&self) -> Result<Vec<FileRecord>, MentatError>;
    async fn read_file_content(&self, relative_path: &Path) -> Result<String, MentatError>;
    async fn read_file_lines(
        &self,
        relative_path: &Path,
        start_line: usize,
        end_line: usize,
    ) -> Result<String, MentatError>;
    async fn create_snapshot(&self) -> Result<RepositorySnapshot, MentatError>;
}

#[async_trait]
pub trait StoragePort: Send + Sync {
    async fn get_app_data_dir(&self) -> Result<PathBuf, MentatError>;
    async fn save_recent_repo(&self, repo: &RepositoryProfile) -> Result<(), MentatError>;
    async fn list_recent_repos(&self) -> Result<Vec<RepositoryProfile>, MentatError>;
}
