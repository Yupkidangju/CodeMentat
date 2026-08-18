use crate::theme::MentatTheme;
use egui::{vec2, Color32, RichText, Rounding, Stroke, Ui};

pub const MIN_QUERY_WIDTH: f32 = 160.0;
const TRAILING_CONTROLS_WIDTH: f32 = 170.0;
const COMPACT_TRAILING_CONTROLS_WIDTH: f32 = 86.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PillLayout {
    pub query_width: f32,
    pub show_quick_chip: bool,
}

pub fn pill_layout(available_width: f32) -> PillLayout {
    let show_quick_chip = available_width >= MIN_QUERY_WIDTH + TRAILING_CONTROLS_WIDTH;
    let reserved_width = if show_quick_chip {
        TRAILING_CONTROLS_WIDTH
    } else {
        COMPACT_TRAILING_CONTROLS_WIDTH
    };
    PillLayout {
        query_width: (available_width - reserved_width)
            .max(MIN_QUERY_WIDTH)
            .min(available_width),
        show_quick_chip,
    }
}

pub struct PillBarAction {
    pub open_repo_clicked: bool,
    pub query_submitted: Option<String>,
    pub quick_chip_clicked: Option<String>,
    pub pin_toggled: bool,
    pub settings_clicked: bool,
}

pub struct PillBar<'a> {
    pub repo_name: Option<&'a str>,
    pub is_read_only: bool,
    pub query_text: &'a mut String,
    pub is_pinned: bool,
    pub status_text: &'a str,
    pub request_focus: bool,
}

impl<'a> PillBar<'a> {
    pub fn new(
        repo_name: Option<&'a str>,
        is_read_only: bool,
        query_text: &'a mut String,
        is_pinned: bool,
        status_text: &'a str,
    ) -> Self {
        Self {
            repo_name,
            is_read_only,
            query_text,
            is_pinned,
            status_text,
            request_focus: false,
        }
    }

    pub fn request_focus(mut self, yes: bool) -> Self {
        self.request_focus = yes;
        self
    }

    pub fn show(self, ui: &mut Ui) -> PillBarAction {
        let mut action = PillBarAction {
            open_repo_clicked: false,
            query_submitted: None,
            quick_chip_clicked: None,
            pin_toggled: false,
            settings_clicked: false,
        };

        ui.horizontal(|ui| {
            // Drag handle area
            let drag_resp = ui.add(
                egui::Label::new(
                    RichText::new(":::")
                        .color(MentatTheme::TEXT_MUTED)
                        .size(13.0),
                )
                .sense(egui::Sense::drag()),
            );
            if drag_resp.dragged() {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
            }

            // Repo Selector Badge
            let repo_label = match self.repo_name {
                Some(name) => format!("저장소: {} ▾", name),
                None => "저장소 열기... ▾".to_string(),
            };

            let repo_btn = egui::Button::new(
                RichText::new(repo_label)
                    .color(MentatTheme::TEXT_PRIMARY)
                    .size(12.0)
                    .strong(),
            )
            .fill(MentatTheme::BG_CARD)
            .stroke(Stroke::new(1.0, MentatTheme::BORDER_COLOR))
            .rounding(Rounding::same(6.0));

            if ui.add(repo_btn).clicked() {
                action.open_repo_clicked = true;
            }

            // Read-Only Invariant Badge
            let ro_tooltip = format!(
                "엄격한 읽기 전용 경계 확립됨: 저장소 수정/쓰기 불가 ({})",
                self.status_text
            );
            ui.add(
                egui::Button::new(
                    RichText::new("R/O")
                        .color(if self.is_read_only {
                            MentatTheme::STATUS_READ_ONLY
                        } else {
                            MentatTheme::STATUS_CONFLICT
                        })
                        .size(11.0)
                        .strong(),
                )
                .fill(Color32::from_rgba_premultiplied(16, 185, 129, 25))
                .stroke(Stroke::new(1.0, MentatTheme::STATUS_READ_ONLY))
                .rounding(Rounding::same(6.0)),
            )
            .on_hover_text(ro_tooltip);

            // Query Input Box
            let pill_layout = pill_layout(ui.available_width());
            let input_resp = ui.add_sized(
                vec2(pill_layout.query_width, 26.0),
                egui::TextEdit::singleline(self.query_text)
                    .hint_text(
                        RichText::new("저장소에 질문하기... (/ 커맨드)")
                            .color(MentatTheme::TEXT_MUTED),
                    )
                    .font(egui::FontId::proportional(12.5)),
            );

            if self.request_focus {
                input_resp.request_focus();
            }

            if input_resp.lost_focus()
                && ui.input(|i| i.key_pressed(egui::Key::Enter))
                && !self.query_text.trim().is_empty()
            {
                action.query_submitted = Some(self.query_text.clone());
            }

            // Quick Chip (e.g. /onboard)
            if pill_layout.show_quick_chip {
                let chip_btn = egui::Button::new(
                    RichText::new("/onboard")
                        .color(MentatTheme::STATUS_INFERENCING)
                        .size(11.0),
                )
                .fill(Color32::from_rgba_premultiplied(56, 189, 248, 20))
                .stroke(Stroke::new(1.0, MentatTheme::STATUS_INFERENCING))
                .rounding(Rounding::same(6.0));

                if ui.add(chip_btn).clicked() {
                    action.quick_chip_clicked = Some("/onboard".to_string());
                }
            }

            // Always-on-top Pin Toggle
            let pin_icon = if self.is_pinned { "고정" } else { "해제" };
            let pin_btn = egui::Button::new(RichText::new(pin_icon).size(13.0))
                .fill(if self.is_pinned {
                    Color32::from_rgb(45, 53, 66)
                } else {
                    MentatTheme::BG_CARD
                })
                .rounding(Rounding::same(6.0));

            if ui
                .add(pin_btn)
                .on_hover_text("최상위 고정(Always on Top) 토글")
                .clicked()
            {
                action.pin_toggled = true;
            }

            // Settings button
            let settings_btn = egui::Button::new(RichText::new("설정").size(11.5))
                .fill(MentatTheme::BG_CARD)
                .rounding(Rounding::same(6.0));

            if ui
                .add(settings_btn)
                .on_hover_text("설정 및 백엔드 관리")
                .clicked()
            {
                action.settings_clicked = true;
            }
        });

        action
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_input_keeps_a_usable_minimum_width() {
        assert_eq!(pill_layout(250.0).query_width, 164.0);
        assert!(!pill_layout(250.0).show_quick_chip);
        assert_eq!(pill_layout(430.0).query_width, 260.0);
        assert!(pill_layout(430.0).show_quick_chip);
    }
}
