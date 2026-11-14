//! NASA-style look, following what NASA publishes:
//!  * Horizon Design System typography: Inter (display / headings), Public Sans (body), DM Mono (numbers, labels).
//!  * Horizon blue as the single accent; neutral greys.
//!  * "Quiet / dark" crew displays: colour is reserved for departures from normal (NASA crew-interface
//!    guidance), so NORMAL is neutral, WATCH is amber, ACT is red, NO DATA is cyan.
//! Sources: nasa.gov brand guidelines, github.com/nasa/hds-core, NASA CLDP "Crew Interfaces" (2024).

use eframe::egui::{self, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Stroke, Vec2};
use std::sync::Arc;
use ui_model::Status;

pub const BG: Color32 = Color32::from_rgb(0x09, 0x0b, 0x0f);
pub const SURFACE: Color32 = Color32::from_rgb(0x11, 0x15, 0x1b);
pub const SURFACE2: Color32 = Color32::from_rgb(0x1a, 0x1f, 0x27);
pub const DEEP: Color32 = Color32::from_rgb(0x07, 0x09, 0x0c);
pub const LINE: Color32 = Color32::from_rgb(0x27, 0x2e, 0x38);
pub const TXT: Color32 = Color32::from_rgb(0xe9, 0xec, 0xf0);
pub const MUT: Color32 = Color32::from_rgb(0x8b, 0x94, 0xa1);
/// Horizon blue (blue-500 is about #1C68E3; this is the lighter step that reads on a dark background)
pub const ACCENT: Color32 = Color32::from_rgb(0x3a, 0x7b, 0xef);
/// normal = neutral: nothing to draw attention to
pub const OK: Color32 = Color32::from_rgb(0x9a, 0xa5, 0xb4);
/// caution
pub const WARN: Color32 = Color32::from_rgb(0xf2, 0xb6, 0x32);
/// warning (functional alert red, brighter than brand red because brand red is not allowed on dark backgrounds)
pub const CRIT: Color32 = Color32::from_rgb(0xff, 0x5b, 0x4d);
/// data not available
pub const NONE: Color32 = Color32::from_rgb(0x4f, 0xb0, 0xd6);

pub fn status_color(s: Status) -> Color32 {
    match s {
        Status::Normal => OK,
        Status::Watch => WARN,
        Status::Act => CRIT,
        Status::NoData => NONE,
    }
}

/// `Stroke::new` with an unambiguous f32 width.
pub fn stroke(w: f32, c: Color32) -> Stroke {
    Stroke::new(w, c)
}

pub fn alpha(c: Color32, a: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), (a.clamp(0.0, 1.0) * 255.0) as u8)
}

/// Headings and wordmark: Inter.
pub fn display(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("display".into()))
}
/// Numbers, units and technical labels: DM Mono.
pub fn mono(size: f32) -> FontId {
    FontId::new(size, FontFamily::Monospace)
}

fn install_fonts(ctx: &egui::Context) {
    let mut f = FontDefinitions::default();
    let add = |f: &mut FontDefinitions, name: &str, bytes: &'static [u8]| {
        f.font_data.insert(name.to_owned(), Arc::new(FontData::from_static(bytes)));
    };
    add(&mut f, "PublicSans", include_bytes!("../assets/fonts/PublicSans.ttf"));
    add(&mut f, "Inter", include_bytes!("../assets/fonts/Inter.ttf"));
    add(&mut f, "DMMono", include_bytes!("../assets/fonts/DMMono-Regular.ttf"));
    f.families.entry(FontFamily::Proportional).or_default().insert(0, "PublicSans".into());
    f.families.entry(FontFamily::Monospace).or_default().insert(0, "DMMono".into());
    let mut disp = vec!["Inter".to_owned()];
    disp.extend(f.families.get(&FontFamily::Proportional).cloned().unwrap_or_default());
    f.families.insert(FontFamily::Name("display".into()), disp);
    ctx.set_fonts(f);
}

pub fn apply(ctx: &egui::Context) {
    install_fonts(ctx);
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
    v.widgets.hovered.bg_fill = Color32::from_rgb(0x22, 0x29, 0x33);
    v.widgets.hovered.weak_bg_fill = Color32::from_rgb(0x22, 0x29, 0x33);
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
