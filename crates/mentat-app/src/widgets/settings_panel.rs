use crate::provider_setup::ProviderSetupStage;
use crate::theme::MentatTheme;
use egui::{ComboBox, RichText, Ui};
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
    pub persona_is_custom: bool,
    pub remember_api_key: &'a mut bool,
    pub available_models: &'a [AvailableModel],
    pub stage: ProviderSetupStage,
    pub provider_status: &'a str,
    pub is_busy: bool,
}

impl<'a> SettingsPanel<'a> {
    pub fn show(self, ui: &mut Ui) -> SettingsPanelAction {
        let mut action = SettingsPanelAction {
            discover_clicked: false,
            verify_clicked: false,
            activate_clicked: false,
            selected_model: None,
            close_clicked: false,
        };

        ui.horizontal(|ui| {
            ui.heading(RichText::new("AI 설정").size(17.0).strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("닫기").clicked() {
                    action.close_clicked = true;
                }
            });
        });
        ui.label(
            RichText::new("공급자 → 모델 조회 → 호환성 확인 → 활성화")
                .small()
                .color(MentatTheme::TEXT_MUTED),
        );
        ui.add_space(8.0);

        ui.label(RichText::new("응답 스타일").strong());
        ComboBox::from_id_salt("persona_select")
            .width(ui.available_width())
            .selected_text(if self.persona_is_custom {
                "사용자 정의 Persona"
            } else {
                self.persona.display_name()
            })
            .show_ui(ui, |ui| {
                for persona in PersonaKind::ALL {
                    ui.selectable_value(self.persona, persona, persona.display_name());
                }
            });

        ui.add_space(8.0);
        ui.separator();
        ui.add_space(8.0);

        ui.label(RichText::new("공급자").strong());
        let previous_provider = self.profile.provider;
        ComboBox::from_id_salt("provider_select")
            .width(ui.available_width())
            .selected_text(provider_label(self.profile.provider))
            .show_ui(ui, |ui| {
                for provider in [
                    ProviderKind::GoogleGemini,
                    ProviderKind::OpenRouter,
                    ProviderKind::OpenAi,
                    ProviderKind::CustomCompatible,
                    ProviderKind::LocalMock,
                ] {
                    ui.selectable_value(
                        &mut self.profile.provider,
                        provider,
                        provider_label(provider),
                    );
                }
            });
        if self.profile.provider != previous_provider {
            self.profile.base_url = self.profile.provider.default_base_url().to_string();
            self.profile.model.clear();
            self.profile.api_key = None;
            *self.remember_api_key = false;
        }

        ui.add_space(6.0);
        ui.label(RichText::new("Base URL").strong());
        ui.add_sized(
            [ui.available_width(), 28.0],
            egui::TextEdit::singleline(&mut self.profile.base_url)
                .font(egui::FontId::monospace(12.0)),
        );

        ui.add_space(6.0);
        ui.label(RichText::new("API Key").strong());
        if self.profile.provider.requires_api_key() {
            let mut current_key = self.profile.api_key.clone().unwrap_or_default();
            let response = ui.add_sized(
                [ui.available_width(), 28.0],
                egui::TextEdit::singleline(&mut current_key)
                    .password(true)
                    .hint_text("세션에만 보관")
                    .font(egui::FontId::monospace(12.0)),
            );
            if response.changed() {
                self.profile.api_key = if current_key.trim().is_empty() {
                    None
                } else {
                    Some(current_key.trim().to_string())
                };
            }
            ui.checkbox(
                self.remember_api_key,
                "이 기기에 안전하게 저장 (OS 자격 증명 저장소)",
            );
            ui.label(
                RichText::new("SQLite에는 key 원문이 저장되지 않습니다.")
                    .small()
                    .color(MentatTheme::TEXT_MUTED),
            );
        } else {
            ui.label("내장 로컬은 API 키가 필요하지 않습니다.");
            *self.remember_api_key = false;
        }

        ui.add_space(6.0);
        ui.label(RichText::new("모델").strong());
        let mut selected_model = self.profile.model.clone();
        ui.add_enabled_ui(!self.available_models.is_empty() && !self.is_busy, |ui| {
            ComboBox::from_id_salt("model_select")
                .width(ui.available_width())
                .selected_text(if selected_model.is_empty() {
                    "모델을 먼저 불러오세요"
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

        ui.add_space(10.0);
        let key_ready = !self.profile.provider.requires_api_key()
            || self
                .profile
                .api_key
                .as_ref()
                .is_some_and(|key| !key.trim().is_empty());
        if ui
            .add_enabled(
                !self.is_busy && key_ready,
                egui::Button::new("1 · API 확인 및 모델 불러오기")
                    .min_size(egui::vec2(ui.available_width(), 32.0)),
            )
            .on_disabled_hover_text("API 키를 입력해야 합니다.")
            .clicked()
        {
            action.discover_clicked = true;
        }
        if ui
            .add_enabled(
                !self.is_busy
                    && !self.available_models.is_empty()
                    && !self.profile.model.is_empty(),
                egui::Button::new("2 · 선택 모델 호환성 확인")
                    .min_size(egui::vec2(ui.available_width(), 32.0)),
            )
            .clicked()
        {
            action.verify_clicked = true;
        }
        if ui
            .add_enabled(
                !self.is_busy && self.stage == ProviderSetupStage::ModelVerified,
                egui::Button::new("3 · 프로그램 AI로 활성화")
                    .min_size(egui::vec2(ui.available_width(), 32.0)),
            )
            .clicked()
        {
            action.activate_clicked = true;
        }

        ui.add_space(8.0);
        ui.label(RichText::new(stage_label(self.stage)).small().color(
            if self.stage == ProviderSetupStage::Active {
                MentatTheme::STATUS_READ_ONLY
            } else {
                MentatTheme::TEXT_MUTED
            },
        ));
        if !self.provider_status.is_empty() {
            ui.label(
                RichText::new(self.provider_status)
                    .small()
                    .color(MentatTheme::STATUS_CONFLICT),
            );
        }

        action
    }
}

fn provider_label(provider: ProviderKind) -> &'static str {
    match provider {
        ProviderKind::GoogleGemini => "Google Gemini",
        ProviderKind::OpenRouter => "OpenRouter",
        ProviderKind::OpenAi | ProviderKind::OpenAICompatible => "OpenAI",
        ProviderKind::CustomCompatible => "OpenAI 호환 사용자 지정",
        ProviderKind::LocalMock => "내장 로컬",
    }
}

fn stage_label(stage: ProviderSetupStage) -> &'static str {
    match stage {
        ProviderSetupStage::Draft => "Draft · API와 모델 목록을 확인하세요.",
        ProviderSetupStage::ModelsDiscovered => "ModelsDiscovered · 모델을 선택하세요.",
        ProviderSetupStage::ModelVerified => "ModelVerified · 활성화할 수 있습니다.",
        ProviderSetupStage::Active => "Active · 현재 프로그램 AI입니다.",
    }
}
