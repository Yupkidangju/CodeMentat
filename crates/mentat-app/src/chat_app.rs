use crate::credential_state::CredentialController;
use crate::hotkeys::GlobalShortcutController;
use crate::provider_setup::ProviderSetupState;
use crate::theme::MentatTheme;
use crate::tool_egress_gate::DurableToolEgressGate;
use crate::widgets::markdown::render_markdown;
use crate::widgets::settings_panel::SettingsPanel;
use eframe::egui::{self, RichText, ScrollArea, ViewportCommand};
use mentat_analysis::agent_loop::AgentLoop;
use mentat_analysis::repository_tools::{repository_tool_definitions, RepositoryToolGateway};
use mentat_analysis::tool_egress::RuntimeConsentCapability;
use mentat_analysis::AnswerBundleNormalizer;
use mentat_core::{
    AnswerBundle, ChatMessage, ChatRole, ComposerSubmitMode, Conversation, ConversationPersistence,
    ConversationTurn, ExperiencePreset, FileRecord, GroundingFreshness, GroundingTrace,
    MessageStatus, NewConversation, ProviderBinding, RepositoryConsentKind, RepositoryConsentScope,
    RepositoryReader, RepositorySnapshot, ResponseContract, SystemPreset, TurnStart,
    TurnTerminalUpdate, UiPreferences,
};
use mentat_inference::{
    AgentCapabilities, AgentEvent, AgentLimits, AgentMessage, AgentRequest, CancelledPayload,
    CompletedPayload, InferenceBackend, ModelCatalog, ModelVerification,
};
use mentat_inference_openai::MultiProviderAdapter;
use mentat_persona::{
    FactoryPromptCatalog, PersonaKind, PromptComposer, PromptCompositionInput,
    RepositoryPromptState, FACTORY_BUNDLE_VERSION, KERNEL_VERSION,
};
use mentat_platform::PlatformManager;
use mentat_repository::{ReadOnlySession, ScanLimits};
use mentat_storage::{FactoryPromptSeed, SqliteStorage};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::runtime::Runtime;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

pub const DEFAULT_WINDOW_SIZE: [f32; 2] = [560.0, 760.0];
pub const MIN_WINDOW_SIZE: [f32; 2] = [360.0, 480.0];
const DEFAULT_PROFILE_ID: Uuid = Uuid::from_u128(0x434f_4445_4d45_4e54_4154_0000_0000_0001);

enum AsyncResult {
    RepositoryFolderSelected {
        conversation_id: Uuid,
        generation: Uuid,
        path: Option<std::path::PathBuf>,
        ctx: egui::Context,
    },
    Catalog {
        requested: mentat_inference::BackendProfile,
        result: Result<ModelCatalog, mentat_core::MentatError>,
    },
    Verification {
        requested: mentat_inference::BackendProfile,
        result: Result<(ModelVerification, AgentCapabilities), mentat_core::MentatError>,
    },
    RepositoryScanned {
        conversation_id: Uuid,
        generation: Uuid,
        session: Arc<ReadOnlySession>,
        result: Result<(RepositorySnapshot, Vec<FileRecord>), mentat_core::MentatError>,
    },
    AgentEvent {
        conversation_id: Uuid,
        turn_id: Uuid,
        event: AgentEvent,
    },
    AgentFinished {
        assistant_message_id: Uuid,
        result: Result<Option<GroundingTrace>, mentat_core::MentatError>,
    },
}

struct ActiveTurn {
    turn_id: Uuid,
    assistant_message_id: Uuid,
    accumulated: String,
    response_contract: ResponseContract,
    pending_completion: Option<(CompletedPayload, Option<Uuid>)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConversationMode {
    Advisor,
    Audit,
}

struct RepositoryBinding {
    watcher: mentat_repository::RepositoryWatcher,
    session: Arc<ReadOnlySession>,
    snapshot: RepositorySnapshot,
    gateway: Arc<RepositoryToolGateway>,
}

#[derive(Debug, Clone, Copy)]
enum DirtyPromptAction {
    CloseApp,
    NewConversation,
    CloseSettings,
}

pub struct MentatChatApp {
    runtime: Arc<Runtime>,
    backend: Arc<MultiProviderAdapter>,
    storage: Option<SqliteStorage>,
    prompt_profile_id: Uuid,
    conversation: Conversation,
    provider_setup: ProviderSetupState,
    provider_status: String,
    provider_busy: bool,
    auto_connect_profile: Option<mentat_inference::BackendProfile>,
    credential_controller: CredentialController,
    remember_api_key: bool,
    persona: PersonaKind,
    persona_is_custom: bool,
    base_system_preset: SystemPreset,
    system_prompt_draft: String,
    persona_prompt_draft: String,
    prompt_dirty: bool,
    delete_confirmation_open: bool,
    repository: Option<RepositoryBinding>,
    repository_busy: bool,
    repository_cancel: Option<CancellationToken>,
    scan_generation: Uuid,
    settings_open: bool,
    composer: String,
    submit_mode: ComposerSubmitMode,
    is_pinned: bool,
    async_tx: mpsc::UnboundedSender<AsyncResult>,
    async_rx: mpsc::UnboundedReceiver<AsyncResult>,
    active_turn: Option<ActiveTurn>,
    stream_cancel: Option<CancellationToken>,
    last_window_size: [f32; 2],
    size_changed_at: Option<Instant>,
    status: String,
    global_shortcuts: GlobalShortcutController,
    pending_dirty_action: Option<DirtyPromptAction>,
    mode: ConversationMode,
    repository_egress_approved: bool,
    active_consent_scope: Option<Uuid>,
    grounding_by_message: HashMap<Uuid, GroundingTrace>,
    audit_by_message: HashMap<Uuid, AnswerBundle>,
    selected_grounding_message: Option<Uuid>,
    selected_source: Option<mentat_core::SourceRef>,
}

impl MentatChatApp {
    pub fn new(creation_context: &eframe::CreationContext<'_>, runtime: Arc<Runtime>) -> Self {
        MentatTheme::apply(&creation_context.egui_ctx);
        let backend = Arc::new(MultiProviderAdapter::new());
        let (async_tx, async_rx) = mpsc::unbounded_channel();
        let catalog = FactoryPromptCatalog::load().expect("내장 prompt asset 검증 실패");
        let mut status = String::new();
        let mut storage = match open_storage() {
            Ok(storage) => {
                if storage.recovery_quarantine_path().is_some() {
                    status = "손상된 이전 DB를 격리하고 새 저장소로 시작했습니다.".to_string();
                }
                Some(storage)
            }
            Err(error) => {
                status = format!("저장되지 않음: {error}");
                None
            }
        };
        if let Some(database) = &storage {
            if let Err(error) = database.seed_factory_prompt_profile(&factory_seed(&catalog)) {
                append_status(
                    &mut status,
                    &format!("프롬프트 저장 초기화 실패 · 세션 전용: {error}"),
                );
                storage = None;
            }
        }
        let conversation = match storage
            .as_ref()
            .map(SqliteStorage::load_most_recent_conversation)
        {
            Some(Ok(Some(conversation))) => conversation,
            Some(Ok(None)) | None => Conversation::new(DEFAULT_PROFILE_ID, None, None),
            Some(Err(error)) => {
                append_status(&mut status, &format!("최근 대화 복원 실패: {error}"));
                Conversation::new(DEFAULT_PROFILE_ID, None, None)
            }
        };
        if let Some(database) = &storage {
            let valid = database
                .load_active_prompt_profile(DEFAULT_PROFILE_ID)
                .and_then(|stored| {
                    let stored = stored.ok_or_else(|| {
                        mentat_core::MentatError::IoError("활성 프롬프트 없음".into())
                    })?;
                    catalog.resolve_source(&stored.system_source)?;
                    catalog.resolve_source(&stored.persona_source)?;
                    Ok(())
                });
            if let Err(error) = valid {
                append_status(
                    &mut status,
                    &format!("프롬프트 복원 실패 · 세션 전용: {error}"),
                );
                storage = None;
            }
        }
        let mut saved_backend = match storage.as_ref().map(SqliteStorage::load_backend_profile) {
            Some(Ok(Some(profile))) => profile,
            Some(Ok(None)) | None => mentat_inference::BackendProfile::default(),
            Some(Err(error)) => {
                append_status(&mut status, &format!("Provider 설정 복원 실패: {error}"));
                mentat_inference::BackendProfile::default()
            }
        };
        let credential_controller = CredentialController::native();
        let mut remember_api_key = match storage
            .as_ref()
            .map(|storage| storage.load_provider_secret_preference(saved_backend.id))
        {
            Some(Ok(Some(preference))) => preference.remember_api_key,
            Some(Ok(None)) | None => false,
            Some(Err(error)) => {
                append_status(
                    &mut status,
                    &format!("API key reference 복원 실패: {error}"),
                );
                false
            }
        };
        if let Some(storage) = &storage {
            match credential_controller.restore(storage, &mut saved_backend) {
                Ok(restore) => {
                    remember_api_key = restore.remember_api_key;
                    if restore.credential_missing {
                        append_status(
                            &mut status,
                            "저장된 API key reference에 native credential이 없어 다시 입력해야 합니다.",
                        );
                    }
                }
                Err(error) => {
                    saved_backend.api_key = None;
                    append_status(
                        &mut status,
                        &format!("API key 자동 복원 실패 · 다시 입력 필요: {error}"),
                    );
                }
            }
        }
        if saved_backend.provider == mentat_inference::ProviderKind::GoogleGemini
            && saved_backend.api_key.is_none()
        {
            match crate::local_credentials::configured_gemini_key() {
                Ok(Some(key)) => {
                    saved_backend.api_key = Some(key);
                    append_status(
                        &mut status,
                        ".env.local에서 Gemini 키를 불러왔습니다. 설정에서 모델을 확인하세요.",
                    );
                }
                Ok(None) => {}
                Err(error) => append_status(&mut status, &error.to_string()),
            }
        }
        let preferences = match storage.as_ref().map(SqliteStorage::load_ui_preferences) {
            Some(Ok(preferences)) => preferences,
            Some(Err(error)) => {
                append_status(&mut status, &format!("창·입력 설정 복원 실패: {error}"));
                UiPreferences::default()
            }
            None => UiPreferences::default(),
        };
        let (
            base_system_preset,
            system_prompt_draft,
            persona_prompt_draft,
            restored_persona,
            persona_is_custom,
        ) = storage
            .as_ref()
            .and_then(|storage| {
                storage
                    .load_active_prompt_profile(DEFAULT_PROFILE_ID)
                    .ok()
                    .flatten()
            })
            .and_then(|stored| {
                let (persona, custom) = persona_selection_from_source(&stored.persona_source);
                Some((
                    stored.profile.base_system_preset,
                    catalog.resolve_source(&stored.system_source).ok()?,
                    catalog.resolve_source(&stored.persona_source).ok()?,
                    persona,
                    custom,
                ))
            })
            .unwrap_or_else(|| {
                (
                    SystemPreset::Intermediate,
                    catalog.system(SystemPreset::Intermediate).to_string(),
                    catalog.persona(PersonaKind::DefaultAnalyst).to_string(),
                    PersonaKind::DefaultAnalyst,
                    false,
                )
            });
        let (grounding_by_message, audit_by_message) = storage
            .as_ref()
            .map(|storage| restore_message_projections(storage, &conversation, &mut status))
            .unwrap_or_default();

        let auto_connect_profile = (!saved_backend.model.is_empty()
            && (!saved_backend.provider.requires_api_key() || saved_backend.api_key.is_some()))
        .then(|| saved_backend.clone());
        let mut app = Self {
            runtime,
            backend,
            storage,
            prompt_profile_id: DEFAULT_PROFILE_ID,
            conversation,
            provider_setup: ProviderSetupState::new(saved_backend),
            provider_status: String::new(),
            provider_busy: false,
            auto_connect_profile,
            credential_controller,
            remember_api_key,
            persona: restored_persona,
            persona_is_custom,
            base_system_preset,
            system_prompt_draft,
            persona_prompt_draft,
            prompt_dirty: false,
            delete_confirmation_open: false,
            repository: None,
            repository_busy: false,
            repository_cancel: None,
            scan_generation: Uuid::new_v4(),
            settings_open: false,
            composer: String::new(),
            submit_mode: preferences.submit_mode,
            is_pinned: preferences.always_on_top,
            async_tx,
            async_rx,
            active_turn: None,
            stream_cancel: None,
            last_window_size: [preferences.width_points, preferences.height_points],
            size_changed_at: None,
            status,
            global_shortcuts: GlobalShortcutController::register(&creation_context.egui_ctx),
            pending_dirty_action: None,
            mode: ConversationMode::Advisor,
            repository_egress_approved: false,
            active_consent_scope: None,
            grounding_by_message,
            audit_by_message,
            selected_grounding_message: None,
            selected_source: None,
        };
        if app.auto_connect_profile.is_some() {
            app.begin_model_discovery();
        }
        app
    }

    fn handle_close_requests(&mut self, ctx: &egui::Context) {
        let native_close = ctx.input(|input| input.viewport().close_requested());
        let shortcut =
            ctx.input_mut(|input| input.consume_key(egui::Modifiers::CTRL, egui::Key::Q));
        if native_close && self.prompt_dirty {
            ctx.send_viewport_cmd(ViewportCommand::CancelClose);
        }
        if shortcut || native_close {
            self.request_dirty_action(DirtyPromptAction::CloseApp, ctx);
        }
    }

    fn show_header(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("chat_header")
            .exact_height(68.0)
            .show(ctx, |ui| {
                let drag = ui
                    .horizontal(|ui| {
                        ui.label(RichText::new("MENTAT").strong().size(15.0));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button("×").on_hover_text("닫기 (Ctrl+Q)").clicked() {
                                self.request_dirty_action(DirtyPromptAction::CloseApp, ctx);
                            }
                            if ui
                                .button(if self.settings_open {
                                    "대화로"
                                } else {
                                    "설정"
                                })
                                .clicked()
                            {
                                if self.settings_open {
                                    self.request_dirty_action(
                                        DirtyPromptAction::CloseSettings,
                                        ctx,
                                    );
                                } else {
                                    self.settings_open = !self.settings_open;
                                }
                            }
                            if ui
                                .button(if self.is_pinned {
                                    "핀 켜짐"
                                } else {
                                    "핀 꺼짐"
                                })
                                .on_hover_text("항상 위")
                                .clicked()
                            {
                                self.is_pinned = !self.is_pinned;
                                let level = if self.is_pinned {
                                    egui::viewport::WindowLevel::AlwaysOnTop
                                } else {
                                    egui::viewport::WindowLevel::Normal
                                };
                                ctx.send_viewport_cmd(ViewportCommand::WindowLevel(level));
                                if let Err(error) = self.persist_window_preferences() {
                                    self.status = format!("핀 설정 저장 실패: {error}");
                                }
                            }
                            if ui.button("새 대화").clicked() {
                                self.request_dirty_action(DirtyPromptAction::NewConversation, ctx);
                            }
                        });
                    })
                    .response;
                if drag.dragged() {
                    ctx.send_viewport_cmd(ViewportCommand::StartDrag);
                }
                ui.add(
                    egui::Label::new(
                        RichText::new(self.active_model_label())
                            .size(13.0)
                            .color(MentatTheme::TEXT_MUTED),
                    )
                    .truncate(),
                );
            });
    }

    fn show_chat(&mut self, ctx: &egui::Context) {
        self.show_repository_bar(ctx);
        egui::TopBottomPanel::bottom("chat_composer")
            .resizable(false)
            .show(ctx, |ui| {
                ui.add_space(4.0);
                ui.label(RichText::new("질문").strong());
                let response = ui.add_sized(
                    [ui.available_width(), 88.0],
                    egui::TextEdit::multiline(&mut self.composer)
                        .id(egui::Id::new("mentor_composer"))
                        .desired_rows(4)
                        .hint_text("코드 위치, 설계 이유, 다음 작업을 물어보세요…")
                        .lock_focus(true),
                );
                let keyboard_submit = response.has_focus()
                    && ui.input(|input| {
                        composer_key_events_should_submit(self.submit_mode, &input.events)
                    });
                ui.horizontal(|ui| {
                    if let Some(active) = &self.active_turn {
                        ui.label(
                            RichText::new(format!(
                                "탐색·응답 중 · {}자",
                                active.accumulated.chars().count()
                            ))
                            .small()
                            .color(MentatTheme::STATUS_INFERENCING),
                        );
                        if ui.button("중지").clicked() {
                            if let Some(token) = &self.stream_cancel {
                                token.cancel();
                            }
                        }
                    } else {
                        ui.label(
                            RichText::new(submit_mode_label(self.submit_mode))
                                .small()
                                .color(MentatTheme::TEXT_MUTED),
                        );
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let send = ui
                            .add_enabled(
                                self.active_turn.is_none()
                                    && !self.repository_busy
                                    && !self.composer.trim().is_empty(),
                                egui::Button::new("전송"),
                            )
                            .clicked();
                        if send || keyboard_submit {
                            self.submit_chat();
                            response.request_focus();
                        }
                    });
                });
                if !self.status.is_empty() {
                    ui.add(
                        egui::Label::new(
                            RichText::new(&self.status)
                                .small()
                                .color(MentatTheme::STATUS_CONFLICT),
                        )
                        .wrap(),
                    );
                }
                ui.add_space(2.0);
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            ScrollArea::vertical()
                .id_salt("conversation_timeline")
                .stick_to_bottom(true)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    if self.conversation.messages.is_empty() {
                        self.show_onboarding(ui);
                    }
                    let mut open_grounding = None;
                    for message in &self.conversation.messages {
                        render_message(ui, message, self.audit_by_message.get(&message.id));
                        if message.role == ChatRole::Assistant {
                            if let Some(trace) = self.grounding_by_message.get(&message.id) {
                                if ui
                                    .small_button(format!(
                                        "근거 {} · 도구 {}",
                                        trace.source_refs.len(),
                                        trace.tool_calls.len()
                                    ))
                                    .clicked()
                                {
                                    open_grounding = Some(message.id);
                                }
                            }
                        }
                        ui.add_space(10.0);
                    }
                    if let Some(message_id) = open_grounding {
                        self.selected_grounding_message = Some(message_id);
                    }
                });
        });
        self.show_grounding_drawer(ctx);
    }

    fn show_repository_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("mentor_connections").show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                if ui.button(if self.provider_setup.active_profile().is_some() { "AI 변경" } else { "AI 연결" }).clicked() {
                    self.settings_open = true;
                }
                let repo_label = self.repository.as_ref().map(|binding| {
                    format!("{} · {}개 파일 · {}", binding.session.profile().display_name,
                        binding.snapshot.file_count,
                        if binding.gateway.is_stale() { "변경됨" } else { "읽기 전용" })
                }).unwrap_or_else(|| "저장소를 연결하면 코드 근거로 답합니다".to_string());
                let available = (ui.available_width() - 115.0).max(100.0);
                ui.add_sized([available, 22.0], egui::Label::new(repo_label).truncate());
                if ui.add_enabled(!self.repository_busy && self.active_turn.is_none(),
                    egui::Button::new(if self.repository.is_some() { "다시 읽기" } else { "저장소 연결" })).clicked() {
                    self.begin_repository_scan(ui.ctx());
                }
            });
            if self.repository_busy {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label("저장소를 읽는 중…");
                    if ui.button("중지").clicked() {
                        if let Some(token) = &self.repository_cancel { token.cancel(); }
                    }
                });
            }
            if let Some(repository) = &self.repository {
                if repository.snapshot.status != mentat_core::SnapshotStatus::Ready {
                    ui.label(RichText::new("저장소가 변경되었거나 일부 파일을 읽지 못했습니다. 다시 읽어 주세요.")
                        .color(MentatTheme::STATUS_CONFLICT));
                }
                let capable = self.provider_setup.active_capabilities()
                    .is_some_and(|cap| cap.repository_advisor_capable);
                if !capable {
                    ui.label("현재 AI는 일반 대화만 지원합니다. 설정에서 저장소 도구 호환성을 확인하세요.");
                } else if self.provider_setup.active_profile().is_some_and(|profile| profile.provider.requires_api_key())
                    && ui.checkbox(&mut self.repository_egress_approved, "필요한 코드 발췌를 선택한 AI에 보내기")
                        .on_hover_text("민감 파일과 키는 제외합니다. 이 대화·저장소·AI 선택에만 적용됩니다.").changed()
                        && !self.repository_egress_approved {
                        if let Some(token) = &self.stream_cancel { token.cancel(); }
                }
            }
        });
    }

    fn show_onboarding(&mut self, ui: &mut egui::Ui) {
        ui.add_space(20.0);
        ui.heading("코드를 함께 살펴보는 멘토");
        ui.label("궁금한 점을 물으면 저장소에서 근거를 찾아 설명합니다.");
        ui.add_space(16.0);
        ui.group(|ui| {
            ui.set_width(ui.available_width());
            if self.provider_setup.active_profile().is_none() {
                ui.label(RichText::new("1. 사용할 AI 연결").strong());
                ui.label("공급자와 모델을 선택하고 호환성을 확인합니다.");
                if ui.button("AI 설정 열기").clicked() {
                    self.settings_open = true;
                }
            } else {
                ui.label(RichText::new("1. AI 준비됨").strong());
            }
            ui.add_space(12.0);
            ui.label(
                RichText::new(if self.repository.is_some() {
                    "2. 저장소 준비됨"
                } else {
                    "2. 읽을 저장소 선택"
                })
                .strong(),
            );
            ui.label("파일을 읽고 검색합니다. 코드를 변경하거나 명령을 실행하지 않습니다.");
            if self.repository.is_none()
                && ui
                    .add_enabled(!self.repository_busy, egui::Button::new("저장소 선택"))
                    .clicked()
            {
                self.begin_repository_scan(ui.ctx());
            }
        });
        ui.add_space(18.0);
        ui.label(RichText::new("이렇게 물어보세요").strong());
        for question in [
            "이 프로젝트의 진입점과 전체 구조를 설명해 줘",
            "이 기능을 고치려면 어느 파일부터 읽으면 좋을까?",
            "앞서 설명한 구조에서 주의할 점을 알려 줘",
        ] {
            if ui.add(egui::Button::new(question).wrap()).clicked() {
                self.composer = question.to_string();
                ui.ctx()
                    .memory_mut(|memory| memory.request_focus(egui::Id::new("mentor_composer")));
            }
        }
    }

    fn show_grounding_drawer(&mut self, ctx: &egui::Context) {
        let Some(message_id) = self.selected_grounding_message else {
            return;
        };
        let Some(trace) = self.grounding_by_message.get(&message_id).cloned() else {
            self.selected_grounding_message = None;
            return;
        };
        let mut open = true;
        egui::Window::new("답변의 코드 근거")
            .open(&mut open)
            .default_width(300.0)
            .resizable(true)
            .show(ctx, |ui| {
                ui.label(
                    RichText::new(format!(
                        "{} · 도구 {}회",
                        match &trace.freshness {
                            GroundingFreshness::FreshAtSend => "답변 생성 당시 코드",
                            GroundingFreshness::ChangedAfterSend { .. } => "답변 이후 코드 변경됨",
                            GroundingFreshness::StaleBeforeSend => "다시 읽기가 필요한 코드",
                        },
                        trace.tool_calls.len()
                    ))
                    .small()
                    .color(MentatTheme::TEXT_MUTED),
                );
                ui.separator();
                if trace.source_refs.is_empty() {
                    ui.label("이 답변에는 인용된 코드가 없습니다.");
                }
                for source in &trace.source_refs {
                    let label = format!(
                        "{}:{}-{}",
                        source.relative_path.display(),
                        source.line_start,
                        source.line_end
                    );
                    if ui.button(label).clicked() {
                        self.selected_source = Some(source.clone());
                    }
                }
                if let Some(source) = &self.selected_source {
                    ui.separator();
                    ui.label(
                        RichText::new(format!(
                            "{} · {}–{}행",
                            source.relative_path.display(),
                            source.line_start,
                            source.line_end
                        ))
                        .strong(),
                    );
                    ScrollArea::horizontal().show(ui, |ui| {
                        ui.add(
                            egui::Label::new(RichText::new(&source.excerpt).monospace())
                                .wrap_mode(egui::TextWrapMode::Extend),
                        );
                    });
                }
            });
        if !open {
            self.selected_grounding_message = None;
            self.selected_source = None;
        }
    }

    fn show_settings(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ScrollArea::vertical().show(ui, |ui| {
                let stage = self.provider_setup.stage();
                ui.label(self.global_shortcuts.status());
                let models = self.provider_setup.catalog.models.clone();
                let previous = self.provider_setup.draft_profile.clone();
                let previous_persona = self.persona;
                let action = SettingsPanel {
                    profile: &mut self.provider_setup.draft_profile,
                    persona: &mut self.persona,
                    persona_is_custom: self.persona_is_custom,
                    remember_api_key: &mut self.remember_api_key,
                    available_models: &models,
                    stage,
                    provider_status: &self.provider_status,
                    is_busy: self.provider_busy,
                }
                .show(ui);
                let provider_target_changed = previous.provider
                    != self.provider_setup.draft_profile.provider
                    || previous.base_url != self.provider_setup.draft_profile.base_url;
                if provider_target_changed {
                    if let Some(token) = &self.stream_cancel {
                        token.cancel();
                    }
                    self.repository_egress_approved = false;
                    self.mode = ConversationMode::Advisor;
                    self.provider_setup.draft_profile.api_key = None;
                    self.remember_api_key = false;
                    if let Some(storage) = &self.storage {
                        if let Err(error) = self
                            .credential_controller
                            .delete_profile(storage, previous.id)
                        {
                            self.provider_status = format!(
                                "이전 API key 제거 실패 · native store를 확인하세요: {error}"
                            );
                        }
                    }
                }
                self.provider_setup.reconcile_edit(&previous);
                if previous != self.provider_setup.draft_profile {
                    self.auto_connect_profile = None;
                }
                if self.persona != previous_persona {
                    if let Ok(catalog) = FactoryPromptCatalog::load() {
                        self.persona_prompt_draft = catalog.persona(self.persona).to_string();
                        self.persona_is_custom = false;
                        self.prompt_dirty = true;
                    }
                }

                if let Some(model) = action.selected_model {
                    if let Err(error) = self.provider_setup.select_model(&model) {
                        self.provider_status = error;
                    } else {
                        self.repository_egress_approved = false;
                        if let Some(token) = &self.stream_cancel {
                            token.cancel();
                        }
                        self.mode = ConversationMode::Advisor;
                    }
                }
                if action.discover_clicked {
                    self.begin_model_discovery();
                }
                if action.verify_clicked {
                    self.begin_model_verification();
                }
                if action.activate_clicked {
                    let draft = self.provider_setup.draft_profile.clone();
                    let persistence = self.storage.as_ref().map_or_else(
                        || {
                            if self.remember_api_key {
                                Err("저장소가 없어 API key를 안전하게 기억할 수 없습니다."
                                    .to_string())
                            } else {
                                Ok(())
                            }
                        },
                        |storage| {
                            self.credential_controller
                                .persist(storage, &draft, self.remember_api_key)
                                .map_err(|error| error.to_string())?;
                            if let Err(error) = storage.save_backend_profile(&draft) {
                                let _ =
                                    self.credential_controller.delete_profile(storage, draft.id);
                                return Err(error.to_string());
                            }
                            Ok(())
                        },
                    );
                    if let Err(error) = persistence {
                        self.provider_status = format!("활성화 전 설정 저장 실패: {error}");
                    } else {
                        match self.provider_setup.activate() {
                            Ok(()) => {
                                self.provider_status = if self.remember_api_key {
                                    "활성 모델과 API key를 OS 자격 증명 저장소에 적용했습니다."
                                        .to_string()
                                } else {
                                    "활성 모델을 적용했습니다. API key는 이 세션에만 유지됩니다."
                                        .to_string()
                                };
                            }
                            Err(error) => self.provider_status = error,
                        }
                    }
                }
                if action.close_clicked {
                    self.request_dirty_action(DirtyPromptAction::CloseSettings, ctx);
                }
                ui.add_space(14.0);
                ui.separator();
                ui.add_space(10.0);
                ui.collapsing("고급 · 설명 스타일과 프롬프트", |ui| {
                    self.show_prompt_settings(ui)
                });
            });
        });
    }

    fn show_prompt_settings(&mut self, ui: &mut egui::Ui) {
        ui.label(RichText::new("입력 동작").strong());
        let previous_submit_mode = self.submit_mode;
        egui::ComboBox::from_id_salt("composer_submit_mode")
            .width(ui.available_width())
            .selected_text(submit_mode_label(self.submit_mode))
            .show_ui(ui, |ui| {
                ui.selectable_value(
                    &mut self.submit_mode,
                    ComposerSubmitMode::EnterSend,
                    submit_mode_label(ComposerSubmitMode::EnterSend),
                );
                ui.selectable_value(
                    &mut self.submit_mode,
                    ComposerSubmitMode::CtrlEnterSend,
                    submit_mode_label(ComposerSubmitMode::CtrlEnterSend),
                );
            });
        if self.submit_mode != previous_submit_mode {
            if let Err(error) = self.persist_window_preferences() {
                self.provider_status = format!("입력 동작 저장 실패: {error}");
            }
        }
        ui.add_space(12.0);
        ui.heading(RichText::new("프롬프트").size(17.0).strong());
        ui.label(
            RichText::new("Kernel은 읽기 전용이며 Apply는 다음 턴부터 적용됩니다.")
                .small()
                .color(MentatTheme::TEXT_MUTED),
        );
        ui.add_space(6.0);
        ui.collapsing("Kernel v1 · 읽기 전용", |ui| {
            if let Ok(catalog) = FactoryPromptCatalog::load() {
                let mut kernel = catalog.kernel().to_string();
                ui.add(
                    egui::TextEdit::multiline(&mut kernel)
                        .desired_rows(8)
                        .interactive(false)
                        .font(egui::FontId::monospace(11.0)),
                );
            }
        });

        ui.add_space(8.0);
        ui.label(RichText::new("System 숙련도").strong());
        let previous_preset = self.base_system_preset;
        egui::ComboBox::from_id_salt("system_preset")
            .width(ui.available_width())
            .selected_text(system_preset_label(self.base_system_preset))
            .show_ui(ui, |ui| {
                for preset in SystemPreset::ALL {
                    ui.selectable_value(
                        &mut self.base_system_preset,
                        preset,
                        system_preset_label(preset),
                    );
                }
            });
        if self.base_system_preset != previous_preset {
            if let Ok(catalog) = FactoryPromptCatalog::load() {
                self.system_prompt_draft = catalog.system(self.base_system_preset).to_string();
                self.prompt_dirty = true;
            }
        }
        if ui
            .add(
                egui::TextEdit::multiline(&mut self.system_prompt_draft)
                    .desired_rows(8)
                    .hint_text("System prompt"),
            )
            .changed()
        {
            self.prompt_dirty = true;
        }

        ui.add_space(8.0);
        ui.label(RichText::new("Persona").strong());
        if ui
            .add(
                egui::TextEdit::multiline(&mut self.persona_prompt_draft)
                    .desired_rows(7)
                    .hint_text("Persona prompt"),
            )
            .changed()
        {
            self.persona_is_custom = true;
            self.prompt_dirty = true;
        }

        if let Some(storage) = &self.storage {
            let system_versions = storage
                .list_prompt_versions(self.prompt_profile_id, mentat_core::PromptLayer::System)
                .unwrap_or_default();
            let persona_versions = storage
                .list_prompt_versions(self.prompt_profile_id, mentat_core::PromptLayer::Persona)
                .unwrap_or_default();
            let mut selected_system = None;
            let mut selected_persona = None;
            egui::ComboBox::from_id_salt("system_version_restore")
                .width(ui.available_width())
                .selected_text("System 과거 version 불러오기…")
                .show_ui(ui, |ui| {
                    for version in &system_versions {
                        if ui
                            .selectable_label(false, format!("System v{}", version.version))
                            .clicked()
                        {
                            selected_system = Some(version.clone());
                        }
                    }
                });
            egui::ComboBox::from_id_salt("persona_version_restore")
                .width(ui.available_width())
                .selected_text("Persona 과거 version 불러오기…")
                .show_ui(ui, |ui| {
                    for version in &persona_versions {
                        if ui
                            .selectable_label(false, format!("Persona v{}", version.version))
                            .clicked()
                        {
                            selected_persona = Some(version.clone());
                        }
                    }
                });
            if let Ok(catalog) = FactoryPromptCatalog::load() {
                if let Some(version) = selected_system {
                    if let Ok(text) = catalog.resolve_source(&version.source) {
                        self.system_prompt_draft = text;
                        self.prompt_dirty = true;
                    }
                }
                if let Some(version) = selected_persona {
                    let (persona, custom) = persona_selection_from_source(&version.source);
                    if let Ok(text) = catalog.resolve_source(&version.source) {
                        self.persona = persona;
                        self.persona_is_custom = custom;
                        self.persona_prompt_draft = text;
                        self.prompt_dirty = true;
                    }
                }
            }
        }

        ui.add_space(8.0);
        if ui
            .add_enabled(
                self.prompt_dirty && self.storage.is_some(),
                egui::Button::new("Apply · 다음 턴부터 적용")
                    .min_size(egui::vec2(ui.available_width(), 32.0)),
            )
            .clicked()
        {
            self.apply_prompt_settings();
        }
        ui.horizontal(|ui| {
            if ui
                .add_enabled(self.prompt_dirty, egui::Button::new("Cancel"))
                .clicked()
            {
                self.reload_prompt_settings();
            }
            if ui.button("Factory Reset").clicked() {
                if let Ok(catalog) = FactoryPromptCatalog::load() {
                    self.system_prompt_draft = catalog.system(self.base_system_preset).to_string();
                    self.persona_prompt_draft = catalog.persona(self.persona).to_string();
                    self.persona_is_custom = false;
                    self.prompt_dirty = true;
                }
            }
        });
        ui.horizontal(|ui| {
            if ui.button("System 기본값").clicked() {
                if let Ok(catalog) = FactoryPromptCatalog::load() {
                    self.system_prompt_draft = catalog.system(self.base_system_preset).to_string();
                    self.prompt_dirty = true;
                }
            }
            if ui.button("Persona 기본값").clicked() {
                if let Ok(catalog) = FactoryPromptCatalog::load() {
                    self.persona_prompt_draft = catalog.persona(self.persona).to_string();
                    self.persona_is_custom = false;
                    self.prompt_dirty = true;
                }
            }
        });
        if self.storage.is_none() {
            ui.label(
                RichText::new("저장되지 않음 · factory prompt만 사용할 수 있습니다.")
                    .small()
                    .color(MentatTheme::STATUS_CONFLICT),
            );
        }
        ui.add_space(14.0);
        ui.separator();
        ui.add_space(8.0);
        ui.label(RichText::new("대화 데이터").strong());
        if !self.delete_confirmation_open {
            if ui.button("현재 대화 삭제…").clicked() {
                self.delete_confirmation_open = true;
            }
        } else {
            ui.label(
                RichText::new("메시지·근거·receipt·Audit 결과가 함께 삭제됩니다.")
                    .small()
                    .color(MentatTheme::STATUS_ERROR),
            );
            ui.horizontal(|ui| {
                if ui.button("취소").clicked() {
                    self.delete_confirmation_open = false;
                }
                if ui.button("삭제 확인").clicked() {
                    self.delete_current_conversation();
                }
            });
        }
    }

    fn apply_prompt_settings(&mut self) {
        let Some(storage) = &self.storage else {
            self.provider_status = "저장소가 없어 prompt를 적용할 수 없습니다.".to_string();
            return;
        };
        let catalog = match FactoryPromptCatalog::load() {
            Ok(catalog) => catalog,
            Err(error) => {
                self.provider_status = error.to_string();
                return;
            }
        };
        let stored = match storage.load_active_prompt_profile(self.prompt_profile_id) {
            Ok(Some(stored)) => stored,
            Ok(None) => {
                self.provider_status = "활성 prompt profile이 없습니다.".to_string();
                return;
            }
            Err(error) => {
                self.provider_status = error.to_string();
                return;
            }
        };
        let factory_system = catalog.system(self.base_system_preset);
        let system = if self.system_prompt_draft == factory_system {
            mentat_core::PromptLayerDraft::ResetToFactory {
                resource_key: self.base_system_preset.resource_key().to_string(),
                resource_version: FACTORY_BUNDLE_VERSION.to_string(),
                expected_checksum: catalog.checksum(factory_system),
            }
        } else {
            mentat_core::PromptLayerDraft::UserText(self.system_prompt_draft.clone())
        };
        let matching_persona = PersonaKind::ALL
            .into_iter()
            .find(|persona| self.persona_prompt_draft == catalog.persona(*persona));
        let persona = if let Some(persona) = matching_persona {
            self.persona = persona;
            self.persona_is_custom = false;
            mentat_core::PromptLayerDraft::ResetToFactory {
                resource_key: persona.resource_key().to_string(),
                resource_version: FACTORY_BUNDLE_VERSION.to_string(),
                expected_checksum: catalog.checksum(catalog.persona(persona)),
            }
        } else {
            self.persona_is_custom = true;
            mentat_core::PromptLayerDraft::UserText(self.persona_prompt_draft.clone())
        };
        let factory_experience = match self.base_system_preset {
            SystemPreset::Beginner => ExperiencePreset::Beginner,
            SystemPreset::Intermediate => ExperiencePreset::Intermediate,
            SystemPreset::Professional => ExperiencePreset::Professional,
            SystemPreset::Senior => ExperiencePreset::Senior,
        };
        let experience_preset = if self.system_prompt_draft == factory_system {
            factory_experience
        } else {
            ExperiencePreset::Custom
        };
        match storage.apply_prompt_draft(
            stored.revision.id,
            &mentat_core::PromptDraft {
                profile_id: self.prompt_profile_id,
                name: "사용자 멘토".to_string(),
                experience_preset,
                base_system_preset: self.base_system_preset,
                system,
                persona,
            },
        ) {
            Ok(_) => {
                self.prompt_dirty = false;
                self.provider_status = "Prompt Apply 완료 · 다음 턴부터 적용됩니다.".to_string();
            }
            Err(error) => self.provider_status = error.to_string(),
        }
    }

    fn reload_prompt_settings(&mut self) {
        let Some(storage) = &self.storage else {
            return;
        };
        let Ok(Some(stored)) = storage.load_active_prompt_profile(self.prompt_profile_id) else {
            return;
        };
        let Ok(catalog) = FactoryPromptCatalog::load() else {
            return;
        };
        if let (Ok(system), Ok(persona)) = (
            catalog.resolve_source(&stored.system_source),
            catalog.resolve_source(&stored.persona_source),
        ) {
            self.base_system_preset = stored.profile.base_system_preset;
            self.system_prompt_draft = system;
            self.persona_prompt_draft = persona;
            let (persona, custom) = persona_selection_from_source(&stored.persona_source);
            self.persona = persona;
            self.persona_is_custom = custom;
            self.prompt_dirty = false;
        }
    }

    fn delete_current_conversation(&mut self) {
        if let Some(token) = &self.stream_cancel {
            token.cancel();
        }
        if let Some(token) = &self.repository_cancel {
            token.cancel();
        }
        if self.active_turn.is_some() || self.repository_busy {
            self.provider_status =
                "진행 중 요청이 terminal 상태가 된 뒤 다시 삭제해 주세요.".to_string();
            return;
        }
        if let Some(storage) = &self.storage {
            if let Err(error) = storage.delete_conversation(self.conversation.id) {
                self.provider_status = format!("대화 삭제 실패: {error}");
                return;
            }
        }
        self.repository = None;
        self.delete_confirmation_open = false;
        self.start_new_conversation();
        self.provider_status = "현재 대화를 삭제했습니다.".to_string();
    }

    fn request_dirty_action(&mut self, action: DirtyPromptAction, ctx: &egui::Context) {
        if self.prompt_dirty {
            self.pending_dirty_action = Some(action);
        } else {
            self.execute_dirty_action(action, ctx);
        }
    }

    fn execute_dirty_action(&mut self, action: DirtyPromptAction, ctx: &egui::Context) {
        if matches!(action, DirtyPromptAction::CloseApp) {
            if let Some(token) = &self.stream_cancel {
                token.cancel();
            }
            if let Some(token) = &self.repository_cancel {
                token.cancel();
            }
        }
        match action {
            DirtyPromptAction::CloseApp => match self.persist_window_preferences() {
                Ok(()) => ctx.send_viewport_cmd(ViewportCommand::Close),
                Err(error) => {
                    self.status = format!(
                        "마지막 창 크기 저장 실패로 종료를 보류했습니다. 다시 시도하세요: {error}"
                    );
                }
            },
            DirtyPromptAction::NewConversation => self.start_new_conversation(),
            DirtyPromptAction::CloseSettings => self.settings_open = false,
        }
    }

    fn show_dirty_prompt_confirmation(&mut self, ctx: &egui::Context) {
        let Some(action) = self.pending_dirty_action else {
            return;
        };
        let mut keep_editing = false;
        let mut discard = false;
        egui::Window::new("적용하지 않은 프롬프트")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                ui.label("System/Persona draft가 아직 Apply되지 않았습니다.");
                if ui.button("계속 편집").clicked() {
                    keep_editing = true;
                }
                if ui.button("변경사항 폐기").clicked() {
                    discard = true;
                }
            });
        if keep_editing {
            self.pending_dirty_action = None;
        } else if discard {
            self.reload_prompt_settings();
            self.prompt_dirty = false;
            self.pending_dirty_action = None;
            self.execute_dirty_action(action, ctx);
        }
    }

    fn begin_model_discovery(&mut self) {
        let requested = self.provider_setup.begin_discovery();
        let backend = self.backend.clone();
        let tx = self.async_tx.clone();
        self.provider_busy = true;
        self.provider_status = "공급자 API 확인 및 모델 목록 조회 중…".to_string();
        self.runtime.spawn(async move {
            let result = backend.discover_models(&requested).await;
            let _ = tx.send(AsyncResult::Catalog { requested, result });
        });
    }

    fn begin_repository_scan(&mut self, ctx: &egui::Context) {
        if self.repository_busy {
            return;
        }
        self.repository_busy = true;
        self.scan_generation = Uuid::new_v4();
        let generation = self.scan_generation;
        let conversation_id = self.conversation.id;
        let tx = self.async_tx.clone();
        let ctx = ctx.clone();
        ctx.send_viewport_cmd(ViewportCommand::WindowLevel(
            egui::viewport::WindowLevel::Normal,
        ));
        self.runtime.spawn_blocking(move || {
            let path = PlatformManager::pick_folder();
            let _ = tx.send(AsyncResult::RepositoryFolderSelected {
                conversation_id,
                generation,
                path,
                ctx,
            });
        });
    }

    fn scan_repository_path(&mut self, path: std::path::PathBuf) {
        if let Ok(app_data) = PlatformManager::get_app_data_dir() {
            if let Err(error) = PlatformManager::validate_storage_isolation(&app_data, &path) {
                self.status = error.to_string();
                return;
            }
        }
        let known_id = self
            .storage
            .as_ref()
            .and_then(|storage| storage.find_repo_by_root(&path).ok().flatten())
            .map(|profile| profile.id);
        let session = match ReadOnlySession::open_with_known_id(&path, known_id) {
            Ok(session) => Arc::new(session),
            Err(error) => {
                self.status = error.to_string();
                return;
            }
        };
        if self.conversation.messages.is_empty() && self.storage.is_some() {
            self.ensure_durable_conversation();
        }
        let cancel = CancellationToken::new();
        self.repository_busy = true;
        self.status = "저장소를 읽기 전용으로 인덱싱하는 중…".to_string();
        let tx = self.async_tx.clone();
        let session_for_task = session.clone();
        if let Some(token) = self.repository_cancel.replace(cancel.clone()) {
            token.cancel();
        }
        self.scan_generation = Uuid::new_v4();
        let generation = self.scan_generation;
        let conversation_id = self.conversation.id;
        self.runtime.spawn(async move {
            let result = session_for_task
                .scan_files_with_limits(ScanLimits::default(), cancel)
                .await
                .map(|outcome| {
                    let snapshot = session_for_task.create_snapshot_from_outcome(&outcome);
                    (snapshot, outcome.files)
                });
            let _ = tx.send(AsyncResult::RepositoryScanned {
                conversation_id,
                generation,
                session,
                result,
            });
        });
    }

    fn begin_model_verification(&mut self) {
        let requested = match self.provider_setup.verification_request() {
            Ok(profile) => profile,
            Err(error) => {
                self.provider_status = error;
                return;
            }
        };
        let backend = self.backend.clone();
        let tx = self.async_tx.clone();
        self.provider_busy = true;
        self.provider_status = "선택 모델의 실제 텍스트 생성 호환성 확인 중…".to_string();
        self.runtime.spawn(async move {
            let result = async {
                let verification = backend.verify_model(&requested).await?;
                if !verification.compatible {
                    return Ok((
                        verification,
                        AgentCapabilities {
                            chat_capable: false,
                            native_tool_capable: false,
                            emulated_tool_capable: false,
                            repository_advisor_capable: false,
                        },
                    ));
                }
                let capabilities = backend.verify_capabilities(&requested).await?;
                Ok((verification, capabilities))
            }
            .await;
            let _ = tx.send(AsyncResult::Verification { requested, result });
        });
    }

    fn submit_chat(&mut self) {
        if self.active_turn.is_some() || self.repository_busy {
            return;
        }
        let Some(profile) = self.provider_setup.active_profile().cloned() else {
            self.status = "설정에서 공급자 모델을 확인하고 활성화해 주세요.".to_string();
            self.settings_open = true;
            return;
        };
        let question = self.composer.trim().to_string();
        if question.is_empty() {
            return;
        }
        let capabilities = self
            .provider_setup
            .active_capabilities()
            .unwrap_or(AgentCapabilities::CHAT_ONLY);
        let repository_capable = capabilities.repository_advisor_capable
            && self.repository.as_ref().is_some_and(|repository| {
                repository.snapshot.status == mentat_core::SnapshotStatus::Ready
            });
        if self.mode == ConversationMode::Audit && !repository_capable {
            self.status =
                "Audit은 Ready 저장소와 repository advisor 검증 모델이 필요합니다.".to_string();
            return;
        }
        if repository_capable
            && profile.provider.requires_api_key()
            && !self.repository_egress_approved
        {
            self.status =
                "저장소 발췌를 공급자에 전송하려면 위 동의 항목을 먼저 선택하세요.".to_string();
            return;
        }
        if repository_capable && profile.provider.requires_api_key() && self.storage.is_none() {
            self.status =
                "durable receipt 저장소가 없어 cloud repository 도구를 실행할 수 없습니다."
                    .to_string();
            return;
        }
        if self.conversation.messages.is_empty() && self.storage.is_some() {
            self.ensure_durable_conversation();
        }
        let mut composition = match self.compose_prompt() {
            Ok(value) => value,
            Err(error) => {
                self.status = error.to_string();
                return;
            }
        };
        let turn_id = Uuid::new_v4();
        let response_contract = match self.mode {
            ConversationMode::Advisor => ResponseContract::AdvisorMarkdown,
            ConversationMode::Audit => ResponseContract::AuditAnswerBundle {
                schema_version: "answer_bundle.v1".to_string(),
            },
        };
        let repository = if repository_capable {
            self.repository.as_ref()
        } else {
            None
        };
        if matches!(
            response_contract,
            ResponseContract::AuditAnswerBundle { .. }
        ) {
            let snapshot_id = repository
                .map(|repository| repository.snapshot.id)
                .expect("Audit repository capability가 앞에서 검증됨");
            composition.effective_system_prompt.push_str("\n\n");
            composition
                .effective_system_prompt
                .push_str(&AnswerBundleNormalizer::system_contract(snapshot_id));
        }
        let next_ordinal = self.conversation.messages.len() as u64;
        let user_message = ChatMessage::new(
            self.conversation.id,
            turn_id,
            ChatRole::User,
            next_ordinal,
            question,
            MessageStatus::Completed,
        );
        let assistant_message = ChatMessage::new(
            self.conversation.id,
            turn_id,
            ChatRole::Assistant,
            next_ordinal + 1,
            "",
            MessageStatus::Pending,
        );
        let revision_id = self
            .storage
            .as_ref()
            .and_then(|storage| {
                storage
                    .load_active_prompt_profile(self.prompt_profile_id)
                    .ok()
                    .flatten()
            })
            .map(|stored| stored.revision.id)
            .unwrap_or(Uuid::nil());
        let turn = ConversationTurn {
            id: turn_id,
            conversation_id: self.conversation.id,
            sequence: next_ordinal / 2 + 1,
            prompt_profile_id: self.prompt_profile_id,
            prompt_profile_revision_id: revision_id,
            kernel_version: KERNEL_VERSION.to_string(),
            kernel_digest: composition.kernel_digest.clone(),
            snapshot_id: repository.map(|repository| repository.snapshot.id),
            response_contract: response_contract.clone(),
            audit_result_id: None,
            started_at: chrono::Utc::now(),
            completed_at: None,
        };
        if let Some(storage) = &self.storage {
            if let Err(error) = storage.begin_turn(&TurnStart {
                turn,
                user_message: user_message.clone(),
                assistant_placeholder: assistant_message.clone(),
            }) {
                self.status = format!("대화 저장 실패: {error}");
                return;
            }
        }
        let mut messages: Vec<AgentMessage> = self
            .conversation
            .messages
            .iter()
            .filter_map(chat_to_agent_message)
            .collect();
        messages.push(AgentMessage::user(user_message.markdown.clone()));

        let repository_request = repository.map(|binding| {
            (
                &binding.snapshot,
                binding.session.profile().display_name.as_str(),
            )
        });
        let request = build_agent_request(
            self.conversation.id,
            turn_id,
            profile.clone(),
            composition.effective_system_prompt,
            messages,
            repository_request,
            response_contract.clone(),
        );
        let trace_id = repository.map(|_| Uuid::new_v4());
        if let (Some(storage), Some(repository), Some(trace_id)) =
            (&self.storage, repository, trace_id)
        {
            let empty_trace = GroundingTrace {
                id: trace_id,
                conversation_id: self.conversation.id,
                turn_id,
                snapshot_id: Some(repository.snapshot.id),
                tool_calls: Vec::new(),
                source_refs: Vec::new(),
                egress_receipt_ids: Vec::new(),
                freshness: GroundingFreshness::FreshAtSend,
            };
            if let Err(error) = storage.prepare_grounding_trace(&empty_trace) {
                let _ = storage.finish_turn(&TurnTerminalUpdate::Failed {
                    turn_id,
                    assistant_message_id: assistant_message.id,
                    error_code: "TOOL_EGRESS_RECEIPT_FAILED".to_string(),
                    safe_message: "Grounding trace를 준비하지 못했습니다.".to_string(),
                    completed_at: chrono::Utc::now(),
                });
                self.status = format!("Grounding 준비 실패: {error}");
                return;
            }
        }
        let cancel = CancellationToken::new();
        let egress_gate = if profile.provider.requires_api_key() {
            match (&self.storage, repository, trace_id) {
                (Some(storage), Some(repository), Some(trace_id)) => {
                    let endpoint = MultiProviderAdapter::agent_endpoint(&profile);
                    let binding = match ProviderBinding::new(
                        profile.id,
                        format!("{:?}", profile.provider),
                        &endpoint,
                        profile.model.clone(),
                    ) {
                        Ok(binding) => binding,
                        Err(error) => {
                            fail_started_turn(
                                storage,
                                turn_id,
                                assistant_message.id,
                                "TOOL_EGRESS_BINDING_INVALID",
                            );
                            self.status = error.to_string();
                            return;
                        }
                    };
                    let scope = RepositoryConsentScope {
                        id: Uuid::new_v4(),
                        conversation_id: self.conversation.id,
                        repository_id: repository.snapshot.repo_id,
                        snapshot_id: repository.snapshot.id,
                        provider_binding: binding,
                        kind: RepositoryConsentKind::RepositorySession,
                        granted_at: chrono::Utc::now(),
                        revoked_at: None,
                    };
                    if let Err(error) = storage.save_repository_consent_scope(&scope) {
                        fail_started_turn(
                            storage,
                            turn_id,
                            assistant_message.id,
                            "TOOL_EGRESS_RECEIPT_FAILED",
                        );
                        self.status = format!("동의 기록 저장 실패: {error}");
                        return;
                    }
                    self.active_consent_scope = Some(scope.id);
                    Some(Arc::new(DurableToolEgressGate::new(
                        storage.clone(),
                        RuntimeConsentCapability::new(scope).with_revocation(cancel.clone()),
                        trace_id,
                    ))
                        as Arc<dyn mentat_inference::ProviderBodyEgressGate>)
                }
                _ if repository.is_some() => {
                    self.status = "cloud tool egress에 durable storage가 필요합니다.".to_string();
                    return;
                }
                _ => None,
            }
        } else {
            None
        };
        self.conversation.messages.push(user_message);
        self.conversation.messages.push(assistant_message.clone());
        self.composer.clear();
        self.status.clear();
        self.stream_cancel = Some(cancel.clone());
        self.active_turn = Some(ActiveTurn {
            turn_id,
            assistant_message_id: assistant_message.id,
            accumulated: String::new(),
            response_contract,
            pending_completion: None,
        });
        let backend = self.backend.clone();
        let tx = self.async_tx.clone();
        let assistant_message_id = assistant_message.id;
        let gateway = repository.map(|repository| repository.gateway.clone());
        let conversation_id = self.conversation.id;
        let stream_storage = self.storage.clone();
        self.runtime.spawn(async move {
            let event_tx = tx.clone();
            let (delta_tx, delta_rx) = mpsc::unbounded_channel();
            let writer = tokio::spawn(crate::stream_store::persist_deltas(
                stream_storage,
                assistant_message_id,
                delta_rx,
            ));
            let mut agent =
                AgentLoop::new(backend, gateway).with_event_sink(Arc::new(move |event| {
                    if matches!(
                        event,
                        AgentEvent::Completed { .. }
                            | AgentEvent::Cancelled { .. }
                            | AgentEvent::Failed { .. }
                    ) {
                        return;
                    }
                    if let AgentEvent::TextDelta(ref text) = event {
                        let _ = delta_tx.send(text.clone());
                    }
                    let _ = event_tx.send(AsyncResult::AgentEvent {
                        conversation_id,
                        turn_id,
                        event,
                    });
                }));
            if let Some(trace_id) = trace_id {
                agent = agent.with_trace_id(trace_id);
            }
            if let Some(gate) = egress_gate {
                agent = agent.with_egress_gate(gate);
            }
            let outcome = agent.run(request, cancel).await;
            drop(agent);
            let stored = writer
                .await
                .map_err(|e| mentat_core::MentatError::IoError(e.to_string()))
                .and_then(|result| result);
            let result = stored.and(outcome).map(|outcome| {
                if let Some(event) = outcome.events.last() {
                    let _ = tx.send(AsyncResult::AgentEvent {
                        conversation_id,
                        turn_id,
                        event: event.clone(),
                    });
                }
                outcome.grounding_trace
            });
            if let Err(error) = &result {
                let _ = tx.send(AsyncResult::AgentEvent {
                    conversation_id,
                    turn_id,
                    event: AgentEvent::Failed {
                        error_code: "AGENT_LOOP_FAILED".to_string(),
                        safe_message: error.to_string(),
                    },
                });
            }
            let _ = tx.send(AsyncResult::AgentFinished {
                assistant_message_id,
                result,
            });
        });
    }

    fn compose_prompt(
        &self,
    ) -> Result<mentat_persona::PromptComposition, mentat_core::MentatError> {
        let catalog = FactoryPromptCatalog::load()?;
        let (revision_id, system_prompt, persona_prompt) = if let Some(storage) = &self.storage {
            let stored = storage
                .load_active_prompt_profile(self.prompt_profile_id)?
                .ok_or_else(|| mentat_core::MentatError::PromptError {
                    code: "PROMPT_PROFILE_NOT_FOUND".to_string(),
                    message: "활성 prompt profile이 없습니다.".to_string(),
                })?;
            (
                stored.revision.id,
                catalog.resolve_source(&stored.system_source)?,
                catalog.resolve_source(&stored.persona_source)?,
            )
        } else {
            (
                Uuid::nil(),
                catalog.system(SystemPreset::Intermediate).to_string(),
                catalog.persona(PersonaKind::DefaultAnalyst).to_string(),
            )
        };
        let repository = self
            .repository
            .as_ref()
            .map(|repository| RepositoryPromptState {
                repository_id: Some(repository.gateway.snapshot().repo_id),
                snapshot_id: Some(repository.gateway.snapshot().id),
                status: Some(repository.gateway.snapshot().status.clone()),
                tools_available: repository.gateway.snapshot().status
                    == mentat_core::SnapshotStatus::Ready
                    && self
                        .provider_setup
                        .active_capabilities()
                        .is_some_and(|capabilities| capabilities.repository_advisor_capable),
            })
            .unwrap_or_else(RepositoryPromptState::none);
        PromptComposer::compose(&PromptCompositionInput {
            profile_revision_id: revision_id,
            system_prompt,
            persona_prompt,
            repository,
        })
    }

    fn poll_async(&mut self) {
        if self
            .stream_cancel
            .as_ref()
            .is_some_and(|token| token.is_cancelled())
        {
            if let Some(scope) = self.active_consent_scope.take() {
                if let Some(storage) = &self.storage {
                    if let Err(error) = storage.revoke_repository_consent(scope) {
                        self.status = error.to_string();
                    }
                }
            }
        }
        if let Some(repository) = self.repository.as_mut() {
            if repository.watcher.poll_changes().unwrap_or(true)
                && repository.snapshot.status != mentat_core::SnapshotStatus::Stale
            {
                repository.gateway.mark_stale();
                repository.snapshot.status = mentat_core::SnapshotStatus::Stale;
                for trace in self
                    .grounding_by_message
                    .values_mut()
                    .filter(|trace| trace.snapshot_id == Some(repository.snapshot.id))
                {
                    trace.freshness = GroundingFreshness::ChangedAfterSend {
                        detected_at: chrono::Utc::now(),
                    };
                }
                if let Some(storage) = &self.storage {
                    if let Err(error) = storage.mark_snapshot_changed(repository.snapshot.id) {
                        self.status = error.to_string();
                    }
                }
                self.repository_egress_approved = false;
                if let Some(token) = &self.stream_cancel {
                    token.cancel();
                }
                self.status =
                    "저장소가 변경되었습니다. 재연결하여 인덱스를 갱신하세요.".to_string();
            }
        }
        while let Ok(result) = self.async_rx.try_recv() {
            match result {
                AsyncResult::RepositoryFolderSelected {
                    conversation_id,
                    generation,
                    path,
                    ctx,
                } => {
                    ctx.send_viewport_cmd(ViewportCommand::WindowLevel(if self.is_pinned {
                        egui::viewport::WindowLevel::AlwaysOnTop
                    } else {
                        egui::viewport::WindowLevel::Normal
                    }));
                    if conversation_id != self.conversation.id || generation != self.scan_generation
                    {
                        continue;
                    }
                    self.repository_busy = false;
                    if let Some(path) = path {
                        self.scan_repository_path(path);
                    }
                }
                AsyncResult::Catalog { requested, result } => {
                    self.provider_busy = false;
                    match result {
                        Ok(catalog) => {
                            let count = catalog.models.len();
                            match self.provider_setup.accept_catalog(&requested, catalog) {
                                Ok(()) => {
                                    self.provider_status =
                                        format!("활성 가능 모델 {count}개를 불러왔습니다.");
                                    if self.auto_connect_profile.as_ref() == Some(&requested) {
                                        if self.provider_setup.draft_profile.model.is_empty() {
                                            self.auto_connect_profile = None;
                                        } else {
                                            self.begin_model_verification();
                                        }
                                    }
                                }
                                Err(error) => self.provider_status = error,
                            }
                        }
                        Err(error) => self.provider_status = error.to_string(),
                    }
                }
                AsyncResult::Verification { requested, result } => {
                    self.provider_busy = false;
                    match result {
                        Ok((verification, capabilities)) => {
                            match self.provider_setup.accept_capability_verification(
                                &requested,
                                verification,
                                capabilities,
                            ) {
                                Ok(()) => {
                                    self.provider_status =
                                        "텍스트 생성 및 프로그램 AI 호환성이 확인되었습니다."
                                            .to_string();
                                    if self.auto_connect_profile.as_ref() == Some(&requested) {
                                        self.auto_connect_profile = None;
                                        match self.provider_setup.activate() {
                                            Ok(()) => {
                                                self.provider_status =
                                                    "이전 AI를 확인하고 다시 연결했습니다.".into()
                                            }
                                            Err(error) => self.provider_status = error,
                                        }
                                    }
                                }
                                Err(error) => self.provider_status = error,
                            }
                        }
                        Err(error) => self.provider_status = error.to_string(),
                    }
                }
                AsyncResult::RepositoryScanned {
                    conversation_id,
                    generation,
                    session,
                    result,
                } => {
                    if conversation_id != self.conversation.id || generation != self.scan_generation
                    {
                        continue;
                    }
                    self.repository_busy = false;
                    self.repository_cancel = None;
                    match result {
                        Ok((snapshot, files)) => {
                            let mut watcher =
                                mentat_repository::RepositoryWatcher::new(session.root_path());
                            watcher.spawn_background();
                            if let Some(storage) = &self.storage {
                                let persisted = storage
                                    .save_recent_repo(session.profile())
                                    .and_then(|_| storage.save_snapshot_meta(&snapshot))
                                    .and_then(|_| {
                                        storage.bind_conversation_repository(
                                            self.conversation.id,
                                            snapshot.repo_id,
                                            snapshot.id,
                                        )
                                    });
                                if let Err(error) = persisted {
                                    self.status = format!("저장소 연결 저장 실패: {error}");
                                    continue;
                                }
                            }
                            self.conversation.repository_id = Some(snapshot.repo_id);
                            self.conversation.active_snapshot_id = Some(snapshot.id);
                            self.repository_egress_approved = false;
                            self.mode = ConversationMode::Advisor;
                            self.repository = Some(RepositoryBinding {
                                watcher,
                                gateway: Arc::new(RepositoryToolGateway::new(
                                    session.clone(),
                                    snapshot.clone(),
                                    files,
                                )),
                                session,
                                snapshot: snapshot.clone(),
                            });
                            self.status = if snapshot.status == mentat_core::SnapshotStatus::Ready {
                                "저장소 연결 완료 · 필요할 때만 읽기 도구를 사용합니다.".to_string()
                            } else {
                                "불완전 snapshot · 재인덱싱 전 저장소 도구가 차단됩니다."
                                    .to_string()
                            };
                        }
                        Err(error) => self.status = error.to_string(),
                    }
                }
                AsyncResult::AgentEvent {
                    conversation_id,
                    turn_id,
                    event,
                } => {
                    if conversation_id == self.conversation.id
                        && self
                            .active_turn
                            .as_ref()
                            .is_some_and(|active| active.turn_id == turn_id)
                    {
                        self.apply_agent_event(event);
                    }
                }
                AsyncResult::AgentFinished {
                    assistant_message_id,
                    result,
                } => {
                    if self
                        .active_turn
                        .as_ref()
                        .is_some_and(|active| active.assistant_message_id == assistant_message_id)
                    {
                        self.finish_agent_outcome(assistant_message_id, result);
                    }
                }
            }
        }
    }

    fn apply_agent_event(&mut self, event: AgentEvent) {
        let Some(active) = self.active_turn.as_mut() else {
            return;
        };
        match event {
            AgentEvent::Started { .. }
            | AgentEvent::ThinkingDelta(_)
            | AgentEvent::UsageUpdate { .. } => {}
            AgentEvent::ToolProgress {
                round,
                completed_calls,
                total_calls,
            } => {
                self.status =
                    format!("저장소 조사 중 · round {round} · {completed_calls}/{total_calls}");
            }
            AgentEvent::TextDelta(delta) => {
                active.accumulated.push_str(&delta);
                if let Some(message) = self
                    .conversation
                    .messages
                    .iter_mut()
                    .find(|message| message.id == active.assistant_message_id)
                {
                    message.markdown.push_str(&delta);
                    message.status = MessageStatus::Streaming;
                }
            }
            AgentEvent::Completed { payload, trace_id } => {
                active.pending_completion = Some((payload, trace_id));
                self.status = "응답과 근거를 원자적으로 저장하는 중…".to_string();
            }
            AgentEvent::Cancelled { payload } => {
                let update = match payload {
                    CancelledPayload::AdvisorPartialMarkdown(partial_markdown) => {
                        if let Some(message) = self
                            .conversation
                            .messages
                            .iter_mut()
                            .find(|message| message.id == active.assistant_message_id)
                        {
                            message.markdown = partial_markdown.clone();
                            message.status = MessageStatus::Cancelled;
                        }
                        TurnTerminalUpdate::AdvisorCancelled {
                            turn_id: active.turn_id,
                            assistant_message_id: active.assistant_message_id,
                            partial_markdown,
                            completed_at: chrono::Utc::now(),
                        }
                    }
                    CancelledPayload::AuditNoContent => {
                        if let Some(message) = self
                            .conversation
                            .messages
                            .iter_mut()
                            .find(|message| message.id == active.assistant_message_id)
                        {
                            message.markdown.clear();
                            message.status = MessageStatus::Cancelled;
                        }
                        TurnTerminalUpdate::AuditCancelled {
                            turn_id: active.turn_id,
                            assistant_message_id: active.assistant_message_id,
                            completed_at: chrono::Utc::now(),
                        }
                    }
                };
                if let Some(storage) = &self.storage {
                    let _ = storage.finish_turn(&update);
                }
                self.status = "요청이 취소되었습니다.".to_string();
                self.active_turn = None;
                self.stream_cancel = None;
            }
            AgentEvent::Failed {
                error_code,
                safe_message,
            } => {
                let cancelled = error_code == "CANCELLED";
                let update = if cancelled
                    && matches!(active.response_contract, ResponseContract::AdvisorMarkdown)
                {
                    TurnTerminalUpdate::AdvisorCancelled {
                        turn_id: active.turn_id,
                        assistant_message_id: active.assistant_message_id,
                        partial_markdown: active.accumulated.clone(),
                        completed_at: chrono::Utc::now(),
                    }
                } else if cancelled {
                    TurnTerminalUpdate::AuditCancelled {
                        turn_id: active.turn_id,
                        assistant_message_id: active.assistant_message_id,
                        completed_at: chrono::Utc::now(),
                    }
                } else {
                    TurnTerminalUpdate::Failed {
                        turn_id: active.turn_id,
                        assistant_message_id: active.assistant_message_id,
                        error_code: error_code.clone(),
                        safe_message: safe_message.clone(),
                        completed_at: chrono::Utc::now(),
                    }
                };
                if let Some(message) = self
                    .conversation
                    .messages
                    .iter_mut()
                    .find(|message| message.id == active.assistant_message_id)
                {
                    message.status = if cancelled {
                        MessageStatus::Cancelled
                    } else {
                        message.markdown = safe_message.clone();
                        MessageStatus::Failed {
                            error_code: error_code.clone(),
                        }
                    };
                }
                if let Some(storage) = &self.storage {
                    let _ = storage.finish_turn(&update);
                }
                self.status = safe_message;
                self.active_turn = None;
                self.stream_cancel = None;
            }
        }
    }

    fn finish_agent_outcome(
        &mut self,
        assistant_message_id: Uuid,
        result: Result<Option<GroundingTrace>, mentat_core::MentatError>,
    ) {
        let Some(mut active) = self.active_turn.take() else {
            return;
        };
        if active.assistant_message_id != assistant_message_id {
            self.status = "완료 outcome의 assistant message 결속이 다릅니다.".to_string();
            self.active_turn = Some(active);
            return;
        }
        let mut trace = match result {
            Ok(trace) => trace,
            Err(error) => {
                self.fail_atomic_completion(&active, "AGENT_LOOP_FAILED", &error.to_string());
                self.stream_cancel = None;
                return;
            }
        };
        if self
            .repository
            .as_ref()
            .is_some_and(|repo| repo.gateway.is_stale())
        {
            if let Some(trace) = trace.as_mut() {
                trace.freshness = GroundingFreshness::ChangedAfterSend {
                    detected_at: chrono::Utc::now(),
                };
            }
        }
        let Some((payload, event_trace_id)) = active.pending_completion.take() else {
            self.fail_atomic_completion(
                &active,
                "AGENT_TERMINAL_MISSING",
                "Agent outcome 전에 completion terminal이 도착하지 않았습니다.",
            );
            self.stream_cancel = None;
            return;
        };
        if event_trace_id != trace.as_ref().map(|trace| trace.id) {
            self.fail_atomic_completion(
                &active,
                "TURN_GROUNDING_BINDING_INVALID",
                "completion terminal과 final GroundingTrace ID가 다릅니다.",
            );
            self.stream_cancel = None;
            return;
        }
        let completed_at = chrono::Utc::now();
        let update = match &payload {
            CompletedPayload::AdvisorMarkdown(markdown) => TurnTerminalUpdate::AdvisorCompleted {
                turn_id: active.turn_id,
                assistant_message_id,
                markdown: markdown.clone(),
                grounding_trace_id: event_trace_id,
                freshness: trace.as_ref().map(|trace| trace.freshness.clone()),
                completed_at,
            },
            CompletedPayload::ValidatedAuditBundle(bundle) => {
                let Some(trace) = trace.as_ref() else {
                    self.fail_atomic_completion(
                        &active,
                        "TURN_GROUNDING_BINDING_INVALID",
                        "Audit completion에 final GroundingTrace가 없습니다.",
                    );
                    self.stream_cancel = None;
                    return;
                };
                TurnTerminalUpdate::AuditCompleted {
                    turn_id: active.turn_id,
                    assistant_message_id,
                    result: bundle.clone(),
                    grounding_trace_id: trace.id,
                    freshness: trace.freshness.clone(),
                    completed_at,
                }
            }
        };
        let persisted = self.storage.as_ref().map_or(Ok(()), |storage| {
            if let Some(trace) = &trace {
                storage.finish_turn_with_grounding(trace, &update)
            } else {
                storage.finish_turn(&update)
            }
        });
        if let Err(error) = persisted {
            self.fail_atomic_completion(
                &active,
                "TURN_GROUNDING_COMMIT_FAILED",
                &error.to_string(),
            );
            self.stream_cancel = None;
            return;
        }
        if let Some(message) = self
            .conversation
            .messages
            .iter_mut()
            .find(|message| message.id == assistant_message_id)
        {
            match payload {
                CompletedPayload::AdvisorMarkdown(markdown) => message.markdown = markdown,
                CompletedPayload::ValidatedAuditBundle(ref bundle) => {
                    message.markdown.clear();
                    self.audit_by_message
                        .insert(assistant_message_id, bundle.clone());
                }
            }
            message.status = MessageStatus::Completed;
            message.grounding_trace_id = event_trace_id;
            message.grounding_freshness = trace.as_ref().map(|trace| trace.freshness.clone());
        }
        if let Some(trace) = trace {
            self.grounding_by_message
                .insert(assistant_message_id, trace);
        }
        self.status.clear();
        self.stream_cancel = None;
    }

    fn fail_atomic_completion(&mut self, active: &ActiveTurn, error_code: &str, message: &str) {
        let safe_message = "응답과 근거를 함께 저장하지 못해 완료 처리를 취소했습니다.";
        if let Some(storage) = &self.storage {
            let _ = storage.finish_turn(&TurnTerminalUpdate::Failed {
                turn_id: active.turn_id,
                assistant_message_id: active.assistant_message_id,
                error_code: error_code.to_string(),
                safe_message: safe_message.to_string(),
                completed_at: chrono::Utc::now(),
            });
        }
        if let Some(chat_message) = self
            .conversation
            .messages
            .iter_mut()
            .find(|chat_message| chat_message.id == active.assistant_message_id)
        {
            chat_message.markdown = safe_message.to_string();
            chat_message.status = MessageStatus::Failed {
                error_code: error_code.to_string(),
            };
        }
        self.status = format!("{safe_message} {message}");
    }

    fn ensure_durable_conversation(&mut self) {
        let Some(storage) = &self.storage else {
            return;
        };
        match storage.create_conversation(&NewConversation {
            repository_id: None,
            active_snapshot_id: None,
            prompt_profile_id: self.prompt_profile_id,
            persistence: ConversationPersistence::Durable,
        }) {
            Ok(conversation) => self.conversation = conversation,
            Err(error) => {
                self.status = format!("대화 생성 실패 · 세션 전용: {error}");
                self.storage = None;
            }
        }
    }

    fn start_new_conversation(&mut self) {
        if let Some(scope) = self.active_consent_scope.take() {
            if let Some(storage) = &self.storage {
                if let Err(error) = storage.revoke_repository_consent(scope) {
                    self.status = error.to_string();
                    return;
                }
            }
        }
        if let Some(token) = &self.stream_cancel {
            token.cancel();
        }
        if let Some(active) = self.active_turn.take() {
            if let Some(storage) = &self.storage {
                let update = TurnTerminalUpdate::Failed {
                    turn_id: active.turn_id,
                    assistant_message_id: active.assistant_message_id,
                    error_code: "CONVERSATION_REPLACED".to_string(),
                    safe_message: "새 대화로 전환하여 이전 요청이 중단되었습니다.".to_string(),
                    completed_at: chrono::Utc::now(),
                };
                if let Err(error) = storage.finish_turn(&update) {
                    self.status = error.to_string();
                    return;
                }
            }
        }
        if let Some(token) = self.repository_cancel.take() {
            token.cancel();
        }
        self.scan_generation = Uuid::new_v4();
        self.repository_busy = false;
        self.repository = None;
        self.stream_cancel = None;
        self.mode = ConversationMode::Advisor;
        self.repository_egress_approved = false;
        self.grounding_by_message.clear();
        self.audit_by_message.clear();
        self.selected_grounding_message = None;
        self.selected_source = None;
        self.status.clear();
        self.conversation = Conversation::new(self.prompt_profile_id, None, None);
        self.ensure_durable_conversation();
    }

    fn active_model_label(&self) -> String {
        let model = self
            .provider_setup
            .active_profile()
            .map(|profile| format!("{} · {}", profile.name, profile.model))
            .unwrap_or_else(|| "AI 미활성 · 설정 필요".to_string());
        let model = match self.provider_setup.active_capabilities() {
            Some(capabilities) if capabilities.repository_advisor_capable => {
                format!("{model} · repo tools")
            }
            Some(capabilities) if capabilities.chat_capable => format!("{model} · chat-only"),
            _ => model,
        };
        self.repository
            .as_ref()
            .map(|repository| {
                format!(
                    "{model} · R/O {}",
                    repository.session.profile().display_name
                )
            })
            .unwrap_or(model)
    }

    fn update_window_preferences(&mut self, ctx: &egui::Context) {
        let current = ctx.input(|input| input.viewport().inner_rect.map(|rect| rect.size()));
        let Some(current) = current else {
            return;
        };
        let clamped = clamp_window_size([current.x, current.y]);
        if size_changed(self.last_window_size, clamped) {
            self.last_window_size = clamped;
            self.size_changed_at = Some(Instant::now());
        }
        if self
            .size_changed_at
            .is_some_and(|changed| changed.elapsed() >= Duration::from_millis(500))
        {
            if let Err(error) = self.persist_window_preferences() {
                self.status = format!("창 크기 저장 실패: {error}");
            }
            self.size_changed_at = None;
        }
    }

    fn persist_window_preferences(&self) -> Result<(), mentat_core::MentatError> {
        if let Some(storage) = &self.storage {
            storage.save_ui_preferences(&UiPreferences {
                width_points: self.last_window_size[0],
                height_points: self.last_window_size[1],
                submit_mode: self.submit_mode,
                always_on_top: self.is_pinned,
                layout_revision: 2,
                updated_at: chrono::Utc::now(),
            })?;
        }
        Ok(())
    }
}

impl eframe::App for MentatChatApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_async();
        self.update_window_preferences(ctx);
        if let Some(visible) = self.global_shortcuts.take_visibility_request() {
            ctx.send_viewport_cmd(ViewportCommand::Visible(visible));
        }
        self.handle_close_requests(ctx);
        if ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
            self.selected_grounding_message = None;
            self.selected_source = None;
            if let Some(token) = &self.stream_cancel {
                token.cancel();
            } else {
                self.settings_open = false;
            }
        }
        self.show_header(ctx);
        if self.settings_open {
            self.show_settings(ctx);
        } else {
            self.show_chat(ctx);
        }
        self.show_dirty_prompt_confirmation(ctx);
        if self.active_turn.is_some() || self.provider_busy || self.repository.is_some() {
            ctx.request_repaint_after(Duration::from_millis(16));
        }
    }
}

impl Drop for MentatChatApp {
    fn drop(&mut self) {
        if let Some(token) = &self.stream_cancel {
            token.cancel();
        }
        if let Some(token) = &self.repository_cancel {
            token.cancel();
        }
        let _ = self.persist_window_preferences();
    }
}

pub fn initial_ui_preferences() -> UiPreferences {
    open_storage()
        .ok()
        .and_then(|storage| storage.load_ui_preferences().ok())
        .map(|mut preferences| {
            let size = clamp_window_size([preferences.width_points, preferences.height_points]);
            preferences.width_points = size[0];
            preferences.height_points = size[1];
            preferences
        })
        .unwrap_or_default()
}

fn open_storage() -> Result<SqliteStorage, mentat_core::MentatError> {
    let app_data = PlatformManager::get_app_data_dir()?;
    SqliteStorage::open(app_data.join("mentat.db"))
}

pub(crate) fn factory_seed(catalog: &FactoryPromptCatalog) -> FactoryPromptSeed {
    let system = catalog.system(SystemPreset::Intermediate);
    let persona = catalog.persona(PersonaKind::DefaultAnalyst);
    FactoryPromptSeed {
        profile_id: DEFAULT_PROFILE_ID,
        profile_name: "기본 멘토".to_string(),
        experience_preset: ExperiencePreset::Intermediate,
        base_system_preset: SystemPreset::Intermediate,
        system_resource_key: SystemPreset::Intermediate.resource_key().to_string(),
        system_resource_version: FACTORY_BUNDLE_VERSION.to_string(),
        system_checksum: catalog.checksum(system),
        persona_resource_key: PersonaKind::DefaultAnalyst.resource_key().to_string(),
        persona_resource_version: FACTORY_BUNDLE_VERSION.to_string(),
        persona_checksum: catalog.checksum(persona),
    }
}

fn chat_to_agent_message(message: &ChatMessage) -> Option<AgentMessage> {
    if !matches!(
        message.status,
        MessageStatus::Completed | MessageStatus::Cancelled
    ) {
        return None;
    }
    match message.role {
        ChatRole::User => Some(AgentMessage::user(message.markdown.clone())),
        ChatRole::Assistant => Some(AgentMessage::assistant(message.markdown.clone())),
    }
}

fn restore_message_projections(
    storage: &SqliteStorage,
    conversation: &Conversation,
    status: &mut String,
) -> (HashMap<Uuid, GroundingTrace>, HashMap<Uuid, AnswerBundle>) {
    let mut grounding = HashMap::new();
    let mut audit = HashMap::new();
    for message in conversation
        .messages
        .iter()
        .filter(|message| message.role == ChatRole::Assistant)
    {
        if let Some(trace_id) = message.grounding_trace_id {
            match storage.load_grounding_trace(trace_id) {
                Ok(Some(trace)) => {
                    grounding.insert(message.id, trace);
                }
                Ok(None) => append_status(status, "저장된 근거를 찾을 수 없습니다."),
                Err(error) => append_status(status, &format!("근거 복원 실패: {error}")),
            }
        }
        match storage.load_audit_result_for_turn(message.turn_id) {
            Ok(Some(result)) => {
                audit.insert(message.id, result);
            }
            Ok(None) => {}
            Err(error) => append_status(status, &format!("Audit 복원 실패: {error}")),
        }
    }
    (grounding, audit)
}

fn fail_started_turn(
    storage: &SqliteStorage,
    turn_id: Uuid,
    assistant_message_id: Uuid,
    error_code: &str,
) {
    let _ = storage.finish_turn(&TurnTerminalUpdate::Failed {
        turn_id,
        assistant_message_id,
        error_code: error_code.to_string(),
        safe_message: "요청 준비 단계가 실패했습니다.".to_string(),
        completed_at: chrono::Utc::now(),
    });
}

pub(crate) fn build_agent_request(
    conversation_id: Uuid,
    turn_id: Uuid,
    profile: mentat_inference::BackendProfile,
    effective_system_prompt: String,
    messages: Vec<AgentMessage>,
    repository: Option<(&RepositorySnapshot, &str)>,
    response_contract: ResponseContract,
) -> AgentRequest {
    let repository_context =
        repository.map(
            |(snapshot, display_name)| mentat_inference::RepositoryContext {
                repository_id: snapshot.repo_id,
                snapshot_id: snapshot.id,
                snapshot_status: snapshot.status.clone(),
                tools_available: snapshot.status == mentat_core::SnapshotStatus::Ready,
                display_name: display_name.to_string(),
            },
        );
    let tools = match repository_context.as_ref() {
        Some(context) if context.tools_available => repository_tool_definitions(),
        Some(_) => repository_tool_definitions()
            .into_iter()
            .filter(|definition| definition.name == "repo_status")
            .collect(),
        None => Vec::new(),
    };
    AgentRequest {
        request_id: Uuid::new_v4(),
        conversation_id,
        turn_id,
        profile,
        effective_system_prompt,
        messages,
        tools,
        repository_context,
        response_contract,
        limits: AgentLimits::default(),
    }
}

fn append_status(status: &mut String, message: &str) {
    if !status.is_empty() {
        status.push('\n');
    }
    status.push_str(message);
}

fn render_message(ui: &mut egui::Ui, message: &ChatMessage, audit: Option<&AnswerBundle>) {
    egui::Frame::none()
        .fill(if message.role == ChatRole::User {
            MentatTheme::BG_CARD
        } else {
            MentatTheme::BG_BASE
        })
        .inner_margin(egui::Margin::same(12.0))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let role = match message.role {
                ChatRole::User => "나",
                ChatRole::Assistant => "MENTAT",
            };
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(role)
                        .strong()
                        .size(13.0)
                        .color(MentatTheme::TEXT_MUTED),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if !message.markdown.is_empty() && ui.small_button("복사").clicked() {
                        ui.ctx().copy_text(message.markdown.clone());
                    }
                });
            });
            if let Some(audit) = audit {
                render_audit_result(ui, audit);
            } else if message.markdown.is_empty() {
                ui.label(RichText::new("응답 준비 중…").italics());
            } else if message.role == ChatRole::Assistant {
                render_markdown(ui, &message.markdown);
            } else {
                ui.add(egui::Label::new(&message.markdown).wrap());
            }
            match &message.status {
                MessageStatus::Cancelled => {
                    ui.label(
                        RichText::new("취소됨")
                            .small()
                            .color(MentatTheme::TEXT_MUTED),
                    );
                }
                MessageStatus::Failed { error_code } => {
                    ui.label(
                        RichText::new(format!("실패 · {error_code}"))
                            .small()
                            .color(MentatTheme::STATUS_ERROR),
                    );
                }
                _ => {}
            }
        });
}

fn render_audit_result(ui: &mut egui::Ui, result: &AnswerBundle) {
    ui.label(RichText::new("검증된 Audit 결과").strong());
    ui.add(egui::Label::new(&result.direct_answer).wrap());
    if !result.claims.is_empty() {
        ui.collapsing(format!("Claims {}", result.claims.len()), |ui| {
            for claim in &result.claims {
                ui.label(format!("{:?} · {}", claim.classification, claim.statement));
            }
        });
    }
    if !result.conflicts.is_empty() {
        ui.collapsing(format!("Conflicts {}", result.conflicts.len()), |ui| {
            for conflict in &result.conflicts {
                ui.label(format!(
                    "{} ↔ {} · {}",
                    conflict.side_a, conflict.side_b, conflict.impact
                ));
            }
        });
    }
}

fn system_preset_label(preset: SystemPreset) -> &'static str {
    match preset {
        SystemPreset::Beginner => "Beginner · 쉬운 설명",
        SystemPreset::Intermediate => "Intermediate · 기본",
        SystemPreset::Professional => "Professional · 구현 중심",
        SystemPreset::Senior => "Senior · 아키텍처 중심",
    }
}

fn persona_selection_from_source(source: &mentat_core::PromptContentSource) -> (PersonaKind, bool) {
    if let mentat_core::PromptContentSource::FactoryRef { resource_key, .. } = source {
        if let Some(persona) = PersonaKind::ALL
            .into_iter()
            .find(|persona| persona.resource_key() == resource_key)
        {
            return (persona, false);
        }
    }
    (PersonaKind::DefaultAnalyst, true)
}

fn submit_mode_label(mode: ComposerSubmitMode) -> &'static str {
    match mode {
        ComposerSubmitMode::EnterSend => "Enter 전송 · Shift+Enter 줄바꿈",
        ComposerSubmitMode::CtrlEnterSend => "Ctrl+Enter 전송 · Enter 줄바꿈",
    }
}

fn clamp_window_size(size: [f32; 2]) -> [f32; 2] {
    let width = if size[0].is_finite() && size[0] > 0.0 {
        size[0]
    } else {
        DEFAULT_WINDOW_SIZE[0]
    };
    let height = if size[1].is_finite() && size[1] > 0.0 {
        size[1]
    } else {
        DEFAULT_WINDOW_SIZE[1]
    };
    [
        width.clamp(MIN_WINDOW_SIZE[0], 8192.0),
        height.clamp(MIN_WINDOW_SIZE[1], 8192.0),
    ]
}

fn size_changed(previous: [f32; 2], current: [f32; 2]) -> bool {
    (previous[0] - current[0]).abs() >= 1.0 || (previous[1] - current[1]).abs() >= 1.0
}

fn composer_should_submit(
    mode: ComposerSubmitMode,
    enter: bool,
    shift: bool,
    ctrl: bool,
    ime_event: bool,
) -> bool {
    if !enter || shift || ime_event {
        return false;
    }
    match mode {
        ComposerSubmitMode::EnterSend => !ctrl,
        ComposerSubmitMode::CtrlEnterSend => ctrl,
    }
}

fn composer_key_events_should_submit(mode: ComposerSubmitMode, events: &[egui::Event]) -> bool {
    let ime = events
        .iter()
        .any(|event| matches!(event, egui::Event::Ime(_)));
    events.iter().any(|event| match event {
        egui::Event::Key {
            key: egui::Key::Enter,
            pressed: true,
            modifiers,
            ..
        } => composer_should_submit(mode, true, modifiers.shift, modifiers.ctrl, ime),
        _ => false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn isolated_app() -> MentatChatApp {
        let (async_tx, async_rx) = mpsc::unbounded_channel();
        MentatChatApp {
            runtime: Arc::new(Runtime::new().unwrap()),
            backend: Arc::new(MultiProviderAdapter::new()),
            storage: None,
            prompt_profile_id: DEFAULT_PROFILE_ID,
            conversation: Conversation::new(DEFAULT_PROFILE_ID, None, None),
            provider_setup: ProviderSetupState::new(Default::default()),
            provider_status: String::new(),
            provider_busy: false,
            credential_controller: CredentialController::native(),
            auto_connect_profile: None,
            remember_api_key: false,
            persona: PersonaKind::DefaultAnalyst,
            persona_is_custom: false,
            base_system_preset: SystemPreset::Intermediate,
            system_prompt_draft: String::new(),
            persona_prompt_draft: String::new(),
            prompt_dirty: false,
            delete_confirmation_open: false,
            repository: None,
            repository_busy: false,
            repository_cancel: None,
            scan_generation: Uuid::new_v4(),
            settings_open: false,
            composer: "second".into(),
            submit_mode: ComposerSubmitMode::EnterSend,
            is_pinned: false,
            async_tx,
            async_rx,
            active_turn: None,
            stream_cancel: None,
            last_window_size: DEFAULT_WINDOW_SIZE,
            size_changed_at: None,
            status: String::new(),
            global_shortcuts: GlobalShortcutController::disabled_for_test(),
            pending_dirty_action: None,
            mode: ConversationMode::Advisor,
            repository_egress_approved: false,
            active_consent_scope: None,
            grounding_by_message: HashMap::new(),
            audit_by_message: HashMap::new(),
            selected_grounding_message: None,
            selected_source: None,
        }
    }

    #[test]
    fn restored_model_activates_only_after_matching_verification() {
        let mut app = isolated_app();
        app.provider_setup.draft_profile.model = "dynamic".into();
        let profile = app.provider_setup.draft_profile.clone();
        app.provider_setup
            .accept_catalog(
                &profile,
                ModelCatalog::from_untrusted(vec![mentat_inference::AvailableModel::new(
                    "dynamic", "Dynamic",
                )]),
            )
            .unwrap();
        app.auto_connect_profile = Some(profile.clone());
        let event = || AsyncResult::Verification {
            requested: profile.clone(),
            result: Ok((
                ModelVerification {
                    compatible: true,
                    message: "ok".into(),
                    latency_ms: None,
                },
                AgentCapabilities {
                    chat_capable: true,
                    native_tool_capable: true,
                    emulated_tool_capable: false,
                    repository_advisor_capable: true,
                },
            )),
        };
        assert!(app.provider_setup.active_profile().is_none());
        app.async_tx.send(event()).unwrap();
        app.poll_async();
        assert_eq!(app.provider_setup.active_profile(), Some(&profile));
        app.provider_setup = ProviderSetupState::new(profile.clone());
        app.auto_connect_profile = Some(profile.clone());
        app.provider_setup.draft_profile.model = "different".into();
        app.async_tx.send(event()).unwrap();
        app.poll_async();
        assert!(app.provider_setup.active_profile().is_none());
    }

    #[test]
    fn native_close_is_blocked_only_for_dirty_edits_and_cancels_clean_tasks() {
        for dirty in [false, true] {
            let mut app = isolated_app();
            app.prompt_dirty = dirty;
            let cancel = CancellationToken::new();
            app.stream_cancel = Some(cancel.clone());
            let ctx = egui::Context::default();
            let mut input = egui::RawInput::default();
            input
                .viewports
                .get_mut(&egui::ViewportId::ROOT)
                .unwrap()
                .events
                .push(egui::ViewportEvent::Close);
            let output = ctx.run(input, |ctx| app.handle_close_requests(ctx));
            let commands = &output.viewport_output[&egui::ViewportId::ROOT].commands;
            assert_eq!(
                commands
                    .iter()
                    .any(|command| matches!(command, ViewportCommand::CancelClose)),
                dirty
            );
            assert_eq!(cancel.is_cancelled(), !dirty);
            if !dirty {
                assert!(commands
                    .iter()
                    .any(|command| matches!(command, ViewportCommand::Close)));
            }
        }
    }

    #[test]
    fn repository_selection_keeps_draft_and_cannot_start_a_turn() {
        let mut app = isolated_app();
        app.repository_busy = true;
        app.composer = "선택한 저장소를 설명해 줘".into();
        app.submit_chat();
        assert!(app.conversation.messages.is_empty());
        assert_eq!(app.composer, "선택한 저장소를 설명해 줘");
        assert!(!app.settings_open);
        assert!(app.active_turn.is_none());
    }

    #[test]
    fn duplicate_submit_and_old_turn_events_cannot_replace_current_message() {
        let mut app = isolated_app();
        let turn_id = Uuid::new_v4();
        let message = ChatMessage::new(
            app.conversation.id,
            turn_id,
            ChatRole::Assistant,
            0,
            "",
            MessageStatus::Pending,
        );
        let message_id = message.id;
        app.conversation.messages.push(message);
        app.active_turn = Some(ActiveTurn {
            turn_id,
            assistant_message_id: message_id,
            accumulated: String::new(),
            response_contract: ResponseContract::AdvisorMarkdown,
            pending_completion: None,
        });
        app.submit_chat();
        assert_eq!(app.active_turn.as_ref().unwrap().turn_id, turn_id);
        assert_eq!(app.composer, "second");
        let stale = Uuid::new_v4();
        app.async_tx
            .send(AsyncResult::AgentEvent {
                conversation_id: app.conversation.id,
                turn_id: stale,
                event: AgentEvent::TextDelta("old".into()),
            })
            .unwrap();
        app.async_tx
            .send(AsyncResult::AgentEvent {
                conversation_id: app.conversation.id,
                turn_id,
                event: AgentEvent::TextDelta("current".into()),
            })
            .unwrap();
        app.poll_async();
        assert_eq!(app.conversation.messages[0].markdown, "current");
        let old_conversation = app.conversation.id;
        app.start_new_conversation();
        app.async_tx
            .send(AsyncResult::AgentEvent {
                conversation_id: old_conversation,
                turn_id,
                event: AgentEvent::Failed {
                    error_code: "old".into(),
                    safe_message: "old".into(),
                },
            })
            .unwrap();
        app.poll_async();
        assert!(app.conversation.messages.is_empty());
        assert!(app.status.is_empty());
    }

    #[test]
    fn vertical_window_defaults_and_invalid_restore_are_bounded() {
        assert_eq!(DEFAULT_WINDOW_SIZE, [560.0, 760.0]);
        assert_eq!(MIN_WINDOW_SIZE, [360.0, 480.0]);
        assert_eq!(clamp_window_size([f32::NAN, -1.0]), DEFAULT_WINDOW_SIZE);
        assert_eq!(clamp_window_size([100.0, 100.0]), MIN_WINDOW_SIZE);
    }

    #[test]
    fn composer_submit_respects_shift_ctrl_and_ime_boundaries() {
        assert!(composer_should_submit(
            ComposerSubmitMode::EnterSend,
            true,
            false,
            false,
            false
        ));
        assert!(!composer_should_submit(
            ComposerSubmitMode::EnterSend,
            true,
            true,
            false,
            false
        ));
        assert!(!composer_should_submit(
            ComposerSubmitMode::EnterSend,
            true,
            false,
            false,
            true
        ));
        assert!(composer_should_submit(
            ComposerSubmitMode::CtrlEnterSend,
            true,
            false,
            true,
            false
        ));
    }

    #[test]
    fn composer_uses_key_modifiers_when_modifier_is_released_in_the_same_frame() {
        let event = |modifiers| egui::Event::Key {
            key: egui::Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        };
        let released = egui::Event::Key {
            key: egui::Key::Enter,
            physical_key: None,
            pressed: false,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        assert!(composer_key_events_should_submit(
            ComposerSubmitMode::CtrlEnterSend,
            &[event(egui::Modifiers::CTRL), released.clone()]
        ));
        assert!(!composer_key_events_should_submit(
            ComposerSubmitMode::EnterSend,
            &[event(egui::Modifiers::SHIFT), released]
        ));
    }

    #[test]
    fn default_chat_path_never_emits_state_based_inner_size_commands() {
        let source = include_str!("chat_app.rs");
        let forbidden = ["ViewportCommand", "InnerSize"].join("::");
        assert!(!source.contains(&forbidden));
    }

    #[test]
    fn persona_selector_is_derived_from_the_persisted_prompt_source() {
        let factory = mentat_core::PromptContentSource::FactoryRef {
            resource_key: PersonaKind::ConciseAuditor.resource_key().to_string(),
            resource_version: FACTORY_BUNDLE_VERSION.to_string(),
            checksum: "checksum".to_string(),
        };
        let custom = mentat_core::PromptContentSource::UserText {
            content: "custom persona".to_string(),
            checksum: "checksum".to_string(),
        };

        assert_eq!(
            persona_selection_from_source(&factory),
            (PersonaKind::ConciseAuditor, false)
        );
        assert_eq!(
            persona_selection_from_source(&custom),
            (PersonaKind::DefaultAnalyst, true)
        );
    }

    #[test]
    fn ready_repository_request_contains_gateway_catalog_and_context() {
        let snapshot = RepositorySnapshot {
            id: Uuid::new_v4(),
            repo_id: Uuid::new_v4(),
            status: mentat_core::SnapshotStatus::Ready,
            file_count: 1,
            total_bytes: 10,
            tree_digest: "root".to_string(),
            created_at: chrono::Utc::now(),
        };
        let request = build_agent_request(
            Uuid::new_v4(),
            Uuid::new_v4(),
            mentat_inference::BackendProfile::default(),
            "system".to_string(),
            vec![AgentMessage::user("구현을 찾아줘")],
            Some((&snapshot, "fixture")),
            ResponseContract::AdvisorMarkdown,
        );

        assert_eq!(request.tools.len(), 6);
        assert_eq!(request.repository_context.unwrap().snapshot_id, snapshot.id);
    }

    #[test]
    fn production_chat_source_uses_agent_loop_instead_of_direct_round_stream() {
        let source = include_str!("chat_app.rs");
        assert!(source.contains("AgentLoop::new"));
        let forbidden = ["backend", ".infer_round_stream(request"].join("");
        assert!(!source.contains(&forbidden));
        assert!(source.contains("AsyncResult::AgentFinished"));
        assert!(source.contains("pending_completion"));
        assert!(source.contains("finish_turn_with_grounding"));
    }

    #[test]
    fn audit_request_keeps_tagged_response_contract() {
        let request = build_agent_request(
            Uuid::new_v4(),
            Uuid::new_v4(),
            mentat_inference::BackendProfile::default(),
            "system".to_string(),
            vec![AgentMessage::user("감사")],
            None,
            ResponseContract::AuditAnswerBundle {
                schema_version: "answer_bundle.v1".to_string(),
            },
        );
        assert!(matches!(
            request.response_contract,
            ResponseContract::AuditAnswerBundle { ref schema_version }
                if schema_version == "answer_bundle.v1"
        ));
    }
}
