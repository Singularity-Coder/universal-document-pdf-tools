//! Vector icon set (24x24, stroke based). Icons are white SVGs tinted at paint time, so one set serves every theme.
use eframe::egui::{self, Color32, Rect, Ui, Vec2};

macro_rules! svg {
    ($d:expr) => {
        concat!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="white" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d=""#,
            $d,
            r#""/></svg>"#
        )
    };
}

pub fn svg_of(name: &str) -> &'static str {
    match name {
        "create" => svg!("M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8zM14 3v5h5M12 11v6M9 14h6"),
        "convert" => svg!("M4 9a8 8 0 0 1 14-3l2 2M20 4v4h-4M20 15a8 8 0 0 1-14 3l-2-2M4 20v-4h4"),
        "organize" => svg!("M4 4h7v7H4zM13 4h7v7h-7zM4 13h7v7H4zM13 13h7v7h-7z"),
        "finish" => svg!("M4 7h10M18 7h2M4 17h2M10 17h10M14 4v6M8 14v6"),
        "ocr" => svg!("M4 8V5a1 1 0 0 1 1-1h3M16 4h3a1 1 0 0 1 1 1v3M20 16v3a1 1 0 0 1-1 1h-3M8 20H5a1 1 0 0 1-1-1v-3M8 9h8M8 12h8M8 15h5"),
        "sign" => svg!("M4 20h4L19 9a2.8 2.8 0 0 0-4-4L4 16zM13.5 6.5l4 4M14 20h6"),
        "inspect" => svg!("M12 3l7 3v5c0 5-3 8-7 10-4-2-7-5-7-10V6zM9.5 12l2 2 3.5-4"),
        "settings" => svg!("M12 15a3 3 0 1 0 0-6 3 3 0 0 0 0 6zM19 12l2-1-2-3-2 .6a7 7 0 0 0-1.5-.9L15 6h-4l-.5 1.7a7 7 0 0 0-1.5.9L7 8 5 11l2 1v1l-2 1 2 3 2-.6c.5.4 1 .7 1.5.9L11 20h4l.5-1.7c.5-.2 1-.5 1.5-.9l2 .6 2-3-2-1z"),
        "home" => svg!("M3 11l9-8 9 8M5 10v10h14V10"),
        "upload" => svg!("M12 16V4M7 9l5-5 5 5M4 20h16"),
        "download" => svg!("M12 4v12M7 11l5 5 5-5M4 20h16"),
        "x" => svg!("M6 6l12 12M18 6L6 18"),
        "up" => svg!("M12 19V5M6 11l6-6 6 6"),
        "down" => svg!("M12 5v14M6 13l6 6 6-6"),
        "left" => svg!("M19 12H5M11 6l-6 6 6 6"),
        "right" => svg!("M5 12h14M13 6l6 6-6 6"),
        "rotl" => svg!("M4 4v6h6M4.5 10A8 8 0 1 1 7 17"),
        "rotr" => svg!("M20 4v6h-6M19.5 10A8 8 0 1 0 17 17"),
        "trash" => svg!("M4 7h16M10 11v6M14 11v6M6 7l1 13h10l1-13M9 7V4h6v3"),
        "copy" => svg!("M9 9h10v11H9zM5 15V4h10"),
        "check" => svg!("M5 12l5 5 9-10"),
        "lock" => svg!("M6 11h12v9H6zM8 11V8a4 4 0 0 1 8 0v3"),
        "shield" => svg!("M12 3l8 3v6c0 5-3.5 8-8 9-4.5-1-8-4-8-9V6z"),
        "bolt" => svg!("M13 3L5 14h6l-1 7 8-11h-6z"),
        "folder" => svg!("M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"),
        "plus" => svg!("M12 5v14M5 12h14"),
        "save" => svg!("M5 4h11l3 3v13H5zM8 4v5h7V4M8 20v-6h8v6"),
        "stop" => svg!("M6 6h12v12H6z"),
        "play" => svg!("M7 4l13 8-13 8z"),
        "highlight" => svg!("M4 20h6M14 4l6 6-9 9H5v-6zM12 6l6 6"),
        "text" => svg!("M5 6V4h14v2M12 4v16M9 20h6"),
        "redact" => svg!("M4 5h16v14H4zM8 12h8"),
        "file" => svg!("M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8zM14 3v5h5"),
        "wifioff" => svg!("M3 3l18 18M8.5 8.5A12 12 0 0 0 2 12M22 12a14 14 0 0 0-8-3.8M5 16a10 10 0 0 1 4-2.5M12 20h.01"),
        "info" => svg!("M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18zM12 11v5M12 8h.01"),
        "warn" => svg!("M12 3l10 18H2zM12 10v5M12 18h.01"),
        "eye" => svg!("M2 12s4-7 10-7 10 7 10 7-4 7-10 7S2 12 2 12zM12 15a3 3 0 1 0 0-6 3 3 0 0 0 0 6z"),
        _ => svg!("M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8zM14 3v5h5"),
    }
}

/// Paint an icon into `rect` with the given colour.
pub fn paint(ui: &Ui, rect: Rect, name: &str, color: Color32) {
    egui::Image::from_bytes(format!("bytes://omni-{name}.svg"), svg_of(name).as_bytes()).tint(color).paint_at(ui, rect);
}

/// Allocate space and draw an icon inline.
pub fn show(ui: &mut Ui, name: &str, size: f32, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), egui::Sense::hover());
    paint(ui, rect, name, color);
}
