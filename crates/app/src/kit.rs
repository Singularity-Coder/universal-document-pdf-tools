//! Small widget kit. Screens only use these helpers, which is what keeps buttons, cards and spacing identical everywhere.
use crate::icons;
use crate::theme::*;
use eframe::egui::{self, Align2, Color32, CursorIcon, FontId, Margin, Pos2, Rect, Response, RichText, Rounding, Sense, Shape, Stroke, Ui, Vec2};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Btn {
    Primary,
    Secondary,
    Ghost,
    Danger,
}

pub fn button(ui: &mut Ui, pal: &Palette, icon: Option<&str>, label: &str, kind: Btn, enabled: bool) -> Response {
    let font = FontId::proportional(14.5);
    let fg = match kind {
        Btn::Primary => pal.on_accent,
        Btn::Danger => pal.danger,
        _ => pal.text,
    };
    let galley = ui.painter().layout_no_wrap(label.to_string(), font, fg);
    let icon_w = if icon.is_some() { 18.0 + 8.0 } else { 0.0 };
    let w = 28.0 + icon_w + galley.size().x;
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, CONTROL_H), if enabled { Sense::click() } else { Sense::hover() });
    let hover = enabled && resp.hovered();
    let down = enabled && resp.is_pointer_button_down_on();
    let (fill, stroke) = match kind {
        Btn::Primary => (if down { mix(pal.accent, Color32::BLACK, 0.15) } else if hover { mix(pal.accent, Color32::WHITE, 0.14) } else { pal.accent }, Stroke::NONE),
        Btn::Secondary => (if hover { mix(pal.panel2, pal.accent, 0.12) } else { pal.panel2 }, Stroke::new(1.0_f32, if hover { mix(pal.line, pal.accent, 0.6) } else { pal.line })),
        Btn::Ghost => (if hover { pal.panel2 } else { Color32::TRANSPARENT }, Stroke::NONE),
        Btn::Danger => (if hover { mix(pal.panel2, pal.danger, 0.15) } else { Color32::TRANSPARENT }, Stroke::new(1.0_f32, mix(pal.line, pal.danger, 0.5))),
    };
    let fade = if enabled { 1.0 } else { 0.45 };
    ui.painter().rect(rect, Rounding::same(RADIUS), fill.gamma_multiply(fade), Stroke::new(stroke.width, stroke.color.gamma_multiply(fade)));
    let mut x = rect.left() + 14.0;
    if let Some(ic) = icon {
        let r = Rect::from_min_size(Pos2::new(x, rect.center().y - 9.0), Vec2::splat(18.0));
        icons::paint(ui, r, ic, fg.gamma_multiply(fade));
        x += 26.0;
    }
    ui.painter().galley(Pos2::new(x, rect.center().y - galley.size().y / 2.0), galley, fg.gamma_multiply(fade));
    if enabled {
        resp.on_hover_cursor(CursorIcon::PointingHand)
    } else {
        resp
    }
}

/// Square icon-only button with a tooltip.
pub fn icon_button(ui: &mut Ui, pal: &Palette, icon: &str, tip: &str, enabled: bool, size: f32) -> Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(size), if enabled { Sense::click() } else { Sense::hover() });
    let hover = enabled && resp.hovered();
    if hover {
        ui.painter().rect_filled(rect, Rounding::same(RADIUS - 3.0), mix(pal.panel2, pal.accent, 0.18));
    }
    let col = if enabled { if hover { pal.accent } else { pal.text } } else { pal.muted.gamma_multiply(0.6) };
    icons::paint(ui, Rect::from_center_size(rect.center(), Vec2::splat(size * 0.56)), icon, col);
    let resp = resp.on_hover_text(tip);
    if enabled {
        resp.on_hover_cursor(CursorIcon::PointingHand)
    } else {
        resp
    }
}

/// Pill used for tabs / tool pickers.
pub fn chip(ui: &mut Ui, pal: &Palette, icon: Option<&str>, label: &str, selected: bool) -> Response {
    let galley = ui.painter().layout_no_wrap(label.to_string(), FontId::proportional(14.0), if selected { pal.accent } else { pal.text });
    let w = 26.0 + galley.size().x + if icon.is_some() { 22.0 } else { 0.0 };
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, 34.0), Sense::click());
    let hover = resp.hovered();
    let fill = if selected { pal.accent_soft } else if hover { pal.panel2 } else { pal.panel };
    let stroke = if selected { Stroke::new(1.0_f32, mix(pal.line, pal.accent, 0.7)) } else { Stroke::new(1.0_f32, pal.line) };
    ui.painter().rect(rect, Rounding::same(17.0), fill, stroke);
    let mut x = rect.left() + 13.0;
    let col = if selected { pal.accent } else { pal.text };
    if let Some(ic) = icon {
        icons::paint(ui, Rect::from_min_size(Pos2::new(x, rect.center().y - 8.0), Vec2::splat(16.0)), ic, col);
        x += 22.0;
    }
    ui.painter().galley(Pos2::new(x, rect.center().y - galley.size().y / 2.0), galley, col);
    resp.on_hover_cursor(CursorIcon::PointingHand)
}

pub fn card<R>(ui: &mut Ui, pal: &Palette, add: impl FnOnce(&mut Ui) -> R) -> R {
    egui::Frame::none()
        .fill(pal.panel)
        .stroke(Stroke::new(1.0_f32, pal.line))
        .rounding(Rounding::same(CARD_RADIUS))
        .inner_margin(Margin::same(PAD))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui)
        })
        .inner
}

pub fn card_title(ui: &mut Ui, pal: &Palette, icon: &str, title: &str) {
    ui.horizontal(|ui| {
        icons::show(ui, icon, 18.0, pal.accent);
        ui.label(RichText::new(title).size(16.0).strong());
    });
    ui.add_space(2.0);
}

pub fn muted(ui: &mut Ui, pal: &Palette, text: &str) {
    ui.label(RichText::new(text).color(pal.muted));
}
pub fn small_muted(ui: &mut Ui, pal: &Palette, text: &str) {
    ui.label(RichText::new(text).small().color(pal.muted));
}
pub fn field_label(ui: &mut Ui, pal: &Palette, text: &str) {
    ui.label(RichText::new(text).size(12.5).strong().color(pal.muted));
}

/// Page header: icon tile, title, tagline.
pub fn page_header(ui: &mut Ui, pal: &Palette, icon: &str, title: &str, tagline: &str) {
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(Vec2::splat(52.0), Sense::hover());
        ui.painter().rect_filled(rect, Rounding::same(14.0), pal.accent_soft);
        icons::paint(ui, Rect::from_center_size(rect.center(), Vec2::splat(26.0)), icon, pal.accent);
        ui.add_space(6.0);
        ui.vertical(|ui| {
            ui.label(RichText::new(title).heading().strong());
            muted(ui, pal, tagline);
        });
    });
}

pub fn badge(ui: &mut Ui, text: &str, color: Color32, icon: Option<&str>) {
    let galley = ui.painter().layout_no_wrap(text.to_string(), FontId::proportional(12.5), color);
    let w = galley.size().x + 20.0 + if icon.is_some() { 18.0 } else { 0.0 };
    let (rect, _) = ui.allocate_exact_size(Vec2::new(w, 26.0), Sense::hover());
    ui.painter().rect(rect, Rounding::same(13.0), color.gamma_multiply(0.14), Stroke::new(1.0_f32, color.gamma_multiply(0.35)));
    let mut x = rect.left() + 10.0;
    if let Some(ic) = icon {
        icons::paint(ui, Rect::from_min_size(Pos2::new(x, rect.center().y - 7.0), Vec2::splat(14.0)), ic, color);
        x += 18.0;
    }
    ui.painter().galley(Pos2::new(x, rect.center().y - galley.size().y / 2.0), galley, color);
}

pub fn notice(ui: &mut Ui, pal: &Palette, kind: &str, color: Color32, text: &str) {
    egui::Frame::none()
        .fill(color.gamma_multiply(if pal.dark { 0.16 } else { 0.10 }))
        .stroke(Stroke::new(1.0_f32, color.gamma_multiply(0.45)))
        .rounding(Rounding::same(RADIUS))
        .inner_margin(Margin::same(14.0))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_top(|ui| {
                icons::show(ui, kind, 18.0, color);
                ui.add(egui::Label::new(RichText::new(text).color(pal.text)).wrap_mode(egui::TextWrapMode::Wrap));
            });
        });
}

// ------------------------------------------------------------------ motion
pub fn spinner(ui: &mut Ui, pal: &Palette, size: f32, reduce_motion: bool) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    paint_spinner(ui, rect.center(), size / 2.0 - 2.0, pal.accent, reduce_motion);
}
pub fn paint_spinner(ui: &Ui, center: Pos2, radius: f32, color: Color32, reduce_motion: bool) {
    let t = ui.input(|i| i.time) as f32;
    let start = if reduce_motion { 0.0 } else { t * 5.0 };
    let sweep = if reduce_motion { 4.0 } else { 1.6 + (t * 2.4).sin() * 0.9 + 1.2 };
    let n = 28;
    let pts: Vec<Pos2> = (0..=n)
        .map(|i| {
            let a = start + sweep * i as f32 / n as f32;
            Pos2::new(center.x + radius * a.cos(), center.y + radius * a.sin())
        })
        .collect();
    ui.painter().circle_stroke(center, radius, Stroke::new(2.5_f32, color.gamma_multiply(0.18)));
    ui.painter().add(Shape::line(pts, Stroke::new(2.5_f32, color)));
    if !reduce_motion {
        ui.ctx().request_repaint();
    }
}

pub fn progress_bar(ui: &mut Ui, pal: &Palette, frac: f32, height: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), height), Sense::hover());
    let r = Rounding::same(height / 2.0);
    ui.painter().rect_filled(rect, r, pal.panel2);
    let w = (rect.width() * frac.clamp(0.0, 1.0)).max(height);
    let fill = Rect::from_min_size(rect.min, Vec2::new(w, height));
    ui.painter().rect_filled(fill, r, pal.accent);
    // moving sheen so long jobs never look frozen
    let t = ui.input(|i| i.time) as f32;
    let sx = fill.left() + ((t * 0.9) % 1.0) * fill.width();
    let sheen = Rect::from_min_max(Pos2::new((sx - 24.0).max(fill.left()), fill.top()), Pos2::new((sx + 24.0).min(fill.right()), fill.bottom()));
    if sheen.width() > 2.0 {
        ui.painter().rect_filled(sheen, r, Color32::WHITE.gamma_multiply(0.22));
    }
    ui.ctx().request_repaint();
}

/// The Universal Document PDF Tools logo mark, drawn with the painter so it scales cleanly and follows the theme accent.
pub fn paint_logo(painter: &egui::Painter, center: Pos2, size: f32, pal: &Palette) {
    let rect = Rect::from_center_size(center, Vec2::splat(size));
    painter.rect_filled(rect, Rounding::same(size * 0.23), pal.accent);
    painter.rect_filled(Rect::from_min_max(rect.min, Pos2::new(rect.max.x, rect.center().y)), Rounding { nw: size * 0.23, ne: size * 0.23, sw: 0.0, se: 0.0 }, Color32::WHITE.gamma_multiply(0.10));
    let u = size / 100.0;
    let o = rect.min;
    let p = |x: f32, y: f32| Pos2::new(o.x + x * u, o.y + y * u);
    let white = Color32::WHITE;
    painter.add(Shape::convex_polygon(vec![p(27.0, 18.0), p(58.0, 18.0), p(73.0, 33.0), p(73.0, 81.0), p(27.0, 81.0)], white, Stroke::NONE));
    painter.add(Shape::convex_polygon(vec![p(58.0, 18.0), p(58.0, 33.0), p(73.0, 33.0)], mix(white, pal.accent, 0.3), Stroke::NONE));
    let bar = |y: f32, w: f32| painter.rect_filled(Rect::from_min_size(p(35.0, y), Vec2::new(w * u, 3.6 * u)), Rounding::same(2.0 * u), mix(white, pal.accent, 0.45));
    bar(44.0, 30.0);
    bar(53.0, 30.0);
    bar(62.0, 21.0);
    painter.circle_filled(p(72.0, 78.0), 11.5 * u, white);
    painter.circle_filled(p(72.0, 78.0), 9.4 * u, pal.accent);
    painter.rect_filled(Rect::from_center_size(p(72.0, 78.0), Vec2::new(2.8 * u, 11.0 * u)), Rounding::same(1.4 * u), white);
    painter.rect_filled(Rect::from_center_size(p(72.0, 78.0), Vec2::new(11.0 * u, 2.8 * u)), Rounding::same(1.4 * u), white);
}

pub fn text_at(painter: &egui::Painter, pos: Pos2, align: Align2, text: &str, size: f32, color: Color32) -> Rect {
    painter.text(pos, align, text, FontId::proportional(size), color)
}
