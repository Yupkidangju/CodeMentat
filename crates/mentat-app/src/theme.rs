use egui::{Color32, FontData, FontDefinitions, FontFamily, Stroke, Style, Visuals};
use std::sync::Arc;

#[allow(dead_code)]
pub struct MentatTheme;

#[allow(dead_code)]
impl MentatTheme {
    pub const KOREAN_FONT_NAME: &str = "nanum_gothic";
    pub const BG_BASE: Color32 = Color32::from_rgb(18, 21, 26);
    pub const BG_CARD: Color32 = Color32::from_rgb(26, 31, 38);
    pub const BG_INPUT: Color32 = Color32::from_rgb(33, 39, 48);

    pub const BORDER_COLOR: Color32 = Color32::from_rgba_premultiplied(255, 255, 255, 20);
    pub const BORDER_FOCUS: Color32 = Color32::from_rgb(56, 189, 248);

    // Semantic Status Colors
    pub const STATUS_READ_ONLY: Color32 = Color32::from_rgb(16, 185, 129); // Emerald
    pub const STATUS_INFERENCING: Color32 = Color32::from_rgb(56, 189, 248); // Sky Blue
    pub const STATUS_CONFLICT: Color32 = Color32::from_rgb(245, 158, 11); // Amber
    pub const STATUS_ERROR: Color32 = Color32::from_rgb(244, 63, 94); // Rose

    pub const TEXT_PRIMARY: Color32 = Color32::from_rgb(243, 244, 246);
    pub const TEXT_MUTED: Color32 = Color32::from_rgb(156, 163, 175);

    pub fn font_definitions() -> FontDefinitions {
        let mut fonts = FontDefinitions::default();
        fonts.font_data.insert(
            Self::KOREAN_FONT_NAME.to_owned(),
            Arc::new(FontData::from_static(include_bytes!(
                "../assets/fonts/NanumGothic-Regular.ttf"
            ))),
        );

        // 기본 라틴/코드 글꼴을 유지하고, 없는 한글 글리프만 내장 글꼴로 보완한다.
        for family in [FontFamily::Proportional, FontFamily::Monospace] {
            fonts
                .families
                .entry(family)
                .or_default()
                .push(Self::KOREAN_FONT_NAME.to_owned());
        }
        fonts
    }

    pub fn apply(ctx: &egui::Context) {
        ctx.set_fonts(Self::font_definitions());

        let mut visuals = Visuals::dark();
        visuals.override_text_color = Some(Self::TEXT_PRIMARY);
        visuals.panel_fill = Self::BG_BASE;
        visuals.window_fill = Self::BG_BASE;
        visuals.window_stroke = Stroke::new(1.0, Self::BORDER_COLOR);
        visuals.window_rounding = egui::Rounding::same(12.0);

        visuals.widgets.noninteractive.bg_fill = Self::BG_CARD;
        visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, Self::BORDER_COLOR);
        visuals.widgets.noninteractive.rounding = egui::Rounding::same(8.0);

        visuals.widgets.inactive.bg_fill = Self::BG_CARD;
        visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, Self::BORDER_COLOR);
        visuals.widgets.inactive.rounding = egui::Rounding::same(8.0);

        visuals.widgets.hovered.bg_fill = Color32::from_rgb(38, 45, 56);
        visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, Self::BORDER_FOCUS);
        visuals.widgets.hovered.rounding = egui::Rounding::same(8.0);

        visuals.widgets.active.bg_fill = Color32::from_rgb(45, 53, 66);
        visuals.widgets.active.bg_stroke = Stroke::new(1.0, Self::BORDER_FOCUS);
        visuals.widgets.active.rounding = egui::Rounding::same(8.0);

        ctx.set_visuals(visuals);

        let mut style: Style = (*ctx.style()).clone();
        style.spacing.item_spacing = egui::vec2(8.0, 8.0);
        ctx.set_style(style);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::FontFamily;

    #[test]
    fn embedded_korean_font_is_registered_for_all_text_families() {
        let fonts = MentatTheme::font_definitions();

        assert!(fonts.font_data.contains_key(MentatTheme::KOREAN_FONT_NAME));
        for family in [FontFamily::Proportional, FontFamily::Monospace] {
            assert!(
                fonts.families.get(&family).is_some_and(|names| names
                    .iter()
                    .any(|name| name == MentatTheme::KOREAN_FONT_NAME)),
                "한글 폰트가 {family:?} 글꼴군에 등록되어야 합니다"
            );
        }
    }
}
