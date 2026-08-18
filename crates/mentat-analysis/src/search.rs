use mentat_core::error::MentatError;
use mentat_core::models::FileRecord;
use mentat_core::ports::RepositoryReader;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchMatch {
    pub relative_path: PathBuf,
    pub line_number: usize,
    pub line_text: String,
}

pub struct RepositorySearcher;

impl RepositorySearcher {
    pub async fn search_text(
        reader: &(impl RepositoryReader + ?Sized),
        files: &[FileRecord],
        query: &str,
        max_matches: usize,
    ) -> Result<Vec<SearchMatch>, MentatError> {
        let query_lower = query.to_lowercase();
        let mut matches = Vec::new();

        for file in files {
            if !file.is_text {
                continue;
            }

            if let Ok(content) = reader.read_file_content(&file.relative_path).await {
                for (idx, line) in content.lines().enumerate() {
                    if line.to_lowercase().contains(&query_lower) {
                        matches.push(SearchMatch {
                            relative_path: file.relative_path.clone(),
                            line_number: idx + 1,
                            line_text: line.trim().to_string(),
                        });

                        if matches.len() >= max_matches {
                            return Ok(matches);
                        }
                    }
                }
            }
        }

        Ok(matches)
    }

    pub fn search_path(files: &[FileRecord], path_query: &str) -> Vec<PathBuf> {
        let query_lower = path_query.to_lowercase();
        files
            .iter()
            .filter(|f| {
                f.relative_path
                    .to_string_lossy()
                    .to_lowercase()
                    .contains(&query_lower)
            })
            .map(|f| f.relative_path.clone())
            .collect()
    }
}
