use ignore::WalkBuilder;
use mentat_core::error::MentatError;
use notify::{RecursiveMode, Watcher};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime};

pub const WATCHER_THROTTLE_INTERVAL: Duration = Duration::from_millis(1000);

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
        let worker = std::thread::Builder::new()
            .name("mentat-watcher".to_string())
            .spawn(move || {
                let (event_tx, event_rx) = mpsc::channel();
                let mut watcher = match notify::recommended_watcher(
                    move |result: notify::Result<notify::Event>| {
                        let _ = event_tx.send(result);
                    },
                ) {
                    Ok(watcher) => watcher,
                    Err(_) => {
                        let _ = tx.send(true);
                        return;
                    }
                };
                if watcher.watch(&root, RecursiveMode::Recursive).is_err() {
                    let _ = tx.send(true);
                    return;
                }
                let mut changed_path_hashes = HashMap::<PathBuf, String>::new();

                while !stop_flag.load(Ordering::Relaxed) {
                    match event_rx.recv_timeout(Duration::from_millis(50)) {
                        Ok(Ok(event)) => {
                            let mut changed = false;
                            for path in event.paths {
                                if path
                                    .components()
                                    .any(|component| component.as_os_str() == ".git")
                                {
                                    continue;
                                }
                                if path.is_file() {
                                    match Self::hash_file(&path) {
                                        Ok(hash) => {
                                            let previous =
                                                changed_path_hashes.insert(path, hash.clone());
                                            if previous.as_deref() != Some(hash.as_str()) {
                                                changed = true;
                                            }
                                        }
                                        Err(_) => changed = true,
                                    }
                                } else {
                                    changed_path_hashes.remove(&path);
                                    changed = true;
                                }
                            }
                            if changed && tx.send(true).is_err() {
                                break;
                            }
                        }
                        Ok(Err(_)) => {
                            if tx.send(true).is_err() {
                                break;
                            }
                        }
                        Err(mpsc::RecvTimeoutError::Timeout) => {}
                        Err(mpsc::RecvTimeoutError::Disconnected) => break,
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
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
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
                    match Self::hash_file(path) {
                        Ok(hash) => hasher.update(hash.as_bytes()),
                        Err(_) => {
                            metadata_errors += 1;
                            hasher.update(b"content-err");
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

    fn hash_file(path: &Path) -> Result<String, MentatError> {
        use std::io::Read;
        let mut file = std::fs::File::open(path)
            .map_err(|e| MentatError::IoError(format!("파일 열기 실패: {e}")))?;
        let mut hasher = Sha256::new();
        let mut buffer = [0u8; 64 * 1024];
        loop {
            let read = file
                .read(&mut buffer)
                .map_err(|e| MentatError::IoError(format!("파일 읽기 실패: {e}")))?;
            if read == 0 {
                break;
            }
            hasher.update(&buffer[..read]);
        }
        Ok(format!("{:x}", hasher.finalize()))
    }
}

impl Drop for RepositoryWatcher {
    fn drop(&mut self) {
        self.stop_background();
    }
}
