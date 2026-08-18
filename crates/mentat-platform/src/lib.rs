use mentat_core::error::MentatError;
use std::path::{Path, PathBuf};

pub struct PlatformManager;

impl PlatformManager {
    /// Returns the OS-standard AppData directory for Code Mentat.
    /// Strictly fails if OS directory cannot be resolved (no dangerous fallback to `.`).
    pub fn get_app_data_dir() -> Result<PathBuf, MentatError> {
        let base = dirs::data_dir().or_else(dirs::config_dir).ok_or_else(|| {
            MentatError::PlatformError("OS 표준 AppData 경로를 확인할 수 없습니다.".to_string())
        })?;

        let app_dir = base.join("CodeMentat");
        std::fs::create_dir_all(&app_dir).map_err(|e| {
            MentatError::PlatformError(format!("AppData 디렉터리 생성 실패: {}", e))
        })?;

        Ok(app_dir)
    }

    /// Verifies that the storage AppData directory is strictly outside the repository root.
    pub fn is_storage_isolated(app_data_dir: &Path, repo_root: &Path) -> bool {
        Self::validate_storage_isolation(app_data_dir, repo_root).is_ok()
    }

    /// Strict validation helper that returns Err if isolation is violated.
    pub fn validate_storage_isolation(
        app_data_dir: &Path,
        repo_root: &Path,
    ) -> Result<(), MentatError> {
        let app_canon = app_data_dir.canonicalize().map_err(|_| {
            MentatError::PlatformError(format!(
                "AppData 경로의 절대경로 확인 불가: {}",
                app_data_dir.display()
            ))
        })?;

        let repo_canon = repo_root.canonicalize().map_err(|_| {
            MentatError::InvalidRepositoryPath(format!(
                "저장소 경로의 절대경로 확인 불가: {}",
                repo_root.display()
            ))
        })?;

        // Isolation Check 1: app_data_dir must not start with repo_root (i.e. AppData is inside repo)
        if app_canon.starts_with(&repo_canon) {
            return Err(MentatError::StorageIsolationViolation(format!(
                "AppData 경로({})가 선택된 저장소({}) 내부에 위치합니다.",
                app_canon.display(),
                repo_canon.display()
            )));
        }

        // Isolation Check 2: repo_root must not start with app_data_dir (i.e. repo is inside AppData)
        if repo_canon.starts_with(&app_canon) {
            return Err(MentatError::StorageIsolationViolation(format!(
                "선택된 저장소({})가 AppData 경로({}) 내부에 위치합니다.",
                repo_canon.display(),
                app_canon.display()
            )));
        }

        Ok(())
    }

    pub fn pick_folder() -> Option<PathBuf> {
        rfd::FileDialog::new().pick_folder()
    }

    pub fn copy_to_clipboard(text: &str) -> Result<(), MentatError> {
        match arboard::Clipboard::new() {
            Ok(mut clipboard) => clipboard
                .set_text(text)
                .map_err(|e| MentatError::PlatformError(e.to_string())),
            Err(e) => Err(MentatError::PlatformError(e.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_storage_isolation_detection() {
        let dir = tempdir().unwrap();
        let repo_path = dir.path().join("my_repo");
        let app_data_path = dir.path().join("app_data");
        std::fs::create_dir_all(&repo_path).unwrap();
        std::fs::create_dir_all(&app_data_path).unwrap();

        // Valid isolated paths
        assert!(PlatformManager::is_storage_isolated(
            &app_data_path,
            &repo_path
        ));
        assert!(PlatformManager::validate_storage_isolation(&app_data_path, &repo_path).is_ok());

        // Violation: AppData is inside repository
        let nested_app_data = repo_path.join(".mentat_appdata");
        std::fs::create_dir_all(&nested_app_data).unwrap();
        assert!(!PlatformManager::is_storage_isolated(
            &nested_app_data,
            &repo_path
        ));
        assert!(PlatformManager::validate_storage_isolation(&nested_app_data, &repo_path).is_err());

        // Violation: Repository is parent of AppData
        assert!(!PlatformManager::is_storage_isolated(
            &app_data_path,
            dir.path()
        ));
        assert!(PlatformManager::validate_storage_isolation(&app_data_path, dir.path()).is_err());
    }
}
