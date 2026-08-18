use ignore::gitignore::{Gitignore, GitignoreBuilder};
use mentat_core::error::MentatError;
use mentat_core::models::{FileKind, FileRecord};
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::Read;
use std::path::Path;

pub struct FileScanner;

impl FileScanner {
    pub(crate) fn is_ignored_path(root: &Path, path: &Path, is_dir: bool) -> bool {
        if !path.starts_with(root)
            || path
                .components()
                .any(|component| component.as_os_str() == ".git")
        {
            return true;
        }

        let (global, _) = Gitignore::global();
        let mut ignored = global.matched(path, is_dir).is_ignore();

        let mut directories = vec![root.to_path_buf()];
        if let Ok(relative) = path.strip_prefix(root) {
            let mut current = root.to_path_buf();
            for component in relative.parent().into_iter().flat_map(Path::components) {
                current.push(component.as_os_str());
                directories.push(current.clone());
            }
        }

        for directory in directories {
            let mut builder = GitignoreBuilder::new(&directory);
            let _ = builder.add(directory.join(".gitignore"));
            if directory == root {
                let _ = builder.add(root.join(".git/info/exclude"));
            }
            if let Ok(matcher) = builder.build() {
                let matched = matcher.matched_path_or_any_parents(path, is_dir);
                if matched.is_ignore() {
                    ignored = true;
                } else if matched.is_whitelist() {
                    ignored = false;
                }
            }
        }

        ignored
    }

    pub fn classify_file(path: &Path) -> FileKind {
        let file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_lowercase();

        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        if file_name == "cargo.toml"
            || file_name == "package.json"
            || file_name == "go.mod"
            || file_name == "pom.xml"
            || file_name == "requirements.txt"
            || file_name == "gemfile"
        {
            return FileKind::Manifest;
        }

        if file_name == "readme.md"
            || file_name.starts_with("spec")
            || file_name.starts_with("design")
            || ext == "md"
            || ext == "txt"
            || ext == "rst"
            || ext == "adoc"
        {
            return FileKind::Documentation;
        }

        if ext == "json"
            || ext == "yaml"
            || ext == "yml"
            || ext == "toml"
            || ext == "ini"
            || ext == "env"
        {
            return FileKind::Configuration;
        }

        if ext == "rs"
            || ext == "ts"
            || ext == "js"
            || ext == "py"
            || ext == "go"
            || ext == "cpp"
            || ext == "c"
            || ext == "h"
            || ext == "hpp"
            || ext == "java"
            || ext == "kt"
            || ext == "swift"
            || ext == "cs"
            || ext == "html"
            || ext == "css"
        {
            return FileKind::SourceCode;
        }

        if ext == "png"
            || ext == "jpg"
            || ext == "jpeg"
            || ext == "gif"
            || ext == "svg"
            || ext == "ico"
            || ext == "webp"
            || ext == "mp4"
            || ext == "mp3"
        {
            return FileKind::Asset;
        }

        if ext == "exe"
            || ext == "dll"
            || ext == "so"
            || ext == "dylib"
            || ext == "bin"
            || ext == "o"
            || ext == "a"
            || ext == "wasm"
        {
            return FileKind::Binary;
        }

        FileKind::Other
    }

    pub fn is_text_buffer(buffer: &[u8]) -> bool {
        if buffer.is_empty() {
            return true;
        }
        let sample = &buffer[..buffer.len().min(8192)];
        !sample.contains(&0)
    }

    /// [SEC-F006] Inspects file safely by canonicalizing path, checking boundaries fail-closed,
    /// and directly opening the verified canonical file handle to prevent TOCTOU symlink races.
    pub fn inspect_file(root: &Path, rel_path: &Path) -> Result<FileRecord, MentatError> {
        let canonical_root = root.canonicalize().map_err(|e| {
            MentatError::InvalidRepositoryPath(format!(
                "저장소 루트 정규화 실패 {}: {}",
                root.display(),
                e
            ))
        })?;

        let full_path = root.join(rel_path);
        let canonical_path = full_path.canonicalize().map_err(|e| {
            MentatError::IoError(format!(
                "파일 경로 정규화 실패 {}: {}",
                full_path.display(),
                e
            ))
        })?;

        // Fail-closed canonical root boundary check
        if !canonical_path.starts_with(&canonical_root) {
            return Err(MentatError::ExternalPathBlocked(format!(
                "심볼릭 링크 또는 경로가 저장소 루트 외부를 가리킵니다: {}",
                canonical_path.display()
            )));
        }

        let metadata = std::fs::metadata(&canonical_path)
            .map_err(|e| MentatError::IoError(format!("파일 메타데이터 조회 실패: {}", e)))?;

        let size_bytes = metadata.len();

        // [DBG-F003] Skip hashing files that already exceed the single-file budget.
        if size_bytes > crate::session::MAX_SINGLE_FILE_BYTES {
            return Err(MentatError::IndexingError(format!(
                "파일 크기({} bytes)가 단일 파일 한도를 초과합니다.",
                size_bytes
            )));
        }

        // DBG-F003: Stream hash directly from the verified canonical file handle
        let mut file = File::open(&canonical_path)
            .map_err(|e| MentatError::IoError(format!("파일 열기 실패: {}", e)))?;

        let mut hasher = Sha256::new();
        let mut sample_buffer = Vec::new();
        let mut chunk = [0u8; 65536];
        let mut total_read = 0u64;

        loop {
            let n = file
                .read(&mut chunk)
                .map_err(|e| MentatError::IoError(format!("파일 읽기 실패: {}", e)))?;
            if n == 0 {
                break;
            }
            hasher.update(&chunk[..n]);
            total_read += n as u64;

            if sample_buffer.len() < 2 * 1024 * 1024 {
                let to_take = n.min(2 * 1024 * 1024 - sample_buffer.len());
                sample_buffer.extend_from_slice(&chunk[..to_take]);
            }
        }

        let content_hash = format!("{:x}", hasher.finalize());
        let is_text = Self::is_text_buffer(&sample_buffer);

        let line_count = if is_text && total_read == sample_buffer.len() as u64 {
            Some(
                bytecount::count(&sample_buffer, b'\n')
                    + if sample_buffer.ends_with(b"\n") || sample_buffer.is_empty() {
                        0
                    } else {
                        1
                    },
            )
        } else if is_text {
            let sample_lines = bytecount::count(&sample_buffer, b'\n');
            let avg_line_len = (sample_buffer.len() / (sample_lines.max(1))).max(1);
            Some((total_read as usize) / avg_line_len)
        } else {
            None
        };

        let kind = Self::classify_file(rel_path);
        let text_preview = if is_text {
            let take = sample_buffer.len().min(16 * 1024);
            std::str::from_utf8(&sample_buffer[..take])
                .ok()
                .map(|s| s.to_string())
        } else {
            None
        };

        Ok(FileRecord {
            relative_path: rel_path.to_path_buf(),
            kind,
            size_bytes,
            content_hash,
            is_text,
            line_count,
            text_preview,
        })
    }
}

mod bytecount {
    pub fn count(haystack: &[u8], needle: u8) -> usize {
        haystack.iter().filter(|&&b| b == needle).count()
    }
}
