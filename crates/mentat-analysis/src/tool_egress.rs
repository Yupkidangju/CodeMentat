use mentat_core::{
    CanonicalToolRef, MentatError, ProviderBinding, RepositoryConsentKind, RepositoryConsentScope,
    RepositoryToolName, ToolEgressReceipt, ToolEgressStatus,
};
use sha2::{Digest, Sha256};
use std::path::Component;
use uuid::Uuid;

pub const TOOL_EGRESS_SEAL_VERSION: &str = "CM_TOOL_EGRESS_V1";

pub struct RuntimeConsentCapability {
    scope: RepositoryConsentScope,
    nonce: [u8; 32],
}

impl RuntimeConsentCapability {
    pub fn new(scope: RepositoryConsentScope) -> Self {
        let mut seed = Vec::with_capacity(32);
        seed.extend_from_slice(Uuid::new_v4().as_bytes());
        seed.extend_from_slice(Uuid::new_v4().as_bytes());
        let nonce: [u8; 32] = Sha256::digest(seed).into();
        Self { scope, nonce }
    }

    pub fn scope(&self) -> &RepositoryConsentScope {
        &self.scope
    }
}

pub struct ToolEgressEnvelope {
    pub trace_id: Uuid,
    pub conversation_id: Uuid,
    pub turn_id: Uuid,
    pub tool_call_id: Uuid,
    pub repository_id: Uuid,
    pub snapshot_id: Uuid,
    pub tool_name: RepositoryToolName,
    pub refs: Vec<CanonicalToolRef>,
    pub provider_binding: ProviderBinding,
    pub semantic_payload: Vec<u8>,
    pub exact_provider_body: Vec<u8>,
}

pub struct ToolEgressSealer;

impl ToolEgressSealer {
    pub fn prepare(
        capability: &RuntimeConsentCapability,
        envelope: &ToolEgressEnvelope,
    ) -> Result<ToolEgressReceipt, MentatError> {
        let scope = capability.scope();
        if capability.nonce == [0u8; 32] {
            return Err(egress_error(
                "TOOL_EGRESS_CAPABILITY_INVALID",
                "runtime consent nonce가 유효하지 않습니다.",
            ));
        }
        if scope.revoked_at.is_some() {
            return Err(egress_error(
                "TOOL_EGRESS_CONSENT_REVOKED",
                "repository consent가 철회되었습니다.",
            ));
        }
        if scope.conversation_id != envelope.conversation_id
            || scope.repository_id != envelope.repository_id
            || scope.snapshot_id != envelope.snapshot_id
            || scope.provider_binding != envelope.provider_binding
        {
            return Err(egress_error(
                "TOOL_EGRESS_SCOPE_MISMATCH",
                "conversation/repository/snapshot/provider scope가 일치하지 않습니다.",
            ));
        }
        if matches!(
            scope.kind,
            RepositoryConsentKind::RequestOnce { turn_id } if turn_id != envelope.turn_id
        ) {
            return Err(egress_error(
                "TOOL_EGRESS_TURN_MISMATCH",
                "RequestOnce consent는 승인된 turn에서만 사용할 수 있습니다.",
            ));
        }
        envelope.provider_binding.verify_target_digest()?;
        validate_refs(&envelope.refs)?;

        let mut refs = envelope.refs.clone();
        refs.sort_by(|left, right| {
            left.relative_path
                .cmp(&right.relative_path)
                .then(left.line_start.cmp(&right.line_start))
                .then(left.line_end.cmp(&right.line_end))
                .then(left.content_hash.cmp(&right.content_hash))
        });
        let now = chrono::Utc::now();
        let mut receipt = ToolEgressReceipt {
            id: Uuid::new_v4(),
            seal_version: TOOL_EGRESS_SEAL_VERSION.to_string(),
            trace_id: envelope.trace_id,
            consent_scope_id: scope.id,
            conversation_id: envelope.conversation_id,
            turn_id: envelope.turn_id,
            tool_call_id: envelope.tool_call_id,
            repository_id: envelope.repository_id,
            snapshot_id: envelope.snapshot_id,
            tool_name: envelope.tool_name,
            canonical_refs: refs,
            provider_binding: envelope.provider_binding.clone(),
            semantic_payload_digest: sha256_hex(&envelope.semantic_payload),
            exact_provider_body_digest: sha256_hex(&envelope.exact_provider_body),
            canonical_digest: String::new(),
            status: ToolEgressStatus::Prepared,
            prepared_at: now,
            updated_at: now,
        };
        receipt.canonical_digest = canonical_digest(&receipt)?;
        Ok(receipt)
    }

    pub fn verify(receipt: &ToolEgressReceipt) -> Result<(), MentatError> {
        if receipt.seal_version != TOOL_EGRESS_SEAL_VERSION {
            return Err(egress_error(
                "TOOL_EGRESS_SEAL_VERSION_INVALID",
                "지원하지 않는 tool egress seal version입니다.",
            ));
        }
        receipt.provider_binding.verify_target_digest()?;
        validate_refs(&receipt.canonical_refs)?;
        if canonical_digest(receipt)? != receipt.canonical_digest {
            return Err(egress_error(
                "TOOL_EGRESS_CANONICAL_DIGEST_MISMATCH",
                "tool egress receipt가 변경되었습니다.",
            ));
        }
        Ok(())
    }

    pub fn verify_exact_body(
        receipt: &ToolEgressReceipt,
        exact_provider_body: &[u8],
    ) -> Result<(), MentatError> {
        Self::verify(receipt)?;
        if sha256_hex(exact_provider_body) != receipt.exact_provider_body_digest {
            return Err(egress_error(
                "TOOL_EGRESS_BODY_DIGEST_MISMATCH",
                "Prepared receipt와 실제 provider body가 다릅니다.",
            ));
        }
        Ok(())
    }
}

fn canonical_digest(receipt: &ToolEgressReceipt) -> Result<String, MentatError> {
    let mut fields = vec![
        receipt.seal_version.clone(),
        receipt.id.to_string(),
        receipt.trace_id.to_string(),
        receipt.consent_scope_id.to_string(),
        receipt.conversation_id.to_string(),
        receipt.turn_id.to_string(),
        receipt.tool_call_id.to_string(),
        receipt.repository_id.to_string(),
        receipt.snapshot_id.to_string(),
        receipt.tool_name.wire_name().to_string(),
        receipt.provider_binding.profile_id.to_string(),
        receipt.provider_binding.provider.clone(),
        receipt.provider_binding.endpoint_identity.clone(),
        receipt.provider_binding.model_id.clone(),
        receipt.provider_binding.target_digest.clone(),
        receipt.semantic_payload_digest.clone(),
        receipt.exact_provider_body_digest.clone(),
    ];
    let mut refs = receipt.canonical_refs.clone();
    refs.sort_by(|left, right| {
        left.relative_path
            .cmp(&right.relative_path)
            .then(left.line_start.cmp(&right.line_start))
            .then(left.line_end.cmp(&right.line_end))
            .then(left.content_hash.cmp(&right.content_hash))
    });
    for source in refs {
        fields.push(source.relative_path.to_string_lossy().replace('\\', "/"));
        fields.push(source.line_start.to_string());
        fields.push(source.line_end.to_string());
        fields.push(source.content_hash);
        fields.push(source.redacted_payload_digest);
    }
    let mut canonical = Vec::new();
    for field in fields {
        let bytes = field.as_bytes();
        let length = u64::try_from(bytes.len()).map_err(|_| {
            egress_error(
                "TOOL_EGRESS_FIELD_TOO_LARGE",
                "canonical field 길이가 u64 범위를 초과했습니다.",
            )
        })?;
        canonical.extend_from_slice(&length.to_be_bytes());
        canonical.extend_from_slice(bytes);
    }
    Ok(sha256_hex(&canonical))
}

fn validate_refs(refs: &[CanonicalToolRef]) -> Result<(), MentatError> {
    for source in refs {
        if source.relative_path.is_absolute()
            || source.relative_path.components().any(|component| {
                matches!(
                    component,
                    Component::ParentDir | Component::RootDir | Component::Prefix(_)
                )
            })
            || source.line_start == 0
            || source.line_end < source.line_start
            || source.content_hash.is_empty()
            || source.redacted_payload_digest.is_empty()
        {
            return Err(egress_error(
                "TOOL_EGRESS_REF_INVALID",
                "canonical tool ref가 유효하지 않습니다.",
            ));
        }
    }
    Ok(())
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn egress_error(code: &str, message: &str) -> MentatError {
    MentatError::BackendError {
        code: code.to_string(),
        message: message.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mentat_core::{
        CanonicalToolRef, ProviderBinding, RepositoryConsentKind, RepositoryConsentScope,
        RepositoryToolName, ToolEgressStatus,
    };
    use std::path::PathBuf;
    use uuid::Uuid;

    fn fixture() -> (RuntimeConsentCapability, ToolEgressEnvelope) {
        let conversation_id = Uuid::new_v4();
        let turn_id = Uuid::new_v4();
        let repository_id = Uuid::new_v4();
        let snapshot_id = Uuid::new_v4();
        let binding = ProviderBinding::new(
            Uuid::new_v4(),
            "GoogleGemini",
            "https://generativelanguage.googleapis.com/v1beta",
            "dynamic-model",
        )
        .unwrap();
        let scope = RepositoryConsentScope {
            id: Uuid::new_v4(),
            conversation_id,
            repository_id,
            snapshot_id,
            provider_binding: binding.clone(),
            kind: RepositoryConsentKind::RequestOnce { turn_id },
            granted_at: chrono::Utc::now(),
            revoked_at: None,
        };
        let capability = RuntimeConsentCapability::new(scope);
        let envelope = ToolEgressEnvelope {
            trace_id: Uuid::new_v4(),
            conversation_id,
            turn_id,
            tool_call_id: Uuid::new_v4(),
            repository_id,
            snapshot_id,
            tool_name: RepositoryToolName::ReadFileLines,
            refs: vec![CanonicalToolRef {
                relative_path: PathBuf::from("src/lib.rs"),
                line_start: 1,
                line_end: 3,
                content_hash: "file-hash".to_string(),
                redacted_payload_digest: "redacted-hash".to_string(),
            }],
            provider_binding: binding,
            semantic_payload: b"tool result".to_vec(),
            exact_provider_body: b"provider body".to_vec(),
        };
        (capability, envelope)
    }

    #[test]
    fn canonical_receipt_binds_every_egress_identity_and_starts_prepared() {
        let (capability, envelope) = fixture();
        let receipt = ToolEgressSealer::prepare(&capability, &envelope).unwrap();

        assert_eq!(receipt.status, ToolEgressStatus::Prepared);
        assert_eq!(receipt.turn_id, envelope.turn_id);
        assert_eq!(receipt.provider_binding.model_id, "dynamic-model");
        assert!(ToolEgressSealer::verify(&receipt).is_ok());
        assert!(ToolEgressSealer::verify_exact_body(&receipt, b"provider body").is_ok());
        assert!(ToolEgressSealer::verify_exact_body(&receipt, b"tampered body").is_err());
    }

    #[test]
    fn request_once_scope_and_tampered_receipt_fail_closed() {
        let (capability, mut envelope) = fixture();
        envelope.turn_id = Uuid::new_v4();
        assert!(ToolEgressSealer::prepare(&capability, &envelope).is_err());

        let (capability, envelope) = fixture();
        let mut receipt = ToolEgressSealer::prepare(&capability, &envelope).unwrap();
        receipt.provider_binding.model_id = "other-model".to_string();
        assert!(ToolEgressSealer::verify(&receipt).is_err());

        let (capability, envelope) = fixture();
        let mut receipt = ToolEgressSealer::prepare(&capability, &envelope).unwrap();
        receipt.id = Uuid::new_v4();
        assert!(ToolEgressSealer::verify(&receipt).is_err());
    }
}
