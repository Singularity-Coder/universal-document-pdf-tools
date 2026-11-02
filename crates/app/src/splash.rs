//! Animated loading screen. While it plays, a background thread checks which offline engines are installed.
use crate::kit;
use crate::theme::Palette;
use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, Rounding, Shape, Stroke, Vec2};
use udpt_core::Capabilities;
use std::sync::{Arc, Mutex};
use std::time::Instant;

pub struct Splash {
    start: Instant,
    caps: Arc<Mutex<Option<Capabilities>>>,
    fade_start: Option<Instant>,
    min_secs: f32,
}

pub enum SplashState {
    Running,
    Done(Capabilities),
}

impl Splash {
    pub fn new(ctx: &egui::Context, reduce_motion: bool) -> Self {
        let caps = Arc::new(Mutex::new(None));
        let c2 = caps.clone();
        let repaint = ctx.clone();
        std::thread::spawn(move || {
            let c = udpt_core::capabilities();
            if let Ok(mut g) = c2.lock() {
                *g = Some(c);
            }
            repaint.request_repaint();
        });
        Self { start: Instant::now(), caps, fade_start: None, min_secs: if reduce_motion { 0.5 } else { 2.4 } }
    }

    pub fn show(&mut self, ctx: &egui::Context, pal: &Palette) -> SplashState {
        let t = self.start.elapsed().as_secs_f32();
        let ready = self.caps.lock().ok().and_then(|g| g.clone());
        // progress eases towards 1 over `min_secs`; it holds at 92% until the engine check is finished
        let mut p = (t / self.min_secs).clamp(0.0, 1.0);
        p = 1.0 - (1.0 - p).powi(3) * 0.85 - (1.0 - p) * 0.15;
        if ready.is_none() {
            p = p.min(0.92);
        }
        if ready.is_some() && t >= self.min_secs && self.fade_start.is_none() {
            self.fade_start = Some(Instant::now());
        }
        let fade = self.fade_start.map(|f| (f.elapsed().as_secs_f32() / 0.35).clamp(0.0, 1.0)).unwrap_or(0.0);
        let alpha = 1.0 - fade;

        egui::CentralPanel::default().frame(egui::Frame::none().fill(pal.bg)).show(ctx, |ui| {
            let rect = ui.max_rect();
            let c = rect.center() + Vec2::new(0.0, -30.0);
            let painter = ui.painter();

            // soft accent glow behind the logo
            for i in 0..6 {
                let r = 150.0 + i as f32 * 34.0 + (t * 1.4).sin() * 5.0;
                painter.circle_filled(c, r, pal.accent.gamma_multiply(0.035 * alpha));
            }
            // orbiting arcs
            let ring = 92.0;
            for (k, (speed, len, w)) in [(2.2f32, 1.5f32, 3.0f32), (-1.5, 1.1, 2.0)].iter().enumerate() {
                let start = t * speed + k as f32 * 2.0;
                let pts: Vec<Pos2> = (0..=36)
                    .map(|i| {
                        let a = start + len * i as f32 / 36.0;
                        let r = ring + k as f32 * 14.0;
                        Pos2::new(c.x + r * a.cos(), c.y + r * a.sin())
                    })
                    .collect();
                painter.circle_stroke(c, ring + k as f32 * 14.0, Stroke::new(1.0_f32, pal.accent.gamma_multiply(0.13 * alpha)));
                painter.add(Shape::line(pts, Stroke::new(*w, pal.accent.gamma_multiply(alpha))));
            }
            // logo with a gentle pulse and entrance
            let enter = ((t / 0.6).clamp(0.0, 1.0) as f32).powi(2);
            let pulse = 1.0 + 0.03 * (t * 3.0).sin();
            let mut logo_pal = *pal;
            logo_pal.accent = pal.accent.gamma_multiply(alpha);
            kit::paint_logo(painter, c, 118.0 * pulse * (0.7 + 0.3 * enter), &logo_pal);

            let title_alpha = (((t - 0.3) / 0.5).clamp(0.0, 1.0)) * alpha;
            painter.text(Pos2::new(c.x, c.y + 130.0), Align2::CENTER_CENTER, "Universal Document PDF Tools", FontId::proportional(34.0), pal.text.gamma_multiply(title_alpha));
            painter.text(Pos2::new(c.x, c.y + 164.0), Align2::CENTER_CENTER, "All-in-one document tools  \u{b7}  100% offline", FontId::proportional(15.0), pal.muted.gamma_multiply(title_alpha));

            // progress bar
            let bw = 340.0;
            let bar = Rect::from_center_size(Pos2::new(c.x, c.y + 212.0), Vec2::new(bw, 6.0));
            painter.rect_filled(bar, Rounding::same(3.0), pal.panel2.gamma_multiply(alpha));
            let fill = Rect::from_min_size(bar.min, Vec2::new((bw * p).max(6.0), 6.0));
            painter.rect_filled(fill, Rounding::same(3.0), pal.accent.gamma_multiply(alpha));
            let sx = fill.left() + ((t * 1.1) % 1.0) * fill.width();
            let sheen = Rect::from_min_max(Pos2::new((sx - 26.0).max(fill.left()), fill.top()), Pos2::new((sx + 26.0).min(fill.right()), fill.bottom()));
            if sheen.width() > 2.0 {
                painter.rect_filled(sheen, Rounding::same(3.0), Color32::WHITE.gamma_multiply(0.25 * alpha));
            }
            let status = if p < 0.25 {
                "Starting up..."
            } else if p < 0.5 {
                "Checking the PDF engine..."
            } else if p < 0.75 {
                "Looking for offline OCR models..."
            } else if p < 0.99 {
                "Preparing your workspace..."
            } else {
                "Ready"
            };
            painter.text(Pos2::new(c.x, c.y + 238.0), Align2::CENTER_CENTER, status, FontId::proportional(13.5), pal.muted.gamma_multiply(alpha));
            painter.text(Pos2::new(c.x, rect.bottom() - 28.0), Align2::CENTER_CENTER, "Your files never leave this computer", FontId::proportional(12.5), pal.muted.gamma_multiply(0.8 * alpha));
        });
        ctx.request_repaint();
        if fade >= 1.0 {
            if let Some(c) = ready {
                return SplashState::Done(c);
            }
        }
        SplashState::Running
    }
}
