use crate::PersonaKind;
use mentat_core::{MentatError, PromptContentSource, SnapshotStatus, SystemPreset};
use sha2::{Digest, Sha256};
use uuid::Uuid;

pub const FACTORY_BUNDLE_VERSION: &str = "cr-ux-001.1";
pub const KERNEL_VERSION: &str = "kernel.v1";
const MAX_EDITABLE_PROMPT_BYTES: usize = 32 * 1024;

const KERNEL: &str = include_str!("../assets/prompts/kernel.v1.txt");
const SYSTEM_BEGINNER: &str = include_str!("../assets/prompts/system.beginner.v1.txt");
const SYSTEM_INTERMEDIATE: &str = include_str!("../assets/prompts/system.intermediate.v1.txt");
const SYSTEM_PROFESSIONAL: &str = include_str!("../assets/prompts/system.professional.v1.txt");
const SYSTEM_SENIOR: &str = include_str!("../assets/prompts/system.senior.v1.txt");
const PERSONA_DEFAULT: &str = include_str!("../assets/prompts/persona.default_analyst.v1.txt");
const PERSONA_MESUGAKI: &str = include_str!("../assets/prompts/persona.mesugaki.v1.txt");
const PERSONA_AUDITOR: &str = include_str!("../assets/prompts/persona.concise_auditor.v1.txt");

fn canonical_asset(text: &'static str) -> &'static str {
    text.strip_suffix("\r\n")
        .or_else(|| text.strip_suffix('\n'))
        .unwrap_or(text)
}

#[derive(Debug, Clone, Copy, Default)]
pub struct FactoryPromptCatalog;

impl FactoryPromptCatalog {
    pub fn load() -> Result<Self, MentatError> {
        let catalog = Self;
        let valid = !catalog.kernel().is_empty()
            && SystemPreset::ALL
                .into_iter()
                .all(|preset| !catalog.system(preset).is_empty())
            && PersonaKind::ALL
                .into_iter()
                .all(|persona| !catalog.persona(persona).is_empty());
        if !valid {
            return Err(prompt_error(
                "FACTORY_PROMPT_EMPTY",
                "factory prompt asset이 비어 있습니다.",
            ));
        }
        Ok(catalog)
    }

    pub fn kernel(&self) -> &'static str {
        canonical_asset(KERNEL)
    }

    pub fn system(&self, preset: SystemPreset) -> &'static str {
        canonical_asset(match preset {
            SystemPreset::Beginner => SYSTEM_BEGINNER,
            SystemPreset::Intermediate => SYSTEM_INTERMEDIATE,
            SystemPreset::Professional => SYSTEM_PROFESSIONAL,
            SystemPreset::Senior => SYSTEM_SENIOR,
        })
    }

    pub fn persona(&self, persona: PersonaKind) -> &'static str {
        canonical_asset(match persona {
            PersonaKind::DefaultAnalyst => PERSONA_DEFAULT,
            PersonaKind::MesugakiAnnouncer => PERSONA_MESUGAKI,
            PersonaKind::ConciseAuditor => PERSONA_AUDITOR,
        })
    }

    pub fn factory_text_count(&self) -> usize {
        1 + SystemPreset::ALL.len() + PersonaKind::ALL.len()
    }

    pub fn checksum(&self, text: &str) -> String {
        sha256_hex(text.as_bytes())
    }

    pub fn resolve_source(&self, source: &PromptContentSource) -> Result<String, MentatError> {
        let (text, expected_checksum) = match source {
            PromptContentSource::FactoryRef {
                resource_key,
                resource_version,
                checksum,
            } => {
                if resource_version != FACTORY_BUNDLE_VERSION {
                    return Err(prompt_error(
                        "FACTORY_PROMPT_VERSION_UNKNOWN",
                        "지원하지 않는 factory prompt version입니다.",
                    ));
                }
                let text = self.factory_by_key(resource_key).ok_or_else(|| {
                    prompt_error(
                        "FACTORY_PROMPT_KEY_UNKNOWN",
                        "지원하지 않는 factory prompt resource key입니다.",
                    )
                })?;
                (text.to_string(), checksum.as_str())
            }
            PromptContentSource::UserText { content, checksum }
            | PromptContentSource::RestoredText {
                content, checksum, ..
            } => {
                validate_editable("Editable", content)?;
                (content.clone(), checksum.as_str())
            }
        };
        if sha256_hex(text.as_bytes()) != expected_checksum {
            return Err(prompt_error(
                "PROMPT_CHECKSUM_MISMATCH",
                "prompt content checksum이 일치하지 않습니다.",
            ));
        }
        Ok(text)
    }

    fn factory_by_key(&self, resource_key: &str) -> Option<&'static str> {
        SystemPreset::ALL
            .into_iter()
            .find(|preset| preset.resource_key() == resource_key)
            .map(|preset| self.system(preset))
            .or_else(|| {
                PersonaKind::ALL
                    .into_iter()
                    .find(|persona| persona.resource_key() == resource_key)
                    .map(|persona| self.persona(persona))
            })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepositoryPromptState {
    pub repository_id: Option<Uuid>,
    pub snapshot_id: Option<Uuid>,
    pub status: Option<SnapshotStatus>,
    pub tools_available: bool,
}

impl RepositoryPromptState {
    pub fn none() -> Self {
        Self {
            repository_id: None,
            snapshot_id: None,
            status: None,
            tools_available: false,
        }
    }

    fn canonical_text(&self) -> String {
        let repository = self
            .repository_id
            .map(|id| id.to_string())
            .unwrap_or_else(|| "none".to_string());
        let snapshot = self
            .snapshot_id
            .map(|id| id.to_string())
            .unwrap_or_else(|| "none".to_string());
        let status = match self.status {
            None => "none",
            Some(SnapshotStatus::Ready) => "ready",
            Some(SnapshotStatus::Stale) => "stale",
            Some(SnapshotStatus::Indexing) => "indexing",
            Some(SnapshotStatus::Incomplete) => "incomplete",
        };
        let tools = if self.tools_available {
            "available"
        } else {
            "unavailable"
        };
        format!("repository={repository};snapshot={snapshot};status={status};tools={tools}")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptCompositionInput {
    pub profile_revision_id: Uuid,
    pub system_prompt: String,
    pub persona_prompt: String,
    pub repository: RepositoryPromptState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptComposition {
    pub effective_system_prompt: String,
    pub digest: String,
    pub kernel_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptSections {
    pub kernel: String,
    pub system: String,
    pub persona: String,
    pub repository: String,
}

pub struct PromptComposer;

impl PromptComposer {
    pub fn compose(input: &PromptCompositionInput) -> Result<PromptComposition, MentatError> {
        validate_editable("System", &input.system_prompt)?;
        validate_editable("Persona", &input.persona_prompt)?;

        let catalog = FactoryPromptCatalog::load()?;
        let kernel = catalog.kernel();
        let repository = input.repository.canonical_text();
        let mut effective = String::new();
        effective.push_str("CM_PROMPT_V1\n");
        push_section(&mut effective, &format!("KERNEL {KERNEL_VERSION}"), kernel);
        push_section(
            &mut effective,
            &format!("SYSTEM {}", input.profile_revision_id),
            &input.system_prompt,
        );
        push_section(
            &mut effective,
            &format!("PERSONA {}", input.profile_revision_id),
            &input.persona_prompt,
        );
        push_section(&mut effective, "REPOSITORY", &repository);

        Ok(PromptComposition {
            digest: sha256_hex(effective.as_bytes()),
            kernel_digest: sha256_hex(kernel.as_bytes()),
            effective_system_prompt: effective,
        })
    }
}

impl PromptComposition {
    pub fn parse_sections(&self) -> Result<PromptSections, MentatError> {
        parse_composition(&self.effective_system_prompt)
    }
}

fn validate_editable(layer: &str, value: &str) -> Result<(), MentatError> {
    if value.len() > MAX_EDITABLE_PROMPT_BYTES {
        return Err(prompt_error(
            "PROMPT_TOO_LARGE",
            &format!("{layer} prompt가 32KiB 상한을 초과했습니다."),
        ));
    }
    Ok(())
}

fn push_section(target: &mut String, header: &str, body: &str) {
    target.push_str(header);
    target.push(' ');
    target.push_str(&body.len().to_string());
    target.push('\n');
    target.push_str(body);
    target.push('\n');
}

fn parse_composition(value: &str) -> Result<PromptSections, MentatError> {
    let bytes = value.as_bytes();
    let mut cursor = 0usize;
    expect_line(bytes, &mut cursor, "CM_PROMPT_V1")?;
    let kernel = read_framed_section(bytes, &mut cursor, "KERNEL", 3)?;
    let system = read_framed_section(bytes, &mut cursor, "SYSTEM", 3)?;
    let persona = read_framed_section(bytes, &mut cursor, "PERSONA", 3)?;
    let repository = read_framed_section(bytes, &mut cursor, "REPOSITORY", 2)?;
    if cursor != bytes.len() {
        return Err(prompt_error(
            "PROMPT_FRAME_INVALID",
            "마지막 section 뒤에 해석되지 않은 바이트가 있습니다.",
        ));
    }
    Ok(PromptSections {
        kernel,
        system,
        persona,
        repository,
    })
}

fn expect_line(bytes: &[u8], cursor: &mut usize, expected: &str) -> Result<(), MentatError> {
    let line = read_line(bytes, cursor)?;
    if line != expected {
        return Err(prompt_error(
            "PROMPT_FRAME_INVALID",
            "prompt magic/version이 올바르지 않습니다.",
        ));
    }
    Ok(())
}

fn read_framed_section(
    bytes: &[u8],
    cursor: &mut usize,
    expected_kind: &str,
    token_count: usize,
) -> Result<String, MentatError> {
    let header = read_line(bytes, cursor)?;
    let tokens: Vec<&str> = header.split(' ').collect();
    if tokens.len() != token_count || tokens.first().copied() != Some(expected_kind) {
        return Err(prompt_error(
            "PROMPT_FRAME_INVALID",
            &format!("{expected_kind} section header가 올바르지 않습니다."),
        ));
    }
    let length = tokens
        .last()
        .and_then(|value| value.parse::<usize>().ok())
        .ok_or_else(|| {
            prompt_error(
                "PROMPT_FRAME_INVALID",
                &format!("{expected_kind} section 길이가 올바르지 않습니다."),
            )
        })?;
    let end = cursor.checked_add(length).ok_or_else(|| {
        prompt_error(
            "PROMPT_FRAME_INVALID",
            &format!("{expected_kind} section 길이 계산이 넘쳤습니다."),
        )
    })?;
    let body = bytes.get(*cursor..end).ok_or_else(|| {
        prompt_error(
            "PROMPT_FRAME_INVALID",
            &format!("{expected_kind} section이 선언 길이보다 짧습니다."),
        )
    })?;
    *cursor = end;
    if bytes.get(*cursor) != Some(&b'\n') {
        return Err(prompt_error(
            "PROMPT_FRAME_INVALID",
            &format!("{expected_kind} section 구분자가 없습니다."),
        ));
    }
    *cursor += 1;
    String::from_utf8(body.to_vec()).map_err(|_| {
        prompt_error(
            "PROMPT_FRAME_INVALID",
            &format!("{expected_kind} section이 UTF-8이 아닙니다."),
        )
    })
}

fn read_line<'a>(bytes: &'a [u8], cursor: &mut usize) -> Result<&'a str, MentatError> {
    let relative_end = bytes
        .get(*cursor..)
        .and_then(|remaining| remaining.iter().position(|byte| *byte == b'\n'))
        .ok_or_else(|| prompt_error("PROMPT_FRAME_INVALID", "header 줄 끝이 없습니다."))?;
    let end = *cursor + relative_end;
    let line = std::str::from_utf8(&bytes[*cursor..end])
        .map_err(|_| prompt_error("PROMPT_FRAME_INVALID", "header가 UTF-8이 아닙니다."))?;
    *cursor = end + 1;
    Ok(line)
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn prompt_error(code: &str, message: &str) -> MentatError {
    MentatError::PromptError {
        code: code.to_string(),
        message: message.to_string(),
    }
}
