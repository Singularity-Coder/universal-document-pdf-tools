//! Sign & annotate: page canvas with highlight / text / signature / redaction tools, plus form filling.
use crate::jobs::Task;
use crate::kit::{self, Btn};
use crate::theme::*;
use eframe::egui::{self, Color32, Pos2, Rect, RichText, Rounding, Sense, Shape, Stroke, TextureHandle, TextureOptions, Ui, Vec2};
use image::RgbaImage;
use udpt_core::annotate::{self, Annot, AnnotKind, FieldType, FormField};
use udpt_core::Output;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver};
use std::sync::Arc;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tool {
    Highlight,
    Text,
    Signature,
    Redact,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Annotate,
    Forms,
}

struct Pad {
    strokes: Vec<Vec<Pos2>>,
}

pub struct Sign {
    file: Option<PathBuf>,
    bytes: Option<Arc<Vec<u8>>>,
    pages: usize,
    page: usize,
    tex: Option<(usize, TextureHandle)>,
    render_rx: Option<Receiver<(usize, Result<(RgbaImage, f32), String>)>>,
    page_pt_w: f32,
    requested: Option<usize>,
    tab: Tab,
    tool: Tool,
    annots: Vec<Annot>,
    color: [u8; 3],
    text: String,
    size: f32,
    sig_png: Option<Vec<u8>>,
    sig_tex: Option<TextureHandle>,
    pad: Option<Pad>,
    drag_start: Option<Pos2>,
    fields: Vec<FormField>,
    values: HashMap<String, String>,
    flatten: bool,
    task: Option<Task<Output>>,
    result: Option<Output>,
    error: Option<String>,
    status: Option<String>,
}

impl Default for Sign {
    fn default() -> Self {
        Self {
            file: None,
            bytes: None,
            pages: 0,
            page: 0,
            tex: None,
            render_rx: None,
            page_pt_w: 595.0,
            requested: None,
            tab: Tab::Annotate,
            tool: Tool::Highlight,
            annots: vec![],
            color: [255, 230, 0],
            text: "Your text".into(),
            size: 14.0,
            sig_png: None,
            sig_tex: None,
            pad: None,
            drag_start: None,
            fields: vec![],
            values: HashMap::new(),
            flatten: false,
            task: None,
            result: None,
            error: None,
            status: None,
        }
    }
}

impl Sign {
    fn load(&mut self, path: PathBuf) {
        let keep_sig = (self.sig_png.take(), self.sig_tex.take());
        *self = Self::default();
        self.sig_png = keep_sig.0;
        self.sig_tex = keep_sig.1;
        match std::fs::read(&path).map_err(|e| e.to_string()).and_then(|b| udpt_core::pdfops::page_count(&b).map(|n| (b, n)).map_err(|e| e.to_string())) {
            Ok((b, n)) => {
                self.fields = annotate::list_fields(&b).unwrap_or_default();
                self.values = self.fields.iter().map(|f| (f.name.clone(), f.value.clone())).collect();
                self.bytes = Some(Arc::new(b));
                self.pages = n;
                self.file = Some(path);
            }
            Err(e) => self.error = Some(e),
        }
    }

    fn request_page(&mut self, ctx: &egui::Context) {
        if self.requested == Some(self.page) || self.bytes.is_none() {
            return;
        }
        self.requested = Some(self.page);
        let (tx, rx) = channel();
        self.render_rx = Some(rx);
        let bytes = self.bytes.clone().unwrap();
        let page = self.page;
        let repaint = ctx.clone();
        std::thread::spawn(move || {
            let r = udpt_core::render::render_one(&bytes, page, 1400)
                .and_then(|img| udpt_core::render::page_dims(&bytes, page).map(|d| (img, d.0)))
                .map_err(|e| e.to_string());
            let _ = tx.send((page, r));
            repaint.request_repaint();
        });
    }

    pub fn ui(&mut self, ui: &mut Ui, pal: Palette, prefs: &Prefs, dropped: Vec<PathBuf>, renderer_ok: Result<(), String>) {
        let ctx = ui.ctx().clone();
        if let Some(p) = dropped.into_iter().find(|p| p.extension().map(|e| e.eq_ignore_ascii_case("pdf")).unwrap_or(false)) {
            self.load(p);
        }
        if let Some(rx) = &self.render_rx {
            if let Ok((page, r)) = rx.try_recv() {
                match r {
                    Ok((img, w_pt)) => {
                        let ci = egui::ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], img.as_raw());
                        self.page_pt_w = w_pt;
                        self.tex = Some((page, ctx.load_texture(format!("page-{page}"), ci, TextureOptions::LINEAR)));
                    }
                    Err(e) => self.error = Some(e),
                }
            }
        }
        if let Some(t) = &mut self.task {
            if let Some(r) = t.poll() {
                self.task = None;
                match r {
                    Ok(o) => {
                        self.result = Some(o);
                        self.error = None;
                    }
                    Err(e) => self.error = Some(e),
                }
            }
        }

        // ---- top bar
        kit::card(ui, &pal, |ui| {
            ui.horizontal(|ui| {
                if kit::button(ui, &pal, Some("folder"), if self.file.is_some() { "Open another PDF..." } else { "Open PDF..." }, Btn::Secondary, true).clicked() {
                    if let Some(p) = rfd::FileDialog::new().add_filter("PDF", &["pdf"]).pick_file() {
                        self.load(p);
                    }
                }
                if let Some(f) = &self.file {
                    ui.label(RichText::new(f.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()).strong());
                    kit::small_muted(ui, &pal, &format!("{} page(s)", self.pages));
                }
                if self.file.is_some() {
                    ui.add_space(12.0);
                    if kit::chip(ui, &pal, Some("sign"), "Annotate & sign", self.tab == Tab::Annotate).clicked() {
                        self.tab = Tab::Annotate;
                    }
                    if kit::chip(ui, &pal, Some("text"), &format!("Form fields ({})", self.fields.len()), self.tab == Tab::Forms).clicked() {
                        self.tab = Tab::Forms;
                    }
                }
            });
            if let Err(e) = &renderer_ok {
                ui.add_space(6.0);
                kit::notice(ui, &pal, "warn", pal.warn, e);
            }
            if let Some(e) = self.error.clone() {
                ui.add_space(6.0);
                kit::notice(ui, &pal, "warn", pal.danger, &e);
            }
        });
        ui.add_space(GAP);
        if self.file.is_none() {
            kit::card(ui, &pal, |ui| {
                ui.add_space(30.0);
                ui.vertical_centered(|ui| {
                    crate::icons::show(ui, "sign", 40.0, pal.accent);
                    ui.label(RichText::new("Open a PDF to sign, annotate or fill").size(17.0).strong());
                    kit::muted(ui, &pal, "Drop a PDF anywhere on this window, or use the button above.");
                });
                ui.add_space(30.0);
            });
            return;
        }

        if let Some(o) = self.result.clone() {
            kit::card(ui, &pal, |ui| {
                ui.horizontal(|ui| {
                    crate::icons::show(ui, "check", 20.0, pal.ok);
                    ui.label(RichText::new(&o.name).strong());
                    kit::small_muted(ui, &pal, &o.note);
                    if kit::button(ui, &pal, Some("save"), "Save as...", Btn::Primary, true).clicked() {
                        if let Some(path) = rfd::FileDialog::new().set_file_name(&o.name).add_filter("PDF", &["pdf"]).save_file() {
                            self.status = Some(match std::fs::write(&path, &o.data) {
                                Ok(_) => format!("Saved to {}", path.display()),
                                Err(e) => format!("Could not save: {e}"),
                            });
                        }
                    }
                    if kit::button(ui, &pal, Some("x"), "Dismiss", Btn::Ghost, true).clicked() {
                        self.result = None;
                    }
                });
                if let Some(s) = &self.status {
                    kit::small_muted(ui, &pal, s);
                }
            });
            ui.add_space(GAP);
        }

        match self.tab {
            Tab::Annotate => self.annotate_tab(ui, pal, prefs, renderer_ok.is_ok()),
            Tab::Forms => self.forms_tab(ui, pal),
        }
        self.signature_pad(&ctx, pal);
    }

    // ------------------------------------------------------------ annotate tab
    fn annotate_tab(&mut self, ui: &mut Ui, pal: Palette, prefs: &Prefs, renderer_ok: bool) {
        let ctx = ui.ctx().clone();
        if renderer_ok {
            self.request_page(&ctx);
        }
        let total_w = ui.available_width();
        let side_w = 340.0f32.min(total_w * 0.34);
        let page_w = total_w - side_w - GAP;
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = GAP;
            ui.allocate_ui_with_layout(Vec2::new(page_w, ui.available_height()), egui::Layout::top_down(egui::Align::Min), |ui| {
                self.canvas(ui, pal, prefs, page_w);
            });
            ui.allocate_ui_with_layout(Vec2::new(side_w, ui.available_height()), egui::Layout::top_down(egui::Align::Min), |ui| {
                self.side(ui, pal, prefs);
            });
        });
    }

    fn canvas(&mut self, ui: &mut Ui, pal: Palette, prefs: &Prefs, width: f32) {
        kit::card(ui, &pal, |ui| {
            ui.horizontal(|ui| {
                if kit::icon_button(ui, &pal, "left", "Previous page", self.page > 0, 32.0).clicked() {
                    self.page -= 1;
                }
                ui.label(RichText::new(format!("Page {} of {}", self.page + 1, self.pages)).strong());
                if kit::icon_button(ui, &pal, "right", "Next page", self.page + 1 < self.pages, 32.0).clicked() {
                    self.page += 1;
                }
                ui.add_space(8.0);
                kit::small_muted(ui, &pal, "Drag on the page to place the selected tool.");
            });
            ui.add_space(6.0);
            let Some((tp, tex)) = &self.tex else {
                ui.add_space(60.0);
                ui.vertical_centered(|ui| {
                    kit::spinner(ui, &pal, 30.0, prefs.reduce_motion);
                    kit::muted(ui, &pal, "Rendering page...");
                });
                ui.add_space(60.0);
                return;
            };
            let avail = (width - 2.0 * PAD).max(200.0);
            let sz = tex.size_vec2();
            let s = (avail / sz.x).max(0.1);
            let size = sz * s;
            let ppp = size.x / self.page_pt_w.max(1.0); // screen px per PDF point
            let tex_id = tex.id();
            let shown_page = *tp;
            let (rect, resp) = ui.allocate_exact_size(size, Sense::click_and_drag());
            ui.painter().rect_filled(rect.expand(1.0), Rounding::same(2.0), pal.line);
            ui.painter().image(tex_id, rect, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), if shown_page == self.page { Color32::WHITE } else { Color32::WHITE.gamma_multiply(0.5) });

            // existing marks on this page
            for a in self.annots.iter().filter(|a| a.page == shown_page) {
                let r = Rect::from_min_size(rect.min + Vec2::new(a.x * rect.width(), a.y * rect.height()), Vec2::new(a.w * rect.width(), a.h * rect.height()));
                match a.kind {
                    AnnotKind::Highlight => {
                        let c = hex(&a.color);
                        ui.painter().rect_filled(r, Rounding::ZERO, Color32::from_rgba_unmultiplied(c[0], c[1], c[2], 105));
                    }
                    AnnotKind::Redact => {
                        ui.painter().rect_filled(r, Rounding::ZERO, Color32::from_black_alpha(230));
                    }
                    AnnotKind::Text => {
                        let c = hex(&a.color);
                        let font = egui::FontId::proportional((a.size * ppp).max(6.0));
                        let g = ui.painter().layout(a.text.clone(), font, Color32::from_rgb(c[0], c[1], c[2]), r.width().max(40.0));
                        ui.painter().galley(r.min, g, Color32::BLACK);
                        ui.painter().rect_stroke(r, Rounding::ZERO, Stroke::new(1.0_f32, pal.accent.gamma_multiply(0.5)));
                    }
                    AnnotKind::Image => {
                        if let Some(t) = &self.sig_tex {
                            let ts = t.size_vec2();
                            let k = (r.width() / ts.x).min(r.height() / ts.y);
                            let dr = Rect::from_center_size(r.center(), ts * k);
                            ui.painter().image(t.id(), dr, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
                        }
                        ui.painter().rect_stroke(r, Rounding::ZERO, Stroke::new(1.0_f32, pal.accent.gamma_multiply(0.5)));
                    }
                }
            }

            // placing a new mark
            if resp.drag_started() {
                self.drag_start = resp.interact_pointer_pos();
            }
            if let (Some(start), Some(cur)) = (self.drag_start, resp.interact_pointer_pos()) {
                let r = Rect::from_two_pos(start, cur).intersect(rect);
                let col = match self.tool {
                    Tool::Highlight => Color32::from_rgba_unmultiplied(self.color[0], self.color[1], self.color[2], 100),
                    Tool::Redact => Color32::from_black_alpha(200),
                    _ => pal.accent.gamma_multiply(0.25),
                };
                ui.painter().rect_filled(r, Rounding::ZERO, col);
                ui.painter().rect_stroke(r, Rounding::ZERO, Stroke::new(1.5_f32, pal.accent));
            }
            let mut placed: Option<Rect> = None;
            if resp.drag_stopped() {
                if let (Some(start), Some(end)) = (self.drag_start.take(), resp.interact_pointer_pos().or(resp.hover_pos())) {
                    placed = Some(Rect::from_two_pos(start, end).intersect(rect));
                }
            } else if resp.clicked() && matches!(self.tool, Tool::Text | Tool::Signature) {
                if let Some(p) = resp.interact_pointer_pos() {
                    let (w, h) = if self.tool == Tool::Text { (rect.width() * 0.3, self.size * 2.4 * ppp) } else { (rect.width() * 0.28, rect.width() * 0.28 * 0.4) };
                    placed = Some(Rect::from_min_size(p, Vec2::new(w, h)).intersect(rect));
                }
            }
            if let Some(r) = placed {
                if r.width() > 6.0 && r.height() > 6.0 {
                    self.place(r, rect, shown_page);
                } else if matches!(self.tool, Tool::Highlight | Tool::Redact) {
                    self.error = Some("Drag a rectangle to mark an area.".into());
                }
            }
        });
    }

    fn place(&mut self, r: Rect, page_rect: Rect, page: usize) {
        let nx = (r.min.x - page_rect.min.x) / page_rect.width();
        let ny = (r.min.y - page_rect.min.y) / page_rect.height();
        let (nw, nh) = (r.width() / page_rect.width(), r.height() / page_rect.height());
        let color = format!("#{:02x}{:02x}{:02x}", self.color[0], self.color[1], self.color[2]);
        let mut a = Annot { page, kind: AnnotKind::Highlight, x: nx, y: ny, w: nw, h: nh, text: String::new(), size: self.size, color, image: None };
        match self.tool {
            Tool::Highlight => {}
            Tool::Redact => a.kind = AnnotKind::Redact,
            Tool::Text => {
                if self.text.trim().is_empty() {
                    self.error = Some("Type the text to place first.".into());
                    return;
                }
                a.kind = AnnotKind::Text;
                a.text = self.text.clone();
                // convert on-screen height into a size in PDF points is handled by `size`
            }
            Tool::Signature => {
                let Some(png) = &self.sig_png else {
                    self.error = Some("Draw or upload a signature first (Signature tool).".into());
                    return;
                };
                a.kind = AnnotKind::Image;
                a.image = Some(png.clone());
            }
        }
        self.error = None;
        self.annots.push(a);
    }

    fn side(&mut self, ui: &mut Ui, pal: Palette, prefs: &Prefs) {
        kit::card(ui, &pal, |ui| {
            kit::card_title(ui, &pal, "sign", "Tools");
            ui.horizontal_wrapped(|ui| {
                for (t, icon, label) in [(Tool::Highlight, "highlight", "Highlight"), (Tool::Text, "text", "Text"), (Tool::Signature, "sign", "Signature"), (Tool::Redact, "redact", "Redact")] {
                    if kit::chip(ui, &pal, Some(icon), label, self.tool == t).clicked() {
                        self.tool = t;
                    }
                }
            });
            ui.add_space(6.0);
            match self.tool {
                Tool::Highlight => {
                    kit::field_label(ui, &pal, "Colour");
                    ui.color_edit_button_srgb(&mut self.color);
                    kit::small_muted(ui, &pal, "Drag over text to highlight it.");
                }
                Tool::Text => {
                    kit::field_label(ui, &pal, "Text");
                    ui.add(egui::TextEdit::multiline(&mut self.text).desired_rows(2).desired_width(f32::INFINITY));
                    kit::field_label(ui, &pal, "Size");
                    ui.add(egui::Slider::new(&mut self.size, 8.0..=48.0).trailing_fill(true));
                    kit::field_label(ui, &pal, "Colour");
                    ui.color_edit_button_srgb(&mut self.color);
                    kit::small_muted(ui, &pal, "Click or drag on the page to place the text. Latin characters only.");
                }
                Tool::Signature => {
                    ui.horizontal(|ui| {
                        if kit::button(ui, &pal, Some("sign"), "Draw...", Btn::Secondary, true).clicked() {
                            self.pad = Some(Pad { strokes: vec![] });
                        }
                        if kit::button(ui, &pal, Some("upload"), "Upload...", Btn::Secondary, true).clicked() {
                            if let Some(p) = rfd::FileDialog::new().add_filter("Image", &["png", "jpg", "jpeg"]).pick_file() {
                                match std::fs::read(&p) {
                                    Ok(b) => self.set_signature(ui.ctx(), b),
                                    Err(e) => self.error = Some(format!("Could not read the image: {e}")),
                                }
                            }
                        }
                    });
                    if let Some(t) = &self.sig_tex {
                        let ts = t.size_vec2();
                        let k = (ui.available_width().min(260.0) / ts.x).min(70.0 / ts.y);
                        egui::Frame::none().fill(Color32::WHITE).rounding(Rounding::same(8.0)).inner_margin(egui::Margin::same(6.0)).show(ui, |ui| {
                            ui.image((t.id(), ts * k));
                        });
                        kit::small_muted(ui, &pal, "Click or drag on the page to place it.");
                    } else {
                        kit::small_muted(ui, &pal, "No signature yet. Draw one or upload a picture (a PNG with a transparent background works best).");
                    }
                }
                Tool::Redact => {
                    kit::notice(ui, &pal, "info", pal.accent, "Redaction really removes the content: the page is converted to a picture with black boxes burned in. Text on that page is no longer selectable.");
                }
            }
        });
        ui.add_space(GAP);
        kit::card(ui, &pal, |ui| {
            kit::card_title(ui, &pal, "file", &format!("Placed items ({})", self.annots.len()));
            if self.annots.is_empty() {
                kit::small_muted(ui, &pal, "Nothing placed yet.");
            }
            let mut del: Option<usize> = None;
            egui::ScrollArea::vertical().max_height(220.0).id_salt("annots").show(ui, |ui| {
                for (i, a) in self.annots.iter().enumerate() {
                    ui.horizontal(|ui| {
                        let kind = match a.kind {
                            AnnotKind::Highlight => "Highlight",
                            AnnotKind::Text => "Text",
                            AnnotKind::Image => "Signature",
                            AnnotKind::Redact => "Redaction",
                        };
                        ui.label(RichText::new(format!("Page {}  \u{b7}  {kind}", a.page + 1)));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if kit::icon_button(ui, &pal, "trash", "Remove", true, 28.0).clicked() {
                                del = Some(i);
                            }
                        });
                    });
                }
            });
            if let Some(i) = del {
                self.annots.remove(i);
            }
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                let busy = self.task.is_some();
                if busy {
                    kit::spinner(ui, &pal, 22.0, prefs.reduce_motion);
                    kit::muted(ui, &pal, "Applying...");
                } else if kit::button(ui, &pal, Some("check"), "Apply to PDF", Btn::Primary, !self.annots.is_empty()).clicked() {
                    self.apply(ui.ctx());
                }
                if !self.annots.is_empty() && !busy && kit::button(ui, &pal, Some("trash"), "Clear", Btn::Ghost, true).clicked() {
                    self.annots.clear();
                }
            });
        });
    }

    fn apply(&mut self, ctx: &egui::Context) {
        let Some(bytes) = self.bytes.clone() else { return };
        let annots = self.annots.clone();
        let name = format!("{}-signed.pdf", udpt_core::base_name(&self.file.as_ref().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().to_string()).unwrap_or_default()));
        self.result = None;
        self.task = Some(Task::spawn(ctx, move |c| {
            let (data, redacted) = annotate::apply(&bytes, &annots, &c)?;
            let note = if redacted > 0 { format!("{} mark(s) applied; {redacted} page(s) redacted", annots.len()) } else { format!("{} mark(s) applied", annots.len()) };
            Ok(Output::new(name, data).note(note))
        }));
    }

    fn set_signature(&mut self, ctx: &egui::Context, bytes: Vec<u8>) {
        match image::load_from_memory(&bytes) {
            Ok(img) => {
                let rgba = img.to_rgba8();
                let ci = egui::ColorImage::from_rgba_unmultiplied([rgba.width() as usize, rgba.height() as usize], rgba.as_raw());
                self.sig_tex = Some(ctx.load_texture("signature", ci, TextureOptions::LINEAR));
                self.sig_png = Some(bytes);
                self.error = None;
            }
            Err(e) => self.error = Some(format!("That picture could not be read: {e}")),
        }
    }

    // ------------------------------------------------------------ signature pad
    fn signature_pad(&mut self, ctx: &egui::Context, pal: Palette) {
        if self.pad.is_none() {
            return;
        }
        let mut close = false;
        let mut use_it = false;
        egui::Window::new("Draw your signature").collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
            let (rect, resp) = ui.allocate_exact_size(Vec2::new(560.0, 220.0), Sense::drag());
            ui.painter().rect(rect, Rounding::same(10.0), Color32::WHITE, Stroke::new(1.0_f32, pal.line));
            let base = rect.left_bottom() + Vec2::new(0.0, -50.0);
            ui.painter().line_segment([base + Vec2::new(18.0, 0.0), base + Vec2::new(rect.width() - 18.0, 0.0)], Stroke::new(1.0_f32, Color32::from_gray(200)));
            ui.painter().text(rect.left_top() + Vec2::new(16.0, 14.0), egui::Align2::LEFT_TOP, "Sign here with the mouse, pen or finger", egui::FontId::proportional(12.5), Color32::from_gray(150));
            if let Some(pad) = &mut self.pad {
                if resp.drag_started() {
                    pad.strokes.push(vec![]);
                }
                if resp.dragged() {
                    if let (Some(p), Some(s)) = (resp.interact_pointer_pos(), pad.strokes.last_mut()) {
                        s.push((p - rect.min).to_pos2());
                    }
                }
                for s in &pad.strokes {
                    let pts: Vec<Pos2> = s.iter().map(|p| rect.min + p.to_vec2()).collect();
                    if pts.len() > 1 {
                        ui.painter().add(Shape::line(pts, Stroke::new(2.6_f32, Color32::from_rgb(20, 30, 120))));
                    } else if let Some(p) = pts.first() {
                        ui.painter().circle_filled(*p, 1.5, Color32::from_rgb(20, 30, 120));
                    }
                }
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if kit::button(ui, &pal, Some("trash"), "Clear", Btn::Secondary, true).clicked() {
                    if let Some(p) = &mut self.pad {
                        p.strokes.clear();
                    }
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let has_ink = self.pad.as_ref().map(|p| p.strokes.iter().any(|s| s.len() > 1)).unwrap_or(false);
                    if kit::button(ui, &pal, Some("check"), "Use signature", Btn::Primary, has_ink).clicked() {
                        use_it = true;
                    }
                    if kit::button(ui, &pal, None, "Cancel", Btn::Ghost, true).clicked() {
                        close = true;
                    }
                });
            });
        });
        if use_it {
            if let Some(pad) = self.pad.take() {
                match rasterize_strokes(&pad.strokes) {
                    Some(png) => self.set_signature(ctx, png),
                    None => self.error = Some("The signature was empty.".into()),
                }
            }
        } else if close {
            self.pad = None;
        }
    }

    // ------------------------------------------------------------ forms tab
    fn forms_tab(&mut self, ui: &mut Ui, pal: Palette) {
        kit::card(ui, &pal, |ui| {
            kit::card_title(ui, &pal, "text", "Fill form fields");
            if self.fields.is_empty() {
                kit::muted(ui, &pal, "This PDF has no fillable form fields. Use the Annotate & sign tab to type text anywhere on a page.");
                return;
            }
            let mut fields = std::mem::take(&mut self.fields);
            for f in &mut fields {
                let mut cur = self.values.get(&f.name).cloned().unwrap_or_default();
                match f.kind {
                    FieldType::Text => {
                        kit::field_label(ui, &pal, &f.name);
                        ui.add(egui::TextEdit::singleline(&mut cur).desired_width(f32::INFINITY).margin(Vec2::new(10.0, 8.0)));
                    }
                    FieldType::Check => {
                        let mut b = cur == "true";
                        ui.checkbox(&mut b, &f.name);
                        cur = b.to_string();
                    }
                    FieldType::Choice => {
                        kit::field_label(ui, &pal, &f.name);
                        egui::ComboBox::from_id_salt(&f.name).width(ui.available_width().min(420.0)).selected_text(if cur.is_empty() { "(none)".to_string() } else { cur.clone() }).show_ui(ui, |ui| {
                            for o in &f.options {
                                if ui.selectable_label(&cur == o, o).clicked() {
                                    cur = o.clone();
                                }
                            }
                        });
                    }
                }
                ui.add_space(4.0);
                self.values.insert(f.name.clone(), cur);
            }
            self.fields = fields;
            ui.add_space(6.0);
            ui.checkbox(&mut self.flatten, "Flatten (make the filled values permanent and remove the fields)");
            ui.add_space(6.0);
            if kit::button(ui, &pal, Some("check"), "Fill form", Btn::Primary, self.task.is_none()).clicked() {
                let Some(bytes) = self.bytes.clone() else { return };
                let values = self.values.clone();
                let flatten = self.flatten;
                let name = format!("{}-filled.pdf", udpt_core::base_name(&self.file.as_ref().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().to_string()).unwrap_or_default()));
                self.result = None;
                self.task = Some(Task::spawn(ui.ctx(), move |_c| {
                    let data = annotate::fill(&bytes, &values, flatten)?;
                    Ok(Output::new(name, data).note(if flatten { "Flattened" } else { "Fields remain editable" }))
                }));
            }
        });
    }
}

fn hex(h: &str) -> [u8; 3] {
    let n = u32::from_str_radix(h.trim().trim_start_matches('#'), 16).unwrap_or(0);
    [(n >> 16) as u8, (n >> 8) as u8, n as u8]
}

/// Draw the strokes into a transparent PNG (2x supersampled), cropped to the ink.
fn rasterize_strokes(strokes: &[Vec<Pos2>]) -> Option<Vec<u8>> {
    let scale = 2.0f32;
    let (w, h) = (560.0 * scale, 220.0 * scale);
    let mut img = RgbaImage::new(w as u32, h as u32);
    let r = 3.2 * scale;
    let ink = image::Rgba([20, 30, 120, 255]);
    let (mut minx, mut miny, mut maxx, mut maxy) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for s in strokes {
        for (i, p) in s.iter().enumerate() {
            let a = p.to_vec2() * scale;
            let b = if i + 1 < s.len() { s[i + 1].to_vec2() * scale } else { a };
            let d = (b - a).length();
            let steps = (d / 1.0).ceil().max(1.0) as i32;
            for k in 0..=steps {
                let t = k as f32 / steps as f32;
                let c = a + (b - a) * t;
                let (x0, x1) = ((c.x - r).floor().max(0.0) as i32, (c.x + r).ceil().min(w - 1.0) as i32);
                let (y0, y1) = ((c.y - r).floor().max(0.0) as i32, (c.y + r).ceil().min(h - 1.0) as i32);
                for yy in y0..=y1 {
                    for xx in x0..=x1 {
                        let (dx, dy) = (xx as f32 - c.x, yy as f32 - c.y);
                        if dx * dx + dy * dy <= r * r {
                            img.put_pixel(xx as u32, yy as u32, ink);
                        }
                    }
                }
                minx = minx.min(c.x - r);
                miny = miny.min(c.y - r);
                maxx = maxx.max(c.x + r);
                maxy = maxy.max(c.y + r);
            }
        }
    }
    if minx > maxx {
        return None;
    }
    let m = 8.0;
    let (cx, cy) = ((minx - m).max(0.0) as u32, (miny - m).max(0.0) as u32);
    let (cw, ch) = (((maxx + m).min(w) as u32).saturating_sub(cx).max(1), ((maxy + m).min(h) as u32).saturating_sub(cy).max(1));
    let cropped = image::imageops::crop_imm(&img, cx, cy, cw, ch).to_image();
    udpt_core::render::encode_png(&cropped).ok()
}
