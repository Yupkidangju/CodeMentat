use mentat_core::error::MentatError;
use mentat_core::models::EvidenceRef;
use mentat_core::ports::RepositoryReader;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::Path;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct EvidenceIndex {
    pub snapshot_id: Uuid,
    entries: HashMap<Uuid, EvidenceRef>,
}

impl EvidenceIndex {
    pub fn new(snapshot_id: Uuid) -> Self {
        Self {
            snapshot_id,
            entries: HashMap::new(),
        }
    }

    pub fn add_evidence(&mut self, evidence: EvidenceRef) {
        self.entries.insert(evidence.id, evidence);
    }

    pub fn get_evidence(&self, id: &Uuid) -> Option<&EvidenceRef> {
        self.entries.get(id)
    }

    pub fn all_evidence(&self) -> Vec<&EvidenceRef> {
        self.entries.values().collect()
    }

    /// Extract an exact evidence snippet from a file and seal it into an EvidenceRef
    pub async fn create_evidence_ref(
        &mut self,
        reader: &(impl RepositoryReader + ?Sized),
        relative_path: &Path,
        line_start: usize,
        line_end: usize,
    ) -> Result<EvidenceRef, MentatError> {
        let excerpt = reader
            .read_file_lines(relative_path, line_start, line_end)
            .await?;

        let mut hasher = Sha256::new();
        hasher.update(relative_path.to_string_lossy().as_bytes());
        hasher.update(line_start.to_le_bytes());
        hasher.update(line_end.to_le_bytes());
        hasher.update(excerpt.as_bytes());
        let content_hash = format!("{:x}", hasher.finalize());

        let evidence = EvidenceRef {
            id: Uuid::new_v4(),
            snapshot_id: self.snapshot_id,
            relative_path: relative_path.to_path_buf(),
            line_start,
            line_end,
            content_hash,
            excerpt,
        };

        self.add_evidence(evidence.clone());
        Ok(evidence)
    }

    /// Find all evidence references by file path
    pub fn find_by_file(&self, path: &Path) -> Vec<&EvidenceRef> {
        self.entries
            .values()
            .filter(|e| e.relative_path == path)
            .collect()
    }
}
