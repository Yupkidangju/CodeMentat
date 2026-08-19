use crate::credential_state::CredentialController;
use crate::hotkeys::GlobalShortcutController;
use crate::provider_setup::ProviderSetupState;
use crate::theme::MentatTheme;
use crate::widgets::markdown::render_markdown;
use crate::widgets::settings_panel::SettingsPanel;
use eframe::egui::{self, RichText, ScrollArea, ViewportCommand};
use futures_util::StreamExt;
use mentat_analysis::repository_tools::{repository_tool_definitions, RepositoryToolGateway};
use mentat_core::{
    ChatMessage, ChatRole, ComposerSubmitMode, Conversation, ConversationPersistence,
    ConversationTurn, ExperiencePreset, FileRecord, MessageStatus, NewConversation,
    RepositoryReader, RepositorySnapshot, ResponseContract, SystemPreset, TurnStart,
    TurnTerminalUpdate, UiPreferences,
};
use mentat_inference::{
    AgentCapabilities, AgentLimits, AgentMessage, AgentRequest, InferenceBackend,
    InferenceRoundEvent, ModelCatalog, ModelVerification,
};
use mentat_inference_openai::MultiProviderAdapter;
use mentat_persona::{
    FactoryPromptCatalog, PersonaKind, PromptComposer, PromptCompositionInput,
    RepositoryPromptState, FACTORY_BUNDLE_VERSION, KERNEL_VERSION,
};
use mentat_platform::PlatformManager;
use mentat_repository::{ReadOnlySession, ScanLimits};
use mentat_storage::{FactoryPromptSeed, SqliteStorage};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::runtime::Runtime;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

pub const DEFAULT_WINDOW_SIZE: [f32; 2] = [312.5, 660.0];
pub const MIN_WINDOW_SIZE: [f32; 2] = [240.0, 360.0];
const DEFAULT_PROFILE_ID: Uuid = Uuid::from_u128(0x434f_4445_4d45_4e54_4154_0000_0000_0001);

enum AsyncResult {
    Catalog {
        requested: mentat_inference::BackendProfile,
        result: Result<ModelCatalog, mentat_core::MentatError>,
    },
    Verification {
        requested: mentat_inference::BackendProfile,
        result: Result<(ModelVerification, AgentCapabilities), mentat_core::MentatError>,
    },
    RepositoryScanned {
        session: Arc<ReadOnlySession>,
        result: Result<(RepositorySnapshot, Vec<FileRecord>), mentat_core::MentatError>,
    },
    Agent(InferenceRoundEvent),
}

struct ActiveTurn {
    turn_id: Uuid,
    assistant_message_id: Uuid,
    accumulated: String,
}

struct RepositoryBinding {
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
}

impl MentatChatApp {
    pub fn new(creation_context: &eframe::CreationContext<'_>, runtime: Arc<Runtime>) -> Self {
        MentatTheme::apply(&creation_context.egui_ctx);
        let backend = Arc::new(MultiProviderAdapter::new());
        let (async_tx, async_rx) = mpsc::unbounded_channel();
        let catalog = FactoryPromptCatalog::load().expect("내장 prompt asset 검증 실패");
        let mut status = String::new();
        let storage = match open_storage() {
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
        if let Some(storage) = &storage {
            let _ = storage.seed_factory_prompt_profile(&factory_seed(&catalog));
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

        Self {
            runtime,
            backend,
            storage,
            prompt_profile_id: DEFAULT_PROFILE_ID,
            conversation,
            provider_setup: ProviderSetupState::new(saved_backend),
            provider_status: String::new(),
            provider_busy: false,
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
        }
    }

    fn show_header(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("chat_header")
            .exact_height(46.0)
            .show(ctx, |ui| {
                let drag = ui
                    .horizontal(|ui| {
                        ui.label(RichText::new("MENTAT").strong().size(15.0));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button("×").on_hover_text("닫기 (Ctrl+Q)").clicked() {
                                self.request_dirty_action(DirtyPromptAction::CloseApp, ctx);
                            }
                            if ui.button("⚙").on_hover_text("설정").clicked() {
                                self.settings_open = !self.settings_open;
                            }
                            if ui
                                .button(if self.is_pinned { "◆" } else { "◇" })
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
                            if ui.button("+").on_hover_text("새 대화").clicked() {
                                self.request_dirty_action(DirtyPromptAction::NewConversation, ctx);
                            }
                        });
                    })
                    .response;
                if drag.dragged() {
                    ctx.send_viewport_cmd(ViewportCommand::StartDrag);
                }
                ui.label(
                    RichText::new(self.active_model_label())
                        .size(11.0)
                        .color(MentatTheme::TEXT_MUTED),
                );
            });
    }

    fn show_chat(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("chat_composer")
            .resizable(false)
            .show(ctx, |ui| {
                ui.add_space(4.0);
                let response = ui.add(
                    egui::TextEdit::multiline(&mut self.composer)
                        .desired_rows(3)
                        .hint_text("메시지를 입력하세요. Shift+Enter: 줄바꿈")
                        .lock_focus(true),
                );
                let ime_event = ui.input(|input| {
                    input
                        .events
                        .iter()
                        .any(|event| matches!(event, egui::Event::Ime(_)))
                });
                let (enter, shift, ctrl) = ui.input(|input| {
                    (
                        input.key_pressed(egui::Key::Enter),
                        input.modifiers.shift,
                        input.modifiers.ctrl,
                    )
                });
                let keyboard_submit = response.has_focus()
                    && composer_should_submit(self.submit_mode, enter, shift, ctrl, ime_event);
                ui.horizontal(|ui| {
                    if let Some(active) = &self.active_turn {
                        ui.label(
                            RichText::new(format!(
                                "응답 중 · {}자",
                                active.accumulated.chars().count()
                            ))
                            .small()
                            .color(MentatTheme::STATUS_INFERENCING),
                        );
                        if ui.button("취소").clicked() {
                            if let Some(token) = &self.stream_cancel {
                                token.cancel();
                            }
                        }
                    } else {
                        ui.label(RichText::new("읽기 전용 멘토").small());
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let send = ui
                            .add_enabled(
                                self.active_turn.is_none() && !self.composer.trim().is_empty(),
                                egui::Button::new("전송"),
                            )
                            .clicked();
                        if send || keyboard_submit {
                            self.submit_chat();
                        }
                    });
                });
                if !self.status.is_empty() {
                    ui.label(
                        RichText::new(&self.status)
                            .small()
                            .color(MentatTheme::STATUS_CONFLICT),
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
                        ui.add_space(24.0);
                        ui.heading("무엇을 같이 살펴볼까요?");
                        ui.label("저장소가 없어도 자유롭게 대화할 수 있습니다.");
                        ui.add_space(8.0);
                        if self.repository_busy {
                            ui.label("저장소를 읽기 전용으로 인덱싱하는 중…");
                        } else if self.repository.is_none() && ui.button("저장소 연결").clicked()
                        {
                            self.begin_repository_scan();
                        }
                    }
                    if let Some(repository) = &self.repository {
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(format!(
                                    "R/O · {}",
                                    repository.session.profile().display_name
                                ))
                                .small()
                                .color(MentatTheme::STATUS_READ_ONLY),
                            );
                            ui.label(
                                RichText::new(format!(
                                    "{} files · {:?}",
                                    repository.snapshot.file_count, repository.snapshot.status
                                ))
                                .small()
                                .color(MentatTheme::TEXT_MUTED),
                            );
                        });
                    }
                    for message in &self.conversation.messages {
                        render_message(ui, message);
                        ui.add_space(10.0);
                    }
                });
        });
    }

    fn show_settings(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ScrollArea::vertical().show(ui, |ui| {
                let stage = self.provider_setup.stage();
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
                self.show_prompt_settings(ui);
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
            self.persona_is_custom = true;
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

    fn begin_repository_scan(&mut self) {
        let Some(path) = PlatformManager::pick_folder() else {
            return;
        };
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
        self.repository_cancel = Some(cancel.clone());
        self.repository_busy = true;
        self.status = "저장소를 읽기 전용으로 인덱싱하는 중…".to_string();
        let tx = self.async_tx.clone();
        let session_for_task = session.clone();
        self.runtime.spawn(async move {
            let result = session_for_task
                .scan_files_with_limits(ScanLimits::default(), cancel)
                .await
                .map(|outcome| {
                    let snapshot = session_for_task.create_snapshot_from_outcome(&outcome);
                    (snapshot, outcome.files)
                });
            let _ = tx.send(AsyncResult::RepositoryScanned { session, result });
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
        let Some(profile) = self.provider_setup.active_profile().cloned() else {
            self.status = "설정에서 공급자 모델을 확인하고 활성화해 주세요.".to_string();
            self.settings_open = true;
            return;
        };
        let question = self.composer.trim().to_string();
        if question.is_empty() {
            return;
        }
        if self.conversation.messages.is_empty() && self.storage.is_some() {
            self.ensure_durable_conversation();
        }
        let composition = match self.compose_prompt() {
            Ok(value) => value,
            Err(error) => {
                self.status = error.to_string();
                return;
            }
        };
        let turn_id = Uuid::new_v4();
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
            snapshot_id: None,
            response_contract: ResponseContract::AdvisorMarkdown,
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
        self.conversation.messages.push(user_message);
        self.conversation.messages.push(assistant_message.clone());
        self.composer.clear();
        self.status.clear();

        let repository = self.repository.as_ref().map(|binding| {
            (
                &binding.snapshot,
                binding.session.profile().display_name.as_str(),
            )
        });
        let request = build_agent_request(
            self.conversation.id,
            turn_id,
            profile,
            composition.effective_system_prompt,
            messages,
            repository,
            ResponseContract::AdvisorMarkdown,
        );
        let cancel = CancellationToken::new();
        self.stream_cancel = Some(cancel.clone());
        self.active_turn = Some(ActiveTurn {
            turn_id,
            assistant_message_id: assistant_message.id,
            accumulated: String::new(),
        });
        let backend = self.backend.clone();
        let tx = self.async_tx.clone();
        self.runtime.spawn(async move {
            match backend.infer_round_stream(request, cancel).await {
                Ok(mut stream) => {
                    while let Some(event) = stream.next().await {
                        let terminal = matches!(
                            event,
                            InferenceRoundEvent::RawCompleted { .. }
                                | InferenceRoundEvent::Failed { .. }
                        );
                        let _ = tx.send(AsyncResult::Agent(event));
                        if terminal {
                            break;
                        }
                    }
                }
                Err(error) => {
                    let _ = tx.send(AsyncResult::Agent(InferenceRoundEvent::Failed {
                        error_code: "CHAT_BACKEND_ERROR".to_string(),
                        safe_message: error.to_string(),
                    }));
                }
            }
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
                tools_available: false,
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
        while let Ok(result) = self.async_rx.try_recv() {
            match result {
                AsyncResult::Catalog { requested, result } => {
                    self.provider_busy = false;
                    match result {
                        Ok(catalog) => {
                            let count = catalog.models.len();
                            match self.provider_setup.accept_catalog(&requested, catalog) {
                                Ok(()) => {
                                    self.provider_status =
                                        format!("활성 가능 모델 {count}개를 불러왔습니다.")
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
                                            .to_string()
                                }
                                Err(error) => self.provider_status = error,
                            }
                        }
                        Err(error) => self.provider_status = error.to_string(),
                    }
                }
                AsyncResult::RepositoryScanned { session, result } => {
                    self.repository_busy = false;
                    self.repository_cancel = None;
                    match result {
                        Ok((snapshot, files)) => {
                            if let Some(storage) = &self.storage {
                                let _ = storage.save_recent_repo(session.profile());
                                let _ = storage.save_snapshot_meta(&snapshot);
                                let _ = storage.bind_conversation_repository(
                                    self.conversation.id,
                                    snapshot.repo_id,
                                    snapshot.id,
                                );
                            }
                            self.conversation.repository_id = Some(snapshot.repo_id);
                            self.conversation.active_snapshot_id = Some(snapshot.id);
                            self.repository = Some(RepositoryBinding {
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
                AsyncResult::Agent(event) => self.apply_agent_event(event),
            }
        }
    }

    fn apply_agent_event(&mut self, event: InferenceRoundEvent) {
        let Some(active) = self.active_turn.as_mut() else {
            return;
        };
        match event {
            InferenceRoundEvent::Started { .. }
            | InferenceRoundEvent::ThinkingDelta(_)
            | InferenceRoundEvent::UsageUpdate { .. } => {}
            InferenceRoundEvent::TextDelta(delta) => {
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
                if let Some(storage) = &self.storage {
                    if let Err(error) =
                        storage.append_assistant_delta(active.assistant_message_id, &delta)
                    {
                        self.status = format!("stream 저장 실패: {error}");
                    }
                }
            }
            InferenceRoundEvent::RawCompleted { full_text } => {
                let update = TurnTerminalUpdate::AdvisorCompleted {
                    turn_id: active.turn_id,
                    assistant_message_id: active.assistant_message_id,
                    markdown: full_text.clone(),
                    grounding_trace_id: None,
                    freshness: None,
                    completed_at: chrono::Utc::now(),
                };
                if let Some(message) = self
                    .conversation
                    .messages
                    .iter_mut()
                    .find(|message| message.id == active.assistant_message_id)
                {
                    message.markdown = full_text;
                    message.status = MessageStatus::Completed;
                }
                if let Some(storage) = &self.storage {
                    if let Err(error) = storage.finish_turn(&update) {
                        self.status = format!("완료 상태 저장 실패: {error}");
                    }
                }
                self.active_turn = None;
                self.stream_cancel = None;
            }
            InferenceRoundEvent::Failed {
                error_code,
                safe_message,
            } => {
                let cancelled = error_code == "CANCELLED";
                let update = if cancelled {
                    TurnTerminalUpdate::AdvisorCancelled {
                        turn_id: active.turn_id,
                        assistant_message_id: active.assistant_message_id,
                        partial_markdown: active.accumulated.clone(),
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
            InferenceRoundEvent::ToolCallsRequested { .. } => {
                self.status =
                    "chat-only 단계에서 예기치 않은 tool 요청을 차단했습니다.".to_string();
                if let Some(token) = &self.stream_cancel {
                    token.cancel();
                }
            }
        }
    }

    fn ensure_durable_conversation(&mut self) {
        let Some(storage) = &self.storage else {
            return;
        };
        if let Ok(conversation) = storage.create_conversation(&NewConversation {
            repository_id: None,
            active_snapshot_id: None,
            prompt_profile_id: self.prompt_profile_id,
            persistence: ConversationPersistence::Durable,
        }) {
            self.conversation = conversation;
        }
    }

    fn start_new_conversation(&mut self) {
        if let Some(token) = &self.stream_cancel {
            token.cancel();
        }
        self.active_turn = None;
        self.stream_cancel = None;
        self.conversation = self
            .storage
            .as_ref()
            .and_then(|storage| {
                storage
                    .create_conversation(&NewConversation {
                        repository_id: None,
                        active_snapshot_id: None,
                        prompt_profile_id: self.prompt_profile_id,
                        persistence: ConversationPersistence::Durable,
                    })
                    .ok()
            })
            .unwrap_or_else(|| Conversation::new(self.prompt_profile_id, None, None));
        self.status.clear();
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
        let close = ctx.input(|input| input.modifiers.ctrl && input.key_pressed(egui::Key::Q));
        if close {
            self.request_dirty_action(DirtyPromptAction::CloseApp, ctx);
        }
        if ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
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
        if self.active_turn.is_some() || self.provider_busy {
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

fn factory_seed(catalog: &FactoryPromptCatalog) -> FactoryPromptSeed {
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

fn build_agent_request(
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

fn render_message(ui: &mut egui::Ui, message: &ChatMessage) {
    ui.group(|ui| {
        let role = match message.role {
            ChatRole::User => "나",
            ChatRole::Assistant => "MENTAT",
        };
        ui.label(RichText::new(role).strong().size(12.0));
        if message.markdown.is_empty() {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vertical_window_defaults_and_invalid_restore_are_bounded() {
        assert_eq!(DEFAULT_WINDOW_SIZE, [312.5, 660.0]);
        assert_eq!(MIN_WINDOW_SIZE, [240.0, 360.0]);
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
}
