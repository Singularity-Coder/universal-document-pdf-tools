//! Generic page for every form-driven action: file list (drag & drop), options, progress and results.
use crate::jobs::Task;
use crate::kit::{self, Btn};
use crate::theme::*;
use eframe::egui::{self, Align2, Color32, FontId, Rect, RichText, Rounding, Sense, Stroke, Ui, Vec2};
use udpt_core::actions::{self, ActionDef, Field, FieldKind, InputKind};
use udpt_core::{fmt_size, office, Capabilities, Input, Output, Values};
use std::path::PathBuf;

pub struct ActionPage {
    pub files: Vec<PathBuf>,
    pub values: Values,
    task: Option<Task<Vec<Output>>>,
    results: Vec<Output>,
    error: Option<String>,
    status: Option<String>,
    preview_open: Option<usize>,
}

impl ActionPage {
    pub fn new(def: &ActionDef) -> Self {
        Self { files: vec![], values: actions::default_values(def), task: None, results: vec![], error: None, status: None, preview_open: None }
    }

    fn accepts(kind: InputKind, p: &std::path::Path) -> bool {
        let ext = p.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
        match kind {
            InputKind::Any => office::kind_of(&ext).is_some(),
            _ => ext == "pdf",
        }
    }
    pub fn add_paths(&mut self, kind: InputKind, paths: Vec<PathBuf>) {
        let (ok, bad): (Vec<_>, Vec<_>) = paths.into_iter().partition(|p| Self::accepts(kind, p));
        if !bad.is_empty() {
            self.error = Some(format!("{} file(s) were skipped because this tool does not support their type.", bad.len()));
        }
        match kind {
            InputKind::Pdf => {
                if let Some(last) = ok.into_iter().last() {
                    self.files = vec![last];
                    self.error = None;
                }
            }
            InputKind::Pdf2 => {
                self.files.extend(ok);
                while self.files.len() > 2 {
                    self.files.remove(0);
                }
            }
            _ => self.files.extend(ok),
        }
    }
    fn ready(&self, kind: InputKind) -> bool {
        match kind {
            InputKind::Any => !self.files.is_empty(),
            InputKind::Pdfs => self.files.len() >= 2,
            InputKind::Pdf => self.files.len() == 1,
            InputKind::Pdf2 => self.files.len() == 2,
        }
    }
    fn busy(&self) -> bool {
        self.task.is_some()
    }

    #[allow(clippy::too_many_arguments)]
    pub fn ui(&mut self, ui: &mut Ui, pal: Palette, prefs: &Prefs, module_id: &'static str, def: &ActionDef, caps: &Capabilities, dropped: Vec<PathBuf>) {
        // poll worker
        if let Some(t) = &mut self.task {
            if let Some(res) = t.poll() {
                self.task = None;
                match res {
                    Ok(out) => {
                        self.results = out;
                        self.error = None;
                        self.status = None;
                        self.preview_open = if self.results.len() == 1 && self.results[0].preview.is_some() { Some(0) } else { None };
                    }
                    Err(e) => {
                        self.error = Some(e);
                        self.results.clear();
                    }
                }
            }
        }
        if !dropped.is_empty() && !self.busy() {
            self.add_paths(def.input, dropped);
        }

        let avail = ui.available_width();
        let left_w = (avail * 0.42).clamp(400.0, 560.0).min(avail - 320.0).max(320.0);
        let right_w = (avail - left_w - GAP).max(280.0);
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = GAP;
            ui.allocate_ui_with_layout(Vec2::new(left_w, ui.available_height()), egui::Layout::top_down(egui::Align::Min), |ui| {
                ui.set_width(left_w);
                self.left(ui, pal, prefs, module_id, def, caps);
            });
            ui.allocate_ui_with_layout(Vec2::new(right_w, ui.available_height()), egui::Layout::top_down(egui::Align::Min), |ui| {
                ui.set_width(right_w);
                self.right(ui, pal, prefs, def);
            });
        });
    }

    // ------------------------------------------------------------ left column
    fn left(&mut self, ui: &mut Ui, pal: Palette, prefs: &Prefs, module_id: &'static str, def: &ActionDef, caps: &Capabilities) {
        kit::card(ui, &pal, |ui| {
            kit::card_title(ui, &pal, "file", match def.input {
                InputKind::Any => "Files",
                InputKind::Pdfs => "PDF files",
                InputKind::Pdf => "PDF file",
                InputKind::Pdf2 => "Two PDF files",
            });
            self.drop_zone(ui, &pal, def);
            ui.add_space(4.0);
            self.file_list(ui, &pal, def);
        });
        ui.add_space(GAP);
        kit::card(ui, &pal, |ui| {
            kit::card_title(ui, &pal, "settings", "Options");
            kit::muted(ui, &pal, def.desc);
            ui.add_space(6.0);
            let visible: Vec<Field> = def.fields.iter().filter(|f| f.when.map(|(k, v)| self.values.s(k) == v).unwrap_or(true)).cloned().collect();
            for f in &visible {
                field_ui(ui, &pal, f, &mut self.values);
            }
            if visible.is_empty() {
                kit::small_muted(ui, &pal, "No options for this tool.");
            }

            // engine requirements
            let mut blocked: Option<String> = None;
            if def.needs_ocr {
                if let Err(e) = &caps.ocr {
                    blocked = Some(e.clone());
                }
            } else if def.needs_render {
                if let Err(e) = &caps.renderer {
                    blocked = Some(e.clone());
                }
            }
            if let Some(b) = &blocked {
                ui.add_space(6.0);
                kit::notice(ui, &pal, "warn", pal.warn, b);
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let can = self.ready(def.input) && !self.busy() && blocked.is_none();
                if self.busy() {
                    if kit::button(ui, &pal, Some("stop"), "Cancel", Btn::Secondary, true).clicked() {
                        if let Some(t) = &self.task {
                            t.cancel();
                        }
                    }
                } else if kit::button(ui, &pal, Some("play"), def.cta, Btn::Primary, can).clicked() {
                    self.start(ui.ctx(), module_id, def);
                }
                if !self.files.is_empty() && !self.busy() && kit::button(ui, &pal, Some("x"), "Clear", Btn::Ghost, true).clicked() {
                    self.files.clear();
                    self.results.clear();
                    self.error = None;
                }
            });
            if !self.ready(def.input) && !self.busy() {
                ui.add_space(2.0);
                kit::small_muted(ui, &pal, match def.input {
                    InputKind::Any => "Add at least one file to continue.",
                    InputKind::Pdfs => "Add two or more PDFs to continue.",
                    InputKind::Pdf => "Add a PDF to continue.",
                    InputKind::Pdf2 => "Add exactly two PDFs to continue.",
                });
            }
        });
        let _ = prefs;
    }

    fn drop_zone(&mut self, ui: &mut Ui, pal: &Palette, def: &ActionDef) {
        let hovering = ui.ctx().input(|i| !i.raw.hovered_files.is_empty());
        let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 118.0), Sense::click());
        let active = hovering || resp.hovered();
        let fill = if hovering { pal.accent_soft } else if active { pal.panel2 } else { pal.bg.gamma_multiply(0.0) };
        ui.painter().rect_filled(rect, Rounding::same(RADIUS), fill);
        // dashed border
        let col = if active { pal.accent } else { pal.line };
        dashed_rect(ui, rect.shrink(0.5), col);
        crate::icons::paint(ui, Rect::from_center_size(rect.center() + Vec2::new(0.0, -26.0), Vec2::splat(30.0)), "upload", if active { pal.accent } else { pal.muted });
        ui.painter().text(rect.center() + Vec2::new(0.0, 10.0), Align2::CENTER_CENTER, if hovering { "Drop to add" } else { "Drop files here, or click to browse" }, FontId::proportional(15.0), pal.text);
        let hint = match def.input {
            InputKind::Any => "Images, text, CSV, HTML, Word, Excel, PowerPoint, PDF",
            _ => "PDF files",
        };
        ui.painter().text(rect.center() + Vec2::new(0.0, 32.0), Align2::CENTER_CENTER, hint, FontId::proportional(12.5), pal.muted);
        if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() && !self.busy() {
            let exts = actions::dialog_extensions(def.input);
            let mut d = rfd::FileDialog::new().add_filter("Supported files", &exts[..]);
            if matches!(def.input, InputKind::Pdf) {
                if let Some(p) = d.pick_file() {
                    self.add_paths(def.input, vec![p]);
                }
            } else {
                d = d.set_title("Add files");
                if let Some(p) = d.pick_files() {
                    self.add_paths(def.input, p);
                }
            }
        }
    }

    fn file_list(&mut self, ui: &mut Ui, pal: &Palette, def: &ActionDef) {
        let mut remove: Option<usize> = None;
        let mut swap: Option<(usize, usize)> = None;
        let n = self.files.len();
        let reorder = n > 1 && !matches!(def.input, InputKind::Pdf);
        for (i, p) in self.files.iter().enumerate() {
            let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            let size = std::fs::metadata(p).map(|m| fmt_size(m.len() as usize)).unwrap_or_default();
            egui::Frame::none().fill(pal.panel2).rounding(Rounding::same(RADIUS - 2.0)).inner_margin(egui::Margin::symmetric(10.0, 6.0)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    crate::icons::show(ui, "file", 18.0, pal.accent);
                    ui.vertical(|ui| {
                        ui.set_width((ui.available_width() - if reorder { 116.0 } else { 40.0 }).max(60.0));
                        ui.add(egui::Label::new(RichText::new(&name).strong()).wrap_mode(egui::TextWrapMode::Truncate));
                        kit::small_muted(ui, pal, &size);
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if kit::icon_button(ui, pal, "x", "Remove", !self.task.is_some(), 28.0).clicked() {
                            remove = Some(i);
                        }
                        if reorder {
                            if kit::icon_button(ui, pal, "down", "Move down", i + 1 < n, 28.0).clicked() {
                                swap = Some((i, i + 1));
                            }
                            if kit::icon_button(ui, pal, "up", "Move up", i > 0, 28.0).clicked() {
                                swap = Some((i, i - 1));
                            }
                        }
                    });
                });
            });
        }
        if let Some((a, b)) = swap {
            self.files.swap(a, b);
        }
        if let Some(i) = remove {
            self.files.remove(i);
        }
    }

    fn start(&mut self, ctx: &egui::Context, module_id: &'static str, def: &ActionDef) {
        self.error = None;
        self.results.clear();
        let inputs: Vec<Input> = self.files.iter().cloned().map(Input::from_path).collect();
        let values = self.values.clone();
        let action = def.id;
        self.task = Some(Task::spawn(ctx, move |c| actions::run(module_id, action, &inputs, &values, &c)));
    }

    // ------------------------------------------------------------ right column
    fn right(&mut self, ui: &mut Ui, pal: Palette, prefs: &Prefs, def: &ActionDef) {
        kit::card(ui, &pal, |ui| {
            kit::card_title(ui, &pal, "download", "Result");
            if let Some(t) = &self.task {
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    kit::spinner(ui, &pal, 22.0, prefs.reduce_motion);
                    ui.label(RichText::new(if t.label.is_empty() { "Working..." } else { &t.label }).strong());
                });
                ui.add_space(6.0);
                kit::progress_bar(ui, &pal, t.progress, 8.0);
                kit::small_muted(ui, &pal, &format!("{:.0}%  \u{b7}  everything runs locally on this computer", t.progress * 100.0));
                ui.add_space(8.0);
                return;
            }
            if let Some(e) = self.error.clone() {
                kit::notice(ui, &pal, "warn", pal.danger, &e);
                ui.add_space(8.0);
            }
            if self.results.is_empty() {
                if self.error.is_none() {
                    ui.add_space(24.0);
                    ui.vertical_centered(|ui| {
                        let (rect, _) = ui.allocate_exact_size(Vec2::splat(64.0), Sense::hover());
                        ui.painter().rect_filled(rect, Rounding::same(18.0), pal.accent_soft);
                        crate::icons::paint(ui, Rect::from_center_size(rect.center(), Vec2::splat(30.0)), "file", pal.accent);
                        ui.add_space(8.0);
                        ui.label(RichText::new("Nothing here yet").size(16.0).strong());
                        kit::muted(ui, &pal, &format!("Add your file(s), then press \"{}\".", def.cta));
                    });
                    ui.add_space(24.0);
                }
                return;
            }
            self.result_list(ui, &pal);
        });
    }

    fn result_list(&mut self, ui: &mut Ui, pal: &Palette) {
        if let Some(s) = &self.status {
            kit::notice(ui, pal, "check", pal.ok, s);
            ui.add_space(8.0);
        }
        if self.results.len() > 1 {
            ui.horizontal(|ui| {
                kit::muted(ui, pal, &format!("{} files ready", self.results.len()));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if kit::button(ui, pal, Some("folder"), "Save all to folder...", Btn::Primary, true).clicked() {
                        if let Some(dir) = rfd::FileDialog::new().set_title("Choose a folder").pick_folder() {
                            let mut saved = 0;
                            let mut err = None;
                            for o in &self.results {
                                match std::fs::write(unique_path(&dir, &o.name), &o.data) {
                                    Ok(_) => saved += 1,
                                    Err(e) => err = Some(format!("Could not save \"{}\": {e}", o.name)),
                                }
                            }
                            match err {
                                Some(e) => self.error = Some(e),
                                None => self.status = Some(format!("Saved {saved} file(s) to {}", dir.display())),
                            }
                        }
                    }
                });
            });
            ui.add_space(4.0);
        }
        let mut toggle: Option<usize> = None;
        let mut save_one: Option<usize> = None;
        for (i, o) in self.results.iter().enumerate() {
            egui::Frame::none().fill(pal.panel2).rounding(Rounding::same(RADIUS)).inner_margin(egui::Margin::same(12.0)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    crate::icons::show(ui, "file", 22.0, pal.accent);
                    ui.vertical(|ui| {
                        ui.set_width((ui.available_width() - 190.0).max(80.0));
                        ui.add(egui::Label::new(RichText::new(&o.name).strong()).wrap_mode(egui::TextWrapMode::Truncate));
                        let info = if o.note.is_empty() { fmt_size(o.data.len()) } else { format!("{}  \u{b7}  {}", fmt_size(o.data.len()), o.note) };
                        ui.add(egui::Label::new(RichText::new(info).small().color(pal.muted)).wrap_mode(egui::TextWrapMode::Wrap));
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if kit::button(ui, pal, Some("save"), "Save as...", Btn::Primary, true).clicked() {
                            save_one = Some(i);
                        }
                        if o.preview.is_some() && kit::button(ui, pal, Some("eye"), "View", Btn::Secondary, true).clicked() {
                            toggle = Some(i);
                        }
                    });
                });
                if self.preview_open == Some(i) {
                    if let Some(p) = &o.preview {
                        ui.add_space(6.0);
                        egui::Frame::none().fill(pal.panel).rounding(Rounding::same(8.0)).stroke(Stroke::new(1.0_f32, pal.line)).inner_margin(egui::Margin::same(10.0)).show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            egui::ScrollArea::vertical().max_height(360.0).id_salt(("preview", i)).show(ui, |ui| {
                                ui.add(egui::Label::new(RichText::new(p).monospace().size(12.5)).wrap_mode(egui::TextWrapMode::Wrap));
                            });
                        });
                    }
                }
            });
            ui.add_space(2.0);
        }
        if let Some(i) = toggle {
            self.preview_open = if self.preview_open == Some(i) { None } else { Some(i) };
        }
        if let Some(i) = save_one {
            let o = &self.results[i];
            let ext = udpt_core::ext_of(&o.name);
            let mut d = rfd::FileDialog::new().set_file_name(&o.name);
            if !ext.is_empty() {
                d = d.add_filter(&ext.to_uppercase(), &[ext.as_str()]);
            }
            if let Some(path) = d.save_file() {
                match std::fs::write(&path, &o.data) {
                    Ok(_) => self.status = Some(format!("Saved to {}", path.display())),
                    Err(e) => self.error = Some(format!("Could not save the file: {e}")),
                }
            }
        }
    }
}

fn unique_path(dir: &std::path::Path, name: &str) -> PathBuf {
    let mut p = dir.join(name);
    let (stem, ext) = (udpt_core::base_name(name), udpt_core::ext_of(name));
    let mut k = 1;
    while p.exists() {
        k += 1;
        p = dir.join(if ext.is_empty() { format!("{stem}-{k}") } else { format!("{stem}-{k}.{ext}") });
    }
    p
}

pub fn dashed_rect(ui: &Ui, rect: Rect, color: Color32) {
    let stroke = Stroke::new(1.5_f32, color);
    let (dash, gap) = (7.0, 5.0);
    let seg = |a: egui::Pos2, b: egui::Pos2| {
        let len = (b - a).length();
        let dir = (b - a) / len.max(0.001);
        let mut d = 0.0;
        while d < len {
            let e = (d + dash).min(len);
            ui.painter().line_segment([a + dir * d, a + dir * e], stroke);
            d += dash + gap;
        }
    };
    seg(rect.left_top(), rect.right_top());
    seg(rect.right_top(), rect.right_bottom());
    seg(rect.right_bottom(), rect.left_bottom());
    seg(rect.left_bottom(), rect.left_top());
}

fn fmt_num(x: f64) -> String {
    if x.fract() == 0.0 {
        format!("{}", x as i64)
    } else {
        format!("{x}")
    }
}
fn hex_to_rgb(h: &str) -> [u8; 3] {
    let n = u32::from_str_radix(h.trim().trim_start_matches('#'), 16).unwrap_or(0x888888);
    [(n >> 16) as u8, (n >> 8) as u8, n as u8]
}

pub fn field_ui(ui: &mut Ui, pal: &Palette, f: &Field, values: &mut Values) {
    let mut cur = values.s(f.key).to_string();
    if !matches!(f.kind, FieldKind::Check) {
        kit::field_label(ui, pal, f.label);
    }
    match &f.kind {
        FieldKind::Text => {
            ui.add(egui::TextEdit::singleline(&mut cur).desired_width(f32::INFINITY).margin(Vec2::new(10.0, 8.0)));
        }
        FieldKind::Number { min, max } => {
            let mut x: f64 = cur.trim().parse().unwrap_or_else(|_| f.default.parse().unwrap_or(0.0));
            ui.add(egui::DragValue::new(&mut x).range(*min..=*max).speed(if *max - *min > 500.0 { 1.0 } else { 0.5 }));
            cur = fmt_num(x);
        }
        FieldKind::Select(opts) => {
            let label = opts.iter().find(|(v, _)| *v == cur).map(|(_, l)| *l).unwrap_or("");
            egui::ComboBox::from_id_salt(f.key).width(ui.available_width().min(420.0)).selected_text(label).show_ui(ui, |ui| {
                for (v, l) in opts {
                    if ui.selectable_label(cur == *v, *l).clicked() {
                        cur = v.to_string();
                    }
                }
            });
        }
        FieldKind::Check => {
            let mut b = cur == "true";
            ui.checkbox(&mut b, f.label);
            cur = b.to_string();
        }
        FieldKind::Color => {
            let mut rgb = hex_to_rgb(&cur);
            ui.horizontal(|ui| {
                ui.color_edit_button_srgb(&mut rgb);
                ui.label(RichText::new(format!("#{:02x}{:02x}{:02x}", rgb[0], rgb[1], rgb[2])).monospace().color(pal.muted));
            });
            cur = format!("#{:02x}{:02x}{:02x}", rgb[0], rgb[1], rgb[2]);
        }
        FieldKind::Range { min, max, step } => {
            let mut x: f64 = cur.trim().parse().unwrap_or(*min);
            ui.add(egui::Slider::new(&mut x, *min..=*max).step_by(*step).trailing_fill(true));
            cur = fmt_num((x * 1000.0).round() / 1000.0);
        }
    }
    if !f.help.is_empty() {
        kit::small_muted(ui, pal, f.help);
    }
    ui.add_space(4.0);
    values.set(f.key, cur);
}
