mod app;
mod hotkeys;
mod provider_setup;
mod theme;
mod widgets;

use app::MentatApp;
use eframe::egui;
use std::sync::Arc;
use tokio::runtime::Runtime;

fn main() -> eframe::Result<()> {
    tracing_subscriber::fmt::init();

    let rt = Arc::new(Runtime::new().expect("Tokio 런타임 초기화 실패"));

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Code Mentat")
            .with_inner_size(app::TIER1_SIZE)
            .with_min_inner_size([640.0, 56.0])
            .with_decorations(false)
            .with_transparent(false)
            .with_always_on_top(),
        ..Default::default()
    };

    eframe::run_native(
        "Code Mentat",
        native_options,
        Box::new(|cc| Ok(Box::new(MentatApp::new(cc, rt)))),
    )
}
