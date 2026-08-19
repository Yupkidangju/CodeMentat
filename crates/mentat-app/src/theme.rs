use egui::{
    Color32, FontData, FontDefinitions, FontFamily, FontId, Stroke, Style, TextStyle, Visuals,
};
use std::sync::Arc;

#[allow(dead_code)]
pub struct MentatTheme;

#[allow(dead_code)]
impl MentatTheme {
    pub const KOREAN_FONT_NAME: &str = "nanum_gothic";
    pub const BG_BASE: Color32 = Color32::WHITE;
    pub const BG_CARD: Color32 = Color32::from_rgb(245, 245, 242);
    pub const BG_INPUT: Color32 = Color32::WHITE;
    pub const BG_HOVER: Color32 = Color32::from_rgb(235, 235, 231);
    pub const BG_ACTIVE: Color32 = Color32::from_rgb(224, 224, 219);
    pub const BG_SELECTION: Color32 = Color32::from_rgb(254, 226, 226);
    pub const BG_SUCCESS: Color32 = Color32::from_rgb(237, 247, 239);
    pub const BG_INFO: Color32 = Color32::from_rgb(238, 244, 255);
    pub const BG_WARNING: Color32 = Color32::from_rgb(255, 247, 237);

    pub const BORDER_COLOR: Color32 = Color32::from_rgb(207, 207, 203);
    pub const BORDER_FOCUS: Color32 = Color32::from_rgb(185, 28, 28);

    // 상태는 흰 배경에서도 WCAG AA 대비를 확보하는 어두운 색으로 고정한다.
    pub const STATUS_READ_ONLY: Color32 = Color32::from_rgb(22, 101, 52);
    pub const STATUS_INFERENCING: Color32 = Color32::from_rgb(29, 78, 216);
    pub const STATUS_PROPOSED: Color32 = Color32::from_rgb(107, 33, 168);
    pub const STATUS_CONFLICT: Color32 = Color32::from_rgb(146, 64, 14);
    pub const STATUS_ERROR: Color32 = Color32::from_rgb(185, 28, 28);

    pub const TEXT_PRIMARY: Color32 = Color32::from_rgb(17, 17, 17);
    pub const TEXT_MUTED: Color32 = Color32::from_rgb(82, 82, 82);

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

        let mut visuals = Visuals::light();
        visuals.override_text_color = Some(Self::TEXT_PRIMARY);
        visuals.panel_fill = Self::BG_BASE;
        visuals.window_fill = Self::BG_BASE;
        visuals.extreme_bg_color = Self::BG_INPUT;
        visuals.faint_bg_color = Self::BG_CARD;
        visuals.code_bg_color = Self::BG_CARD;
        visuals.hyperlink_color = Self::STATUS_INFERENCING;
        visuals.warn_fg_color = Self::STATUS_CONFLICT;
        visuals.error_fg_color = Self::STATUS_ERROR;
        visuals.selection.bg_fill = Self::BG_SELECTION;
        visuals.selection.stroke = Stroke::new(1.0, Self::BORDER_FOCUS);
        visuals.window_stroke = Stroke::new(1.0, Self::BORDER_COLOR);
        visuals.window_rounding = egui::Rounding::same(4.0);
        visuals.menu_rounding = egui::Rounding::same(3.0);

        visuals.widgets.noninteractive.bg_fill = Self::BG_CARD;
        visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, Self::BORDER_COLOR);
        visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, Self::TEXT_PRIMARY);
        visuals.widgets.noninteractive.rounding = egui::Rounding::same(3.0);

        visuals.widgets.inactive.bg_fill = Self::BG_CARD;
        visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, Self::BORDER_COLOR);
        visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, Self::TEXT_PRIMARY);
        visuals.widgets.inactive.rounding = egui::Rounding::same(3.0);

        visuals.widgets.hovered.bg_fill = Self::BG_HOVER;
        visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, Self::BORDER_FOCUS);
        visuals.widgets.hovered.fg_stroke = Stroke::new(1.5, Self::TEXT_PRIMARY);
        visuals.widgets.hovered.rounding = egui::Rounding::same(3.0);

        visuals.widgets.active.bg_fill = Self::BG_ACTIVE;
        visuals.widgets.active.bg_stroke = Stroke::new(1.0, Self::BORDER_FOCUS);
        visuals.widgets.active.fg_stroke = Stroke::new(1.5, Self::TEXT_PRIMARY);
        visuals.widgets.active.rounding = egui::Rounding::same(3.0);

        visuals.widgets.open.bg_fill = Self::BG_ACTIVE;
        visuals.widgets.open.bg_stroke = Stroke::new(1.0, Self::BORDER_FOCUS);
        visuals.widgets.open.fg_stroke = Stroke::new(1.5, Self::TEXT_PRIMARY);
        visuals.widgets.open.rounding = egui::Rounding::same(3.0);

        ctx.set_visuals(visuals);

        let mut style: Style = (*ctx.style()).clone();
        style.spacing.item_spacing = egui::vec2(8.0, 8.0);
        style.spacing.button_padding = egui::vec2(10.0, 6.0);
        style.spacing.interact_size = egui::vec2(40.0, 28.0);
        style.text_styles.insert(
            TextStyle::Heading,
            FontId::new(18.0, FontFamily::Proportional),
        );
        style
            .text_styles
            .insert(TextStyle::Body, FontId::new(14.0, FontFamily::Proportional));
        style.text_styles.insert(
            TextStyle::Button,
            FontId::new(14.0, FontFamily::Proportional),
        );
        style.text_styles.insert(
            TextStyle::Small,
            FontId::new(13.0, FontFamily::Proportional),
        );
        style.text_styles.insert(
            TextStyle::Monospace,
            FontId::new(13.0, FontFamily::Monospace),
        );
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

    #[test]
    fn light_theme_uses_explicit_readable_widget_foregrounds() {
        let ctx = egui::Context::default();
        MentatTheme::apply(&ctx);
        let visuals = ctx.style().visuals.clone();

        assert!(!visuals.dark_mode);
        assert_eq!(visuals.panel_fill, Color32::WHITE);
        assert_eq!(
            visuals.widgets.inactive.fg_stroke.color,
            MentatTheme::TEXT_PRIMARY
        );
        assert_eq!(
            visuals.widgets.hovered.fg_stroke.color,
            MentatTheme::TEXT_PRIMARY
        );
        assert_eq!(
            visuals.widgets.active.fg_stroke.color,
            MentatTheme::TEXT_PRIMARY
        );
        assert_eq!(
            visuals.widgets.open.fg_stroke.color,
            MentatTheme::TEXT_PRIMARY
        );
    }

    #[test]
    fn normal_text_and_semantic_statuses_meet_aa_contrast_on_white() {
        fn channel_luminance(channel: u8) -> f32 {
            let value = f32::from(channel) / 255.0;
            if value <= 0.04045 {
                value / 12.92
            } else {
                ((value + 0.055) / 1.055).powf(2.4)
            }
        }

        fn contrast_ratio(foreground: Color32, background: Color32) -> f32 {
            let luminance = |color: Color32| {
                0.2126 * channel_luminance(color.r())
                    + 0.7152 * channel_luminance(color.g())
                    + 0.0722 * channel_luminance(color.b())
            };
            let foreground = luminance(foreground);
            let background = luminance(background);
            (foreground.max(background) + 0.05) / (foreground.min(background) + 0.05)
        }

        for color in [
            MentatTheme::TEXT_PRIMARY,
            MentatTheme::TEXT_MUTED,
            MentatTheme::STATUS_READ_ONLY,
            MentatTheme::STATUS_INFERENCING,
            MentatTheme::STATUS_PROPOSED,
            MentatTheme::STATUS_CONFLICT,
            MentatTheme::STATUS_ERROR,
        ] {
            assert!(contrast_ratio(color, MentatTheme::BG_BASE) >= 4.5);
        }
    }
}
