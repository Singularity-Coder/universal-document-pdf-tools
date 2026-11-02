//! Visual page organizer: thumbnails with move / rotate / duplicate / delete, then export.
use crate::jobs::Task;
use crate::kit::{self, Btn};
use crate::theme::*;
use eframe::egui::{self, Color32, Rect, RichText, Rounding, Sense, Stroke, TextureHandle, TextureOptions, Ui, Vec2};
use udpt_core::{fmt_size, Output};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver};
use std::sync::Arc;

struct Card {
    src: usize,
    rot: i32,
    del: bool,
}
enum Thumb {
    Total(usize),
    Page(usize, image::RgbaImage),
    Failed(String),
}

#[derive(Default)]
pub struct Organizer {
    file: Option<PathBuf>,
    bytes: Option<Arc<Vec<u8>>>,
    cards: Vec<Card>,
    thumbs: HashMap<usize, TextureHandle>,
    rx: Option<Receiver<Thumb>>,
    total: usize,
    export: Option<Task<Vec<u8>>>,
    result: Option<Output>,
    error: Option<String>,
    status: Option<String>,
}

impl Organizer {
    pub fn load(&mut self, ctx: &egui::Context, path: PathBuf) {
        *self = Self::default();
        let bytes = match std::fs::read(&path) {
            Ok(b) => Arc::new(b),
            Err(e) => {
                self.error = Some(format!("Could not read the file: {e}"));
                return;
            }
        };
        self.file = Some(path);
        self.bytes = Some(bytes.clone());
        let (tx, rx) = channel();
        self.rx = Some(rx);
        let repaint = ctx.clone();
        std::thread::spawn(move || {
            let n = match udpt_core::pdfops::page_count(&bytes) {
                Ok(n) => n,
                Err(e) => {
                    let _ = tx.send(Thumb::Failed(e.to_string()));
                    return;
                }
            };
            let _ = tx.send(Thumb::Total(n));
            repaint.request_repaint();
            let idx: Vec<usize> = (0..n).collect();
            let r = udpt_core::render::render_many(&bytes, &idx, 260, |i, img| {
                let ok = tx.send(Thumb::Page(i, img)).is_ok();
                repaint.request_repaint();
                ok
            });
            if let Err(e) = r {
                let _ = tx.send(Thumb::Failed(e.to_string()));
                repaint.request_repaint();
            }
        });
    }

    fn pump(&mut self, ctx: &egui::Context) {
        let mut msgs = Vec::new();
        if let Some(rx) = &self.rx {
            while let Ok(m) = rx.try_recv() {
                msgs.push(m);
            }
        }
        for m in msgs {
            match m {
                Thumb::Total(n) => {
                    self.total = n;
                    self.cards = (0..n).map(|src| Card { src, rot: 0, del: false }).collect();
                }
                Thumb::Page(i, img) => {
                    let ci = egui::ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], img.as_raw());
                    self.thumbs.insert(i, ctx.load_texture(format!("thumb-{i}"), ci, TextureOptions::LINEAR));
                }
                Thumb::Failed(e) => self.error = Some(e),
            }
        }
    }

    pub fn ui(&mut self, ui: &mut Ui, pal: Palette, prefs: &Prefs, dropped: Vec<PathBuf>, renderer_ok: Result<(), String>) {
        let ctx = ui.ctx().clone();
        if let Some(p) = dropped.into_iter().find(|p| p.extension().map(|e| e.eq_ignore_ascii_case("pdf")).unwrap_or(false)) {
            self.load(&ctx, p);
        }
        self.pump(&ctx);
        if let Some(t) = &mut self.export {
            if let Some(r) = t.poll() {
                self.export = None;
                match r {
                    Ok(data) => {
                        let name = format!("{}-organized.pdf", udpt_core::base_name(&self.file_name()));
                        let note = format!("{} page(s)", self.cards.iter().filter(|c| !c.del).count());
                        self.result = Some(Output::new(name, data).note(note));
                        self.error = None;
                    }
                    Err(e) => self.error = Some(e),
                }
            }
        }

        if let Err(e) = &renderer_ok {
            kit::notice(ui, &pal, "warn", pal.warn, e);
            ui.add_space(GAP);
        }
        kit::card(ui, &pal, |ui| {
            ui.horizontal(|ui| {
                if kit::button(ui, &pal, Some("folder"), if self.file.is_some() { "Open another PDF..." } else { "Open PDF..." }, Btn::Secondary, renderer_ok.is_ok() && self.export.is_none()).clicked() {
                    if let Some(p) = rfd::FileDialog::new().add_filter("PDF", &["pdf"]).pick_file() {
                        self.load(&ctx, p);
                    }
                }
                if self.file.is_some() {
                    ui.label(RichText::new(self.file_name()).strong());
                    let kept = self.cards.iter().filter(|c| !c.del).count();
                    kit::small_muted(ui, &pal, &format!("{kept} of {} page(s) will be exported", self.cards.len()));
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let can = self.file.is_some() && self.export.is_none() && self.cards.iter().any(|c| !c.del);
                    if self.export.is_some() {
                        kit::spinner(ui, &pal, 22.0, prefs.reduce_motion);
                    } else if kit::button(ui, &pal, Some("check"), "Export PDF", Btn::Primary, can).clicked() {
                        let bytes = self.bytes.clone().unwrap_or_default();
                        let plan: Vec<(usize, i32)> = self.cards.iter().filter(|c| !c.del).map(|c| (c.src, c.rot.rem_euclid(360))).collect();
                        self.result = None;
                        self.export = Some(Task::spawn(&ctx, move |c| udpt_core::organize_export(&bytes, &plan, &c)));
                    }
                });
            });
            if let Some(e) = self.error.clone() {
                ui.add_space(6.0);
                kit::notice(ui, &pal, "warn", pal.danger, &e);
            }
            if let Some(s) = &self.status {
                ui.add_space(6.0);
                kit::notice(ui, &pal, "check", pal.ok, s);
            }
            if let Some(o) = &self.result {
                ui.add_space(8.0);
                let mut save = false;
                ui.horizontal(|ui| {
                    crate::icons::show(ui, "file", 20.0, pal.accent);
                    ui.label(RichText::new(&o.name).strong());
                    kit::small_muted(ui, &pal, &format!("{}  \u{b7}  {}", fmt_size(o.data.len()), o.note));
                    if kit::button(ui, &pal, Some("save"), "Save as...", Btn::Primary, true).clicked() {
                        save = true;
                    }
                });
                if save {
                    if let Some(path) = rfd::FileDialog::new().set_file_name(&o.name).add_filter("PDF", &["pdf"]).save_file() {
                        self.status = Some(match std::fs::write(&path, &o.data) {
                            Ok(_) => format!("Saved to {}", path.display()),
                            Err(e) => format!("Could not save: {e}"),
                        });
                    }
                }
            }
        });
        ui.add_space(GAP);

        if self.file.is_none() {
            kit::card(ui, &pal, |ui| {
                ui.add_space(30.0);
                ui.vertical_centered(|ui| {
                    crate::icons::show(ui, "organize", 40.0, pal.accent);
                    ui.label(RichText::new("Open a PDF to see its pages").size(17.0).strong());
                    kit::muted(ui, &pal, "You can also drop a PDF anywhere on this window.");
                });
                ui.add_space(30.0);
            });
            return;
        }
        if self.cards.is_empty() && self.error.is_none() {
            ui.horizontal(|ui| {
                kit::spinner(ui, &pal, 22.0, prefs.reduce_motion);
                kit::muted(ui, &pal, "Reading pages...");
            });
            return;
        }
        self.grid(ui, pal);
    }

    fn file_name(&self) -> String {
        self.file.as_ref().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().to_string()).unwrap_or_default()
    }

    fn grid(&mut self, ui: &mut Ui, pal: Palette) {
        enum Act {
            Move(usize, isize),
            Rot(usize, i32),
            Dup(usize),
            Del(usize),
        }
        let mut act: Option<Act> = None;
        let (cw, ch) = (196.0, 300.0);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = Vec2::splat(GAP);
            let n = self.cards.len();
            for (i, card) in self.cards.iter().enumerate() {
                let (rect, _) = ui.allocate_exact_size(Vec2::new(cw, ch), Sense::hover());
                let fill = if card.del { pal.panel2.gamma_multiply(0.6) } else { pal.panel };
                ui.painter().rect(rect, Rounding::same(CARD_RADIUS), fill, Stroke::new(1.0_f32, pal.line));
                // picture area
                let area = Rect::from_min_size(rect.min + Vec2::new(10.0, 10.0), Vec2::new(cw - 20.0, 214.0));
                if let Some(tex) = self.thumbs.get(&card.src) {
                    let sz = tex.size_vec2();
                    let odd = card.rot.rem_euclid(180) == 90;
                    let (vw, vh) = if odd { (sz.y, sz.x) } else { (sz.x, sz.y) };
                    let s = (area.width() / vw).min(area.height() / vh);
                    let draw = Rect::from_center_size(area.center(), sz * s);
                    let tint = if card.del { Color32::WHITE.gamma_multiply(0.3) } else { Color32::WHITE };
                    ui.painter().rect_filled(draw.expand(2.0).intersect(area.expand(2.0)), Rounding::same(3.0), pal.line);
                    egui::Image::new((tex.id(), sz)).rotate((card.rot as f32).to_radians(), Vec2::splat(0.5)).tint(tint).paint_at(ui, draw);
                } else {
                    kit::paint_spinner(ui, area.center(), 14.0, pal.accent, false);
                }
                let label = format!("Page {}{}", card.src + 1, if card.rot != 0 { format!("  ({:+}\u{b0})", card.rot) } else { String::new() });
                ui.painter().text(egui::pos2(rect.center().x, rect.top() + 236.0), egui::Align2::CENTER_CENTER, label, egui::FontId::proportional(13.0), if card.del { pal.danger } else { pal.muted });
                // action buttons
                let mut x = rect.left() + 10.0;
                let y = rect.top() + 252.0;
                let defs: [(&str, &str, bool); 6] = [("left", "Move earlier", i > 0), ("rotl", "Rotate left", true), ("rotr", "Rotate right", true), ("copy", "Duplicate", true), (if card.del { "check" } else { "trash" }, if card.del { "Restore" } else { "Delete" }, true), ("right", "Move later", i + 1 < n)];
                for (k, (icon, tip, en)) in defs.iter().enumerate() {
                    let r = Rect::from_min_size(egui::pos2(x, y), Vec2::splat(28.0));
                    let mut child = ui.child_ui(r, egui::Layout::left_to_right(egui::Align::Center), None);
                    if kit::icon_button(&mut child, &pal, icon, tip, *en, 28.0).clicked() {
                        act = Some(match k {
                            0 => Act::Move(i, -1),
                            1 => Act::Rot(i, -90),
                            2 => Act::Rot(i, 90),
                            3 => Act::Dup(i),
                            4 => Act::Del(i),
                            _ => Act::Move(i, 1),
                        });
                    }
                    x += 29.5;
                }
            }
        });
        match act {
            Some(Act::Move(i, d)) => {
                let j = i as isize + d;
                if j >= 0 && (j as usize) < self.cards.len() {
                    self.cards.swap(i, j as usize);
                }
            }
            Some(Act::Rot(i, d)) => self.cards[i].rot = (self.cards[i].rot + d).rem_euclid(360),
            Some(Act::Dup(i)) => {
                let c = &self.cards[i];
                let copy = Card { src: c.src, rot: c.rot, del: false };
                self.cards.insert(i + 1, copy);
            }
            Some(Act::Del(i)) => self.cards[i].del = !self.cards[i].del,
            None => {}
        }
    }
}
