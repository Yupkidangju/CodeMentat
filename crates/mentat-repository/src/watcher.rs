use ignore::WalkBuilder;
use mentat_core::error::MentatError;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime};

pub const WATCHER_THROTTLE_INTERVAL: Duration = Duration::from_millis(1000);
pub const VERIFIED_REHASH_INTERVAL: Duration = Duration::from_secs(3);

#[derive(Debug, Clone, PartialEq, Eq)]
struct TreeSignature {
    file_count: usize,
    total_size: u64,
    latest_mtime: SystemTime,
    digest: String,
}

pub struct RepositoryWatcher {
    root_path: PathBuf,
    last_signature: Option<TreeSignature>,
    last_content_fp: Option<String>,
    last_check_instant: Instant,
    change_rx: Option<Receiver<bool>>,
    stop: Option<Arc<AtomicBool>>,
    worker: Option<JoinHandle<()>>,
}

impl RepositoryWatcher {
    /// [DBG-F008] Does not walk the tree. The first signature is taken on the worker.
    pub fn new(root_path: impl AsRef<Path>) -> Self {
        Self {
            root_path: root_path.as_ref().to_path_buf(),
            last_signature: None,
            last_content_fp: None,
            last_check_instant: Instant::now(),
            change_rx: None,
            stop: None,
            worker: None,
        }
    }

    /// [DBG-F008] Move full-tree walks off the UI thread. The UI only polls a channel.
    pub fn spawn_background(&mut self) {
        self.stop_background();

        let (tx, rx) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let stop_flag = stop.clone();
        let root = self.root_path.clone();
        let mut last_signature = self.last_signature.clone();
        let mut last_content_fp = self.last_content_fp.clone();
        let mut last_rehash = Instant::now();

        let worker = std::thread::Builder::new()
            .name("mentat-watcher".to_string())
            .spawn(move || {
                if stop_flag.load(Ordering::Relaxed) {
                    return;
                }
                if last_signature.is_none() {
                    last_signature = Self::compute_tree_signature(&root).ok();
                }
                if last_content_fp.is_none() {
                    last_content_fp = Self::compute_content_fingerprint(&root).ok();
                }
                while !stop_flag.load(Ordering::Relaxed) {
                    let started = Instant::now();
                    while started.elapsed() < WATCHER_THROTTLE_INTERVAL {
                        if stop_flag.load(Ordering::Relaxed) {
                            return;
                        }
                        std::thread::sleep(Duration::from_millis(50));
                    }
                    if stop_flag.load(Ordering::Relaxed) {
                        break;
                    }
                    match Self::compute_tree_signature(&root) {
                        Ok(signature) if last_signature.as_ref() != Some(&signature) => {
                            last_signature = Some(signature);
                            if tx.send(true).is_err() {
                                break;
                            }
                        }
                        Ok(_) => {}
                        Err(_) => {
                            // Metadata failures still wake the UI so the session can go STALE.
                            if tx.send(true).is_err() {
                                break;
                            }
                        }
                    }
                    if last_rehash.elapsed() >= VERIFIED_REHASH_INTERVAL {
                        last_rehash = Instant::now();
                        if let Ok(fp) = Self::compute_content_fingerprint(&root) {
                            if last_content_fp.as_ref() != Some(&fp) {
                                last_content_fp = Some(fp);
                                if tx.send(true).is_err() {
                                    break;
                                }
                            }
                        }
                    }
                }
            })
            .ok();

        self.change_rx = Some(rx);
        self.stop = Some(stop);
        self.worker = worker;
    }

    pub fn stop_background(&mut self) {
        if let Some(stop) = self.stop.take() {
            stop.store(true, Ordering::Relaxed);
        }
        self.change_rx = None;
        // Non-blocking: detach the worker instead of joining on the UI thread.
        let _ = self.worker.take();
    }

    /// [DBG-F002] Detects same-size content edits even when mtime is restored.
    pub fn force_content_check(&mut self) -> Result<bool, MentatError> {
        let fingerprint = Self::compute_content_fingerprint(&self.root_path)?;
        match &self.last_content_fp {
            None => {
                self.last_content_fp = Some(fingerprint);
                Ok(false)
            }
            Some(prev) if prev != &fingerprint => {
                self.last_content_fp = Some(fingerprint);
                Ok(true)
            }
            Some(_) => Ok(false),
        }
    }

    /// Non-blocking UI poll. Never walks the tree when a background worker is running.
    pub fn poll_changes(&mut self) -> Result<bool, MentatError> {
        if let Some(rx) = &self.change_rx {
            match rx.try_recv() {
                Ok(true) => Ok(true),
                Ok(false) | Err(mpsc::TryRecvError::Empty) => Ok(false),
                Err(mpsc::TryRecvError::Disconnected) => Ok(false),
            }
        } else {
            self.check_for_changes()
        }
    }

    /// Synchronous check used by tests and as a fallback when no background worker exists.
    /// Throttles filesystem walking to at most once every 1000ms.
    pub fn check_for_changes(&mut self) -> Result<bool, MentatError> {
        let now = Instant::now();
        if now.duration_since(self.last_check_instant) < WATCHER_THROTTLE_INTERVAL {
            return Ok(false);
        }
        self.last_check_instant = now;

        let current_signature = Self::compute_tree_signature(&self.root_path)?;
        match &self.last_signature {
            None => {
                self.last_signature = Some(current_signature);
                Ok(false)
            }
            Some(prev) if prev != &current_signature => {
                self.last_signature = Some(current_signature);
                Ok(true)
            }
            Some(_) => Ok(false),
        }
    }

    /// Recursively computes a completeness-oriented tree signature.
    /// Digest covers every file path, size, and mtime so same-size edits and
    /// mtime rollbacks are visible even when latest_mtime does not increase.
    fn compute_tree_signature(root: &Path) -> Result<TreeSignature, MentatError> {
        let mut latest_mtime = SystemTime::UNIX_EPOCH;
        let mut total_size = 0u64;
        let mut file_count = 0usize;
        let mut metadata_errors = 0usize;
        let mut hasher = Sha256::new();

        let walker = WalkBuilder::new(root)
            .hidden(false)
            .git_ignore(true)
            .git_global(true)
            .git_exclude(true)
            .build();

        for entry in walker {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => {
                    metadata_errors += 1;
                    continue;
                }
            };

            let path = entry.path();
            if path.components().any(|c| c.as_os_str() == ".git") {
                continue;
            }

            match entry.metadata() {
                Ok(meta) if meta.is_file() => {
                    file_count += 1;
                    total_size += meta.len();
                    let mtime = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
                    if mtime > latest_mtime {
                        latest_mtime = mtime;
                    }
                    hasher.update(path.to_string_lossy().as_bytes());
                    hasher.update(meta.len().to_le_bytes());
                    if let Ok(dur) = mtime.duration_since(SystemTime::UNIX_EPOCH) {
                        hasher.update(dur.as_secs().to_le_bytes());
                        hasher.update(dur.subsec_nanos().to_le_bytes());
                    }
                }
                Ok(_) => {}
                Err(_) => {
                    metadata_errors += 1;
                }
            }
        }

        hasher.update(metadata_errors.to_le_bytes());

        Ok(TreeSignature {
            file_count,
            total_size,
            latest_mtime,
            digest: format!("{:x}", hasher.finalize()),
        })
    }

    fn compute_content_fingerprint(root: &Path) -> Result<String, MentatError> {
        use std::io::Read;
        let mut hasher = Sha256::new();
        let mut metadata_errors = 0usize;
        let walker = WalkBuilder::new(root)
            .hidden(false)
            .git_ignore(true)
            .git_global(true)
            .git_exclude(true)
            .build();

        for entry in walker {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => {
                    metadata_errors += 1;
                    hasher.update(b"walk-err");
                    continue;
                }
            };
            let path = entry.path();
            if path.components().any(|c| c.as_os_str() == ".git") {
                continue;
            }
            match entry.metadata() {
                Ok(meta) if meta.is_file() => {
                    hasher.update(path.to_string_lossy().as_bytes());
                    hasher.update(meta.len().to_le_bytes());
                    match std::fs::File::open(path) {
                        Ok(mut file) => {
                            let mut buf = [0u8; 8192];
                            match file.read(&mut buf) {
                                Ok(n) => hasher.update(&buf[..n]),
                                Err(_) => {
                                    metadata_errors += 1;
                                    hasher.update(b"read-err");
                                }
                            }
                        }
                        Err(_) => {
                            metadata_errors += 1;
                            hasher.update(b"open-err");
                        }
                    }
                }
                Ok(_) => {}
                Err(_) => {
                    metadata_errors += 1;
                    hasher.update(b"meta-err");
                }
            }
        }
        hasher.update(metadata_errors.to_le_bytes());
        Ok(format!("{:x}", hasher.finalize()))
    }
}

impl Drop for RepositoryWatcher {
    fn drop(&mut self) {
        self.stop_background();
    }
}
