//! Application shell: sidebar, routing, home, settings.
use crate::kit;
use crate::organizer::Organizer;
use crate::props::Props;
use crate::runner::ActionPage;
use crate::sign::Sign;
use crate::splash::{Splash, SplashState};
use crate::theme::{self, *};
use crate::icons;
use eframe::egui::{self, Align2, Color32, FontId, Margin, Pos2, Rect, RichText, Rounding, Sense, Stroke, Ui, Vec2};
use udpt_core::actions::{self, ModuleDef, Special};
use udpt_core::Capabilities;
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Route {
    Home,
    Module(usize),
    Settings,
}

pub struct App {
    prefs: Prefs,
    pal: Palette,
    mods: Vec<ModuleDef>,
    route: Route,
    selected: Vec<usize>,
    splash: Option<Splash>,
    caps: Capabilities,
    pages: HashMap<(usize, usize), ActionPage>,
    organizer: Organizer,
    sign: Sign,
    props: Props,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        egui_extras::install_image_loaders(&cc.egui_ctx);
        let prefs: Prefs = cc.storage.and_then(|s| eframe::get_value(s, "prefs")).unwrap_or_default();
        let pal = palette(&prefs);
        theme::apply(&cc.egui_ctx, &pal, prefs.compact);
        let mods = actions::modules();
        let selected = vec![0; mods.len()];
        let splash = Some(Splash::new(&cc.egui_ctx, prefs.reduce_motion));
        Self {
            prefs,
            pal,
            mods,
            route: Route::Home,
            selected,
            splash,
            caps: Capabilities { renderer: Err("Checking...".into()), ocr: Err("Checking...".into()) },
            pages: HashMap::new(),
            organizer: Organizer::default(),
            sign: Sign::default(),
            props: Props::default(),
        }
    }

    fn restyle(&mut self, ctx: &egui::Context) {
        self.pal = palette(&self.prefs);
        theme::apply(ctx, &self.pal, self.prefs.compact);
    }
}

impl eframe::App for App {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, "prefs", &self.prefs);
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if let Some(s) = &mut self.splash {
            if let SplashState::Done(caps) = s.show(ctx, &self.pal) {
                self.caps = caps;
                self.splash = None;
            }
            return;
        }
        let dropped: Vec<PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().filter_map(|f| f.path.clone()).collect());
        self.sidebar(ctx);
        let pal = self.pal;
        egui::CentralPanel::default().frame(egui::Frame::none().fill(pal.bg).inner_margin(Margin::symmetric(32.0, 26.0))).show(ctx, |ui| {
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| match self.route {
                Route::Home => self.home(ui),
                Route::Settings => self.settings(ui, ctx),
                Route::Module(m) => self.module(ui, m, dropped),
            });
        });
    }
}

// ---------------------------------------------------------------- sidebar
fn nav_item(ui: &mut Ui, pal: &Palette, icon: &str, label: &str, selected: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 42.0), Sense::click());
    let hover = resp.hovered();
    if selected {
        ui.painter().rect_filled(rect, Rounding::same(RADIUS), pal.accent_soft);
        ui.painter().rect_filled(Rect::from_min_size(rect.min + Vec2::new(0.0, 9.0), Vec2::new(3.5, 24.0)), Rounding::same(2.0), pal.accent);
    } else if hover {
        ui.painter().rect_filled(rect, Rounding::same(RADIUS), pal.panel2);
    }
    let col = if selected { pal.accent } else { pal.text };
    icons::paint(ui, Rect::from_min_size(Pos2::new(rect.left() + 16.0, rect.center().y - 10.0), Vec2::splat(20.0)), icon, col);
    ui.painter().text(Pos2::new(rect.left() + 48.0, rect.center().y), Align2::LEFT_CENTER, label, FontId::proportional(14.5), col);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

impl App {
    fn sidebar(&mut self, ctx: &egui::Context) {
        let pal = self.pal;
        egui::SidePanel::left("side")
            .exact_width(276.0)
            .resizable(false)
            .frame(egui::Frame::none().fill(pal.panel).stroke(Stroke::new(1.0_f32, pal.line)).inner_margin(Margin::symmetric(14.0, 18.0)))
            .show(ctx, |ui| {
                // brand
                ui.horizontal(|ui| {
                    let (rect, _) = ui.allocate_exact_size(Vec2::splat(44.0), Sense::hover());
                    kit::paint_logo(ui.painter(), rect.center(), 44.0, &pal);
                    ui.add_space(4.0);
                    ui.vertical(|ui| {
                        ui.add_space(2.0);
                        ui.label(RichText::new("Universal Document").size(16.5).strong());
                        ui.label(RichText::new("PDF Tools").size(16.5).strong().color(pal.accent));
                    });
                });
                ui.add_space(18.0);
                ui.spacing_mut().item_spacing.y = 4.0;
                if nav_item(ui, &pal, "home", "Overview", self.route == Route::Home).clicked() {
                    self.route = Route::Home;
                }
                ui.add_space(12.0);
                ui.label(RichText::new("TOOLS").size(11.5).strong().color(pal.muted));
                ui.add_space(2.0);
                for (i, m) in self.mods.iter().enumerate() {
                    if nav_item(ui, &pal, m.icon, m.label, self.route == Route::Module(i)).clicked() {
                        self.route = Route::Module(i);
                    }
                }
                // bottom block
                let bottom_h = 42.0 + 12.0 + 64.0;
                ui.add_space((ui.available_height() - bottom_h).max(10.0));
                if nav_item(ui, &pal, "settings", "Settings", self.route == Route::Settings).clicked() {
                    self.route = Route::Settings;
                }
                ui.add_space(8.0);
                egui::Frame::none().fill(pal.accent_soft).rounding(Rounding::same(RADIUS)).inner_margin(Margin::same(12.0)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        icons::show(ui, "lock", 20.0, pal.accent);
                        ui.vertical(|ui| {
                            ui.label(RichText::new("Fully offline").strong().color(pal.accent));
                            kit::small_muted(ui, &pal, "Files never leave this PC");
                        });
                    });
                });
            });
    }

    // ---------------------------------------------------------------- home
    fn home(&mut self, ui: &mut Ui) {
        let pal = self.pal;
        // hero
        egui::Frame::none().fill(pal.accent).rounding(Rounding::same(CARD_RADIUS + 4.0)).inner_margin(Margin::same(28.0)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(RichText::new("All your document work, in one offline app.").size(30.0).strong().color(pal.on_accent));
                    ui.add_space(4.0);
                    ui.label(RichText::new("Create PDFs from almost anything, convert, organize, OCR, sign and clean - without uploading a single file.").size(15.5).color(pal.on_accent.gamma_multiply(0.9)));
                    ui.add_space(14.0);
                    ui.horizontal(|ui| {
                        for (icon, t) in [("lock", "Private by design"), ("wifioff", "Works offline"), ("bolt", "Free to use")] {
                            let g = ui.painter().layout_no_wrap(t.to_string(), FontId::proportional(13.0), pal.on_accent);
                            let (rect, _) = ui.allocate_exact_size(Vec2::new(g.size().x + 44.0, 30.0), Sense::hover());
                            ui.painter().rect_filled(rect, Rounding::same(15.0), pal.on_accent.gamma_multiply(0.16));
                            icons::paint(ui, Rect::from_center_size(Pos2::new(rect.left() + 18.0, rect.center().y), Vec2::splat(15.0)), icon, pal.on_accent);
                            ui.painter().galley(Pos2::new(rect.left() + 32.0, rect.center().y - g.size().y / 2.0), g, pal.on_accent);
                        }
                    });
                });
            });
        });
        ui.add_space(GAP + 4.0);

        // engine status
        ui.horizontal_wrapped(|ui| {
            for (label, st) in [("PDF engine", &self.caps.renderer), ("Offline OCR", &self.caps.ocr)] {
                match st {
                    Ok(_) => kit::badge(ui, &format!("{label}: ready"), pal.ok, Some("check")),
                    Err(_) => kit::badge(ui, &format!("{label}: not installed (see Settings)"), pal.warn, Some("warn")),
                }
            }
        });
        ui.add_space(GAP);

        ui.label(RichText::new("Choose a tool").size(18.0).strong());
        ui.add_space(4.0);
        let cols = if ui.available_width() > 1000.0 { 3 } else { 2 };
        let w = (ui.available_width() - GAP * (cols as f32 - 1.0)) / cols as f32;
        let mut go: Option<usize> = None;
        for row in (0..self.mods.len()).collect::<Vec<_>>().chunks(cols) {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = GAP;
                for &i in row {
                    let m = &self.mods[i];
                    let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, 148.0), Sense::click());
                    let hover = resp.hovered();
                    ui.painter().rect(rect, Rounding::same(CARD_RADIUS), if hover { mix(pal.panel, pal.accent, 0.05) } else { pal.panel }, Stroke::new(1.0_f32, if hover { mix(pal.line, pal.accent, 0.7) } else { pal.line }));
                    let tile = Rect::from_min_size(rect.min + Vec2::new(PAD, PAD), Vec2::splat(46.0));
                    ui.painter().rect_filled(tile, Rounding::same(13.0), pal.accent_soft);
                    icons::paint(ui, Rect::from_center_size(tile.center(), Vec2::splat(24.0)), m.icon, pal.accent);
                    ui.painter().text(Pos2::new(tile.right() + 14.0, tile.center().y - 8.0), Align2::LEFT_CENTER, m.label, FontId::proportional(17.0), pal.text);
                    ui.painter().text(Pos2::new(tile.right() + 14.0, tile.center().y + 12.0), Align2::LEFT_CENTER, format!("{} tool{}", m.actions.len(), if m.actions.len() == 1 { "" } else { "s" }), FontId::proportional(12.5), pal.muted);
                    let g = ui.painter().layout(m.tagline.to_string(), FontId::proportional(13.5), pal.muted, rect.width() - PAD * 2.0);
                    ui.painter().galley(Pos2::new(rect.left() + PAD, tile.bottom() + 14.0), g, pal.muted);
                    if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                        go = Some(i);
                    }
                }
            });
            ui.add_space(GAP - 6.0);
        }
        if let Some(i) = go {
            self.route = Route::Module(i);
        }
    }

    // ---------------------------------------------------------------- module page
    fn module(&mut self, ui: &mut Ui, mi: usize, dropped: Vec<PathBuf>) {
        let pal = self.pal;
        let module = self.mods[mi].clone();
        kit::page_header(ui, &pal, module.icon, module.label, module.tagline);
        ui.add_space(GAP);
        let sel = self.selected[mi].min(module.actions.len() - 1);
        if module.actions.len() > 1 {
            ui.horizontal_wrapped(|ui| {
                for (i, a) in module.actions.iter().enumerate() {
                    if kit::chip(ui, &pal, None, a.label, i == sel).clicked() {
                        self.selected[mi] = i;
                    }
                }
            });
            ui.add_space(GAP);
        }
        let def = module.actions[sel].clone();
        match def.special {
            Special::Organizer => self.organizer.ui(ui, pal, &self.prefs, dropped, self.caps.renderer.clone()),
            Special::Sign => self.sign.ui(ui, pal, &self.prefs, dropped, self.caps.renderer.clone()),
            Special::Properties => self.props.ui(ui, pal, dropped),
            Special::None => {
                let page = self.pages.entry((mi, sel)).or_insert_with(|| ActionPage::new(&def));
                page.ui(ui, pal, &self.prefs, module.id, &def, &self.caps, dropped);
            }
        }
    }

    // ---------------------------------------------------------------- settings
    fn settings(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        let pal = self.pal;
        kit::page_header(ui, &pal, "settings", "Settings", "Appearance, behaviour and the offline engines.");
        ui.add_space(GAP);
        let mut changed = false;
        let w = ui.available_width();
        let left_w = (w * 0.56).max(420.0).min(w - 300.0);
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = GAP;
            ui.allocate_ui_with_layout(Vec2::new(left_w, ui.available_height()), egui::Layout::top_down(egui::Align::Min), |ui| {
                kit::card(ui, &pal, |ui| {
                    kit::card_title(ui, &pal, "eye", "Theme");
                    ui.horizontal_wrapped(|ui| {
                        for p in PRESETS.iter() {
                            let (rect, resp) = ui.allocate_exact_size(Vec2::new(118.0, 76.0), Sense::click());
                            let sel = self.prefs.theme == p.id;
                            let c = |v: [u8; 3]| Color32::from_rgb(v[0], v[1], v[2]);
                            ui.painter().rect(rect, Rounding::same(RADIUS), c(p.bg), Stroke::new(if sel { 2.5 } else { 1.0 }, if sel { pal.accent } else { pal.line }));
                            let inner = Rect::from_min_size(rect.min + Vec2::new(10.0, 10.0), Vec2::new(98.0, 30.0));
                            ui.painter().rect_filled(inner, Rounding::same(6.0), c(p.panel));
                            ui.painter().circle_filled(inner.left_center() + Vec2::new(14.0, 0.0), 6.0, c(p.accent));
                            ui.painter().rect_filled(Rect::from_min_size(inner.min + Vec2::new(28.0, 11.0), Vec2::new(46.0, 8.0)), Rounding::same(4.0), c(p.text).gamma_multiply(0.6));
                            ui.painter().text(rect.left_bottom() + Vec2::new(12.0, -14.0), Align2::LEFT_CENTER, p.label, FontId::proportional(13.0), c(p.text));
                            if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                                self.prefs.theme = p.id.to_string();
                                changed = true;
                            }
                        }
                        let (rect, resp) = ui.allocate_exact_size(Vec2::new(118.0, 76.0), Sense::click());
                        let sel = self.prefs.theme == "custom";
                        let cc = self.prefs.custom;
                        let col = |v: [u8; 3]| Color32::from_rgb(v[0], v[1], v[2]);
                        ui.painter().rect(rect, Rounding::same(RADIUS), col(cc[0]), Stroke::new(if sel { 2.5 } else { 1.0 }, if sel { pal.accent } else { pal.line }));
                        ui.painter().rect_filled(Rect::from_min_size(rect.min + Vec2::new(10.0, 10.0), Vec2::new(98.0, 30.0)), Rounding::same(6.0), col(cc[1]));
                        ui.painter().circle_filled(rect.min + Vec2::new(24.0, 25.0), 6.0, col(cc[3]));
                        ui.painter().text(rect.left_bottom() + Vec2::new(12.0, -14.0), Align2::LEFT_CENTER, "Custom", FontId::proportional(13.0), col(cc[2]));
                        if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                            self.prefs.theme = "custom".into();
                            changed = true;
                        }
                    });
                    if self.prefs.theme == "custom" {
                        ui.add_space(8.0);
                        ui.horizontal(|ui| {
                            for (i, l) in ["Background", "Panels", "Text", "Accent"].iter().enumerate() {
                                kit::small_muted(ui, &pal, l);
                                changed |= ui.color_edit_button_srgb(&mut self.prefs.custom[i]).changed();
                                ui.add_space(8.0);
                            }
                        });
                    }
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        let mut over = self.prefs.accent.is_some();
                        if ui.checkbox(&mut over, "Override accent colour").changed() {
                            self.prefs.accent = if over { Some([0x4f, 0x5b, 0xe8]) } else { None };
                            changed = true;
                        }
                        if let Some(a) = &mut self.prefs.accent {
                            changed |= ui.color_edit_button_srgb(a).changed();
                        }
                    });
                });
                ui.add_space(GAP);
                kit::card(ui, &pal, |ui| {
                    kit::card_title(ui, &pal, "settings", "Behaviour");
                    changed |= ui.checkbox(&mut self.prefs.compact, "Compact layout (tighter spacing)").changed();
                    changed |= ui.checkbox(&mut self.prefs.reduce_motion, "Reduce motion (calmer loading animations)").changed();
                    ui.add_space(4.0);
                    kit::field_label(ui, &pal, "Preview resolution for scanned pages");
                    let mut dpi = self.prefs.preview_dpi as f32;
                    if ui.add(egui::Slider::new(&mut dpi, 72.0..=300.0).step_by(6.0).suffix(" dpi").trailing_fill(true)).changed() {
                        self.prefs.preview_dpi = dpi as u32;
                    }
                });
            });
            ui.allocate_ui_with_layout(Vec2::new(ui.available_width(), ui.available_height()), egui::Layout::top_down(egui::Align::Min), |ui| {
                kit::card(ui, &pal, |ui| {
                    kit::card_title(ui, &pal, "shield", "Offline engines");
                    for (label, desc, st) in [("PDF engine (PDFium)", "Needed for thumbnails, page images, OCR, redaction and scan clean-up.", &self.caps.renderer), ("OCR models", "Needed for \"Make searchable\" and \"Extract text\".", &self.caps.ocr)] {
                        ui.horizontal(|ui| {
                            match st {
                                Ok(_) => kit::badge(ui, "Ready", pal.ok, Some("check")),
                                Err(_) => kit::badge(ui, "Missing", pal.warn, Some("warn")),
                            }
                            ui.label(RichText::new(label).strong());
                        });
                        kit::small_muted(ui, &pal, desc);
                        if let Err(e) = st {
                            ui.add(egui::Label::new(RichText::new(e).small().color(pal.warn)).wrap_mode(egui::TextWrapMode::Wrap));
                        }
                        ui.add_space(8.0);
                    }
                    kit::small_muted(ui, &pal, "Searched in these folders:");
                    for d in udpt_core::render::runtime_dirs().into_iter().take(4) {
                        ui.add(egui::Label::new(RichText::new(d.display().to_string()).monospace().size(11.5).color(pal.muted)).wrap_mode(egui::TextWrapMode::Wrap));
                    }
                    ui.add_space(6.0);
                    kit::small_muted(ui, &pal, "Copy the files into one of these folders, then restart the app.");
                });
                ui.add_space(GAP);
                kit::card(ui, &pal, |ui| {
                    kit::card_title(ui, &pal, "info", "About");
                    ui.label(RichText::new(format!("Universal Document PDF Tools {}", env!("CARGO_PKG_VERSION"))).strong());
                    kit::muted(ui, &pal, "No accounts, no telemetry, no network access. Every file is processed on this computer.");
                });
            });
        });
        if changed {
            self.restyle(ctx);
        }
    }
}
