use crate::db::{parse_datetime, parse_uuid, storage_error, SqliteStorage};
use mentat_core::{
    ExperiencePreset, MentatError, PromptContentSource, PromptContentVersion, PromptDraft,
    PromptLayer, PromptLayerDraft, PromptProfile, PromptProfileRevision, StoredPromptProfile,
    SystemPreset,
};
use rusqlite::{params, OptionalExtension, Row, TransactionBehavior};
use sha2::{Digest, Sha256};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FactoryPromptSeed {
    pub profile_id: Uuid,
    pub profile_name: String,
    pub experience_preset: ExperiencePreset,
    pub base_system_preset: SystemPreset,
    pub system_resource_key: String,
    pub system_resource_version: String,
    pub system_checksum: String,
    pub persona_resource_key: String,
    pub persona_resource_version: String,
    pub persona_checksum: String,
}

impl SqliteStorage {
    pub fn seed_factory_prompt_profile(
        &self,
        seed: &FactoryPromptSeed,
    ) -> Result<PromptProfile, MentatError> {
        if let Some(existing) = self.load_active_prompt_profile(seed.profile_id)? {
            return Ok(existing.profile);
        }

        let system_version_id = Uuid::new_v4();
        let persona_version_id = Uuid::new_v4();
        let revision_id = Uuid::new_v4();
        let now = chrono::Utc::now();
        let mut conn = self.lock_conn()?;
        let transaction = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| storage_error("PROMPT_SEED_BEGIN_FAILED", &error.to_string()))?;

        transaction
            .execute(
                "INSERT INTO prompt_profiles (
                    id, name, experience_preset, base_system_preset, active_revision_id,
                    factory_system_version, factory_persona_version, created_at, updated_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)",
                params![
                    seed.profile_id.to_string(),
                    seed.profile_name,
                    experience_preset_text(seed.experience_preset),
                    system_preset_text(seed.base_system_preset),
                    revision_id.to_string(),
                    seed.system_resource_version,
                    seed.persona_resource_version,
                    now.to_rfc3339(),
                ],
            )
            .map_err(|error| storage_error("PROMPT_SEED_PROFILE_FAILED", &error.to_string()))?;
        insert_factory_content(
            &transaction,
            system_version_id,
            seed.profile_id,
            PromptLayer::System,
            &PromptContentSource::FactoryRef {
                resource_key: seed.system_resource_key.clone(),
                resource_version: seed.system_resource_version.clone(),
                checksum: seed.system_checksum.clone(),
            },
            &now,
        )?;
        insert_factory_content(
            &transaction,
            persona_version_id,
            seed.profile_id,
            PromptLayer::Persona,
            &PromptContentSource::FactoryRef {
                resource_key: seed.persona_resource_key.clone(),
                resource_version: seed.persona_resource_version.clone(),
                checksum: seed.persona_checksum.clone(),
            },
            &now,
        )?;
        transaction
            .execute(
                "INSERT INTO prompt_profile_revisions (
                    id, profile_id, revision, system_version_id, persona_version_id,
                    system_checksum, persona_checksum, content_deleted,
                    expected_previous_revision_id, created_at
                 ) VALUES (?1, ?2, 1, ?3, ?4, ?5, ?6, 0, NULL, ?7)",
                params![
                    revision_id.to_string(),
                    seed.profile_id.to_string(),
                    system_version_id.to_string(),
                    persona_version_id.to_string(),
                    seed.system_checksum,
                    seed.persona_checksum,
                    now.to_rfc3339(),
                ],
            )
            .map_err(|error| storage_error("PROMPT_SEED_REVISION_FAILED", &error.to_string()))?;
        transaction
            .commit()
            .map_err(|error| storage_error("PROMPT_SEED_COMMIT_FAILED", &error.to_string()))?;
        drop(conn);

        self.load_active_prompt_profile(seed.profile_id)?
            .map(|stored| stored.profile)
            .ok_or_else(|| {
                storage_error(
                    "PROMPT_SEED_VERIFY_FAILED",
                    "seed transaction 뒤 profile을 다시 읽을 수 없습니다.",
                )
            })
    }

    pub fn load_active_prompt_profile(
        &self,
        profile_id: Uuid,
    ) -> Result<Option<StoredPromptProfile>, MentatError> {
        let conn = self.lock_conn()?;
        let profile_row = conn
            .query_row(
                "SELECT id, name, experience_preset, base_system_preset, active_revision_id,
                        factory_system_version, factory_persona_version, created_at, updated_at
                 FROM prompt_profiles WHERE id = ?1",
                [profile_id.to_string()],
                decode_profile_row,
            )
            .optional()
            .map_err(|error| storage_error("PROMPT_PROFILE_READ_FAILED", &error.to_string()))?;
        let Some(profile_values) = profile_row else {
            return Ok(None);
        };
        let profile = profile_values.into_profile()?;
        let revision_values = conn
            .query_row(
                "SELECT id, profile_id, revision, system_version_id, persona_version_id,
                        system_checksum, persona_checksum, content_deleted,
                        expected_previous_revision_id, created_at
                 FROM prompt_profile_revisions WHERE id = ?1 AND profile_id = ?2",
                params![
                    profile.active_revision_id.to_string(),
                    profile.id.to_string()
                ],
                decode_revision_row,
            )
            .map_err(|error| storage_error("PROMPT_REVISION_READ_FAILED", &error.to_string()))?;
        let revision = revision_values.into_revision()?;
        let system_version_id = revision.system_version_id.ok_or_else(|| {
            storage_error(
                "PROMPT_ACTIVE_CONTENT_DELETED",
                "active revision의 System 원문이 삭제되어 실행할 수 없습니다.",
            )
        })?;
        let persona_version_id = revision.persona_version_id.ok_or_else(|| {
            storage_error(
                "PROMPT_ACTIVE_CONTENT_DELETED",
                "active revision의 Persona 원문이 삭제되어 실행할 수 없습니다.",
            )
        })?;
        let system_source = load_content_source(&conn, profile.id, system_version_id)?;
        let persona_source = load_content_source(&conn, profile.id, persona_version_id)?;

        Ok(Some(StoredPromptProfile {
            profile,
            revision,
            system_source,
            persona_source,
        }))
    }

    pub fn apply_prompt_draft(
        &self,
        expected_revision_id: Uuid,
        draft: &PromptDraft,
    ) -> Result<PromptProfileRevision, MentatError> {
        if draft.name.trim().is_empty() || draft.name.len() > 256 {
            return Err(storage_error(
                "PROMPT_PROFILE_NAME_INVALID",
                "prompt profile 이름은 1~256 UTF-8 bytes여야 합니다.",
            ));
        }
        let mut conn = self.lock_conn()?;
        let transaction = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| storage_error("PROMPT_APPLY_BEGIN_FAILED", &error.to_string()))?;
        let active = transaction
            .query_row(
                "SELECT p.active_revision_id, r.revision, r.system_version_id,
                        r.persona_version_id, r.system_checksum, r.persona_checksum
                 FROM prompt_profiles p
                 JOIN prompt_profile_revisions r ON r.id = p.active_revision_id
                 WHERE p.id = ?1",
                [draft.profile_id.to_string()],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, Option<String>>(2)?,
                        row.get::<_, Option<String>>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, String>(5)?,
                    ))
                },
            )
            .optional()
            .map_err(|error| storage_error("PROMPT_APPLY_READ_FAILED", &error.to_string()))?
            .ok_or_else(|| {
                storage_error(
                    "PROMPT_PROFILE_NOT_FOUND",
                    "적용할 prompt profile이 없습니다.",
                )
            })?;
        let active_revision_id = parse_uuid(&active.0, "prompt_profiles.active_revision_id")?;
        if active_revision_id != expected_revision_id {
            return Err(storage_error(
                "PROMPT_REVISION_CONFLICT",
                "active prompt revision이 편집 시작 이후 변경되었습니다.",
            ));
        }
        let current_revision = u64::try_from(active.1).map_err(|_| {
            storage_error(
                "STORAGE_DECODE_INTEGER",
                "active prompt revision 값이 유효하지 않습니다.",
            )
        })?;
        let current_system_id = parse_required_content_id(active.2.as_deref(), "System")?;
        let current_persona_id = parse_required_content_id(active.3.as_deref(), "Persona")?;
        let now = chrono::Utc::now();
        let (system_version_id, system_checksum) = materialize_draft(
            &transaction,
            draft.profile_id,
            PromptLayer::System,
            current_system_id,
            &active.4,
            &draft.system,
            &now,
        )?;
        let (persona_version_id, persona_checksum) = materialize_draft(
            &transaction,
            draft.profile_id,
            PromptLayer::Persona,
            current_persona_id,
            &active.5,
            &draft.persona,
            &now,
        )?;
        let revision = PromptProfileRevision {
            id: Uuid::new_v4(),
            profile_id: draft.profile_id,
            revision: current_revision + 1,
            system_version_id: Some(system_version_id),
            persona_version_id: Some(persona_version_id),
            system_checksum,
            persona_checksum,
            content_deleted: false,
            expected_previous_revision_id: Some(expected_revision_id),
            created_at: now,
        };
        transaction
            .execute(
                "INSERT INTO prompt_profile_revisions (
                    id, profile_id, revision, system_version_id, persona_version_id,
                    system_checksum, persona_checksum, content_deleted,
                    expected_previous_revision_id, created_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0, ?8, ?9)",
                params![
                    revision.id.to_string(),
                    revision.profile_id.to_string(),
                    i64::try_from(revision.revision).map_err(|_| storage_error(
                        "PROMPT_REVISION_OVERFLOW",
                        "prompt revision이 SQLite 범위를 초과했습니다."
                    ))?,
                    system_version_id.to_string(),
                    persona_version_id.to_string(),
                    revision.system_checksum,
                    revision.persona_checksum,
                    expected_revision_id.to_string(),
                    revision.created_at.to_rfc3339(),
                ],
            )
            .map_err(|error| storage_error("PROMPT_APPLY_REVISION_FAILED", &error.to_string()))?;
        let changed = transaction
            .execute(
                "UPDATE prompt_profiles SET
                    name = ?1,
                    experience_preset = ?2,
                    base_system_preset = ?3,
                    active_revision_id = ?4,
                    updated_at = ?5
                 WHERE id = ?6 AND active_revision_id = ?7",
                params![
                    draft.name,
                    experience_preset_text(draft.experience_preset),
                    system_preset_text(draft.base_system_preset),
                    revision.id.to_string(),
                    revision.created_at.to_rfc3339(),
                    draft.profile_id.to_string(),
                    expected_revision_id.to_string(),
                ],
            )
            .map_err(|error| storage_error("PROMPT_APPLY_PROFILE_FAILED", &error.to_string()))?;
        if changed != 1 {
            return Err(storage_error(
                "PROMPT_REVISION_CONFLICT",
                "active prompt revision CAS가 실패했습니다.",
            ));
        }
        transaction
            .commit()
            .map_err(|error| storage_error("PROMPT_APPLY_COMMIT_FAILED", &error.to_string()))?;
        Ok(revision)
    }

    pub fn list_prompt_versions(
        &self,
        profile_id: Uuid,
        layer: PromptLayer,
    ) -> Result<Vec<PromptContentVersion>, MentatError> {
        let conn = self.lock_conn()?;
        let mut statement = conn
            .prepare(
                "SELECT id, profile_id, layer, version, source_kind, resource_key,
                        resource_version, content, checksum, restored_from,
                        parent_version_id, created_at
                 FROM prompt_content_versions
                 WHERE profile_id = ?1 AND layer = ?2
                 ORDER BY version DESC",
            )
            .map_err(|error| storage_error("PROMPT_VERSION_READ_FAILED", &error.to_string()))?;
        let rows = statement
            .query_map(
                params![profile_id.to_string(), prompt_layer_text(layer)],
                |row| {
                    Ok(ContentValues {
                        id: row.get(0)?,
                        profile_id: row.get(1)?,
                        layer: row.get(2)?,
                        version: row.get(3)?,
                        source_kind: row.get(4)?,
                        resource_key: row.get(5)?,
                        resource_version: row.get(6)?,
                        content: row.get(7)?,
                        checksum: row.get(8)?,
                        restored_from: row.get(9)?,
                        parent_version_id: row.get(10)?,
                        created_at: row.get(11)?,
                    })
                },
            )
            .map_err(|error| storage_error("PROMPT_VERSION_READ_FAILED", &error.to_string()))?;
        let mut versions = Vec::new();
        for row in rows {
            let values = row
                .map_err(|error| storage_error("PROMPT_VERSION_READ_FAILED", &error.to_string()))?;
            versions.push(values.into_version()?);
        }
        Ok(versions)
    }
}

fn parse_required_content_id(value: Option<&str>, layer: &str) -> Result<Uuid, MentatError> {
    value
        .ok_or_else(|| {
            storage_error(
                "PROMPT_ACTIVE_CONTENT_DELETED",
                &format!("active {layer} content가 없습니다."),
            )
        })
        .and_then(|value| parse_uuid(value, "prompt active content id"))
}

fn materialize_draft(
    transaction: &rusqlite::Transaction<'_>,
    profile_id: Uuid,
    layer: PromptLayer,
    current_content_id: Uuid,
    current_checksum: &str,
    draft: &PromptLayerDraft,
    created_at: &chrono::DateTime<chrono::Utc>,
) -> Result<(Uuid, String), MentatError> {
    match draft {
        PromptLayerDraft::Preserve => Ok((current_content_id, current_checksum.to_string())),
        PromptLayerDraft::UserText(content) => {
            validate_user_prompt(content)?;
            let source = PromptContentSource::UserText {
                content: content.clone(),
                checksum: sha256_hex(content.as_bytes()),
            };
            insert_content_source(transaction, profile_id, layer, source, None, created_at)
        }
        PromptLayerDraft::ResetToFactory {
            resource_key,
            resource_version,
            expected_checksum,
        } => {
            if resource_key.is_empty()
                || resource_version.is_empty()
                || expected_checksum.is_empty()
            {
                return Err(storage_error(
                    "PROMPT_FACTORY_REF_INVALID",
                    "factory prompt reference가 비어 있습니다.",
                ));
            }
            let source = PromptContentSource::FactoryRef {
                resource_key: resource_key.clone(),
                resource_version: resource_version.clone(),
                checksum: expected_checksum.clone(),
            };
            insert_content_source(
                transaction,
                profile_id,
                layer,
                source,
                Some(current_content_id),
                created_at,
            )
        }
        PromptLayerDraft::RestoreVersion { content_version_id } => {
            let old = load_content_version(transaction, profile_id, *content_version_id)?;
            if old.layer != layer {
                return Err(storage_error(
                    "PROMPT_RESTORE_LAYER_MISMATCH",
                    "다른 prompt layer의 version을 복구할 수 없습니다.",
                ));
            }
            let source = match old.source {
                PromptContentSource::FactoryRef {
                    resource_key,
                    resource_version,
                    checksum,
                } => PromptContentSource::FactoryRef {
                    resource_key,
                    resource_version,
                    checksum,
                },
                PromptContentSource::UserText { content, checksum }
                | PromptContentSource::RestoredText {
                    content, checksum, ..
                } => PromptContentSource::RestoredText {
                    content,
                    checksum,
                    restored_from: *content_version_id,
                },
            };
            insert_content_source(
                transaction,
                profile_id,
                layer,
                source,
                Some(*content_version_id),
                created_at,
            )
        }
    }
}

fn insert_content_source(
    transaction: &rusqlite::Transaction<'_>,
    profile_id: Uuid,
    layer: PromptLayer,
    source: PromptContentSource,
    parent_version_id: Option<Uuid>,
    created_at: &chrono::DateTime<chrono::Utc>,
) -> Result<(Uuid, String), MentatError> {
    let next_version: i64 = transaction
        .query_row(
            "SELECT COALESCE(MAX(version), 0) + 1 FROM prompt_content_versions
             WHERE profile_id = ?1 AND layer = ?2",
            params![profile_id.to_string(), prompt_layer_text(layer)],
            |row| row.get(0),
        )
        .map_err(|error| storage_error("PROMPT_VERSION_READ_FAILED", &error.to_string()))?;
    let id = Uuid::new_v4();
    let (source_kind, resource_key, resource_version, content, checksum, restored_from) =
        match source {
            PromptContentSource::FactoryRef {
                resource_key,
                resource_version,
                checksum,
            } => (
                "FactoryRef",
                Some(resource_key),
                Some(resource_version),
                None,
                checksum,
                None,
            ),
            PromptContentSource::UserText { content, checksum } => {
                ("UserText", None, None, Some(content), checksum, None)
            }
            PromptContentSource::RestoredText {
                content,
                checksum,
                restored_from,
            } => (
                "RestoredText",
                None,
                None,
                Some(content),
                checksum,
                Some(restored_from.to_string()),
            ),
        };
    transaction
        .execute(
            "INSERT INTO prompt_content_versions (
                id, profile_id, layer, version, source_kind, resource_key,
                resource_version, content, checksum, restored_from,
                parent_version_id, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                id.to_string(),
                profile_id.to_string(),
                prompt_layer_text(layer),
                next_version,
                source_kind,
                resource_key,
                resource_version,
                content,
                checksum,
                restored_from,
                parent_version_id.map(|id| id.to_string()),
                created_at.to_rfc3339(),
            ],
        )
        .map_err(|error| storage_error("PROMPT_CONTENT_INSERT_FAILED", &error.to_string()))?;
    Ok((id, checksum))
}

fn load_content_version(
    conn: &rusqlite::Connection,
    profile_id: Uuid,
    content_id: Uuid,
) -> Result<PromptContentVersion, MentatError> {
    let values = conn
        .query_row(
            "SELECT id, profile_id, layer, version, source_kind, resource_key,
                    resource_version, content, checksum, restored_from,
                    parent_version_id, created_at
             FROM prompt_content_versions WHERE id = ?1 AND profile_id = ?2",
            params![content_id.to_string(), profile_id.to_string()],
            |row| {
                Ok(ContentValues {
                    id: row.get(0)?,
                    profile_id: row.get(1)?,
                    layer: row.get(2)?,
                    version: row.get(3)?,
                    source_kind: row.get(4)?,
                    resource_key: row.get(5)?,
                    resource_version: row.get(6)?,
                    content: row.get(7)?,
                    checksum: row.get(8)?,
                    restored_from: row.get(9)?,
                    parent_version_id: row.get(10)?,
                    created_at: row.get(11)?,
                })
            },
        )
        .map_err(|error| storage_error("PROMPT_CONTENT_READ_FAILED", &error.to_string()))?;
    values.into_version()
}

fn validate_user_prompt(content: &str) -> Result<(), MentatError> {
    if content.len() > 32 * 1024 {
        return Err(storage_error(
            "PROMPT_TOO_LARGE",
            "editable prompt가 32KiB 상한을 초과했습니다.",
        ));
    }
    Ok(())
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn insert_factory_content(
    transaction: &rusqlite::Transaction<'_>,
    id: Uuid,
    profile_id: Uuid,
    layer: PromptLayer,
    source: &PromptContentSource,
    created_at: &chrono::DateTime<chrono::Utc>,
) -> Result<(), MentatError> {
    let PromptContentSource::FactoryRef {
        resource_key,
        resource_version,
        checksum,
    } = source
    else {
        return Err(storage_error(
            "PROMPT_SEED_SOURCE_INVALID",
            "factory seed에는 FactoryRef source만 허용됩니다.",
        ));
    };
    transaction
        .execute(
            "INSERT INTO prompt_content_versions (
                id, profile_id, layer, version, source_kind, resource_key,
                resource_version, content, checksum, restored_from,
                parent_version_id, created_at
             ) VALUES (?1, ?2, ?3, 1, 'FactoryRef', ?4, ?5, NULL, ?6, NULL, NULL, ?7)",
            params![
                id.to_string(),
                profile_id.to_string(),
                prompt_layer_text(layer),
                resource_key,
                resource_version,
                checksum,
                created_at.to_rfc3339(),
            ],
        )
        .map_err(|error| storage_error("PROMPT_SEED_CONTENT_FAILED", &error.to_string()))?;
    Ok(())
}

fn load_content_source(
    conn: &rusqlite::Connection,
    profile_id: Uuid,
    content_id: Uuid,
) -> Result<PromptContentSource, MentatError> {
    load_content_version(conn, profile_id, content_id).map(|version| version.source)
}

fn experience_preset_text(value: ExperiencePreset) -> &'static str {
    match value {
        ExperiencePreset::Beginner => "Beginner",
        ExperiencePreset::Intermediate => "Intermediate",
        ExperiencePreset::Professional => "Professional",
        ExperiencePreset::Senior => "Senior",
        ExperiencePreset::Custom => "Custom",
    }
}

fn parse_experience_preset(value: &str) -> Result<ExperiencePreset, MentatError> {
    match value {
        "Beginner" => Ok(ExperiencePreset::Beginner),
        "Intermediate" => Ok(ExperiencePreset::Intermediate),
        "Professional" => Ok(ExperiencePreset::Professional),
        "Senior" => Ok(ExperiencePreset::Senior),
        "Custom" => Ok(ExperiencePreset::Custom),
        _ => Err(storage_error(
            "STORAGE_DECODE_ENUM",
            "experience_preset 값이 유효하지 않습니다.",
        )),
    }
}

fn system_preset_text(value: SystemPreset) -> &'static str {
    match value {
        SystemPreset::Beginner => "Beginner",
        SystemPreset::Intermediate => "Intermediate",
        SystemPreset::Professional => "Professional",
        SystemPreset::Senior => "Senior",
    }
}

fn parse_system_preset(value: &str) -> Result<SystemPreset, MentatError> {
    match value {
        "Beginner" => Ok(SystemPreset::Beginner),
        "Intermediate" => Ok(SystemPreset::Intermediate),
        "Professional" => Ok(SystemPreset::Professional),
        "Senior" => Ok(SystemPreset::Senior),
        _ => Err(storage_error(
            "STORAGE_DECODE_ENUM",
            "base_system_preset 값이 유효하지 않습니다.",
        )),
    }
}

fn prompt_layer_text(value: PromptLayer) -> &'static str {
    match value {
        PromptLayer::System => "System",
        PromptLayer::Persona => "Persona",
    }
}

fn parse_prompt_layer(value: &str) -> Result<PromptLayer, MentatError> {
    match value {
        "System" => Ok(PromptLayer::System),
        "Persona" => Ok(PromptLayer::Persona),
        _ => Err(storage_error(
            "STORAGE_DECODE_ENUM",
            "prompt layer 값이 유효하지 않습니다.",
        )),
    }
}

struct ProfileValues {
    id: String,
    name: String,
    experience_preset: String,
    base_system_preset: String,
    active_revision_id: String,
    factory_system_version: String,
    factory_persona_version: String,
    created_at: String,
    updated_at: String,
}

impl ProfileValues {
    fn into_profile(self) -> Result<PromptProfile, MentatError> {
        Ok(PromptProfile {
            id: parse_uuid(&self.id, "prompt_profiles.id")?,
            name: self.name,
            experience_preset: parse_experience_preset(&self.experience_preset)?,
            base_system_preset: parse_system_preset(&self.base_system_preset)?,
            active_revision_id: parse_uuid(
                &self.active_revision_id,
                "prompt_profiles.active_revision_id",
            )?,
            factory_system_version: self.factory_system_version,
            factory_persona_version: self.factory_persona_version,
            created_at: parse_datetime(&self.created_at, "prompt_profiles.created_at")?,
            updated_at: parse_datetime(&self.updated_at, "prompt_profiles.updated_at")?,
        })
    }
}

fn decode_profile_row(row: &Row<'_>) -> rusqlite::Result<ProfileValues> {
    Ok(ProfileValues {
        id: row.get(0)?,
        name: row.get(1)?,
        experience_preset: row.get(2)?,
        base_system_preset: row.get(3)?,
        active_revision_id: row.get(4)?,
        factory_system_version: row.get(5)?,
        factory_persona_version: row.get(6)?,
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
    })
}

struct RevisionValues {
    id: String,
    profile_id: String,
    revision: i64,
    system_version_id: Option<String>,
    persona_version_id: Option<String>,
    system_checksum: String,
    persona_checksum: String,
    content_deleted: i64,
    expected_previous_revision_id: Option<String>,
    created_at: String,
}

impl RevisionValues {
    fn into_revision(self) -> Result<PromptProfileRevision, MentatError> {
        let revision = u64::try_from(self.revision).map_err(|_| {
            storage_error(
                "STORAGE_DECODE_INTEGER",
                "prompt revision이 음수이거나 범위를 벗어났습니다.",
            )
        })?;
        let system_version_id = self
            .system_version_id
            .as_deref()
            .map(|value| parse_uuid(value, "prompt revision system_version_id"))
            .transpose()?;
        let persona_version_id = self
            .persona_version_id
            .as_deref()
            .map(|value| parse_uuid(value, "prompt revision persona_version_id"))
            .transpose()?;
        let expected_previous_revision_id = self
            .expected_previous_revision_id
            .as_deref()
            .map(|value| parse_uuid(value, "prompt revision expected_previous_revision_id"))
            .transpose()?;
        let content_deleted = match self.content_deleted {
            0 => false,
            1 => true,
            _ => {
                return Err(storage_error(
                    "STORAGE_DECODE_BOOL",
                    "prompt revision content_deleted 값이 0/1이 아닙니다.",
                ))
            }
        };
        Ok(PromptProfileRevision {
            id: parse_uuid(&self.id, "prompt_profile_revisions.id")?,
            profile_id: parse_uuid(&self.profile_id, "prompt_profile_revisions.profile_id")?,
            revision,
            system_version_id,
            persona_version_id,
            system_checksum: self.system_checksum,
            persona_checksum: self.persona_checksum,
            content_deleted,
            expected_previous_revision_id,
            created_at: parse_datetime(&self.created_at, "prompt_profile_revisions.created_at")?,
        })
    }
}

fn decode_revision_row(row: &Row<'_>) -> rusqlite::Result<RevisionValues> {
    Ok(RevisionValues {
        id: row.get(0)?,
        profile_id: row.get(1)?,
        revision: row.get(2)?,
        system_version_id: row.get(3)?,
        persona_version_id: row.get(4)?,
        system_checksum: row.get(5)?,
        persona_checksum: row.get(6)?,
        content_deleted: row.get(7)?,
        expected_previous_revision_id: row.get(8)?,
        created_at: row.get(9)?,
    })
}

struct ContentValues {
    id: String,
    profile_id: String,
    layer: String,
    version: i64,
    source_kind: String,
    resource_key: Option<String>,
    resource_version: Option<String>,
    content: Option<String>,
    checksum: String,
    restored_from: Option<String>,
    parent_version_id: Option<String>,
    created_at: String,
}

impl ContentValues {
    fn into_version(self) -> Result<PromptContentVersion, MentatError> {
        let version = u64::try_from(self.version).map_err(|_| {
            storage_error(
                "STORAGE_DECODE_INTEGER",
                "prompt content version이 음수이거나 범위를 벗어났습니다.",
            )
        })?;
        let source = match self.source_kind.as_str() {
            "FactoryRef" => PromptContentSource::FactoryRef {
                resource_key: self.resource_key.ok_or_else(|| {
                    storage_error("STORAGE_DECODE_SOURCE", "factory resource_key가 없습니다.")
                })?,
                resource_version: self.resource_version.ok_or_else(|| {
                    storage_error(
                        "STORAGE_DECODE_SOURCE",
                        "factory resource_version이 없습니다.",
                    )
                })?,
                checksum: self.checksum,
            },
            "UserText" => PromptContentSource::UserText {
                content: self.content.ok_or_else(|| {
                    storage_error("STORAGE_DECODE_SOURCE", "user prompt content가 없습니다.")
                })?,
                checksum: self.checksum,
            },
            "RestoredText" => PromptContentSource::RestoredText {
                content: self.content.ok_or_else(|| {
                    storage_error("STORAGE_DECODE_SOURCE", "restored content가 없습니다.")
                })?,
                checksum: self.checksum,
                restored_from: parse_uuid(
                    &self.restored_from.ok_or_else(|| {
                        storage_error("STORAGE_DECODE_SOURCE", "restored_from이 없습니다.")
                    })?,
                    "prompt_content_versions.restored_from",
                )?,
            },
            _ => {
                return Err(storage_error(
                    "STORAGE_DECODE_ENUM",
                    "prompt source_kind 값이 유효하지 않습니다.",
                ))
            }
        };
        Ok(PromptContentVersion {
            id: parse_uuid(&self.id, "prompt_content_versions.id")?,
            profile_id: parse_uuid(&self.profile_id, "prompt_content_versions.profile_id")?,
            layer: parse_prompt_layer(&self.layer)?,
            version,
            source,
            parent_version_id: self
                .parent_version_id
                .as_deref()
                .map(|value| parse_uuid(value, "prompt_content_versions.parent_version_id"))
                .transpose()?,
            created_at: parse_datetime(&self.created_at, "prompt_content_versions.created_at")?,
        })
    }
}
