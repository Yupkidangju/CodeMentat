use crate::db::{parse_uuid, storage_error, SqliteStorage};
use mentat_core::MentatError;
use rusqlite::{params, OptionalExtension};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderSecretPreference {
    pub profile_id: Uuid,
    pub credential_ref: String,
    pub remember_api_key: bool,
}

impl SqliteStorage {
    pub fn save_provider_secret_preference(
        &self,
        preference: &ProviderSecretPreference,
    ) -> Result<(), MentatError> {
        validate_preference(preference)?;
        let conn = self.lock_conn()?;
        conn.execute(
            "INSERT INTO provider_secret_preferences (
                profile_id, credential_ref, remember_api_key, updated_at
             ) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(profile_id) DO UPDATE SET
                credential_ref = excluded.credential_ref,
                remember_api_key = excluded.remember_api_key,
                updated_at = excluded.updated_at",
            params![
                preference.profile_id.to_string(),
                preference.credential_ref,
                preference.remember_api_key,
                chrono::Utc::now().to_rfc3339(),
            ],
        )
        .map_err(|error| storage_error("SECRET_PREFERENCE_SAVE_FAILED", &error.to_string()))?;
        Ok(())
    }

    pub fn load_provider_secret_preference(
        &self,
        profile_id: Uuid,
    ) -> Result<Option<ProviderSecretPreference>, MentatError> {
        let conn = self.lock_conn()?;
        let values = conn
            .query_row(
                "SELECT profile_id, credential_ref, remember_api_key
                 FROM provider_secret_preferences WHERE profile_id = ?1",
                [profile_id.to_string()],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, i64>(2)?,
                    ))
                },
            )
            .optional()
            .map_err(|error| storage_error("SECRET_PREFERENCE_READ_FAILED", &error.to_string()))?;
        let Some((stored_profile_id, credential_ref, remember_api_key)) = values else {
            return Ok(None);
        };
        let preference = ProviderSecretPreference {
            profile_id: parse_uuid(&stored_profile_id, "provider_secret_preferences.profile_id")?,
            credential_ref,
            remember_api_key: match remember_api_key {
                0 => false,
                1 => true,
                _ => {
                    return Err(storage_error(
                        "STORAGE_DECODE_BOOL",
                        "remember_api_key 값이 0/1이 아닙니다.",
                    ))
                }
            },
        };
        validate_preference(&preference)?;
        Ok(Some(preference))
    }

    pub fn delete_provider_secret_preference(&self, profile_id: Uuid) -> Result<(), MentatError> {
        let conn = self.lock_conn()?;
        conn.execute(
            "DELETE FROM provider_secret_preferences WHERE profile_id = ?1",
            [profile_id.to_string()],
        )
        .map_err(|error| storage_error("SECRET_PREFERENCE_DELETE_FAILED", &error.to_string()))?;
        Ok(())
    }
}

fn validate_preference(preference: &ProviderSecretPreference) -> Result<(), MentatError> {
    let expected = format!("provider:{}", preference.profile_id);
    if preference.credential_ref != expected {
        return Err(storage_error(
            "SECRET_REFERENCE_PROFILE_MISMATCH",
            "credential reference가 provider profile ID와 일치하지 않습니다.",
        ));
    }
    Ok(())
}
