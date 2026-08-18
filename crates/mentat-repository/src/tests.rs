use super::*;
use mentat_core::ports::RepositoryReader;
use std::fs;
use tempfile::tempdir;

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
