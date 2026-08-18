use mentat_core::models::{FileKind, FileRecord};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectStructureSummary {
    pub primary_language: Option<String>,
    pub languages: HashMap<String, usize>, // Language name -> file count
    pub manifests: Vec<PathBuf>,
    pub documents: Vec<PathBuf>,
    pub test_files: Vec<PathBuf>,
    pub entry_points: Vec<PathBuf>,
    pub total_source_files: usize,
    pub total_lines_of_code: usize,
}

pub struct ProjectDetector;

impl ProjectDetector {
    pub fn detect_language(path: &std::path::Path) -> Option<&'static str> {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        match ext.as_str() {
            "rs" => Some("Rust"),
            "ts" | "tsx" => Some("TypeScript"),
            "js" | "jsx" | "mjs" | "cjs" => Some("JavaScript"),
            "py" => Some("Python"),
            "go" => Some("Go"),
            "java" => Some("Java"),
            "kt" | "kts" => Some("Kotlin"),
            "cpp" | "cc" | "cxx" | "c" | "h" | "hpp" => Some("C/C++"),
            "cs" => Some("C#"),
            "swift" => Some("Swift"),
            "rb" => Some("Ruby"),
            "php" => Some("PHP"),
            "html" | "css" | "scss" => Some("Web/UI"),
            _ => None,
        }
    }

    pub fn is_entry_point(rel_path: &std::path::Path) -> bool {
        let file_name = rel_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_lowercase();

        file_name == "main.rs"
            || file_name == "lib.rs"
            || file_name == "index.ts"
            || file_name == "index.js"
            || file_name == "main.py"
            || file_name == "app.py"
            || file_name == "main.go"
            || file_name == "app.ts"
            || file_name == "app.js"
    }

    pub fn is_test_file(rel_path: &std::path::Path) -> bool {
        let path_str = rel_path.to_string_lossy().to_lowercase();
        path_str.contains("test")
            || path_str.contains("spec")
            || path_str.starts_with("tests")
            || path_str.starts_with("test")
    }

    pub fn summarize(files: &[FileRecord]) -> ProjectStructureSummary {
        let mut languages: HashMap<String, usize> = HashMap::new();
        let mut manifests = Vec::new();
        let mut documents = Vec::new();
        let mut test_files = Vec::new();
        let mut entry_points = Vec::new();
        let mut total_source_files = 0;
        let mut total_lines_of_code = 0;

        for file in files {
            if let Some(lines) = file.line_count {
                if file.kind == FileKind::SourceCode {
                    total_lines_of_code += lines;
                }
            }

            match file.kind {
                FileKind::SourceCode => {
                    total_source_files += 1;
                    if let Some(lang) = Self::detect_language(&file.relative_path) {
                        *languages.entry(lang.to_string()).or_insert(0) += 1;
                    }

                    if Self::is_entry_point(&file.relative_path) {
                        entry_points.push(file.relative_path.clone());
                    }

                    if Self::is_test_file(&file.relative_path) {
                        test_files.push(file.relative_path.clone());
                    }
                }
                FileKind::Manifest => {
                    manifests.push(file.relative_path.clone());
                }
                FileKind::Documentation => {
                    documents.push(file.relative_path.clone());
                }
                _ => {
                    if Self::is_test_file(&file.relative_path) {
                        test_files.push(file.relative_path.clone());
                    }
                }
            }
        }

        let primary_language = languages
            .iter()
            .max_by_key(|entry| entry.1)
            .map(|entry| entry.0.clone());

        ProjectStructureSummary {
            primary_language,
            languages,
            manifests,
            documents,
            test_files,
            entry_points,
            total_source_files,
            total_lines_of_code,
        }
    }
}
