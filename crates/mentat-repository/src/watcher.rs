use ignore::WalkBuilder;
use mentat_core::error::MentatError;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

pub struct RepositoryWatcher {
    root_path: PathBuf,
    last_known_mtime: SystemTime,
}

impl RepositoryWatcher {
    pub fn new(root_path: impl AsRef<Path>) -> Self {
        let last_known_mtime =
            Self::get_latest_mtime(root_path.as_ref()).unwrap_or(SystemTime::UNIX_EPOCH);
        Self {
            root_path: root_path.as_ref().to_path_buf(),
            last_known_mtime,
        }
    }

    /// [DBG-F002] Check if any file in the repository tree has been modified since last check
    pub fn check_for_changes(&mut self) -> Result<bool, MentatError> {
        let current_mtime = Self::get_latest_mtime(&self.root_path)?;
        if current_mtime > self.last_known_mtime {
            self.last_known_mtime = current_mtime;
            Ok(true) // Changed -> STALE
        } else {
            Ok(false) // No changes
        }
    }

    /// Recursively inspects file mtimes within budget (max 2000 files checked per poll, bounded depth 10)
    fn get_latest_mtime(root: &Path) -> Result<SystemTime, MentatError> {
        let mut latest = SystemTime::UNIX_EPOCH;
        let walker = WalkBuilder::new(root)
            .hidden(false)
            .git_ignore(true)
            .git_global(true)
            .git_exclude(true)
            .max_depth(Some(10))
            .build();

        let mut checked_count = 0;
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
                if let Ok(mtime) = meta.modified() {
                    if mtime > latest {
                        latest = mtime;
                    }
                }
            }

            checked_count += 1;
            if checked_count >= 2000 {
                break;
            }
        }
        Ok(latest)
    }
}
