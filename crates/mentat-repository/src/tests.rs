use super::*;
use mentat_core::ports::RepositoryReader;
use std::fs;
use std::io::Write;
use tempfile::tempdir;
use tokio_util::sync::CancellationToken;

#[tokio::test]
async fn test_read_only_session_scan_and_snapshot() {
    let dir = tempdir().unwrap();
    let root = dir.path();

    // Create dummy files
    fs::write(root.join("Cargo.toml"), "[package]\nname = \"test\"\n").unwrap();
    fs::create_dir(root.join("src")).unwrap();
    fs::write(
        root.join("src/main.rs"),
        "fn main() {\n    println!(\"hello\");\n}\n",
    )
    .unwrap();
    fs::write(root.join("README.md"), "# Test Project\n").unwrap();

    let session = ReadOnlySession::open(root).expect("Session should open");
    let files = session.scan_files().await.expect("Scan should succeed");

    assert_eq!(files.len(), 3);
    assert!(files
        .iter()
        .any(|f| f.relative_path.to_str() == Some("Cargo.toml")));
    assert!(files
        .iter()
        .any(|f| f.relative_path.to_str() == Some("src\\main.rs")
            || f.relative_path.to_str() == Some("src/main.rs")));

    let lines = session
        .read_file_lines(std::path::Path::new("src/main.rs"), 1, 2)
        .await
        .expect("Lines should be read");
    assert!(lines.contains("fn main()"));

    let snapshot = session
        .create_snapshot()
        .await
        .expect("Snapshot should succeed");
    assert_eq!(snapshot.file_count, 3);
    assert!(!snapshot.tree_digest.is_empty());
}

#[tokio::test]
async fn test_external_path_blocked() {
    let dir1 = tempdir().unwrap();
    let dir2 = tempdir().unwrap();

    fs::write(dir2.path().join("secret.txt"), "secret data").unwrap();

    let session = ReadOnlySession::open(dir1.path()).unwrap();
    let escaped_rel = std::path::Path::new("../")
        .join(dir2.path().file_name().unwrap())
        .join("secret.txt");

    let result = session.read_file_content(&escaped_rel).await;
    assert!(result.is_err());
}

#[test]
fn test_sec_f006_inspect_file_canonical_safety() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let sub = root.join("src");
    fs::create_dir(&sub).unwrap();
    let file_path = sub.join("lib.rs");
    fs::write(&file_path, "pub fn test() {}").unwrap();

    let record =
        crate::scanner::FileScanner::inspect_file(root, std::path::Path::new("src/lib.rs"))
            .expect("Valid inside-root file should pass");
    assert!(record.is_text);

    // Path traversal attempt should fail
    let bad_path = std::path::Path::new("../outside.txt");
    let err = crate::scanner::FileScanner::inspect_file(root, bad_path);
    assert!(err.is_err());
}

#[tokio::test]
async fn test_dbg_f003_scan_and_snapshot_deterministic_benchmark() {
    let dir = tempdir().unwrap();
    let root = dir.path();

    // Create 100 files across multiple subdirectories
    for i in 0..100 {
        let sub = root.join(format!("mod_{}", i % 10));
        let _ = fs::create_dir_all(&sub);
        fs::write(
            sub.join(format!("file_{}.rs", i)),
            format!("// Code file {}", i),
        )
        .unwrap();
    }

    let start = std::time::Instant::now();
    let session = ReadOnlySession::open(root).expect("Session open");
    let files = session.scan_files().await.expect("Scan files");
    let snapshot1 = session.create_snapshot_from_files(&files);
    let snapshot2 = session.create_snapshot_from_files(&files);

    let elapsed = start.elapsed();
    assert_eq!(files.len(), 100);
    // Deterministic snapshot digest
    assert_eq!(snapshot1.tree_digest, snapshot2.tree_digest);
    assert!(
        elapsed.as_millis() < 500,
        "100 files scan should complete under 500ms"
    );
}

#[test]
fn test_dbg_f008_watcher_throttling_and_change_detection() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    fs::write(root.join("initial.rs"), "initial").unwrap();

    let mut watcher = crate::watcher::RepositoryWatcher::new(root);
    // Immediate second call should be throttled and return false
    assert!(!watcher.check_for_changes().unwrap());
}

#[test]
fn test_dbg_f008_constructor_does_not_walk_tree() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    for i in 0..200 {
        fs::write(root.join(format!("f{i}.rs")), "x").unwrap();
    }

    let start = std::time::Instant::now();
    let mut watcher = crate::watcher::RepositoryWatcher::new(root);
    assert!(start.elapsed().as_millis() < 50);
    watcher.spawn_background();
    assert!(start.elapsed().as_millis() < 80);
}

#[test]
fn test_dbg_f008_background_watcher_poll_is_nonblocking() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    fs::write(root.join("initial.rs"), "initial").unwrap();

    let mut watcher = crate::watcher::RepositoryWatcher::new(root);
    watcher.spawn_background();

    let start = std::time::Instant::now();
    let changed = watcher.poll_changes().unwrap();
    assert!(start.elapsed().as_millis() < 50);
    assert!(!changed);
}

#[test]
fn test_dbg_f008_background_watcher_detects_add_delete_modify() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    fs::write(root.join("initial.rs"), "aaaa").unwrap();

    let mut watcher = crate::watcher::RepositoryWatcher::new(root);
    watcher.spawn_background();
    std::thread::sleep(std::time::Duration::from_millis(150));

    fs::write(root.join("added.rs"), "new").unwrap();
    assert!(wait_for_change(&mut watcher), "add should be detected");

    fs::write(root.join("initial.rs"), "bbbb").unwrap();
    assert!(
        wait_for_change(&mut watcher),
        "same-size modify should be detected"
    );

    fs::remove_file(root.join("added.rs")).unwrap();
    assert!(wait_for_change(&mut watcher), "delete should be detected");
}

#[test]
fn test_imp_f005_open_reuses_known_repo_id() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    fs::write(root.join("Cargo.toml"), "[package]\nname = \"t\"\n").unwrap();

    let first = ReadOnlySession::open(root).expect("first open");
    let known_id = first.profile().id;
    drop(first);

    let second = ReadOnlySession::open_with_known_id(root, Some(known_id)).expect("reopen");
    assert_eq!(second.profile().id, known_id);
    assert_eq!(second.root_path(), root.canonicalize().unwrap().as_path());
}

#[tokio::test]
async fn test_dbg_f003_giant_file_omitted_without_full_hash() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    fs::write(root.join("ok.rs"), "fn main() {}\n").unwrap();
    let giant = root.join("giant.bin");
    let mut file = fs::File::create(&giant).unwrap();
    let chunk = vec![b'x'; 1024 * 1024];
    for _ in 0..11 {
        file.write_all(&chunk).unwrap();
    }
    drop(file);

    let start = std::time::Instant::now();
    let session = ReadOnlySession::open(root).unwrap();
    let outcome = session
        .scan_files_with_limits(ScanLimits::default(), CancellationToken::new())
        .await
        .unwrap();

    assert!(
        start.elapsed().as_secs() < 3,
        "oversized file must not be fully hashed"
    );
    assert_eq!(outcome.files.len(), 1);
    assert!(outcome
        .omissions
        .iter()
        .any(|o| o.reason == ScanOmitReason::FileTooLarge));
}

#[tokio::test]
async fn test_dbg_f003_mid_scan_cancel() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    for i in 0..400 {
        fs::write(root.join(format!("f{i}.rs")), format!("// {i}\n")).unwrap();
    }

    let session = ReadOnlySession::open(root).unwrap();
    let cancel = CancellationToken::new();
    let cancel_flag = cancel.clone();
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(2)).await;
        cancel_flag.cancel();
    });
    let outcome = session
        .scan_files_with_limits(ScanLimits::default(), cancel)
        .await
        .unwrap();
    assert!(
        outcome.cancelled || outcome.files.len() < 400,
        "scan must stop early or record cancellation"
    );
    if outcome.cancelled {
        assert!(outcome
            .omissions
            .iter()
            .any(|o| o.reason == ScanOmitReason::Cancelled));
        let snapshot = session.create_snapshot_from_outcome(&outcome);
        assert_eq!(
            snapshot.status,
            mentat_core::models::SnapshotStatus::Incomplete
        );
    }
}

#[tokio::test]
async fn test_dbg_f003_preview_memory_has_a_global_budget() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let text = "x".repeat(16 * 1024);
    for i in 0..700 {
        fs::write(root.join(format!("preview_{i}.rs")), &text).unwrap();
    }

    let session = ReadOnlySession::open(root).unwrap();
    let outcome = session
        .scan_files_with_limits(ScanLimits::default(), CancellationToken::new())
        .await
        .unwrap();
    let preview_bytes: usize = outcome
        .files
        .iter()
        .filter_map(|file| file.text_preview.as_ref())
        .map(String::len)
        .sum();

    assert!(preview_bytes <= crate::session::MAX_SCAN_PREVIEW_BYTES);
    assert!(outcome.files.iter().any(|file| file.text_preview.is_none()));
}

#[tokio::test]
async fn test_dbg_f003_representative_budget_profile() {
    assert_eq!(MAX_SCAN_FILES_LIMIT, 100_000);
    assert_eq!(MAX_SCAN_TOTAL_BYTES_LIMIT, 2 * 1024 * 1024 * 1024);
    assert_eq!(MAX_SINGLE_FILE_BYTES, 10 * 1024 * 1024);

    let dir = tempdir().unwrap();
    let root = dir.path();
    for i in 0..80 {
        let sub = root.join(format!("mod_{}", i % 8));
        let _ = fs::create_dir_all(&sub);
        fs::write(sub.join(format!("file_{i}.rs")), format!("// {i}\n")).unwrap();
    }

    let session = ReadOnlySession::open(root).unwrap();
    let limited = ScanLimits {
        max_files: 25,
        max_total_bytes: MAX_SCAN_TOTAL_BYTES_LIMIT,
        max_single_file_bytes: MAX_SINGLE_FILE_BYTES,
    };
    let start = std::time::Instant::now();
    let outcome = session
        .scan_files_with_limits(limited, CancellationToken::new())
        .await
        .unwrap();

    assert!(start.elapsed().as_millis() < 2_000);
    assert_eq!(outcome.files.len(), 25);
    assert!(outcome
        .omissions
        .iter()
        .any(|o| o.reason == ScanOmitReason::FileCountLimit));
}

#[test]
fn test_dbg_f002_preserved_mtime_same_size_content_change() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let path = root.join("same.rs");
    let mut original = vec![b'a'; 16 * 1024];
    fs::write(&path, &original).unwrap();
    let original_mtime = fs::metadata(&path).unwrap().modified().unwrap();

    let mut watcher = crate::watcher::RepositoryWatcher::new(root);
    assert!(!watcher.force_content_check().unwrap());

    original[12 * 1024..].fill(b'b');
    fs::write(&path, &original).unwrap();
    let file = fs::File::options().write(true).open(&path).unwrap();
    file.set_modified(original_mtime).unwrap();
    drop(file);

    assert_eq!(
        fs::metadata(&path).unwrap().modified().unwrap(),
        original_mtime
    );
    assert!(
        watcher.force_content_check().unwrap(),
        "same-size content change with restored mtime must be STALE"
    );
}

#[test]
fn test_dbg_f002_watcher_stop_latency_is_bounded() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("watch.rs"), "fn main() {}\n").unwrap();
    let mut watcher = crate::watcher::RepositoryWatcher::new(dir.path());
    watcher.spawn_background();
    let start = std::time::Instant::now();
    watcher.stop_background();
    assert!(start.elapsed() < std::time::Duration::from_millis(250));
}

#[test]
fn test_dbg_f002_rapid_replace_and_metadata_error_fingerprint() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let path = root.join("swap.rs");
    fs::write(&path, "1111").unwrap();
    let mut watcher = crate::watcher::RepositoryWatcher::new(root);
    assert!(!watcher.force_content_check().unwrap());
    fs::write(&path, "2222").unwrap();
    assert!(watcher.force_content_check().unwrap());
    fs::write(&path, "3333").unwrap();
    assert!(watcher.force_content_check().unwrap());

    // Directory in place of a file still changes the fingerprint (metadata edge).
    fs::remove_file(&path).unwrap();
    fs::create_dir(&path).unwrap();
    assert!(watcher.force_content_check().unwrap());
}

#[test]
fn test_dbg_f002_large_tree_poll_stays_nonblocking() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    for i in 0..400 {
        fs::write(root.join(format!("n{i}.rs")), "x").unwrap();
    }
    let mut watcher = crate::watcher::RepositoryWatcher::new(root);
    watcher.spawn_background();
    for _ in 0..20 {
        let start = std::time::Instant::now();
        let _ = watcher.poll_changes().unwrap();
        assert!(start.elapsed().as_millis() < 20);
    }
}

#[test]
#[ignore = "representative 100k-file / 2GiB budget profile; cargo test -- --ignored"]
fn test_dbg_f003_100k_2gib_benchmark_profile() {
    assert_eq!(MAX_SCAN_FILES_LIMIT, 100_000);
    assert_eq!(MAX_SCAN_TOTAL_BYTES_LIMIT, 2 * 1024 * 1024 * 1024);

    let dir = tempdir().unwrap();
    let root = dir.path();
    let full_text_file_count = (MAX_SCAN_TOTAL_BYTES_LIMIT / MAX_SINGLE_FILE_BYTES) as usize;
    let remaining_text_bytes = MAX_SCAN_TOTAL_BYTES_LIMIT % MAX_SINGLE_FILE_BYTES;
    let text_chunk = vec![b'x'; MAX_SINGLE_FILE_BYTES as usize];
    for i in 0..100_000 {
        let sub = root.join(format!("b{}", i / 1000));
        let _ = fs::create_dir_all(&sub);
        let path = sub.join(format!("{i}.rs"));
        if i < full_text_file_count {
            fs::write(path, &text_chunk).unwrap();
        } else if i == full_text_file_count && remaining_text_bytes > 0 {
            fs::write(path, &text_chunk[..remaining_text_bytes as usize]).unwrap();
        } else {
            fs::File::create(path).unwrap();
        }
    }
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let session = ReadOnlySession::open(root).unwrap();
        let start = std::time::Instant::now();
        let peak_before = peak_working_set_bytes();
        let outcome = session
            .scan_files_with_limits(ScanLimits::default(), CancellationToken::new())
            .await
            .unwrap();
        assert!(outcome.files.len() <= MAX_SCAN_FILES_LIMIT);
        assert_eq!(
            outcome.files.iter().map(|file| file.size_bytes).sum::<u64>(),
            MAX_SCAN_TOTAL_BYTES_LIMIT
        );
        let preview_bytes: usize = outcome
            .files
            .iter()
            .filter_map(|file| file.text_preview.as_ref())
            .map(String::len)
            .sum();
        assert!(preview_bytes <= crate::session::MAX_SCAN_PREVIEW_BYTES);
        let peak_after = peak_working_set_bytes();
        let elapsed = start.elapsed();
        println!(
            "files={} corpus_bytes={} preview_bytes={} elapsed_ms={} peak_working_set_before={} peak_working_set_after={}",
            outcome.files.len(),
            MAX_SCAN_TOTAL_BYTES_LIMIT,
            preview_bytes,
            elapsed.as_millis(),
            peak_before.unwrap_or(0),
            peak_after.unwrap_or(0),
        );
        assert!(elapsed.as_secs() < 180);
    });
}

#[cfg(windows)]
fn peak_working_set_bytes() -> Option<usize> {
    use windows_sys::Win32::System::ProcessStatus::{
        GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS,
    };
    use windows_sys::Win32::System::Threading::GetCurrentProcess;

    let mut counters = PROCESS_MEMORY_COUNTERS::default();
    let ok = unsafe {
        GetProcessMemoryInfo(
            GetCurrentProcess(),
            &mut counters,
            std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32,
        )
    };
    (ok != 0).then_some(counters.PeakWorkingSetSize)
}

#[cfg(not(windows))]
fn peak_working_set_bytes() -> Option<usize> {
    None
}

fn wait_for_change(watcher: &mut crate::watcher::RepositoryWatcher) -> bool {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    while std::time::Instant::now() < deadline {
        if watcher.poll_changes().unwrap_or(false) {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    false
}
