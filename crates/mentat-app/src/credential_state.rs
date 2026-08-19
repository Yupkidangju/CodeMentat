use mentat_core::{MentatError, SecretStore};
use mentat_inference::BackendProfile;
use mentat_storage::{ProviderSecretPreference, SqliteStorage};
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CredentialRestoreState {
    pub remember_api_key: bool,
    pub restored_from_native_store: bool,
    pub credential_missing: bool,
}

pub struct CredentialController {
    secret_store: Arc<dyn SecretStore>,
}

impl CredentialController {
    pub fn new(secret_store: Arc<dyn SecretStore>) -> Self {
        Self { secret_store }
    }

    pub fn native() -> Self {
        Self::new(Arc::new(mentat_platform::NativeSecretStore))
    }

    pub fn persist(
        &self,
        storage: &SqliteStorage,
        profile: &BackendProfile,
        remember_api_key: bool,
    ) -> Result<(), MentatError> {
        let credential_ref = credential_ref(profile.id);
        if !profile.provider.requires_api_key() || !remember_api_key {
            self.secret_store.delete_secret(&credential_ref)?;
            storage.delete_provider_secret_preference(profile.id)?;
            return Ok(());
        }
        let secret = profile
            .api_key
            .as_deref()
            .filter(|secret| !secret.trim().is_empty())
            .ok_or_else(|| {
                MentatError::PlatformError(
                    "SECRET_VALUE_REQUIRED: 저장할 API key가 없습니다.".to_string(),
                )
            })?;
        self.secret_store.is_available()?;
        self.secret_store.put_secret(&credential_ref, secret)?;
        let preference = ProviderSecretPreference {
            profile_id: profile.id,
            credential_ref: credential_ref.clone(),
            remember_api_key: true,
        };
        if let Err(error) = storage.save_provider_secret_preference(&preference) {
            let _ = self.secret_store.delete_secret(&credential_ref);
            return Err(error);
        }
        Ok(())
    }

    pub fn restore(
        &self,
        storage: &SqliteStorage,
        profile: &mut BackendProfile,
    ) -> Result<CredentialRestoreState, MentatError> {
        profile.api_key = None;
        let Some(preference) = storage.load_provider_secret_preference(profile.id)? else {
            return Ok(CredentialRestoreState {
                remember_api_key: false,
                restored_from_native_store: false,
                credential_missing: false,
            });
        };
        if !preference.remember_api_key {
            return Ok(CredentialRestoreState {
                remember_api_key: false,
                restored_from_native_store: false,
                credential_missing: false,
            });
        }
        self.secret_store.is_available()?;
        let secret = self.secret_store.get_secret(&preference.credential_ref)?;
        let restored = secret.is_some();
        profile.api_key = secret;
        Ok(CredentialRestoreState {
            remember_api_key: true,
            restored_from_native_store: restored,
            credential_missing: !restored,
        })
    }

    pub fn delete_profile(
        &self,
        storage: &SqliteStorage,
        profile_id: uuid::Uuid,
    ) -> Result<(), MentatError> {
        let credential_ref = credential_ref(profile_id);
        self.secret_store.delete_secret(&credential_ref)?;
        storage.delete_provider_secret_preference(profile_id)
    }
}

fn credential_ref(profile_id: uuid::Uuid) -> String {
    format!("provider:{profile_id}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use mentat_core::{MentatError, SecretStore};
    use mentat_inference::{BackendProfile, ProviderKind};
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};
    use tempfile::tempdir;
    use uuid::Uuid;

    #[derive(Default)]
    struct MemorySecretStore {
        values: Mutex<HashMap<String, String>>,
    }

    impl SecretStore for MemorySecretStore {
        fn is_available(&self) -> Result<(), MentatError> {
            Ok(())
        }

        fn put_secret(&self, credential_ref: &str, secret: &str) -> Result<(), MentatError> {
            self.values
                .lock()
                .unwrap()
                .insert(credential_ref.to_string(), secret.to_string());
            Ok(())
        }

        fn get_secret(&self, credential_ref: &str) -> Result<Option<String>, MentatError> {
            Ok(self.values.lock().unwrap().get(credential_ref).cloned())
        }

        fn delete_secret(&self, credential_ref: &str) -> Result<(), MentatError> {
            self.values.lock().unwrap().remove(credential_ref);
            Ok(())
        }
    }

    fn profile(secret: Option<&str>) -> BackendProfile {
        BackendProfile {
            id: Uuid::new_v4(),
            provider: ProviderKind::GoogleGemini,
            api_key: secret.map(str::to_string),
            model: "dynamic-model".to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn remembered_key_round_trips_via_secret_store_not_sqlite() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("mentat.db");
        let storage = mentat_storage::SqliteStorage::open(&db_path).unwrap();
        let secret_store = Arc::new(MemorySecretStore::default());
        let controller = CredentialController::new(secret_store);
        let original = profile(Some("fixture-sensitive-key"));

        controller.persist(&storage, &original, true).unwrap();
        let mut restored = BackendProfile {
            api_key: None,
            ..original.clone()
        };
        let state = controller.restore(&storage, &mut restored).unwrap();

        assert!(state.remember_api_key);
        assert!(state.restored_from_native_store);
        assert_eq!(restored.api_key.as_deref(), Some("fixture-sensitive-key"));
        let bytes = std::fs::read(db_path).unwrap();
        assert!(!String::from_utf8_lossy(&bytes).contains("fixture-sensitive-key"));
    }

    #[test]
    fn disabling_remember_deletes_native_item_and_reference() {
        let dir = tempdir().unwrap();
        let storage = mentat_storage::SqliteStorage::open(dir.path().join("mentat.db")).unwrap();
        let secret_store = Arc::new(MemorySecretStore::default());
        let controller = CredentialController::new(secret_store);
        let original = profile(Some("fixture-sensitive-key"));
        controller.persist(&storage, &original, true).unwrap();

        controller.persist(&storage, &original, false).unwrap();
        let mut restored = BackendProfile {
            api_key: None,
            ..original.clone()
        };
        let state = controller.restore(&storage, &mut restored).unwrap();

        assert!(!state.remember_api_key);
        assert!(!state.restored_from_native_store);
        assert!(restored.api_key.is_none());
        assert!(storage
            .load_provider_secret_preference(original.id)
            .unwrap()
            .is_none());
    }
}
