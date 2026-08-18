use crate::provider_setup::ProviderSetupStage;
use crate::theme::MentatTheme;
use egui::{vec2, ComboBox, RichText, Rounding, Stroke, Ui};
use mentat_inference::{AvailableModel, BackendProfile, ProviderKind};
use mentat_persona::PersonaKind;

pub struct SettingsPanelAction {
    pub discover_clicked: bool,
    pub verify_clicked: bool,
    pub activate_clicked: bool,
    pub selected_model: Option<String>,
    pub close_clicked: bool,
}

pub struct SettingsPanel<'a> {
    pub profile: &'a mut BackendProfile,
    pub persona: &'a mut PersonaKind,
    pub available_models: &'a [AvailableModel],
    pub stage: ProviderSetupStage,
    pub provider_status: &'a str,
    pub is_busy: bool,
}

impl<'a> SettingsPanel<'a> {
    pub fn new(
        profile: &'a mut BackendProfile,
        persona: &'a mut PersonaKind,
        available_models: &'a [AvailableModel],
        stage: ProviderSetupStage,
        provider_status: &'a str,
        is_busy: bool,
    ) -> Self {
        Self {
            profile,
            persona,
            available_models,
            stage,
            provider_status,
            is_busy,
        }
    }

    pub fn show(self, ui: &mut Ui) -> SettingsPanelAction {
        let mut action = SettingsPanelAction {
            discover_clicked: false,
            verify_clicked: false,
            activate_clicked: false,
            selected_model: None,
            close_clicked: false,
        };

        ui.group(|ui| {
            ui.horizontal(|ui| {
                ui.heading(
                    RichText::new("추론 백엔드 및 페르소나 설정")
                        .size(13.5)
                        .strong(),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("닫기").clicked() {
                        action.close_clicked = true;
                    }
                });
            });

            ui.add_space(6.0);

            // Persona Selection
            ui.horizontal(|ui| {
                ui.label(RichText::new("페르소나 (Persona):").size(12.0).strong());
                ComboBox::from_id_salt("persona_select")
                    .selected_text(self.persona.display_name())
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            self.persona,
                            PersonaKind::DefaultAnalyst,
                            PersonaKind::DefaultAnalyst.display_name(),
                        );
                        ui.selectable_value(
                            self.persona,
                            PersonaKind::MesugakiAnnouncer,
                            PersonaKind::MesugakiAnnouncer.display_name(),
                        );
                        ui.selectable_value(
                            self.persona,
                            PersonaKind::ConciseAuditor,
                            PersonaKind::ConciseAuditor.display_name(),
                        );
                    });
            });

            ui.add_space(4.0);
            ui.separator();
            ui.add_space(4.0);

            // Provider Dropdown
            let prev_provider = self.profile.provider;
            ui.horizontal(|ui| {
                ui.label(RichText::new("공급자 (Provider):").size(12.0).strong());
                ComboBox::from_id_salt("provider_select")
                    .selected_text(match self.profile.provider {
                        ProviderKind::GoogleGemini => "Google Gemini (AI Studio)",
                        ProviderKind::OpenRouter => "OpenRouter (Multi-Model)",
                        ProviderKind::OpenAi | ProviderKind::OpenAICompatible => {
                            "OpenAI (Official API)"
                        }
                        ProviderKind::CustomCompatible => "Custom OpenAI-Compatible",
                        ProviderKind::LocalMock => "내장 로컬",
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut self.profile.provider,
                            ProviderKind::GoogleGemini,
                            "Google Gemini (AI Studio)",
                        );
                        ui.selectable_value(
                            &mut self.profile.provider,
                            ProviderKind::OpenRouter,
                            "OpenRouter (Multi-Model)",
                        );
                        ui.selectable_value(
                            &mut self.profile.provider,
                            ProviderKind::OpenAi,
                            "OpenAI (Official API)",
                        );
                        ui.selectable_value(
                            &mut self.profile.provider,
                            ProviderKind::CustomCompatible,
                            "Custom OpenAI-Compatible",
                        );
                        ui.selectable_value(
                            &mut self.profile.provider,
                            ProviderKind::LocalMock,
                            "내장 로컬",
                        );
                    });
            });

            // 공급자 변경 시 엔드포인트만 초기화하고 모델은 반드시 동적으로 다시 검색한다.
            if self.profile.provider != prev_provider {
                self.profile.base_url = self.profile.provider.default_base_url().to_string();
                self.profile.model.clear();
            }

            ui.add_space(4.0);

            // 공급자에서 검색해 검증한 모델 목록만 선택지로 표시한다.
            ui.horizontal(|ui| {
                ui.label(RichText::new("모델 (Model):").size(12.0));
                let mut selected_model = self.profile.model.clone();
                ui.add_enabled_ui(!self.available_models.is_empty() && !self.is_busy, |ui| {
                    ComboBox::from_id_salt("model_select")
                        .selected_text(if selected_model.is_empty() {
                            "모델을 선택하세요"
                        } else {
                            &selected_model
                        })
                        .show_ui(ui, |ui| {
                            for model in self.available_models {
                                ui.selectable_value(
                                    &mut selected_model,
                                    model.id.clone(),
                                    &model.display_name,
                                );
                            }
                        });
                });
                if selected_model != self.profile.model {
                    action.selected_model = Some(selected_model);
                }
            });

            ui.add_space(4.0);

            // Custom Base URL
            ui.horizontal(|ui| {
                ui.label(RichText::new("Base URL:").size(12.0));
                ui.add_sized(
                    vec2(ui.available_width() - 20.0, 24.0),
                    egui::TextEdit::singleline(&mut self.profile.base_url)
                        .font(egui::FontId::monospace(11.5)),
                );
            });

            ui.add_space(4.0);

            // API 키는 세션 메모리에만 유지하며 내장 로컬에는 요구하지 않는다.
            ui.horizontal(|ui| {
                ui.label(RichText::new("API Key:").size(12.0));
                if self.profile.provider.requires_api_key() {
                    let mut current_key = self.profile.api_key.clone().unwrap_or_default();
                    let resp = ui.add_sized(
                        vec2(ui.available_width() - 20.0, 24.0),
                        egui::TextEdit::singleline(&mut current_key)
                            .password(true)
                            .hint_text("API 키를 입력하세요...")
                            .font(egui::FontId::monospace(11.5)),
                    );
                    if resp.changed() {
                        self.profile.api_key = if current_key.trim().is_empty() {
                            None
                        } else {
                            Some(current_key.trim().to_string())
                        };
                    }
                } else {
                    ui.label(RichText::new("내장 로컬은 API 키가 필요하지 않습니다.").size(11.5));
                }
            });

            ui.add_space(6.0);
            ui.horizontal_wrapped(|ui| {
                let key_ready = !self.profile.provider.requires_api_key()
                    || self
                        .profile
                        .api_key
                        .as_ref()
                        .is_some_and(|key| !key.trim().is_empty());
                if ui
                    .add_enabled(
                        !self.is_busy && key_ready,
                        egui::Button::new("1. API 확인 및 모델 불러오기")
                            .fill(MentatTheme::BG_CARD)
                            .stroke(Stroke::new(1.0, MentatTheme::STATUS_INFERENCING))
                            .rounding(Rounding::same(6.0)),
                    )
                    .clicked()
                {
                    action.discover_clicked = true;
                }
                if ui
                    .add_enabled(
                        !self.is_busy
                            && !self.available_models.is_empty()
                            && !self.profile.model.is_empty(),
                        egui::Button::new("2. 선택 모델 호환성 확인")
                            .fill(MentatTheme::BG_CARD)
                            .rounding(Rounding::same(6.0)),
                    )
                    .clicked()
                {
                    action.verify_clicked = true;
                }
                if ui
                    .add_enabled(
                        !self.is_busy && self.stage == ProviderSetupStage::ModelVerified,
                        egui::Button::new("3. 활성화")
                            .fill(MentatTheme::BG_CARD)
                            .stroke(Stroke::new(1.0, MentatTheme::STATUS_READ_ONLY))
                            .rounding(Rounding::same(6.0)),
                    )
                    .clicked()
                {
                    action.activate_clicked = true;
                }
            });

            ui.add_space(4.0);
            let stage_text = match self.stage {
                ProviderSetupStage::Draft => "상태: Draft — API와 모델 목록을 확인하세요.",
                ProviderSetupStage::ModelsDiscovered => {
                    "상태: ModelsDiscovered — 모델을 선택하고 호환성을 확인하세요."
                }
                ProviderSetupStage::ModelVerified => {
                    "상태: ModelVerified — 현재 검증된 설정을 활성화할 수 있습니다."
                }
                ProviderSetupStage::Active => "상태: Active — 현재 프로그램 AI로 사용 중입니다.",
            };
            ui.label(
                RichText::new(stage_text)
                    .color(MentatTheme::TEXT_MUTED)
                    .size(11.5),
            );
            if !self.provider_status.is_empty() {
                ui.label(
                    RichText::new(self.provider_status)
                        .color(if self.stage == ProviderSetupStage::Active {
                            MentatTheme::STATUS_READ_ONLY
                        } else {
                            MentatTheme::STATUS_CONFLICT
                        })
                        .size(11.5),
                );
            }
        });

        action
    }
}
