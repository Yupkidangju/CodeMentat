use ignore::WalkBuilder;
use mentat_core::error::MentatError;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

pub const WATCHER_THROTTLE_INTERVAL: Duration = Duration::from_millis(1000);

#[derive(Debug, Clone, PartialEq, Eq)]
struct TreeSignature {
    file_count: usize,
    total_size: u64,
    latest_mtime: SystemTime,
}

pub struct RepositoryWatcher {
    root_path: PathBuf,
    last_signature: TreeSignature,
    last_check_instant: Instant,
}

impl RepositoryWatcher {
    pub fn new(root_path: impl AsRef<Path>) -> Self {
        let last_signature =
            Self::compute_tree_signature(root_path.as_ref()).unwrap_or(TreeSignature {
                file_count: 0,
                total_size: 0,
                latest_mtime: SystemTime::UNIX_EPOCH,
            });

        Self {
            root_path: root_path.as_ref().to_path_buf(),
            last_signature,
            last_check_instant: Instant::now(),
        }
    }

    /// [DBG-F008 & DBG-F002] Check if any file in the repository tree has been modified, added, or deleted.
    /// Throttles filesystem walking to at most once every 1000ms to decouple from UI 60fps frame rate.
    pub fn check_for_changes(&mut self) -> Result<bool, MentatError> {
        let now = Instant::now();
        if now.duration_since(self.last_check_instant) < WATCHER_THROTTLE_INTERVAL {
            return Ok(false); // Throttle: skip disk I/O on rapid UI frames
        }
        self.last_check_instant = now;

        let current_signature = Self::compute_tree_signature(&self.root_path)?;
        if current_signature != self.last_signature {
            self.last_signature = current_signature;
            Ok(true) // Tree modified, deleted, or added -> STALE
        } else {
            Ok(false) // No changes
        }
    }

    /// Recursively computes tree signature (file count, total size, latest mtime) across the repository
    fn compute_tree_signature(root: &Path) -> Result<TreeSignature, MentatError> {
        let mut latest_mtime = SystemTime::UNIX_EPOCH;
        let mut total_size = 0u64;
        let mut file_count = 0usize;

        let walker = WalkBuilder::new(root)
            .hidden(false)
            .git_ignore(true)
            .git_global(true)
            .git_exclude(true)
            .build();

        for entry in walker {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };

            let path = entry.path();
            if path.components().any(|c| c.as_os_str() == ".git") {
                continue;
            }

            if let Ok(meta) = entry.metadata() {
                if meta.is_file() {
                    file_count += 1;
                    total_size += meta.len();
                    if let Ok(mtime) = meta.modified() {
                        if mtime > latest_mtime {
                            latest_mtime = mtime;
                        }
                    }
                }
            }
        }

        Ok(TreeSignature {
            file_count,
            total_size,
            latest_mtime,
        })
    }
}
