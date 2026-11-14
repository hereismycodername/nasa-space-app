//! Mission-control look: one accent colour, muted status colours, flat surfaces, hairline borders.

use eframe::egui::{self, Color32, CornerRadius, Stroke, Vec2};
use ui_model::Status;

pub const BG: Color32 = Color32::from_rgb(0x0d, 0x11, 0x17);
pub const SURFACE: Color32 = Color32::from_rgb(0x13, 0x1a, 0x24);
pub const SURFACE2: Color32 = Color32::from_rgb(0x1a, 0x23, 0x30);
pub const DEEP: Color32 = Color32::from_rgb(0x0a, 0x0e, 0x14);
pub const LINE: Color32 = Color32::from_rgb(0x26, 0x30, 0x41);
pub const TXT: Color32 = Color32::from_rgb(0xe6, 0xed, 0xf3);
pub const MUT: Color32 = Color32::from_rgb(0x8b, 0x98, 0xa9);
pub const ACCENT: Color32 = Color32::from_rgb(0x3b, 0x8e, 0xea);
pub const OK: Color32 = Color32::from_rgb(0x2f, 0xbf, 0x71);
pub const WARN: Color32 = Color32::from_rgb(0xe3, 0xa0, 0x08);
pub const CRIT: Color32 = Color32::from_rgb(0xe5, 0x48, 0x4d);
pub const NONE: Color32 = Color32::from_rgb(0x5b, 0x66, 0x76);

pub fn status_color(s: Status) -> Color32 {
    match s {
        Status::Normal => OK,
        Status::Watch => WARN,
        Status::Act => CRIT,
        Status::NoData => NONE,
    }
}

/// `Stroke::new` with an unambiguous f32 width (avoids float-literal type fallback warnings).
pub fn stroke(w: f32, c: Color32) -> Stroke {
    Stroke::new(w, c)
}

pub fn alpha(c: Color32, a: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), (a.clamp(0.0, 1.0) * 255.0) as u8)
}

pub fn apply(ctx: &egui::Context) {
    let mut v = egui::Visuals::dark();
    v.panel_fill = BG;
    v.window_fill = SURFACE;
    v.window_stroke = stroke(1.0, LINE);
    v.extreme_bg_color = DEEP;
    v.faint_bg_color = SURFACE2;
    v.selection.bg_fill = alpha(ACCENT, 0.45);
    v.selection.stroke = stroke(1.0, ACCENT);
    v.hyperlink_color = ACCENT;
    let r = CornerRadius::same(4);
    for w in [&mut v.widgets.noninteractive, &mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active, &mut v.widgets.open] {
        w.corner_radius = r;
    }
    v.widgets.noninteractive.bg_stroke = stroke(1.0, LINE);
    v.widgets.noninteractive.fg_stroke = stroke(1.0, TXT);
    v.widgets.inactive.bg_fill = SURFACE2;
    v.widgets.inactive.weak_bg_fill = SURFACE2;
    v.widgets.inactive.bg_stroke = stroke(1.0, LINE);
    v.widgets.inactive.fg_stroke = stroke(1.0, TXT);
    v.widgets.hovered.bg_fill = Color32::from_rgb(0x22, 0x2e, 0x40);
    v.widgets.hovered.weak_bg_fill = Color32::from_rgb(0x22, 0x2e, 0x40);
    v.widgets.hovered.bg_stroke = stroke(1.0, ACCENT);
    v.widgets.active.bg_fill = alpha(ACCENT, 0.6);
    v.widgets.active.weak_bg_fill = alpha(ACCENT, 0.6);
    ctx.set_visuals(v);
    ctx.style_mut(|s| {
        s.spacing.item_spacing = Vec2::new(8.0, 6.0);
        s.spacing.button_padding = Vec2::new(12.0, 6.0);
        s.spacing.slider_width = 170.0;
        s.spacing.interact_size.y = 26.0;
    });
}
