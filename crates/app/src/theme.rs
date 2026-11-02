//! Theme engine and design tokens. Every screen takes its colours from `Palette`, so the whole app stays consistent.
use eframe::egui::{self, Color32, Context, FontFamily, FontId, Rounding, Stroke, TextStyle, Vec2};
use serde::{Deserialize, Serialize};

// ---- design tokens (the only place spacing / radii are defined)
pub const RADIUS: f32 = 10.0;
pub const CARD_RADIUS: f32 = 14.0;
pub const GAP: f32 = 16.0;
pub const PAD: f32 = 20.0;
pub const CONTROL_H: f32 = 38.0;

pub struct Preset {
    pub id: &'static str,
    pub label: &'static str,
    pub dark: bool,
    pub bg: [u8; 3],
    pub panel: [u8; 3],
    pub text: [u8; 3],
    pub accent: [u8; 3],
}
pub const PRESETS: [Preset; 6] = [
    Preset { id: "light", label: "Light", dark: false, bg: [0xf5, 0xf6, 0xfa], panel: [0xff, 0xff, 0xff], text: [0x17, 0x1a, 0x24], accent: [0x4f, 0x5b, 0xe8] },
    Preset { id: "dark", label: "Dark", dark: true, bg: [0x0f, 0x10, 0x15], panel: [0x17, 0x19, 0x23], text: [0xec, 0xee, 0xf6], accent: [0x7c, 0x88, 0xff] },
    Preset { id: "midnight", label: "Midnight", dark: true, bg: [0x0b, 0x10, 0x20], panel: [0x12, 0x1a, 0x30], text: [0xe4, 0xeb, 0xff], accent: [0x4c, 0xc9, 0xf0] },
    Preset { id: "forest", label: "Forest", dark: true, bg: [0x0e, 0x15, 0x12], panel: [0x15, 0x1f, 0x1a], text: [0xe5, 0xf1, 0xea], accent: [0x4a, 0xde, 0x80] },
    Preset { id: "sunset", label: "Sunset", dark: false, bg: [0xff, 0xf6, 0xf0], panel: [0xff, 0xff, 0xff], text: [0x2a, 0x17, 0x10], accent: [0xf2, 0x6a, 0x3d] },
    Preset { id: "paper", label: "Paper", dark: false, bg: [0xf3, 0xec, 0xdc], panel: [0xfb, 0xf6, 0xe9], text: [0x2b, 0x24, 0x18], accent: [0x9a, 0x6b, 0x2f] },
];

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct Prefs {
    pub theme: String,
    /// custom theme: bg, panel, text, accent
    pub custom: [[u8; 3]; 4],
    pub accent: Option<[u8; 3]>,
    pub reduce_motion: bool,
    pub compact: bool,
    pub preview_dpi: u32,
}
impl Default for Prefs {
    fn default() -> Self {
        Self { theme: "light".into(), custom: [[0x10, 0x12, 0x14], [0x1a, 0x1d, 0x21], [0xf1, 0xf3, 0xf5], [0xff, 0x7a, 0x59]], accent: None, reduce_motion: false, compact: false, preview_dpi: 150 }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Palette {
    pub dark: bool,
    pub bg: Color32,
    pub panel: Color32,
    pub panel2: Color32,
    pub text: Color32,
    pub muted: Color32,
    pub line: Color32,
    pub accent: Color32,
    pub accent_soft: Color32,
    pub on_accent: Color32,
    pub danger: Color32,
    pub ok: Color32,
    pub warn: Color32,
}

fn c(v: [u8; 3]) -> Color32 {
    Color32::from_rgb(v[0], v[1], v[2])
}
pub fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round().clamp(0.0, 255.0) as u8;
    Color32::from_rgb(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()))
}
fn luminance(col: Color32) -> f32 {
    (0.2126 * col.r() as f32 + 0.7152 * col.g() as f32 + 0.0722 * col.b() as f32) / 255.0
}

pub fn palette(p: &Prefs) -> Palette {
    let (dark, bg, panel, text, mut accent) = if p.theme == "custom" {
        let [bg, panel, text, acc] = p.custom;
        (luminance(c(bg)) < 0.45, c(bg), c(panel), c(text), c(acc))
    } else {
        let pr = PRESETS.iter().find(|x| x.id == p.theme).unwrap_or(&PRESETS[0]);
        (pr.dark, c(pr.bg), c(pr.panel), c(pr.text), c(pr.accent))
    };
    if let Some(a) = p.accent {
        accent = c(a);
    }
    Palette {
        dark,
        bg,
        panel,
        panel2: mix(panel, text, 0.05),
        text,
        muted: mix(text, bg, 0.45),
        line: mix(panel, text, 0.13),
        accent,
        accent_soft: mix(panel, accent, if dark { 0.22 } else { 0.13 }),
        on_accent: if luminance(accent) > 0.62 { Color32::from_rgb(20, 20, 28) } else { Color32::WHITE },
        danger: if dark { Color32::from_rgb(248, 113, 113) } else { Color32::from_rgb(200, 40, 50) },
        ok: if dark { Color32::from_rgb(74, 222, 128) } else { Color32::from_rgb(22, 140, 80) },
        warn: if dark { Color32::from_rgb(251, 191, 36) } else { Color32::from_rgb(180, 110, 10) },
    }
}

pub fn apply(ctx: &Context, pal: &Palette, compact: bool) {
    let mut v = if pal.dark { egui::Visuals::dark() } else { egui::Visuals::light() };
    v.panel_fill = pal.bg;
    v.window_fill = pal.panel;
    v.window_stroke = Stroke::new(1.0_f32, pal.line);
    v.window_rounding = Rounding::same(CARD_RADIUS);
    v.extreme_bg_color = pal.panel2;
    v.faint_bg_color = pal.panel2;
    v.hyperlink_color = pal.accent;
    v.selection.bg_fill = mix(pal.panel, pal.accent, 0.35);
    v.selection.stroke = Stroke::new(1.0_f32, pal.accent);
    v.popup_shadow = egui::epaint::Shadow { offset: Vec2::new(0.0, 6.0), blur: 20.0, spread: 0.0, color: Color32::from_black_alpha(60) };
    let r = Rounding::same(RADIUS - 2.0);
    let w = &mut v.widgets;
    w.noninteractive.bg_fill = pal.panel;
    w.noninteractive.weak_bg_fill = pal.panel;
    w.noninteractive.bg_stroke = Stroke::new(1.0_f32, pal.line);
    w.noninteractive.fg_stroke = Stroke::new(1.0_f32, pal.text);
    w.noninteractive.rounding = r;
    w.inactive.bg_fill = pal.panel2;
    w.inactive.weak_bg_fill = pal.panel2;
    w.inactive.bg_stroke = Stroke::new(1.0_f32, pal.line);
    w.inactive.fg_stroke = Stroke::new(1.0_f32, pal.text);
    w.inactive.rounding = r;
    w.hovered.bg_fill = mix(pal.panel2, pal.accent, 0.12);
    w.hovered.weak_bg_fill = mix(pal.panel2, pal.accent, 0.12);
    w.hovered.bg_stroke = Stroke::new(1.0_f32, mix(pal.line, pal.accent, 0.6));
    w.hovered.fg_stroke = Stroke::new(1.0_f32, pal.text);
    w.hovered.rounding = r;
    w.active.bg_fill = mix(pal.panel2, pal.accent, 0.25);
    w.active.weak_bg_fill = mix(pal.panel2, pal.accent, 0.25);
    w.active.bg_stroke = Stroke::new(1.0_f32, pal.accent);
    w.active.fg_stroke = Stroke::new(1.0_f32, pal.text);
    w.active.rounding = r;
    w.open = w.hovered;
    ctx.set_visuals(v);

    ctx.style_mut(|s| {
        s.spacing.item_spacing = if compact { Vec2::new(8.0, 6.0) } else { Vec2::new(10.0, 10.0) };
        s.spacing.button_padding = Vec2::new(14.0, 8.0);
        s.spacing.interact_size.y = 34.0;
        s.spacing.scroll.bar_width = 8.0;
        s.spacing.combo_width = 120.0;
        s.text_styles = [
            (TextStyle::Heading, FontId::new(26.0, FontFamily::Proportional)),
            (TextStyle::Body, FontId::new(14.5, FontFamily::Proportional)),
            (TextStyle::Button, FontId::new(14.5, FontFamily::Proportional)),
            (TextStyle::Small, FontId::new(12.0, FontFamily::Proportional)),
            (TextStyle::Monospace, FontId::new(13.5, FontFamily::Monospace)),
        ]
        .into();
    });
}
