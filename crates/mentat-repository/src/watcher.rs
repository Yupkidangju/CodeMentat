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

    /// Check if any file in the repository has been modified since last check
    pub fn check_for_changes(&mut self) -> Result<bool, MentatError> {
        let current_mtime = Self::get_latest_mtime(&self.root_path)?;
        if current_mtime > self.last_known_mtime {
            self.last_known_mtime = current_mtime;
            Ok(true) // Changed -> STALE
        } else {
            Ok(false) // No changes
        }
    }

    fn get_latest_mtime(root: &Path) -> Result<SystemTime, MentatError> {
        let mut latest = SystemTime::UNIX_EPOCH;
        if let Ok(entries) = std::fs::read_dir(root) {
            for entry in entries.flatten() {
                if let Ok(meta) = entry.metadata() {
                    if let Ok(mtime) = meta.modified() {
                        if mtime > latest {
                            latest = mtime;
                        }
                    }
                }
            }
        }
        Ok(latest)
    }
}
