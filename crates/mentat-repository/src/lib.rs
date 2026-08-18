pub mod scanner;
pub mod session;
pub mod watcher;

pub use scanner::FileScanner;
pub use session::{
    ReadOnlySession, ScanLimits, ScanOmission, ScanOmitReason, ScanOutcome, MAX_SCAN_FILES_LIMIT,
    MAX_SCAN_TOTAL_BYTES_LIMIT, MAX_SINGLE_FILE_BYTES,
};
pub use watcher::RepositoryWatcher;

#[cfg(test)]
mod tests;
