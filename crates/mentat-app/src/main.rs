#[allow(dead_code)]
mod app;
mod chat_app;
mod credential_state;
#[allow(dead_code)]
mod hotkeys;
mod provider_setup;
mod theme;
mod tool_egress_gate;
mod widgets;

use chat_app::MentatChatApp;
use eframe::egui;
use std::sync::Arc;
use tokio::runtime::Runtime;

fn main() -> eframe::Result<()> {
    tracing_subscriber::fmt::init();

    let rt = Arc::new(Runtime::new().expect("Tokio 런타임 초기화 실패"));

    let initial_preferences = chat_app::initial_ui_preferences();
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("Code Mentat")
        .with_inner_size([
            initial_preferences.width_points,
            initial_preferences.height_points,
        ])
        .with_min_inner_size(chat_app::MIN_WINDOW_SIZE)
        .with_resizable(true)
        .with_decorations(false)
        .with_transparent(false);
    if initial_preferences.always_on_top {
        viewport = viewport.with_always_on_top();
    }
    let native_options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        "Code Mentat",
        native_options,
        Box::new(|cc| Ok(Box::new(MentatChatApp::new(cc, rt)))),
    )
}
