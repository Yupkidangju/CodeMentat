use keyring::{Entry, Error as KeyringError};
use mentat_core::{MentatError, SecretStore};
use std::path::{Path, PathBuf};

pub struct PlatformManager;

#[derive(Debug, Default, Clone, Copy)]
pub struct NativeSecretStore;

impl NativeSecretStore {
    const SERVICE: &'static str = "CodeMentat";

    pub fn credential_ref(profile_id: uuid::Uuid) -> String {
        format!("provider:{profile_id}")
    }

    fn entry(credential_ref: &str) -> Result<Entry, MentatError> {
        validate_credential_ref(credential_ref)?;
        Entry::new(Self::SERVICE, credential_ref)
            .map_err(|error| map_keyring_error("SECRET_ENTRY_OPEN_FAILED", error))
    }
}

impl SecretStore for NativeSecretStore {
    fn is_available(&self) -> Result<(), MentatError> {
        Entry::store_status()
            .as_ref()
            .map_err(|error| map_keyring_error_ref("SECRET_STORE_UNAVAILABLE", error))
            .copied()
    }

    fn put_secret(&self, credential_ref: &str, secret: &str) -> Result<(), MentatError> {
        if secret.is_empty() || secret.len() > 8 * 1024 {
            return Err(secret_error(
                "SECRET_VALUE_INVALID",
                "API key는 1~8192 UTF-8 bytes여야 합니다.",
            ));
        }
        Self::entry(credential_ref)?
            .set_password(secret)
            .map_err(|error| map_keyring_error("SECRET_STORE_WRITE_FAILED", error))
    }

    fn get_secret(&self, credential_ref: &str) -> Result<Option<String>, MentatError> {
        match Self::entry(credential_ref)?.get_password() {
            Ok(secret) if secret.is_empty() || secret.len() > 8 * 1024 => Err(secret_error(
                "SECRET_VALUE_INVALID",
                "native credential의 API key 길이가 유효하지 않습니다.",
            )),
            Ok(secret) => Ok(Some(secret)),
            Err(KeyringError::NoEntry) => Ok(None),
            Err(error) => Err(map_keyring_error("SECRET_STORE_READ_FAILED", error)),
        }
    }

    fn delete_secret(&self, credential_ref: &str) -> Result<(), MentatError> {
        match Self::entry(credential_ref)?.delete_credential() {
            Ok(()) | Err(KeyringError::NoEntry) => Ok(()),
            Err(error) => Err(map_keyring_error("SECRET_STORE_DELETE_FAILED", error)),
        }
    }
}

fn validate_credential_ref(credential_ref: &str) -> Result<(), MentatError> {
    let Some(id) = credential_ref.strip_prefix("provider:") else {
        return Err(secret_error(
            "SECRET_REFERENCE_INVALID",
            "credential reference prefix가 유효하지 않습니다.",
        ));
    };
    uuid::Uuid::parse_str(id).map_err(|_| {
        secret_error(
            "SECRET_REFERENCE_INVALID",
            "credential reference profile UUID가 유효하지 않습니다.",
        )
    })?;
    Ok(())
}

fn map_keyring_error(code: &str, error: KeyringError) -> MentatError {
    map_keyring_error_ref(code, &error)
}

fn map_keyring_error_ref(code: &str, error: &KeyringError) -> MentatError {
    let message = match error {
        KeyringError::NoStorageAccess(_) => "OS credential store가 잠겼거나 접근이 거부되었습니다.",
        KeyringError::NoEntry => "OS credential item이 없습니다.",
        KeyringError::BadEncoding(_) | KeyringError::BadDataFormat(_, _) => {
            "OS credential item 형식이 손상되었습니다."
        }
        KeyringError::BadStoreFormat(_) => "OS credential store 형식이 손상되었습니다.",
        KeyringError::TooLong(_, _) => "OS credential store 길이 제한을 초과했습니다.",
        KeyringError::Invalid(_, _) => "OS credential store 입력이 유효하지 않습니다.",
        KeyringError::Ambiguous(_) => "같은 reference의 OS credential item이 중복되었습니다.",
        KeyringError::NoDefaultStore => {
            "이 플랫폼에서 native credential store를 초기화하지 못했습니다."
        }
        KeyringError::NotSupportedByStore(_) => {
            "native credential store가 요청 작업을 지원하지 않습니다."
        }
        KeyringError::PlatformFailure(_) => "native credential store 작업이 실패했습니다.",
        _ => "알 수 없는 native credential store 오류입니다.",
    };
    secret_error(code, message)
}

fn secret_error(code: &str, message: &str) -> MentatError {
    MentatError::PlatformError(format!("{code}: {message}"))
}

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

    #[test]
    fn native_secret_reference_is_stable_and_profile_scoped() {
        let first = uuid::Uuid::new_v4();
        let second = uuid::Uuid::new_v4();

        let first_ref = NativeSecretStore::credential_ref(first);
        let second_ref = NativeSecretStore::credential_ref(second);

        assert_eq!(first_ref, format!("provider:{first}"));
        assert_ne!(first_ref, second_ref);
        assert!(!format!("{first_ref:?}").contains("api_key"));
    }

    #[test]
    #[ignore = "운영체제 native credential store에 임시 항목을 만들고 즉시 삭제하는 smoke"]
    fn native_secret_store_round_trip_and_delete() {
        let store = NativeSecretStore;
        let credential_ref = NativeSecretStore::credential_ref(uuid::Uuid::new_v4());
        let secret = "codementat-native-store-roundtrip";

        store.put_secret(&credential_ref, secret).unwrap();
        assert_eq!(
            store.get_secret(&credential_ref).unwrap().as_deref(),
            Some(secret)
        );
        store.delete_secret(&credential_ref).unwrap();
        assert_eq!(store.get_secret(&credential_ref).unwrap(), None);
    }
}
