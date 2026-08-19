use crate::egress::EgressFilter;
use mentat_core::{
    FileRecord, MentatError, RepositoryReader, RepositorySnapshot, RepositoryToolArguments,
    RepositoryToolCall, RepositoryToolName, RepositoryToolResult, SnapshotStatus, SourceRef,
    ToolOmission, ToolOmissionReason,
};
use mentat_inference::ToolDefinition;
use sha2::{Digest, Sha256};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

const MAX_RESULT_BYTES: usize = 64 * 1024;
const MAX_READ_LINES: usize = 400;

pub fn repository_tool_definitions() -> Vec<ToolDefinition> {
    RepositoryToolName::ALL
        .into_iter()
        .map(|name| ToolDefinition {
            name: name.wire_name().to_string(),
            schema_version: "repository-tool.v1".to_string(),
            description: tool_description(name).to_string(),
            input_schema: tool_input_schema(name),
        })
        .collect()
}

fn tool_description(name: RepositoryToolName) -> &'static str {
    match name {
        RepositoryToolName::RepoStatus => "현재 repository snapshot의 metadata 상태를 조회합니다.",
        RepositoryToolName::ListTree => "상대 경로 아래의 정렬된 파일 트리를 조회합니다.",
        RepositoryToolName::SearchPaths => "파일 상대 경로에서 문자열을 검색합니다.",
        RepositoryToolName::SearchText => "텍스트 파일 내용에서 문자열과 SourceRef를 검색합니다.",
        RepositoryToolName::ReadFileLines => "텍스트 파일의 제한된 줄 범위를 읽습니다.",
        RepositoryToolName::FileMetadata => "파일 크기, hash, line count metadata를 조회합니다.",
    }
}

fn tool_input_schema(name: RepositoryToolName) -> serde_json::Value {
    match name {
        RepositoryToolName::RepoStatus => serde_json::json!({
            "type": "object", "properties": {}, "additionalProperties": false
        }),
        RepositoryToolName::ListTree => serde_json::json!({
            "type": "object",
            "properties": {
                "relative_path": {"type": ["string", "null"]},
                "depth": {"type": "integer", "minimum": 1, "maximum": 4},
                "limit": {"type": "integer", "minimum": 1, "maximum": 500}
            },
            "required": ["depth", "limit"], "additionalProperties": false
        }),
        RepositoryToolName::SearchPaths => serde_json::json!({
            "type": "object",
            "properties": {"query": {"type": "string"}, "limit": {"type": "integer", "minimum": 1, "maximum": 100}},
            "required": ["query", "limit"], "additionalProperties": false
        }),
        RepositoryToolName::SearchText => serde_json::json!({
            "type": "object",
            "properties": {
                "query": {"type": "string"},
                "path_filter": {"type": ["string", "null"]},
                "limit": {"type": "integer", "minimum": 1, "maximum": 100}
            },
            "required": ["query", "limit"], "additionalProperties": false
        }),
        RepositoryToolName::ReadFileLines => serde_json::json!({
            "type": "object",
            "properties": {
                "relative_path": {"type": "string"},
                "start_line": {"type": "integer", "minimum": 1},
                "end_line": {"type": "integer", "minimum": 1}
            },
            "required": ["relative_path", "start_line", "end_line"], "additionalProperties": false
        }),
        RepositoryToolName::FileMetadata => serde_json::json!({
            "type": "object",
            "properties": {"relative_path": {"type": "string"}},
            "required": ["relative_path"], "additionalProperties": false
        }),
    }
}

pub struct RepositoryToolGateway {
    reader: Arc<dyn RepositoryReader>,
    snapshot: RepositorySnapshot,
    files: Vec<FileRecord>,
}

impl RepositoryToolGateway {
    pub fn new(
        reader: Arc<dyn RepositoryReader>,
        snapshot: RepositorySnapshot,
        mut files: Vec<FileRecord>,
    ) -> Self {
        files.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
        Self {
            reader,
            snapshot,
            files,
        }
    }

    pub fn snapshot(&self) -> &RepositorySnapshot {
        &self.snapshot
    }

    pub async fn execute(
        &self,
        call: RepositoryToolCall,
        cancel: CancellationToken,
    ) -> Result<RepositoryToolResult, MentatError> {
        if call.snapshot_id != self.snapshot.id {
            return Err(tool_error(
                "TOOL_SNAPSHOT_MISMATCH",
                "tool call snapshot이 현재 gateway snapshot과 다릅니다.",
            ));
        }
        validate_call_shape(call.name, &call.arguments)?;
        if call.name != RepositoryToolName::RepoStatus
            && self.snapshot.status != SnapshotStatus::Ready
        {
            return Err(tool_error(
                "REPOSITORY_REINDEX_REQUIRED",
                "STALE/Incomplete snapshot에서는 repo_status 외 도구를 실행할 수 없습니다.",
            ));
        }
        if cancel.is_cancelled() {
            return Err(MentatError::Cancelled);
        }

        let (content, source_refs, omissions) = match call.arguments {
            RepositoryToolArguments::RepoStatus => (
                serde_json::json!({
                    "repository_id": self.snapshot.repo_id,
                    "snapshot_id": self.snapshot.id,
                    "status": self.snapshot.status,
                    "file_count": self.snapshot.file_count,
                    "total_bytes": self.snapshot.total_bytes,
                })
                .to_string(),
                Vec::new(),
                Vec::new(),
            ),
            RepositoryToolArguments::ListTree {
                relative_path,
                depth,
                limit,
            } => self.list_tree(relative_path.as_deref(), depth, limit)?,
            RepositoryToolArguments::SearchPaths { query, limit } => {
                self.search_paths(&query, limit)?
            }
            RepositoryToolArguments::SearchText {
                query,
                path_filter,
                limit,
            } => {
                self.search_text(&query, path_filter.as_deref(), limit, &cancel)
                    .await?
            }
            RepositoryToolArguments::ReadFileLines {
                relative_path,
                start_line,
                end_line,
            } => {
                self.read_file_lines(&relative_path, start_line, end_line, &cancel)
                    .await?
            }
            RepositoryToolArguments::FileMetadata { relative_path } => {
                self.file_metadata(&relative_path)?
            }
        };
        let (content, omissions) = bound_content(content, omissions);
        let (content, _) = EgressFilter::scan_and_redact_secrets(&content);
        let (content, omissions) = bound_content(content, omissions);
        let source_refs = source_refs
            .into_iter()
            .map(|mut source| {
                source.excerpt = EgressFilter::scan_and_redact_secrets(&source.excerpt).0;
                source.excerpt = source.excerpt.trim_end_matches('\n').to_string();
                source
            })
            .collect();
        Ok(RepositoryToolResult {
            call_id: call.call_id,
            snapshot_id: call.snapshot_id,
            content_bytes: u32::try_from(content.len()).unwrap_or(u32::MAX),
            content,
            source_refs,
            omissions,
        })
    }

    fn list_tree(
        &self,
        relative_path: Option<&Path>,
        depth: u8,
        limit: u16,
    ) -> Result<(String, Vec<SourceRef>, Vec<ToolOmission>), MentatError> {
        let root = relative_path.unwrap_or_else(|| Path::new(""));
        validate_relative_path(root)?;
        let depth = depth.clamp(1, 4) as usize;
        let limit = usize::from(limit.clamp(1, 500));
        let root_components = root.components().count();
        let mut paths = Vec::new();
        let mut omitted = 0u64;
        for file in &self.files {
            if !file.relative_path.starts_with(root) {
                continue;
            }
            if file
                .relative_path
                .components()
                .count()
                .saturating_sub(root_components)
                > depth
            {
                continue;
            }
            if paths.len() == limit {
                omitted += 1;
                continue;
            }
            paths.push(file.relative_path.to_string_lossy().replace('\\', "/"));
        }
        let omissions = if omitted == 0 {
            Vec::new()
        } else {
            vec![omission(
                ToolOmissionReason::EntryLimit,
                None,
                "LIST_TREE_LIMIT",
                omitted,
                0,
            )]
        };
        Ok((
            serde_json::to_string(&paths)
                .map_err(|error| tool_error("TOOL_RESULT_ENCODE_FAILED", &error.to_string()))?,
            Vec::new(),
            omissions,
        ))
    }

    fn search_paths(
        &self,
        query: &str,
        limit: u16,
    ) -> Result<(String, Vec<SourceRef>, Vec<ToolOmission>), MentatError> {
        validate_query(query)?;
        let query = query.to_lowercase();
        let limit = usize::from(limit.clamp(1, 100));
        let mut matches = Vec::new();
        let mut omitted = 0u64;
        for file in &self.files {
            let path = file.relative_path.to_string_lossy().replace('\\', "/");
            if !path.to_lowercase().contains(&query) {
                continue;
            }
            if matches.len() == limit {
                omitted += 1;
            } else {
                matches.push(path);
            }
        }
        let omissions = if omitted == 0 {
            Vec::new()
        } else {
            vec![omission(
                ToolOmissionReason::EntryLimit,
                None,
                "SEARCH_PATHS_LIMIT",
                omitted,
                0,
            )]
        };
        Ok((
            serde_json::to_string(&matches)
                .map_err(|error| tool_error("TOOL_RESULT_ENCODE_FAILED", &error.to_string()))?,
            Vec::new(),
            omissions,
        ))
    }

    async fn search_text(
        &self,
        query: &str,
        path_filter: Option<&str>,
        limit: u16,
        cancel: &CancellationToken,
    ) -> Result<(String, Vec<SourceRef>, Vec<ToolOmission>), MentatError> {
        validate_query(query)?;
        if let Some(filter) = path_filter {
            validate_relative_path(Path::new(filter))?;
        }
        let limit = usize::from(limit.clamp(1, 100));
        let mut refs = Vec::new();
        let mut omissions = Vec::new();
        let query_lower = query.to_lowercase();
        'files: for file in &self.files {
            if cancel.is_cancelled() {
                return Err(MentatError::Cancelled);
            }
            if !file.is_text {
                continue;
            }
            if let Some(filter) = path_filter {
                if !file
                    .relative_path
                    .to_string_lossy()
                    .replace('\\', "/")
                    .contains(filter)
                {
                    continue;
                }
            }
            if file.size_bytes > MAX_RESULT_BYTES as u64 {
                omissions.push(omission(
                    ToolOmissionReason::ByteLimit,
                    Some(file.relative_path.clone()),
                    "SEARCH_FILE_TOO_LARGE",
                    1,
                    file.size_bytes,
                ));
                continue;
            }
            let content = match self.reader.read_file_content(&file.relative_path).await {
                Ok(content) => content,
                Err(_) => {
                    omissions.push(omission(
                        ToolOmissionReason::ReadError,
                        Some(file.relative_path.clone()),
                        "SEARCH_READ_FAILED",
                        1,
                        0,
                    ));
                    continue;
                }
            };
            if sha256_hex(content.as_bytes()) != file.content_hash {
                return Err(tool_error(
                    "SOURCE_LIVE_HASH_MISMATCH",
                    "scan hash와 live file body가 달라 tool result를 폐기했습니다.",
                ));
            }
            for (index, line) in content.lines().enumerate() {
                if !line.to_lowercase().contains(&query_lower) {
                    continue;
                }
                refs.push(source_ref(
                    self.snapshot.id,
                    file,
                    index + 1,
                    index + 1,
                    line,
                ));
                if refs.len() == limit {
                    break 'files;
                }
            }
        }
        let summary: Vec<_> = refs
            .iter()
            .map(|source| {
                serde_json::json!({
                    "source_ref_id": source.id,
                    "path": source.relative_path,
                    "line": source.line_start,
                    "excerpt": source.excerpt,
                })
            })
            .collect();
        Ok((
            serde_json::to_string(&summary)
                .map_err(|error| tool_error("TOOL_RESULT_ENCODE_FAILED", &error.to_string()))?,
            refs,
            omissions,
        ))
    }

    async fn read_file_lines(
        &self,
        relative_path: &Path,
        start_line: usize,
        end_line: usize,
        cancel: &CancellationToken,
    ) -> Result<(String, Vec<SourceRef>, Vec<ToolOmission>), MentatError> {
        validate_relative_path(relative_path)?;
        if start_line == 0 || end_line < start_line || end_line - start_line + 1 > MAX_READ_LINES {
            return Err(tool_error(
                "READ_FILE_RANGE_INVALID",
                "line range는 1부터 시작하며 최대 400행이어야 합니다.",
            ));
        }
        if cancel.is_cancelled() {
            return Err(MentatError::Cancelled);
        }
        let file = self.file_record(relative_path)?;
        if !file.is_text {
            return Err(tool_error(
                "READ_FILE_BINARY",
                "binary file은 읽을 수 없습니다.",
            ));
        }
        let content = self.reader.read_file_content(relative_path).await?;
        if sha256_hex(content.as_bytes()) != file.content_hash {
            return Err(tool_error(
                "SOURCE_LIVE_HASH_MISMATCH",
                "scan hash와 live file body가 달라 tool result를 폐기했습니다.",
            ));
        }
        let lines: Vec<&str> = content.lines().collect();
        if start_line > lines.len() {
            return Err(tool_error(
                "READ_FILE_RANGE_INVALID",
                "start_line이 파일 길이를 초과했습니다.",
            ));
        }
        let actual_end = end_line.min(lines.len());
        let selected = lines[start_line - 1..actual_end].join("\n");
        let source = source_ref(self.snapshot.id, file, start_line, actual_end, &selected);
        Ok((selected, vec![source], Vec::new()))
    }

    fn file_metadata(
        &self,
        relative_path: &Path,
    ) -> Result<(String, Vec<SourceRef>, Vec<ToolOmission>), MentatError> {
        validate_relative_path(relative_path)?;
        let file = self.file_record(relative_path)?;
        Ok((
            serde_json::json!({
                "path": file.relative_path,
                "kind": file.kind,
                "size_bytes": file.size_bytes,
                "content_hash": file.content_hash,
                "is_text": file.is_text,
                "line_count": file.line_count,
            })
            .to_string(),
            Vec::new(),
            Vec::new(),
        ))
    }

    fn file_record(&self, path: &Path) -> Result<&FileRecord, MentatError> {
        self.files
            .iter()
            .find(|file| file.relative_path == path)
            .ok_or_else(|| tool_error("TOOL_PATH_NOT_FOUND", "snapshot에 없는 경로입니다."))
    }

    #[cfg(test)]
    fn set_snapshot_status_for_test(&mut self, status: SnapshotStatus) {
        self.snapshot.status = status;
    }
}

fn validate_call_shape(
    name: RepositoryToolName,
    arguments: &RepositoryToolArguments,
) -> Result<(), MentatError> {
    let matches = matches!(
        (name, arguments),
        (
            RepositoryToolName::RepoStatus,
            RepositoryToolArguments::RepoStatus
        ) | (
            RepositoryToolName::ListTree,
            RepositoryToolArguments::ListTree { .. }
        ) | (
            RepositoryToolName::SearchPaths,
            RepositoryToolArguments::SearchPaths { .. }
        ) | (
            RepositoryToolName::SearchText,
            RepositoryToolArguments::SearchText { .. }
        ) | (
            RepositoryToolName::ReadFileLines,
            RepositoryToolArguments::ReadFileLines { .. }
        ) | (
            RepositoryToolName::FileMetadata,
            RepositoryToolArguments::FileMetadata { .. }
        )
    );
    if matches {
        Ok(())
    } else {
        Err(tool_error(
            "TOOL_ARGUMENT_VARIANT_MISMATCH",
            "tool name과 argument variant가 일치하지 않습니다.",
        ))
    }
}

fn validate_relative_path(path: &Path) -> Result<(), MentatError> {
    if path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(MentatError::ExternalPathBlocked(
            "repository tool은 정규 상대 경로만 허용합니다.".to_string(),
        ));
    }
    Ok(())
}

fn validate_query(query: &str) -> Result<(), MentatError> {
    if query.trim().is_empty() || query.len() > 512 {
        return Err(tool_error(
            "TOOL_QUERY_INVALID",
            "검색어는 1~512 UTF-8 bytes여야 합니다.",
        ));
    }
    Ok(())
}

fn source_ref(
    snapshot_id: Uuid,
    file: &FileRecord,
    line_start: usize,
    line_end: usize,
    excerpt: &str,
) -> SourceRef {
    SourceRef {
        id: Uuid::new_v4(),
        snapshot_id,
        relative_path: file.relative_path.clone(),
        line_start,
        line_end,
        content_hash: file.content_hash.clone(),
        excerpt: truncate_chars(excerpt, 512),
    }
}

fn omission(
    reason: ToolOmissionReason,
    relative_path: Option<PathBuf>,
    detail_code: &str,
    omitted_count: u64,
    omitted_bytes: u64,
) -> ToolOmission {
    ToolOmission {
        reason,
        relative_path,
        detail_code: detail_code.to_string(),
        omitted_count,
        omitted_bytes,
    }
}

fn bound_content(
    mut content: String,
    mut omissions: Vec<ToolOmission>,
) -> (String, Vec<ToolOmission>) {
    if content.len() <= MAX_RESULT_BYTES {
        return (content, omissions);
    }
    let original = content.len();
    let mut end = MAX_RESULT_BYTES;
    while !content.is_char_boundary(end) {
        end -= 1;
    }
    content.truncate(end);
    omissions.push(omission(
        ToolOmissionReason::ByteLimit,
        None,
        "TOOL_RESULT_BYTE_LIMIT",
        0,
        (original - end) as u64,
    ));
    (content, omissions)
}

fn truncate_chars(value: &str, limit: usize) -> String {
    value.chars().take(limit).collect()
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn tool_error(code: &str, message: &str) -> MentatError {
    MentatError::BackendError {
        code: code.to_string(),
        message: message.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mentat_core::{
        RepositoryReader, RepositoryToolArguments, RepositoryToolCall, RepositoryToolName,
        SnapshotStatus,
    };
    use mentat_repository::ReadOnlySession;
    use std::path::PathBuf;
    use std::sync::Arc;
    use tempfile::tempdir;
    use tokio_util::sync::CancellationToken;
    use uuid::Uuid;

    async fn fixture() -> RepositoryToolGateway {
        let dir = tempdir().unwrap().keep();
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(
            dir.join("src/lib.rs"),
            "pub fn mentor() {\n    println!(\"read only\");\n}\n",
        )
        .unwrap();
        std::fs::write(dir.join("README.md"), "# Fixture\nmentor module\n").unwrap();
        let session = Arc::new(ReadOnlySession::open(&dir).unwrap());
        let files = session.scan_files().await.unwrap();
        let snapshot = session.create_snapshot_from_files(&files);
        RepositoryToolGateway::new(session, snapshot, files)
    }

    async fn fixture_for_path(path: &Path) -> RepositoryToolGateway {
        let session = Arc::new(ReadOnlySession::open(path).unwrap());
        let files = session.scan_files().await.unwrap();
        let snapshot = session.create_snapshot_from_files(&files);
        RepositoryToolGateway::new(session, snapshot, files)
    }

    #[tokio::test]
    async fn search_then_read_returns_bounded_source_refs() {
        let gateway = fixture().await;
        let search = gateway
            .execute(
                RepositoryToolCall {
                    call_id: Uuid::new_v4(),
                    snapshot_id: gateway.snapshot().id,
                    name: RepositoryToolName::SearchText,
                    arguments: RepositoryToolArguments::SearchText {
                        query: "mentor".to_string(),
                        path_filter: None,
                        limit: 10,
                    },
                },
                CancellationToken::new(),
            )
            .await
            .unwrap();
        assert!(!search.source_refs.is_empty());
        assert!(search.content_bytes <= 64 * 1024);

        let read = gateway
            .execute(
                RepositoryToolCall {
                    call_id: Uuid::new_v4(),
                    snapshot_id: gateway.snapshot().id,
                    name: RepositoryToolName::ReadFileLines,
                    arguments: RepositoryToolArguments::ReadFileLines {
                        relative_path: PathBuf::from("src/lib.rs"),
                        start_line: 1,
                        end_line: 3,
                    },
                },
                CancellationToken::new(),
            )
            .await
            .unwrap();
        assert_eq!(
            read.source_refs[0].relative_path,
            PathBuf::from("src/lib.rs")
        );
        assert_eq!(read.source_refs[0].line_start, 1);
        assert_eq!(read.source_refs[0].line_end, 3);
    }

    #[tokio::test]
    async fn repository_tool_result_redacts_secrets_before_provider_boundary() {
        let dir = tempdir().unwrap();
        std::fs::write(
            dir.path().join("config.txt"),
            "token = ghp_abcdefghijklmnopqrstuvwxyz1234567890\n",
        )
        .unwrap();
        let gateway = fixture_for_path(dir.path()).await;
        let snapshot_id = gateway.snapshot().id;
        let result = gateway
            .execute(
                RepositoryToolCall {
                    call_id: Uuid::new_v4(),
                    snapshot_id,
                    name: RepositoryToolName::ReadFileLines,
                    arguments: RepositoryToolArguments::ReadFileLines {
                        relative_path: PathBuf::from("config.txt"),
                        start_line: 1,
                        end_line: 1,
                    },
                },
                CancellationToken::new(),
            )
            .await
            .unwrap();

        assert!(!result.content.contains("ghp_"));
        assert!(result.content.contains("[REDACTED"));
        assert!(!result.source_refs[0].excerpt.contains("ghp_"));
    }

    #[tokio::test]
    async fn stale_snapshot_allows_only_metadata_status() {
        let mut gateway = fixture().await;
        gateway.set_snapshot_status_for_test(SnapshotStatus::Stale);
        let status = gateway
            .execute(
                RepositoryToolCall {
                    call_id: Uuid::new_v4(),
                    snapshot_id: gateway.snapshot().id,
                    name: RepositoryToolName::RepoStatus,
                    arguments: RepositoryToolArguments::RepoStatus,
                },
                CancellationToken::new(),
            )
            .await;
        assert!(status.is_ok());

        let blocked = gateway
            .execute(
                RepositoryToolCall {
                    call_id: Uuid::new_v4(),
                    snapshot_id: gateway.snapshot().id,
                    name: RepositoryToolName::SearchPaths,
                    arguments: RepositoryToolArguments::SearchPaths {
                        query: "lib".to_string(),
                        limit: 10,
                    },
                },
                CancellationToken::new(),
            )
            .await
            .unwrap_err();
        assert!(blocked.to_string().contains("REPOSITORY_REINDEX_REQUIRED"));
    }

    #[test]
    fn production_catalog_exposes_exactly_six_read_only_tools() {
        let definitions = repository_tool_definitions();
        assert_eq!(definitions.len(), RepositoryToolName::ALL.len());
        assert_eq!(
            definitions
                .iter()
                .map(|definition| definition.name.as_str())
                .collect::<Vec<_>>(),
            RepositoryToolName::ALL
                .iter()
                .map(|name| name.wire_name())
                .collect::<Vec<_>>()
        );
        assert!(definitions
            .iter()
            .all(|definition| definition.input_schema.is_object()));
    }
}
