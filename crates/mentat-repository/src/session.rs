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
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

pub const MAX_SCAN_FILES_LIMIT: usize = 100_000;
pub const MAX_SCAN_TOTAL_BYTES_LIMIT: u64 = 2 * 1024 * 1024 * 1024; // 2GiB
pub const MAX_SINGLE_FILE_BYTES: u64 = 10 * 1024 * 1024; // 10MB
pub const MAX_SCAN_PREVIEW_BYTES: usize = 8 * 1024 * 1024;

#[derive(Debug, Clone, Copy)]
pub struct ScanLimits {
    pub max_files: usize,
    pub max_total_bytes: u64,
    pub max_single_file_bytes: u64,
}

impl Default for ScanLimits {
    fn default() -> Self {
        Self {
            max_files: MAX_SCAN_FILES_LIMIT,
            max_total_bytes: MAX_SCAN_TOTAL_BYTES_LIMIT,
            max_single_file_bytes: MAX_SINGLE_FILE_BYTES,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScanOmitReason {
    WalkError,
    MetadataError,
    ReadError,
    FileCountLimit,
    TotalBytesLimit,
    FileTooLarge,
    Cancelled,
}

#[derive(Debug, Clone)]
pub struct ScanOmission {
    pub relative_path: PathBuf,
    pub reason: ScanOmitReason,
}

#[derive(Debug, Clone)]
pub struct ScanOutcome {
    pub files: Vec<FileRecord>,
    pub omissions: Vec<ScanOmission>,
    pub cancelled: bool,
}

impl ScanOutcome {
    pub fn is_complete(&self) -> bool {
        !self.cancelled && self.omissions.is_empty()
    }
}

pub struct ReadOnlySession {
    root_path: PathBuf,
    profile: RepositoryProfile,
}

impl ReadOnlySession {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, MentatError> {
        Self::open_with_known_id(path, None)
    }

    /// [IMP-F005] Reuses a stable repository UUID when the same canonical root is reopened.
    pub fn open_with_known_id(
        path: impl AsRef<Path>,
        known_id: Option<Uuid>,
    ) -> Result<Self, MentatError> {
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
            id: known_id.unwrap_or_else(Uuid::new_v4),
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

    pub fn create_snapshot_from_outcome(&self, outcome: &ScanOutcome) -> RepositorySnapshot {
        let mut snapshot = self.create_snapshot_from_files(&outcome.files);
        if !outcome.is_complete() {
            snapshot.status = SnapshotStatus::Incomplete;
        }
        snapshot
    }

    /// [DBG-F003] Cancellable scan with explicit omission reasons and metadata preflight.
    pub async fn scan_files_with_limits(
        &self,
        limits: ScanLimits,
        cancel: CancellationToken,
    ) -> Result<ScanOutcome, MentatError> {
        let root = self.root_path.clone();
        tokio::task::spawn_blocking(move || scan_tree(root, limits, cancel))
            .await
            .map_err(|e| MentatError::IoError(format!("스캔 태스크 실행 오류: {}", e)))?
    }
}

fn scan_tree(
    root: PathBuf,
    limits: ScanLimits,
    cancel: CancellationToken,
) -> Result<ScanOutcome, MentatError> {
    let mut records = Vec::new();
    let mut omissions = Vec::new();
    let mut accumulated_bytes = 0u64;
    let mut cancelled = false;
    let mut preview_bytes = 0usize;

    let walker = WalkBuilder::new(&root)
        .hidden(false)
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true)
        .build();

    for entry in walker {
        if cancel.is_cancelled() {
            cancelled = true;
            omissions.push(ScanOmission {
                relative_path: PathBuf::from("<scan>"),
                reason: ScanOmitReason::Cancelled,
            });
            break;
        }

        let entry = match entry {
            Ok(e) => e,
            Err(_) => {
                omissions.push(ScanOmission {
                    relative_path: PathBuf::from("<walk>"),
                    reason: ScanOmitReason::WalkError,
                });
                continue;
            }
        };

        let path = entry.path();
        if path.components().any(|c| c.as_os_str() == ".git") {
            continue;
        }

        let meta = match entry.metadata() {
            Ok(m) if m.is_file() => m,
            Ok(_) => continue,
            Err(_) => {
                omissions.push(ScanOmission {
                    relative_path: path.strip_prefix(&root).unwrap_or(path).to_path_buf(),
                    reason: ScanOmitReason::MetadataError,
                });
                continue;
            }
        };

        let rel_path = match path.strip_prefix(&root) {
            Ok(p) => p.to_path_buf(),
            Err(_) => continue,
        };

        if meta.len() > limits.max_single_file_bytes {
            omissions.push(ScanOmission {
                relative_path: rel_path,
                reason: ScanOmitReason::FileTooLarge,
            });
            continue;
        }

        if accumulated_bytes.saturating_add(meta.len()) > limits.max_total_bytes {
            omissions.push(ScanOmission {
                relative_path: rel_path,
                reason: ScanOmitReason::TotalBytesLimit,
            });
            continue;
        }

        if records.len() >= limits.max_files {
            omissions.push(ScanOmission {
                relative_path: rel_path,
                reason: ScanOmitReason::FileCountLimit,
            });
            continue;
        }

        if let Ok(mut record) = FileScanner::inspect_file(&root, &rel_path) {
            if let Some(preview) = record.text_preview.as_ref() {
                if preview_bytes.saturating_add(preview.len()) <= MAX_SCAN_PREVIEW_BYTES {
                    preview_bytes += preview.len();
                } else {
                    record.text_preview = None;
                }
            }
            accumulated_bytes += record.size_bytes;
            records.push(record);
        } else {
            omissions.push(ScanOmission {
                relative_path: rel_path,
                reason: ScanOmitReason::ReadError,
            });
        }
    }

    Ok(ScanOutcome {
        files: records,
        omissions,
        cancelled,
    })
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
        let outcome = self
            .scan_files_with_limits(ScanLimits::default(), CancellationToken::new())
            .await?;
        Ok(outcome.files)
    }

    async fn read_file_content(&self, relative_path: &Path) -> Result<String, MentatError> {
        let root = self.root_path.clone();
        let relative = relative_path.to_path_buf();
        tokio::task::spawn_blocking(move || {
            use std::io::Read;
            let file = crate::safe_file::open_beneath(&root, &relative)?;
            let meta = file
                .metadata()
                .map_err(|e| MentatError::IoError(e.to_string()))?;

            // DBG-F003: 10MB max single file read bound
            if meta.len() > MAX_SINGLE_FILE_BYTES {
                return Err(MentatError::IoError(format!(
                    "파일 크기({} bytes)가 10MB 한도를 초과하여 읽기가 제한됩니다.",
                    meta.len()
                )));
            }

            let mut bytes = Vec::new();
            file.take(MAX_SINGLE_FILE_BYTES + 1)
                .read_to_end(&mut bytes)
                .map_err(|e| MentatError::IoError(e.to_string()))?;
            if bytes.len() as u64 > MAX_SINGLE_FILE_BYTES {
                return Err(MentatError::IoError("읽기 상한 초과".into()));
            }
            String::from_utf8(bytes)
                .map_err(|_| MentatError::IoError("UTF-8 텍스트가 아닙니다.".into()))
        })
        .await
        .map_err(|e| MentatError::IoError(e.to_string()))?
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
