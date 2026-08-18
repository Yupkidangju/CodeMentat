mod app;
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
            .with_inner_size(app::TIER1_SIZE) // Starts compact as Pill Bar
            .with_min_inner_size([460.0, 48.0])
            .with_decorations(false) // Frameless widget style
            .with_transparent(true)
            .with_always_on_top(),
        ..Default::default()
    };

    eframe::run_native(
        "Code Mentat",
        native_options,
        Box::new(|cc| Ok(Box::new(MentatApp::new(cc, rt)))),
    )
}
