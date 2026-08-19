use crate::error::MentatError;
use crate::models::{
    Conversation, DeleteReceipt, NewConversation, PromptContentVersion, PromptDraft, PromptLayer,
    PromptProfileRevision, StoredPromptProfile, TurnStart, TurnTerminalUpdate, UiPreferences,
};
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

#[async_trait]
pub trait ConversationStore: Send + Sync {
    async fn create_conversation(
        &self,
        draft: NewConversation,
    ) -> Result<Conversation, MentatError>;
    async fn load_conversation(&self, id: uuid::Uuid) -> Result<Option<Conversation>, MentatError>;
    async fn begin_turn(&self, start: TurnStart) -> Result<(), MentatError>;
    async fn append_assistant_delta(
        &self,
        message_id: uuid::Uuid,
        delta: &str,
    ) -> Result<(), MentatError>;
    async fn finish_turn(&self, terminal: TurnTerminalUpdate) -> Result<(), MentatError>;
    async fn delete_conversation(&self, id: uuid::Uuid) -> Result<DeleteReceipt, MentatError>;
}

#[async_trait]
pub trait PromptProfileStore: Send + Sync {
    async fn load_active_prompt_profile(
        &self,
        id: uuid::Uuid,
    ) -> Result<Option<StoredPromptProfile>, MentatError>;
    async fn apply_prompt_draft(
        &self,
        expected_revision_id: uuid::Uuid,
        draft: PromptDraft,
    ) -> Result<PromptProfileRevision, MentatError>;
    async fn list_prompt_versions(
        &self,
        profile_id: uuid::Uuid,
        layer: PromptLayer,
    ) -> Result<Vec<PromptContentVersion>, MentatError>;
}

#[async_trait]
pub trait UiPreferencesStore: Send + Sync {
    async fn load_ui_preferences(&self) -> Result<UiPreferences, MentatError>;
    async fn save_ui_preferences(&self, preferences: UiPreferences) -> Result<(), MentatError>;
}

pub trait SecretStore: Send + Sync {
    fn is_available(&self) -> Result<(), MentatError>;
    fn put_secret(&self, credential_ref: &str, secret: &str) -> Result<(), MentatError>;
    fn get_secret(&self, credential_ref: &str) -> Result<Option<String>, MentatError>;
    fn delete_secret(&self, credential_ref: &str) -> Result<(), MentatError>;
}
