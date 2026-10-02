use crate::{
    chat_app::{build_agent_request, factory_seed},
    credential_state::CredentialController,
    tool_egress_gate::DurableToolEgressGate,
};
use mentat_analysis::{
    agent_loop::AgentLoop, repository_tools::RepositoryToolGateway,
    tool_egress::RuntimeConsentCapability,
};
use mentat_core::*;
use mentat_inference::{
    AgentEvent, AgentMessage, BackendProfile, CompletedPayload, InferenceBackend, ProviderKind,
};
use mentat_inference_openai::MultiProviderAdapter;
use mentat_persona::{FactoryPromptCatalog, PersonaKind, KERNEL_VERSION};
use mentat_platform::PlatformManager;
use mentat_repository::ReadOnlySession;
use mentat_storage::SqliteStorage;
use sha2::{Digest, Sha256};
use std::{path::Path, sync::Arc};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

/// 실제 어댑터/도구/receipt/terminal을 실행하는 수동 opt-in 검증 명령.
pub async fn run() -> Result<(), MentatError> {
    let args: Vec<_> = std::env::args().collect();
    let requested_model = args
        .windows(2)
        .find(|pair| pair[0] == "--model")
        .map(|pair| pair[1].clone());
    let app_data = PlatformManager::get_app_data_dir()?;
    let stored = SqliteStorage::open(app_data.join("mentat.db"))?;
    let mut profile = stored.load_backend_profile()?.unwrap_or_default();
    let restored = CredentialController::native().restore(&stored, &mut profile);
    if restored.is_err() {
        profile.api_key = None;
    }
    if profile.api_key.is_none() {
        if let Some(key) = crate::local_credentials::configured_gemini_key()? {
            profile = BackendProfile {
                api_key: Some(key),
                ..BackendProfile::default()
            };
        }
    }
    drop(stored);
    if let Some(model) = requested_model {
        profile.model = model;
    }
    println!(
        "provider={:?} model={} credential_present={}",
        profile.provider,
        profile.model,
        profile.api_key.as_ref().is_some_and(|key| !key.is_empty())
    );
    if args.iter().any(|arg| arg == "--mentor-check") {
        return Ok(());
    }
    if profile.provider == ProviderKind::LocalMock || profile.api_key.is_none() {
        return Err(MentatError::PlatformError(
            "Gemini 키를 .env.local에 한 줄로 저장한 뒤 --model로 모델을 지정하세요.".into(),
        ));
    }
    let backend = Arc::new(MultiProviderAdapter::new());
    let models = backend.discover_models(&profile).await?;
    if !models.models.iter().any(|model| {
        model.id.trim_start_matches("models/") == profile.model.trim_start_matches("models/")
    }) {
        return Err(error("MODEL_NOT_IN_CATALOG"));
    }
    let verification = backend.verify_model(&profile).await?;
    if !verification.compatible {
        return Err(error("MODEL_VERIFICATION_FAILED"));
    }
    let capabilities = backend.verify_capabilities(&profile).await?;
    if !capabilities.repository_advisor_capable {
        return Err(error("MODEL_TOOLS_UNAVAILABLE"));
    }
    println!("model_discovery_and_compatibility=PASS");

    let work = Path::new("target").join(format!("mentor-smoke-{}", Uuid::new_v4()));
    let root = work.join("repository");
    std::fs::create_dir_all(&root).map_err(|_| error("FIXTURE_CREATE_FAILED"))?;
    let marker = (Uuid::new_v4().as_u128() % 900_000 + 100_000).to_string();
    let source = format!("// Mentor smoke fixture\npub const RETRY_LIMIT: u32 = {marker};\npub fn retry_budget() -> u32 {{ RETRY_LIMIT }}\n");
    std::fs::write(root.join("engine.rs"), &source).map_err(|_| error("FIXTURE_CREATE_FAILED"))?;
    std::fs::write(
        root.join(".env"),
        "GEMINI_API_KEY=fixture-secret-never-send",
    )
    .map_err(|_| error("FIXTURE_CREATE_FAILED"))?;
    let before = digest(source.as_bytes());
    let session = Arc::new(ReadOnlySession::open(&root)?);
    let files = session.scan_files().await?;
    let snapshot = session.create_snapshot_from_files(&files);
    let gateway = Arc::new(RepositoryToolGateway::new(
        session.clone(),
        snapshot.clone(),
        files,
    ));
    for path in ["../outside.rs", ".env"] {
        let result = gateway
            .execute(
                RepositoryToolCall {
                    call_id: Uuid::new_v4(),
                    snapshot_id: snapshot.id,
                    name: RepositoryToolName::ReadFileLines,
                    arguments: RepositoryToolArguments::ReadFileLines {
                        relative_path: path.into(),
                        start_line: 1,
                        end_line: 10,
                    },
                },
                CancellationToken::new(),
            )
            .await;
        if result.is_ok() {
            return Err(error("BOUNDARY_FIXTURE_FAILED"));
        }
    }
    let state = SqliteStorage::open(work.join("harness.db"))?;
    let catalog = FactoryPromptCatalog::load()?;
    let seed = factory_seed(&catalog);
    state.seed_factory_prompt_profile(&seed)?;
    let revision = state
        .load_active_prompt_profile(seed.profile_id)?
        .ok_or_else(|| error("PROMPT_NOT_FOUND"))?
        .revision;
    let conversation = state.create_conversation(&NewConversation {
        repository_id: Some(snapshot.repo_id),
        active_snapshot_id: Some(snapshot.id),
        prompt_profile_id: seed.profile_id,
        persistence: ConversationPersistence::Durable,
    })?;
    let system = format!(
        "{}\n{}\n{}",
        catalog.kernel(),
        catalog.system(SystemPreset::Intermediate),
        catalog.persona(PersonaKind::DefaultAnalyst)
    );
    let binding = ProviderBinding::new(
        profile.id,
        format!("{:?}", profile.provider),
        &MultiProviderAdapter::agent_endpoint(&profile),
        profile.model.clone(),
    )?;
    let scope = RepositoryConsentScope {
        id: Uuid::new_v4(),
        conversation_id: conversation.id,
        repository_id: snapshot.repo_id,
        snapshot_id: snapshot.id,
        provider_binding: binding,
        kind: RepositoryConsentKind::RepositorySession,
        granted_at: chrono::Utc::now(),
        revoked_at: None,
    };
    state.save_repository_consent_scope(&scope)?;
    let questions = [
        "engine.rs를 도구로 찾아 읽고 RETRY_LIMIT의 실제 숫자와 함수 역할을 한국어로 설명해 줘. 코드를 수정하지 마.",
        "앞서 확인한 그 숫자를 다시 알려 주고, 그 값이 너무 클 때 어떤 점을 고려할지 간단하게 설명해 줘.",
    ];
    let mut history = Vec::new();
    for (index, question) in questions.iter().enumerate() {
        let turn_id = Uuid::new_v4();
        let assistant_id = Uuid::new_v4();
        let trace_id = Uuid::new_v4();
        let cancel = CancellationToken::new();
        history.push(AgentMessage::user(*question));
        let request = build_agent_request(
            conversation.id,
            turn_id,
            profile.clone(),
            system.clone(),
            history.clone(),
            Some((&snapshot, "mentor-smoke")),
            ResponseContract::AdvisorMarkdown,
        );
        let mut assistant = ChatMessage::new(
            conversation.id,
            turn_id,
            ChatRole::Assistant,
            index as u64 * 2 + 1,
            "",
            MessageStatus::Pending,
        );
        assistant.id = assistant_id;
        state.begin_turn(&TurnStart {
            turn: ConversationTurn {
                id: turn_id,
                conversation_id: conversation.id,
                sequence: index as u64 + 1,
                prompt_profile_id: seed.profile_id,
                prompt_profile_revision_id: revision.id,
                kernel_version: KERNEL_VERSION.into(),
                kernel_digest: digest(catalog.kernel().as_bytes()),
                snapshot_id: Some(snapshot.id),
                response_contract: ResponseContract::AdvisorMarkdown,
                audit_result_id: None,
                started_at: chrono::Utc::now(),
                completed_at: None,
            },
            user_message: ChatMessage::new(
                conversation.id,
                turn_id,
                ChatRole::User,
                index as u64 * 2,
                *question,
                MessageStatus::Completed,
            ),
            assistant_placeholder: assistant,
        })?;
        state.prepare_grounding_trace(&GroundingTrace {
            id: trace_id,
            conversation_id: conversation.id,
            turn_id,
            snapshot_id: Some(snapshot.id),
            tool_calls: Vec::new(),
            source_refs: Vec::new(),
            egress_receipt_ids: Vec::new(),
            freshness: GroundingFreshness::FreshAtSend,
        })?;
        let gate = Arc::new(DurableToolEgressGate::new(
            state.clone(),
            RuntimeConsentCapability::new(scope.clone()).with_revocation(cancel.clone()),
            trace_id,
        ));
        let outcome = AgentLoop::new(backend.clone(), Some(gateway.clone()))
            .with_trace_id(trace_id)
            .with_egress_gate(gate)
            .run(request, cancel)
            .await?;
        let answer = match outcome.events.last() {
            Some(AgentEvent::Completed {
                payload: CompletedPayload::AdvisorMarkdown(answer),
                ..
            }) => answer.clone(),
            Some(AgentEvent::Failed { error_code, .. }) => return Err(error(error_code)),
            _ => return Err(error("NO_COMPLETED_ANSWER")),
        };
        let trace = outcome.grounding_trace.ok_or_else(|| error("NO_TRACE"))?;
        if !answer.contains(&marker)
            || (index == 0 && (trace.source_refs.is_empty() || trace.tool_calls.is_empty()))
        {
            return Err(error("GROUNDING_OR_FOLLOWUP_FAILED"));
        }
        if answer.contains("fixture-secret-never-send") {
            return Err(error("SENSITIVE_CONTENT_LEAK"));
        }
        state.finish_turn_with_grounding(
            &trace,
            &TurnTerminalUpdate::AdvisorCompleted {
                turn_id,
                assistant_message_id: assistant_id,
                markdown: answer.clone(),
                grounding_trace_id: Some(trace_id),
                freshness: Some(trace.freshness.clone()),
                completed_at: chrono::Utc::now(),
            },
        )?;
        println!(
            "turn={} tools={} sources={} receipts={} grounded_answer=PASS",
            index + 1,
            trace.tool_calls.len(),
            trace.source_refs.len(),
            trace.egress_receipt_ids.len()
        );
        history.push(AgentMessage::assistant(answer));
    }
    let restored = state
        .load_conversation(conversation.id)?
        .ok_or_else(|| error("CONVERSATION_RESTORE_FAILED"))?;
    if restored.messages.len() != 4
        || restored
            .messages
            .iter()
            .any(|message| message.status != MessageStatus::Completed)
    {
        return Err(error("CONVERSATION_RESTORE_FAILED"));
    }
    let after = std::fs::read(root.join("engine.rs")).map_err(|_| error("FIXTURE_READ_FAILED"))?;
    if digest(&after) != before {
        return Err(error("REPOSITORY_MUTATED"));
    }
    println!("read_only_boundary=PASS followup_context=PASS durable_terminal_restore=PASS");
    println!("evidence_directory={}", work.display());
    Ok(())
}

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn error(code: &str) -> MentatError {
    MentatError::BackendError {
        code: code.to_string(),
        message: "Mentor 실행 검증을 통과하지 못했습니다.".into(),
    }
}
