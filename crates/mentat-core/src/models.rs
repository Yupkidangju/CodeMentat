use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RepositoryType {
    Git,
    Directory,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepositoryProfile {
    pub id: Uuid,
    pub display_name: String,
    pub root_path: PathBuf,
    pub repo_type: RepositoryType,
    pub consent_policy: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SnapshotStatus {
    Ready,
    Stale,
    Indexing,
    Incomplete,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepositorySnapshot {
    pub id: Uuid,
    pub repo_id: Uuid,
    pub created_at: DateTime<Utc>,
    pub tree_digest: String,
    pub status: SnapshotStatus,
    pub file_count: usize,
    pub total_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FileKind {
    SourceCode,
    Documentation,
    Manifest,
    Configuration,
    Asset,
    Binary,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileRecord {
    pub relative_path: PathBuf,
    pub kind: FileKind,
    pub size_bytes: u64,
    pub content_hash: String,
    pub is_text: bool,
    pub line_count: Option<usize>,
    /// First bytes of text used to verify cloud citation excerpts without a second read.
    #[serde(default)]
    pub text_preview: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceRef {
    pub id: Uuid,
    pub snapshot_id: Uuid,
    pub relative_path: PathBuf,
    pub line_start: usize,
    pub line_end: usize,
    pub content_hash: String,
    pub excerpt: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClaimClassification {
    Observed,
    Inferred,
    Proposed,
    Conflict,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claim {
    pub id: Uuid,
    pub classification: ClaimClassification,
    pub statement: String,
    pub confidence: f32,
    pub evidence_ids: Vec<Uuid>,
    pub rationale: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RecommendationBasis {
    GeneralPractice,
    ProjectIntentAligned,
    NeedsUserDecision,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Recommendation {
    pub id: Uuid,
    pub basis: RecommendationBasis,
    pub impact: String,
    pub rationale: String,
    pub evidence_ids: Vec<Uuid>,
    pub decision_required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConflictItem {
    pub id: Uuid,
    pub side_a: String,
    pub side_b: String,
    pub evidence_ids: Vec<Uuid>,
    pub impact: String,
    pub unresolved_question: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnswerBundle {
    pub request_id: Uuid,
    pub snapshot_id: Uuid,
    pub direct_answer: String,
    pub claims: Vec<Claim>,
    pub evidence_map: Vec<EvidenceRef>,
    pub recommendations: Vec<Recommendation>,
    pub conflicts: Vec<ConflictItem>,
    pub raw_model_response: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Conversation {
    pub id: Uuid,
    pub repository_id: Option<Uuid>,
    pub active_snapshot_id: Option<Uuid>,
    pub prompt_profile_id: Uuid,
    pub messages: Vec<ChatMessage>,
    pub compact_summary: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Conversation {
    pub fn new(
        prompt_profile_id: Uuid,
        repository_id: Option<Uuid>,
        active_snapshot_id: Option<Uuid>,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            repository_id,
            active_snapshot_id,
            prompt_profile_id,
            messages: Vec::new(),
            compact_summary: None,
            created_at: now,
            updated_at: now,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChatRole {
    User,
    Assistant,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MessageStatus {
    Pending,
    Streaming,
    Completed,
    Cancelled,
    Failed { error_code: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatMessage {
    pub id: Uuid,
    pub conversation_id: Uuid,
    pub turn_id: Uuid,
    pub role: ChatRole,
    pub ordinal: u64,
    pub markdown: String,
    pub status: MessageStatus,
    pub source_ref_ids: Vec<Uuid>,
    pub grounding_trace_id: Option<Uuid>,
    pub grounding_freshness: Option<GroundingFreshness>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl ChatMessage {
    pub fn new(
        conversation_id: Uuid,
        turn_id: Uuid,
        role: ChatRole,
        ordinal: u64,
        markdown: impl Into<String>,
        status: MessageStatus,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            conversation_id,
            turn_id,
            role,
            ordinal,
            markdown: markdown.into(),
            status,
            source_ref_ids: Vec::new(),
            grounding_trace_id: None,
            grounding_freshness: None,
            created_at: now,
            updated_at: now,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConversationTurn {
    pub id: Uuid,
    pub conversation_id: Uuid,
    pub sequence: u64,
    pub prompt_profile_id: Uuid,
    pub prompt_profile_revision_id: Uuid,
    pub kernel_version: String,
    pub kernel_digest: String,
    pub snapshot_id: Option<Uuid>,
    pub response_contract: ResponseContract,
    pub audit_result_id: Option<Uuid>,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseContract {
    AdvisorMarkdown,
    AuditAnswerBundle { schema_version: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SystemPreset {
    Beginner,
    Intermediate,
    Professional,
    Senior,
}

impl SystemPreset {
    pub const ALL: [Self; 4] = [
        Self::Beginner,
        Self::Intermediate,
        Self::Professional,
        Self::Senior,
    ];

    pub fn resource_key(self) -> &'static str {
        match self {
            Self::Beginner => "system.beginner.v1",
            Self::Intermediate => "system.intermediate.v1",
            Self::Professional => "system.professional.v1",
            Self::Senior => "system.senior.v1",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExperiencePreset {
    Beginner,
    Intermediate,
    Professional,
    Senior,
    Custom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ComposerSubmitMode {
    EnterSend,
    CtrlEnterSend,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PromptProfile {
    pub id: Uuid,
    pub name: String,
    pub experience_preset: ExperiencePreset,
    pub base_system_preset: SystemPreset,
    pub active_revision_id: Uuid,
    pub factory_system_version: String,
    pub factory_persona_version: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PromptLayer {
    System,
    Persona,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PromptContentSource {
    FactoryRef {
        resource_key: String,
        resource_version: String,
        checksum: String,
    },
    UserText {
        content: String,
        checksum: String,
    },
    RestoredText {
        content: String,
        checksum: String,
        restored_from: Uuid,
    },
}

impl PromptContentSource {
    pub fn checksum(&self) -> &str {
        match self {
            Self::FactoryRef { checksum, .. }
            | Self::UserText { checksum, .. }
            | Self::RestoredText { checksum, .. } => checksum,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PromptContentVersion {
    pub id: Uuid,
    pub profile_id: Uuid,
    pub layer: PromptLayer,
    pub version: u64,
    pub source: PromptContentSource,
    pub parent_version_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PromptProfileRevision {
    pub id: Uuid,
    pub profile_id: Uuid,
    pub revision: u64,
    pub system_version_id: Option<Uuid>,
    pub persona_version_id: Option<Uuid>,
    pub system_checksum: String,
    pub persona_checksum: String,
    pub content_deleted: bool,
    pub expected_previous_revision_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct UiPreferences {
    pub width_points: f32,
    pub height_points: f32,
    pub submit_mode: ComposerSubmitMode,
    pub always_on_top: bool,
    pub layout_revision: u32,
    pub updated_at: DateTime<Utc>,
}

impl Default for UiPreferences {
    fn default() -> Self {
        Self {
            width_points: 312.5,
            height_points: 660.0,
            submit_mode: ComposerSubmitMode::EnterSend,
            always_on_top: true,
            layout_revision: 2,
            updated_at: Utc::now(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConversationPersistence {
    Durable,
    Ephemeral,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewConversation {
    pub repository_id: Option<Uuid>,
    pub active_snapshot_id: Option<Uuid>,
    pub prompt_profile_id: Uuid,
    pub persistence: ConversationPersistence,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TurnStart {
    pub turn: ConversationTurn,
    pub user_message: ChatMessage,
    pub assistant_placeholder: ChatMessage,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TurnTerminalUpdate {
    AdvisorCompleted {
        turn_id: Uuid,
        assistant_message_id: Uuid,
        markdown: String,
        grounding_trace_id: Option<Uuid>,
        freshness: Option<GroundingFreshness>,
        completed_at: DateTime<Utc>,
    },
    AuditCompleted {
        turn_id: Uuid,
        assistant_message_id: Uuid,
        result: AnswerBundle,
        grounding_trace_id: Uuid,
        freshness: GroundingFreshness,
        completed_at: DateTime<Utc>,
    },
    AdvisorCancelled {
        turn_id: Uuid,
        assistant_message_id: Uuid,
        partial_markdown: String,
        completed_at: DateTime<Utc>,
    },
    AuditCancelled {
        turn_id: Uuid,
        assistant_message_id: Uuid,
        completed_at: DateTime<Utc>,
    },
    Failed {
        turn_id: Uuid,
        assistant_message_id: Uuid,
        error_code: String,
        safe_message: String,
        completed_at: DateTime<Utc>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeleteReceipt {
    pub operation_id: Uuid,
    pub deleted_counts: std::collections::BTreeMap<String, u64>,
    pub removed_artifacts: Vec<String>,
    pub completed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PromptLayerDraft {
    Preserve,
    UserText(String),
    ResetToFactory {
        resource_key: String,
        resource_version: String,
        expected_checksum: String,
    },
    RestoreVersion {
        content_version_id: Uuid,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PromptDraft {
    pub profile_id: Uuid,
    pub name: String,
    pub experience_preset: ExperiencePreset,
    pub base_system_preset: SystemPreset,
    pub system: PromptLayerDraft,
    pub persona: PromptLayerDraft,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedPromptProfile {
    pub profile: PromptProfile,
    pub revision: PromptProfileRevision,
    pub system_text: String,
    pub persona_text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredPromptProfile {
    pub profile: PromptProfile,
    pub revision: PromptProfileRevision,
    pub system_source: PromptContentSource,
    pub persona_source: PromptContentSource,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum GroundingFreshness {
    FreshAtSend,
    ChangedAfterSend { detected_at: DateTime<Utc> },
    StaleBeforeSend,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RepositoryToolName {
    RepoStatus,
    ListTree,
    SearchPaths,
    SearchText,
    ReadFileLines,
    FileMetadata,
}

impl RepositoryToolName {
    pub const ALL: [Self; 6] = [
        Self::RepoStatus,
        Self::ListTree,
        Self::SearchPaths,
        Self::SearchText,
        Self::ReadFileLines,
        Self::FileMetadata,
    ];

    pub fn wire_name(self) -> &'static str {
        match self {
            Self::RepoStatus => "repo_status",
            Self::ListTree => "list_tree",
            Self::SearchPaths => "search_paths",
            Self::SearchText => "search_text",
            Self::ReadFileLines => "read_file_lines",
            Self::FileMetadata => "file_metadata",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RepositoryToolArguments {
    RepoStatus,
    ListTree {
        relative_path: Option<PathBuf>,
        depth: u8,
        limit: u16,
    },
    SearchPaths {
        query: String,
        limit: u16,
    },
    SearchText {
        query: String,
        path_filter: Option<String>,
        limit: u16,
    },
    ReadFileLines {
        relative_path: PathBuf,
        start_line: usize,
        end_line: usize,
    },
    FileMetadata {
        relative_path: PathBuf,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepositoryToolCall {
    pub call_id: Uuid,
    pub snapshot_id: Uuid,
    pub name: RepositoryToolName,
    pub arguments: RepositoryToolArguments,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ToolOmissionReason {
    EntryLimit,
    ByteLimit,
    Binary,
    Ignored,
    PermissionDenied,
    ReadError,
    StaleSnapshot,
    LiveHashMismatch,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolOmission {
    pub reason: ToolOmissionReason,
    pub relative_path: Option<PathBuf>,
    pub detail_code: String,
    pub omitted_count: u64,
    pub omitted_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceRef {
    pub id: Uuid,
    pub snapshot_id: Uuid,
    pub relative_path: PathBuf,
    pub line_start: usize,
    pub line_end: usize,
    pub content_hash: String,
    pub excerpt: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepositoryToolResult {
    pub call_id: Uuid,
    pub snapshot_id: Uuid,
    pub content: String,
    pub source_refs: Vec<SourceRef>,
    pub omissions: Vec<ToolOmission>,
    pub content_bytes: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RepositoryToolCallStatus {
    Pending,
    Completed,
    Omitted,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepositoryToolCallRecord {
    pub trace_id: Uuid,
    pub call_id: Uuid,
    pub round: u8,
    pub name: RepositoryToolName,
    pub canonical_arguments_digest: String,
    pub result_digest: Option<String>,
    pub content_bytes: u32,
    pub source_ref_ids: Vec<Uuid>,
    pub status: RepositoryToolCallStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroundingTrace {
    pub id: Uuid,
    pub conversation_id: Uuid,
    pub turn_id: Uuid,
    pub snapshot_id: Option<Uuid>,
    pub tool_calls: Vec<RepositoryToolCallRecord>,
    pub source_refs: Vec<SourceRef>,
    pub egress_receipt_ids: Vec<Uuid>,
    pub freshness: GroundingFreshness,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderBinding {
    pub profile_id: Uuid,
    pub provider: String,
    pub endpoint_identity: String,
    pub model_id: String,
    pub target_digest: String,
}

impl ProviderBinding {
    pub fn new(
        profile_id: Uuid,
        provider: impl Into<String>,
        endpoint: &str,
        model_id: impl Into<String>,
    ) -> Result<Self, crate::MentatError> {
        let provider = provider.into();
        let model_id = model_id.into();
        if provider.trim().is_empty() || model_id.trim().is_empty() || model_id.len() > 256 {
            return Err(crate::MentatError::EgressViolation(
                "provider/model binding 값이 비어 있거나 너무 깁니다.".to_string(),
            ));
        }
        let parsed = url::Url::parse(endpoint).map_err(|error| {
            crate::MentatError::EgressViolation(format!("provider endpoint 형식 오류: {error}"))
        })?;
        if !parsed.username().is_empty()
            || parsed.password().is_some()
            || parsed.query().is_some()
            || parsed.fragment().is_some()
        {
            return Err(crate::MentatError::EgressViolation(
                "provider endpoint identity에는 userinfo/query/fragment를 포함할 수 없습니다."
                    .to_string(),
            ));
        }
        let host = parsed.host_str().ok_or_else(|| {
            crate::MentatError::EgressViolation(
                "provider endpoint identity에 host가 없습니다.".to_string(),
            )
        })?;
        let loopback = matches!(host, "localhost" | "127.0.0.1" | "::1" | "[::1]");
        if parsed.scheme() != "https" && !(parsed.scheme() == "http" && loopback) {
            return Err(crate::MentatError::EgressViolation(
                "provider endpoint는 HTTPS 또는 local loopback HTTP여야 합니다.".to_string(),
            ));
        }
        let port = parsed
            .port()
            .map(|port| format!(":{port}"))
            .unwrap_or_default();
        let path = if parsed.path().is_empty() {
            "/"
        } else {
            parsed.path()
        };
        let endpoint_identity = format!(
            "{}://{}{}{}",
            parsed.scheme(),
            host.to_lowercase(),
            port,
            path.trim_end_matches('/').to_string() + "/"
        );
        let canonical = format!(
            "CM_PROVIDER_TARGET_V1\n{}\n{}\n{}\n{}",
            profile_id, provider, endpoint_identity, model_id
        );
        let target_digest = format!("{:x}", Sha256::digest(canonical.as_bytes()));
        Ok(Self {
            profile_id,
            provider,
            endpoint_identity,
            model_id,
            target_digest,
        })
    }

    pub fn verify_target_digest(&self) -> Result<(), crate::MentatError> {
        let rebuilt = Self::new(
            self.profile_id,
            self.provider.clone(),
            &self.endpoint_identity,
            self.model_id.clone(),
        )?;
        if rebuilt.target_digest != self.target_digest {
            return Err(crate::MentatError::EgressViolation(
                "provider target digest가 일치하지 않습니다.".to_string(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RepositoryConsentKind {
    RequestOnce { turn_id: Uuid },
    RepositorySession,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepositoryConsentScope {
    pub id: Uuid,
    pub conversation_id: Uuid,
    pub repository_id: Uuid,
    pub snapshot_id: Uuid,
    pub provider_binding: ProviderBinding,
    pub kind: RepositoryConsentKind,
    pub granted_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CanonicalToolRef {
    pub relative_path: PathBuf,
    pub line_start: usize,
    pub line_end: usize,
    pub content_hash: String,
    pub redacted_payload_digest: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ToolEgressStatus {
    Prepared,
    Sent,
    Failed,
    OutcomeUnknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolEgressReceipt {
    pub id: Uuid,
    pub seal_version: String,
    pub trace_id: Uuid,
    pub consent_scope_id: Uuid,
    pub conversation_id: Uuid,
    pub turn_id: Uuid,
    pub tool_call_id: Uuid,
    pub repository_id: Uuid,
    pub snapshot_id: Uuid,
    pub tool_name: RepositoryToolName,
    pub canonical_refs: Vec<CanonicalToolRef>,
    pub provider_binding: ProviderBinding,
    pub semantic_payload_digest: String,
    pub exact_provider_body_digest: String,
    pub canonical_digest: String,
    pub status: ToolEgressStatus,
    pub prepared_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[cfg(test)]
mod conversation_contract_tests {
    use super::*;

    #[test]
    fn conversation_without_repository_is_a_valid_first_class_state() {
        let prompt_profile_id = Uuid::new_v4();
        let conversation = Conversation::new(prompt_profile_id, None, None);

        assert_eq!(conversation.prompt_profile_id, prompt_profile_id);
        assert_eq!(conversation.repository_id, None);
        assert_eq!(conversation.active_snapshot_id, None);
        assert!(conversation.messages.is_empty());
    }

    #[test]
    fn factory_prompt_source_serializes_reference_without_prompt_text() {
        let source = PromptContentSource::FactoryRef {
            resource_key: "system.intermediate.v1".to_string(),
            resource_version: "cr-ux-001.1".to_string(),
            checksum: "abc123".to_string(),
        };

        let serialized = serde_json::to_string(&source).expect("factory ref should serialize");
        assert!(serialized.contains("system.intermediate.v1"));
        assert!(!serialized.contains("Answer clearly and directly"));
    }

    #[test]
    fn audit_and_advisor_turn_contracts_are_explicit_variants() {
        let advisor = ResponseContract::AdvisorMarkdown;
        let audit = ResponseContract::AuditAnswerBundle {
            schema_version: "answer_bundle.v1".to_string(),
        };

        assert_ne!(advisor, audit);
    }

    #[test]
    fn repository_tool_surface_contains_only_six_read_only_operations() {
        let names: Vec<&str> = RepositoryToolName::ALL
            .into_iter()
            .map(RepositoryToolName::wire_name)
            .collect();

        assert_eq!(names.len(), 6);
        assert_eq!(
            names,
            vec![
                "repo_status",
                "list_tree",
                "search_paths",
                "search_text",
                "read_file_lines",
                "file_metadata"
            ]
        );
        for forbidden in [
            "write", "delete", "rename", "patch", "process", "build", "test",
        ] {
            assert!(!names.iter().any(|name| name.contains(forbidden)));
        }
    }
}
