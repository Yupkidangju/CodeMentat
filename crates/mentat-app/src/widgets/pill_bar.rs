use crate::theme::MentatTheme;
use egui::{vec2, RichText, Rounding, Stroke, Ui};

pub const MIN_QUERY_WIDTH: f32 = 200.0;
pub const REPO_MAX_WIDTH: f32 = 120.0;
const REPO_MIN_WIDTH: f32 = 72.0;
const BRAND_WIDTH: f32 = 92.0;
const READ_ONLY_WIDTH: f32 = 44.0;
const QUICK_CHIP_WIDTH: f32 = 76.0;
const ROW_HEIGHT: f32 = 30.0;
const TRAILING_WIDTH: f32 = 180.0;
const CONTROL_GAP: f32 = 8.0;
const ROW_END_SLACK: f32 = CONTROL_GAP;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PillLayout {
    pub repo_width: f32,
    pub query_width: f32,
    pub show_quick_chip: bool,
}

pub fn pill_layout(available_width: f32) -> PillLayout {
    let compact_fixed = BRAND_WIDTH + READ_ONLY_WIDTH + CONTROL_GAP * 3.0;
    let expanded_fixed = compact_fixed + QUICK_CHIP_WIDTH + CONTROL_GAP;
    let show_quick_chip =
        available_width >= expanded_fixed + REPO_MIN_WIDTH + MIN_QUERY_WIDTH + ROW_END_SLACK;
    let fixed_width = if show_quick_chip {
        expanded_fixed
    } else {
        compact_fixed
    };
    let flexible_width = (available_width - fixed_width).max(0.0);
    // egui horizontal은 마지막 위젯 뒤 wrap 여유를 보존하므로 이를 저장소 폭에서 선차감한다.
    let repo_width = (flexible_width - MIN_QUERY_WIDTH - ROW_END_SLACK).clamp(0.0, REPO_MAX_WIDTH);

    PillLayout {
        repo_width,
        query_width: (flexible_width - repo_width).max(0.0),
        show_quick_chip,
    }
}

#[cfg(test)]
#[derive(Debug, Clone, Copy)]
pub struct PillBarGeometry {
    pub repo: egui::Rect,
    pub query: egui::Rect,
    pub trailing: egui::Rect,
    pub pin: egui::Rect,
    pub settings: egui::Rect,
    pub close: egui::Rect,
}

#[cfg(test)]
impl Default for PillBarGeometry {
    fn default() -> Self {
        Self {
            repo: egui::Rect::NOTHING,
            query: egui::Rect::NOTHING,
            trailing: egui::Rect::NOTHING,
            pin: egui::Rect::NOTHING,
            settings: egui::Rect::NOTHING,
            close: egui::Rect::NOTHING,
        }
    }
}

pub struct PillBarAction {
    pub open_repo_clicked: bool,
    pub query_submitted: Option<String>,
    pub quick_chip_clicked: Option<String>,
    pub pin_toggled: bool,
    pub settings_clicked: bool,
    pub close_clicked: bool,
    #[cfg(test)]
    pub geometry: PillBarGeometry,
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
            close_clicked: false,
            #[cfg(test)]
            geometry: PillBarGeometry::default(),
        };

        let row_width = ui.available_width();
        ui.allocate_ui_with_layout(
            vec2(row_width, ROW_HEIGHT),
            egui::Layout::right_to_left(egui::Align::Center),
            |ui| {
                // 안전 조작은 동적 문자열보다 먼저 우측 고정 영역을 예약한다.
                let _trailing = ui.allocate_ui_with_layout(
                    vec2(TRAILING_WIDTH, ROW_HEIGHT),
                    egui::Layout::right_to_left(egui::Align::Center),
                    |ui| {
                        let close_btn = egui::Button::new(
                            RichText::new("종료 ×")
                                .color(MentatTheme::STATUS_ERROR)
                                .size(13.0)
                                .strong(),
                        )
                        .fill(MentatTheme::BG_CARD)
                        .stroke(Stroke::new(1.0, MentatTheme::STATUS_ERROR))
                        .rounding(Rounding::same(3.0));
                        let close_response = ui
                            .add_sized(vec2(64.0, ROW_HEIGHT), close_btn)
                            .on_hover_text("프로그램 종료 (Ctrl+Q)");
                        if close_response.clicked() {
                            action.close_clicked = true;
                        }

                        let settings_btn = egui::Button::new(RichText::new("설정").size(13.0))
                            .fill(MentatTheme::BG_CARD)
                            .rounding(Rounding::same(3.0));
                        let settings_response = ui
                            .add_sized(vec2(52.0, ROW_HEIGHT), settings_btn)
                            .on_hover_text("설정 및 백엔드 관리");
                        if settings_response.clicked() {
                            action.settings_clicked = true;
                        }

                        let pin_label = if self.is_pinned { "고정" } else { "해제" };
                        let pin_btn = egui::Button::new(RichText::new(pin_label).size(13.0))
                            .fill(if self.is_pinned {
                                MentatTheme::BG_ACTIVE
                            } else {
                                MentatTheme::BG_CARD
                            })
                            .rounding(Rounding::same(3.0));
                        let pin_response = ui
                            .add_sized(vec2(48.0, ROW_HEIGHT), pin_btn)
                            .on_hover_text("최상위 고정(Always on Top) 토글");
                        if pin_response.clicked() {
                            action.pin_toggled = true;
                        }

                        #[cfg(test)]
                        {
                            action.geometry.close = close_response.rect;
                            action.geometry.settings = settings_response.rect;
                            action.geometry.pin = pin_response.rect;
                        }
                    },
                );

                #[cfg(test)]
                {
                    action.geometry.trailing = _trailing.response.rect;
                }

                let leading_width = ui.available_width();
                ui.allocate_ui_with_layout(
                    vec2(leading_width, ROW_HEIGHT),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| {
                        let layout = pill_layout(ui.available_width());

                        // 브랜드 영역은 프레임리스 창의 드래그 손잡이로 사용한다.
                        let drag_response = ui.add_sized(
                            vec2(BRAND_WIDTH, ROW_HEIGHT),
                            egui::Label::new(
                                RichText::new("CODE MENTAT")
                                    .color(MentatTheme::TEXT_PRIMARY)
                                    .size(13.0)
                                    .strong(),
                            )
                            .truncate()
                            .sense(egui::Sense::drag()),
                        );
                        if drag_response.dragged() {
                            ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
                        }

                        let repo_label = match self.repo_name {
                            Some(name) => format!("저장소: {name}"),
                            None => "저장소 열기...".to_string(),
                        };
                        let repo_btn = egui::Button::new(
                            RichText::new(&repo_label)
                                .color(MentatTheme::TEXT_PRIMARY)
                                .size(13.0)
                                .strong(),
                        )
                        .fill(MentatTheme::BG_CARD)
                        .stroke(Stroke::new(1.0, MentatTheme::BORDER_COLOR))
                        .rounding(Rounding::same(3.0))
                        .truncate();
                        let repo_response = ui
                            .add_sized(vec2(layout.repo_width, ROW_HEIGHT), repo_btn)
                            .on_hover_text(&repo_label);
                        if repo_response.clicked() {
                            action.open_repo_clicked = true;
                        }

                        let read_only_color = if self.is_read_only {
                            MentatTheme::STATUS_READ_ONLY
                        } else {
                            MentatTheme::STATUS_CONFLICT
                        };
                        let read_only_btn = egui::Button::new(
                            RichText::new("R/O")
                                .color(read_only_color)
                                .size(13.0)
                                .strong(),
                        )
                        .fill(if self.is_read_only {
                            MentatTheme::BG_SUCCESS
                        } else {
                            MentatTheme::BG_WARNING
                        })
                        .stroke(Stroke::new(1.0, read_only_color))
                        .rounding(Rounding::same(3.0));
                        ui.add_sized(vec2(READ_ONLY_WIDTH, ROW_HEIGHT), read_only_btn)
                            .on_hover_text(format!(
                                "엄격한 읽기 전용 경계 확립됨: 저장소 수정/쓰기 불가 ({})",
                                self.status_text
                            ));

                        let input_response = ui.add_sized(
                            vec2(layout.query_width, ROW_HEIGHT),
                            egui::TextEdit::singleline(self.query_text)
                                .hint_text(
                                    RichText::new("저장소에 질문하기... (/ 커맨드)")
                                        .color(MentatTheme::TEXT_MUTED),
                                )
                                .font(egui::FontId::proportional(14.0)),
                        );
                        if self.request_focus {
                            input_response.request_focus();
                        }
                        if input_response.lost_focus()
                            && ui.input(|i| i.key_pressed(egui::Key::Enter))
                            && !self.query_text.trim().is_empty()
                        {
                            action.query_submitted = Some(self.query_text.clone());
                        }

                        if layout.show_quick_chip {
                            let chip_btn = egui::Button::new(
                                RichText::new("/onboard")
                                    .color(MentatTheme::STATUS_INFERENCING)
                                    .size(13.0),
                            )
                            .fill(MentatTheme::BG_INFO)
                            .stroke(Stroke::new(1.0, MentatTheme::STATUS_INFERENCING))
                            .rounding(Rounding::same(3.0));
                            if ui
                                .add_sized(vec2(QUICK_CHIP_WIDTH, ROW_HEIGHT), chip_btn)
                                .clicked()
                            {
                                action.quick_chip_clicked = Some("/onboard".to_string());
                            }
                        }

                        #[cfg(test)]
                        {
                            action.geometry.repo = repo_response.rect;
                            action.geometry.query = input_response.rect;
                        }
                    },
                );
            },
        );

        action
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    fn render_pill_geometry(viewport_width: f32, repo_name: &str) -> PillBarGeometry {
        let ctx = egui::Context::default();
        MentatTheme::apply(&ctx);
        let captured = Rc::new(RefCell::new(None));
        let captured_in_frame = captured.clone();
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                vec2(viewport_width, 56.0),
            )),
            ..Default::default()
        };

        let _ = ctx.run(input, |ctx| {
            egui::CentralPanel::default()
                .frame(egui::Frame::none().inner_margin(egui::Margin::same(8.0)))
                .show(ctx, |ui| {
                    let mut query = String::new();
                    let action =
                        PillBar::new(Some(repo_name), false, &mut query, true, "테스트").show(ui);
                    *captured_in_frame.borrow_mut() = Some(action.geometry);
                });
        });

        let geometry = captured
            .borrow()
            .as_ref()
            .copied()
            .expect("PillBar geometry must be captured");
        geometry
    }

    fn assert_safe_geometry(viewport_width: f32, repo_name: &str) {
        let geometry = render_pill_geometry(viewport_width, repo_name);

        assert!(
            geometry.repo.width() <= REPO_MAX_WIDTH + 0.5,
            "viewport={viewport_width}, geometry={geometry:?}"
        );
        assert!(
            geometry.query.width() >= MIN_QUERY_WIDTH - 0.5,
            "viewport={viewport_width}, geometry={geometry:?}"
        );
        assert!(
            geometry.pin.left() >= geometry.trailing.left(),
            "viewport={viewport_width}, geometry={geometry:?}"
        );
        assert!(
            geometry.pin.right() <= geometry.settings.left(),
            "viewport={viewport_width}, geometry={geometry:?}"
        );
        assert!(
            geometry.settings.right() <= geometry.close.left(),
            "viewport={viewport_width}, geometry={geometry:?}"
        );
        assert!(
            geometry.close.right() <= viewport_width,
            "viewport={viewport_width}, geometry={geometry:?}"
        );
    }

    #[test]
    fn query_input_keeps_a_usable_minimum_width() {
        let compact = pill_layout(436.0);
        assert_eq!(compact.repo_width, 68.0);
        assert_eq!(compact.query_width, 208.0);
        assert!(!compact.show_quick_chip);

        let expanded = pill_layout(556.0);
        assert_eq!(expanded.repo_width, 104.0);
        assert_eq!(expanded.query_width, 208.0);
        assert!(expanded.show_quick_chip);
    }

    #[test]
    fn long_ascii_repository_name_keeps_trailing_controls_visible_at_640_and_760() {
        let name = "repository-with-a-very-long-ascii-name-".repeat(4);
        assert_safe_geometry(640.0, &name);
        assert_safe_geometry(760.0, &name);
    }

    #[test]
    fn long_cjk_repository_name_keeps_trailing_controls_visible_at_640_and_760() {
        let name = "매우긴저장소이름한글漢字かなカナ".repeat(6);
        assert_safe_geometry(640.0, &name);
        assert_safe_geometry(760.0, &name);
    }
}
