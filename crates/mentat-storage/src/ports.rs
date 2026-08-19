use crate::SqliteStorage;
use async_trait::async_trait;
use mentat_core::{
    Conversation, ConversationStore, DeleteReceipt, MentatError, NewConversation,
    PromptContentVersion, PromptDraft, PromptLayer, PromptProfileRevision, PromptProfileStore,
    StoredPromptProfile, TurnStart, TurnTerminalUpdate, UiPreferences, UiPreferencesStore,
};
use uuid::Uuid;

#[async_trait]
impl ConversationStore for SqliteStorage {
    async fn create_conversation(
        &self,
        draft: NewConversation,
    ) -> Result<Conversation, MentatError> {
        SqliteStorage::create_conversation(self, &draft)
    }

    async fn load_conversation(&self, id: Uuid) -> Result<Option<Conversation>, MentatError> {
        SqliteStorage::load_conversation(self, id)
    }

    async fn begin_turn(&self, start: TurnStart) -> Result<(), MentatError> {
        SqliteStorage::begin_turn(self, &start)
    }

    async fn append_assistant_delta(
        &self,
        message_id: Uuid,
        delta: &str,
    ) -> Result<(), MentatError> {
        SqliteStorage::append_assistant_delta(self, message_id, delta)
    }

    async fn finish_turn(&self, terminal: TurnTerminalUpdate) -> Result<(), MentatError> {
        SqliteStorage::finish_turn(self, &terminal)
    }

    async fn delete_conversation(&self, id: Uuid) -> Result<DeleteReceipt, MentatError> {
        SqliteStorage::delete_conversation(self, id)
    }
}

#[async_trait]
impl PromptProfileStore for SqliteStorage {
    async fn load_active_prompt_profile(
        &self,
        id: Uuid,
    ) -> Result<Option<StoredPromptProfile>, MentatError> {
        SqliteStorage::load_active_prompt_profile(self, id)
    }

    async fn apply_prompt_draft(
        &self,
        expected_revision_id: Uuid,
        draft: PromptDraft,
    ) -> Result<PromptProfileRevision, MentatError> {
        SqliteStorage::apply_prompt_draft(self, expected_revision_id, &draft)
    }

    async fn list_prompt_versions(
        &self,
        profile_id: Uuid,
        layer: PromptLayer,
    ) -> Result<Vec<PromptContentVersion>, MentatError> {
        SqliteStorage::list_prompt_versions(self, profile_id, layer)
    }
}

#[async_trait]
impl UiPreferencesStore for SqliteStorage {
    async fn load_ui_preferences(&self) -> Result<UiPreferences, MentatError> {
        SqliteStorage::load_ui_preferences(self)
    }

    async fn save_ui_preferences(&self, preferences: UiPreferences) -> Result<(), MentatError> {
        SqliteStorage::save_ui_preferences(self, &preferences)
    }
}
