#![cfg_attr(all(not(debug_assertions), windows), windows_subsystem = "windows")]

mod app;
mod icons;
mod jobs;
mod kit;
mod organizer;
mod props;
mod runner;
mod sign;
mod splash;
mod theme;

use std::sync::Arc;

fn window_icon() -> Option<Arc<eframe::egui::IconData>> {
    let img = image::load_from_memory(include_bytes!("../../../assets/icon.png")).ok()?.to_rgba8();
    let (width, height) = img.dimensions();
    Some(Arc::new(eframe::egui::IconData { rgba: img.into_raw(), width, height }))
}

fn main() -> eframe::Result<()> {
    let mut viewport = eframe::egui::ViewportBuilder::default()
        .with_title("Universal Document PDF Tools")
        .with_inner_size([1560.0, 940.0])
        .with_min_inner_size([1200.0, 720.0])
        .with_drag_and_drop(true);
    if let Some(icon) = window_icon() {
        viewport = viewport.with_icon(icon);
    }
    let options = eframe::NativeOptions { viewport, centered: true, ..Default::default() };
    eframe::run_native("Universal Document PDF Tools", options, Box::new(|cc| Ok(Box::new(app::App::new(cc)))))
}
