use crate::scanner::FileScanner;
use async_trait::async_trait;
use chrono::Utc;
use ignore::WalkBuilder;
use mentat_core::error::MentatError;
use mentat_core::models::{
    FileRecord, RepositoryProfile, RepositorySnapshot, RepositoryType, SnapshotStatus,
};
use mentat_core::ports::RepositoryReader;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use uuid::Uuid;

pub const MAX_SCAN_FILES_LIMIT: usize = 100_000;
pub const MAX_SCAN_TOTAL_BYTES_LIMIT: u64 = 2 * 1024 * 1024 * 1024; // 2GiB
pub const MAX_SINGLE_FILE_BYTES: u64 = 10 * 1024 * 1024; // 10MB

pub struct ReadOnlySession {
    root_path: PathBuf,
    profile: RepositoryProfile,
}

impl ReadOnlySession {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, MentatError> {
        let raw_path = path.as_ref();
        let canonical_root = raw_path.canonicalize().map_err(|e| {
            MentatError::InvalidRepositoryPath(format!("{}: {}", raw_path.display(), e))
        })?;

        if !canonical_root.is_dir() {
            return Err(MentatError::InvalidRepositoryPath(format!(
                "지정된 경로가 디렉터리가 아닙니다: {}",
                canonical_root.display()
            )));
        }

        let is_git = canonical_root.join(".git").exists();
        let display_name = canonical_root
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("repository")
            .to_string();

        let profile = RepositoryProfile {
            id: Uuid::new_v4(),
            display_name,
            root_path: canonical_root.clone(),
            repo_type: if is_git {
                RepositoryType::Git
            } else {
                RepositoryType::Directory
            },
            consent_policy: false,
        };

        Ok(Self {
            root_path: canonical_root,
            profile,
        })
    }

    fn validate_child_path(&self, rel_path: &Path) -> Result<PathBuf, MentatError> {
        let full_path = self.root_path.join(rel_path);
        let canonical_path = full_path.canonicalize().map_err(|e| {
            MentatError::IoError(format!("경로 확인 실패 {}: {}", full_path.display(), e))
        })?;

        if !canonical_path.starts_with(&self.root_path) {
            return Err(MentatError::ExternalPathBlocked(format!(
                "저장소 루트 밖을 가리키는 경로는 차단됩니다: {}",
                canonical_path.display()
            )));
        }

        Ok(canonical_path)
    }

    /// [DBG-F002] Constructs deterministic snapshot directly from already-scanned file records (Single-Scan)
    pub fn create_snapshot_from_files(&self, files: &[FileRecord]) -> RepositorySnapshot {
        let mut sorted_files = files.to_vec();
        sorted_files.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));

        let mut hasher = Sha256::new();
        let mut total_bytes = 0;

        for file in &sorted_files {
            hasher.update(file.relative_path.to_string_lossy().as_bytes());
            hasher.update(file.content_hash.as_bytes());
            total_bytes += file.size_bytes;
        }

        let tree_digest = format!("{:x}", hasher.finalize());

        RepositorySnapshot {
            id: Uuid::new_v4(),
            repo_id: self.profile.id,
            created_at: Utc::now(),
            tree_digest,
            status: SnapshotStatus::Ready,
            file_count: files.len(),
            total_bytes,
        }
    }
}

#[async_trait]
impl RepositoryReader for ReadOnlySession {
    fn root_path(&self) -> &Path {
        &self.root_path
    }

    fn profile(&self) -> &RepositoryProfile {
        &self.profile
    }

    /// [DBG-F003] Scans files under resource budgets (max 100,000 files, max 2GiB total scanned size)
    async fn scan_files(&self) -> Result<Vec<FileRecord>, MentatError> {
        let root = self.root_path.clone();
        tokio::task::spawn_blocking(move || {
            let mut records = Vec::new();
            let mut accumulated_bytes = 0u64;

            let walker = WalkBuilder::new(&root)
                .hidden(false)
                .git_ignore(true)
                .git_global(true)
                .git_exclude(true)
                .build();

            for entry in walker {
                if records.len() >= MAX_SCAN_FILES_LIMIT {
                    break;
                }
                if accumulated_bytes >= MAX_SCAN_TOTAL_BYTES_LIMIT {
                    break;
                }

                let entry = match entry {
                    Ok(e) => e,
                    Err(_) => continue,
                };

                let path = entry.path();
                if path.is_file() {
                    // Skip .git internal files
                    if path.components().any(|c| c.as_os_str() == ".git") {
                        continue;
                    }

                    if let Ok(rel_path) = path.strip_prefix(&root) {
                        if let Ok(record) = FileScanner::inspect_file(&root, rel_path) {
                            accumulated_bytes += record.size_bytes;
                            records.push(record);
                        }
                    }
                }
            }
            Ok(records)
        })
        .await
        .map_err(|e| MentatError::IoError(format!("스캔 태스크 실행 오류: {}", e)))?
    }

    async fn read_file_content(&self, relative_path: &Path) -> Result<String, MentatError> {
        let full_path = self.validate_child_path(relative_path)?;
        let meta = tokio::fs::metadata(&full_path)
            .await
            .map_err(|e| MentatError::IoError(format!("파일 메타데이터 조회 실패: {}", e)))?;

        // DBG-F003: 10MB max single file read bound
        if meta.len() > MAX_SINGLE_FILE_BYTES {
            return Err(MentatError::IoError(format!(
                "파일 크기({} bytes)가 10MB 한도를 초과하여 읽기가 제한됩니다.",
                meta.len()
            )));
        }

        tokio::fs::read_to_string(&full_path)
            .await
            .map_err(|e| MentatError::IoError(format!("파일 내용 읽기 실패: {}", e)))
    }

    async fn read_file_lines(
        &self,
        relative_path: &Path,
        start_line: usize,
        end_line: usize,
    ) -> Result<String, MentatError> {
        let content = self.read_file_content(relative_path).await?;
        let lines: Vec<&str> = content.lines().collect();
        if start_line == 0 || start_line > lines.len() {
            return Ok(String::new());
        }

        let start_idx = start_line - 1;
        let end_idx = end_line.min(lines.len());
        if start_idx >= end_idx {
            return Ok(String::new());
        }

        Ok(lines[start_idx..end_idx].join("\n"))
    }

    async fn create_snapshot(&self) -> Result<RepositorySnapshot, MentatError> {
        let files = self.scan_files().await?;
        Ok(self.create_snapshot_from_files(&files))
    }
}
