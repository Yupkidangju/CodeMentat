use crate::theme::MentatTheme;
use egui::{vec2, ComboBox, RichText, Rounding, Stroke, Ui};
use mentat_inference::{BackendProfile, ProviderKind};
use mentat_persona::PersonaKind;

pub struct SettingsPanelAction {
    pub ping_clicked: bool,
    pub close_clicked: bool,
}

pub struct SettingsPanel<'a> {
    pub profile: &'a mut BackendProfile,
    pub persona: &'a mut PersonaKind,
    pub ping_status: &'a str,
    pub is_testing: bool,
}

impl<'a> SettingsPanel<'a> {
    pub fn new(
        profile: &'a mut BackendProfile,
        persona: &'a mut PersonaKind,
        ping_status: &'a str,
        is_testing: bool,
    ) -> Self {
        Self {
            profile,
            persona,
            ping_status,
            is_testing,
        }
    }

    pub fn show(self, ui: &mut Ui) -> SettingsPanelAction {
        let mut action = SettingsPanelAction {
            ping_clicked: false,
            close_clicked: false,
        };

        ui.group(|ui| {
            ui.horizontal(|ui| {
                ui.heading(
                    RichText::new("⚙️ 추론 백엔드 및 페르소나 설정")
                        .size(13.5)
                        .strong(),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("✖ 닫기").clicked() {
                        action.close_clicked = true;
                    }
                });
            });

            ui.add_space(6.0);

            // Persona Selection
            ui.horizontal(|ui| {
                ui.label(RichText::new("🎭 페르소나 (Persona):").size(12.0).strong());
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
                        ProviderKind::GoogleGemini => "🌟 Google Gemini (AI Studio)",
                        ProviderKind::OpenRouter => "🚀 OpenRouter (Multi-Model)",
                        ProviderKind::OpenAi => "⚡ OpenAI (Official API)",
                        ProviderKind::CustomCompatible => "🔌 Custom / Local (vLLM/Ollama)",
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut self.profile.provider,
                            ProviderKind::GoogleGemini,
                            "🌟 Google Gemini (AI Studio)",
                        );
                        ui.selectable_value(
                            &mut self.profile.provider,
                            ProviderKind::OpenRouter,
                            "🚀 OpenRouter (Multi-Model)",
                        );
                        ui.selectable_value(
                            &mut self.profile.provider,
                            ProviderKind::OpenAi,
                            "⚡ OpenAI (Official API)",
                        );
                        ui.selectable_value(
                            &mut self.profile.provider,
                            ProviderKind::CustomCompatible,
                            "🔌 Custom / Local (vLLM/Ollama)",
                        );
                    });
            });

            // If provider changed, auto-update base_url and default model
            if self.profile.provider != prev_provider {
                self.profile.base_url = self.profile.provider.default_base_url().to_string();
                if let Some(first_model) = self.profile.provider.default_models().first() {
                    self.profile.model = first_model.to_string();
                }
            }

            ui.add_space(4.0);

            // Model Selection / Presets
            ui.horizontal(|ui| {
                ui.label(RichText::new("모델 (Model):").size(12.0));
                ComboBox::from_id_salt("model_select")
                    .selected_text(&self.profile.model)
                    .show_ui(ui, |ui| {
                        for model in self.profile.provider.default_models() {
                            ui.selectable_value(&mut self.profile.model, model.to_string(), *model);
                        }
                    });
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

            // API Key input (masked)
            ui.horizontal(|ui| {
                ui.label(RichText::new("API Key:").size(12.0));
                let mut current_key = self.profile.api_key.clone().unwrap_or_default();
                let resp = ui.add_sized(
                    vec2(ui.available_width() - 120.0, 24.0),
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

                let ping_btn_label = if self.is_testing {
                    "⏳ 시험 중..."
                } else {
                    "📡 Ping 시험"
                };
                let ping_btn = egui::Button::new(RichText::new(ping_btn_label).size(11.5))
                    .fill(MentatTheme::BG_CARD)
                    .stroke(Stroke::new(1.0, MentatTheme::STATUS_INFERENCING))
                    .rounding(Rounding::same(6.0));

                if ui.add_enabled(!self.is_testing, ping_btn).clicked() {
                    action.ping_clicked = true;
                }
            });

            if !self.ping_status.is_empty() {
                ui.add_space(4.0);
                let color = if self.ping_status.contains("성공") {
                    MentatTheme::STATUS_READ_ONLY
                } else {
                    MentatTheme::STATUS_CONFLICT
                };
                ui.label(RichText::new(self.ping_status).color(color).size(11.5));
            }
        });

        action
    }
}
