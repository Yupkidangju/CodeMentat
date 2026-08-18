use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
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
