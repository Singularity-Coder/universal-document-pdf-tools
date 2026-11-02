//! Document properties editor (title, author, subject, keywords...).
use crate::kit::{self, Btn};
use crate::theme::*;
use eframe::egui::{self, RichText, Ui};
use udpt_core::pdfops::{self, Meta};
use std::path::PathBuf;

#[derive(Default)]
pub struct Props {
    file: Option<PathBuf>,
    bytes: Vec<u8>,
    meta: Meta,
    error: Option<String>,
    status: Option<String>,
}

impl Props {
    fn load(&mut self, p: PathBuf) {
        self.status = None;
        match std::fs::read(&p).map_err(|e| e.to_string()).and_then(|b| pdfops::read_meta(&b).map(|m| (b, m)).map_err(|e| e.to_string())) {
            Ok((b, m)) => {
                self.bytes = b;
                self.meta = m;
                self.file = Some(p);
                self.error = None;
            }
            Err(e) => self.error = Some(e),
        }
    }

    pub fn ui(&mut self, ui: &mut Ui, pal: Palette, dropped: Vec<PathBuf>) {
        if let Some(p) = dropped.into_iter().find(|p| p.extension().map(|e| e.eq_ignore_ascii_case("pdf")).unwrap_or(false)) {
            self.load(p);
        }
        kit::card(ui, &pal, |ui| {
            kit::card_title(ui, &pal, "file", "PDF file");
            ui.horizontal(|ui| {
                if kit::button(ui, &pal, Some("folder"), if self.file.is_some() { "Open another PDF..." } else { "Open PDF..." }, Btn::Secondary, true).clicked() {
                    if let Some(p) = rfd::FileDialog::new().add_filter("PDF", &["pdf"]).pick_file() {
                        self.load(p);
                    }
                }
                if let Some(f) = &self.file {
                    ui.label(RichText::new(f.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()).strong());
                }
            });
            if let Some(e) = &self.error {
                ui.add_space(6.0);
                kit::notice(ui, &pal, "warn", pal.danger, e);
            }
        });
        let Some(file) = self.file.clone() else { return };
        ui.add_space(GAP);
        let w = ui.available_width();
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = GAP;
            ui.allocate_ui_with_layout(egui::vec2((w * 0.58).max(360.0), ui.available_height()), egui::Layout::top_down(egui::Align::Min), |ui| {
                kit::card(ui, &pal, |ui| {
                    kit::card_title(ui, &pal, "settings", "Edit properties");
                    for (label, val) in [("Title", &mut self.meta.title), ("Author", &mut self.meta.author), ("Subject", &mut self.meta.subject), ("Keywords", &mut self.meta.keywords), ("Creator", &mut self.meta.creator), ("Producer", &mut self.meta.producer)] {
                        kit::field_label(ui, &pal, label);
                        ui.add(egui::TextEdit::singleline(val).desired_width(f32::INFINITY).margin(egui::vec2(10.0, 8.0)));
                        ui.add_space(4.0);
                    }
                    ui.add_space(6.0);
                    if kit::button(ui, &pal, Some("save"), "Save as...", Btn::Primary, true).clicked() {
                        let name = format!("{}-properties.pdf", udpt_core::base_name(&file.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()));
                        if let Some(path) = rfd::FileDialog::new().set_file_name(&name).add_filter("PDF", &["pdf"]).save_file() {
                            self.status = Some(match pdfops::write_meta(&self.bytes, &self.meta).map_err(|e| e.to_string()).and_then(|d| std::fs::write(&path, d).map_err(|e| e.to_string())) {
                                Ok(_) => format!("Saved to {}", path.display()),
                                Err(e) => format!("Could not save: {e}"),
                            });
                        }
                    }
                    if let Some(s) = &self.status {
                        ui.add_space(6.0);
                        kit::notice(ui, &pal, "info", pal.accent, s);
                    }
                });
            });
            ui.allocate_ui_with_layout(egui::vec2(ui.available_width(), ui.available_height()), egui::Layout::top_down(egui::Align::Min), |ui| {
                kit::card(ui, &pal, |ui| {
                    kit::card_title(ui, &pal, "info", "Document info");
                    let m = &self.meta;
                    let rows = [
                        ("Pages", m.pages.to_string()),
                        ("First page size", m.first_page.clone()),
                        ("Encrypted", if m.encrypted { "yes (opened with an empty password)".to_string() } else { "no".to_string() }),
                        ("Created", m.created.clone()),
                        ("Modified", m.modified.clone()),
                    ];
                    for (k, v) in rows {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(k).color(pal.muted));
                            ui.label(RichText::new(if v.is_empty() { "-".to_string() } else { v }).strong());
                        });
                    }
                });
            });
        });
    }
}
