//! NASA-style look, following what NASA publishes:
//!  * Horizon Design System typography: Inter (display / headings), Public Sans (body), DM Mono (numbers, labels).
//!  * Horizon blue as the single accent; neutral greys.
//!  * "Quiet / dark" crew displays: colour is reserved for departures from normal (NASA crew-interface
//!    guidance), so NORMAL is neutral, WATCH is amber, ACT is red, NO DATA is cyan.
//! Sources: nasa.gov brand guidelines, github.com/nasa/hds-core, NASA CLDP "Crew Interfaces" (2024).
//!
//! Shape language: an instrument panel, not an app. Square corners everywhere, hairline borders, one
//! chamfered corner on panels, square status markers, segmented gauges, tracked mono captions.

use eframe::egui::{self, epaint::Shadow, pos2, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Pos2, Rect, Shape, Stroke, Vec2};
use std::sync::Arc;
use ui_model::Status;

pub const BG: Color32 = Color32::from_rgb(0x08, 0x0a, 0x0d);
pub const SURFACE: Color32 = Color32::from_rgb(0x0f, 0x12, 0x17);
pub const SURFACE2: Color32 = Color32::from_rgb(0x17, 0x1c, 0x23);
pub const DEEP: Color32 = Color32::from_rgb(0x06, 0x08, 0x0a);
pub const LINE: Color32 = Color32::from_rgb(0x23, 0x29, 0x32);
/// brighter hairline: chamfers, ticks, active borders
pub const LINE_HI: Color32 = Color32::from_rgb(0x3a, 0x43, 0x50);
pub const TXT: Color32 = Color32::from_rgb(0xe6, 0xe9, 0xed);
pub const MUT: Color32 = Color32::from_rgb(0x85, 0x8e, 0x9b);
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
/// size of the chamfered corner on panels and tiles
pub const CUT: f32 = 9.0;

pub fn status_color(s: Status) -> Color32 {
    match s {
        Status::Normal | Status::Unassessed => OK,
        Status::Watch => WARN,
        Status::Act => CRIT,
        Status::NoData => NONE,
    }
}

/// `Stroke::new` with an unambiguous f32 width.
pub fn stroke(w: f32, c: Color32) -> Stroke {
    Stroke::new(w, c)
}

/// `c` at opacity `a`, premultiplied in gamma space, which is how egui blends: exactly `a` of `c` over the
/// background. (`Color32::from_rgba_unmultiplied` premultiplies in linear space, so faint colours land
/// several times brighter than asked, which turns quiet tints into loud ones.)
pub fn alpha(c: Color32, a: f32) -> Color32 {
    let a = a.clamp(0.0, 1.0);
    let f = |v: u8| (v as f32 * a + 0.5) as u8;
    Color32::from_rgba_premultiplied(f(c.r()), f(c.g()), f(c.b()), (a * 255.0 + 0.5) as u8)
}

/// Headings and wordmark: Inter.
pub fn display(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("display".into()))
}
/// Numbers, units and technical labels: DM Mono.
pub fn mono(size: f32) -> FontId {
    FontId::new(size, FontFamily::Monospace)
}

/// Outline of a rectangle with its top-right corner cut at 45 degrees (clockwise from top-left).
pub fn chamfer(r: Rect, cut: f32) -> Vec<Pos2> {
    let c = cut.min(r.width() / 2.0).min(r.height() / 2.0);
    vec![r.left_top(), pos2(r.right() - c, r.top()), pos2(r.right(), r.top() + c), r.right_bottom(), r.left_bottom()]
}

/// Filled, outlined chamfered panel (the outline sits inside the rectangle).
pub fn panel_shape(r: Rect, cut: f32, fill: Color32, line: Stroke) -> Shape {
    let inner = r.shrink(line.width / 2.0);
    Shape::convex_polygon(chamfer(inner, cut), fill, line)
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
    v.window_stroke = stroke(1.0, LINE_HI);
    v.window_corner_radius = CornerRadius::ZERO;
    v.menu_corner_radius = CornerRadius::ZERO;
    v.window_shadow = Shadow { offset: [6, 6], blur: 0, spread: 0, color: Color32::from_black_alpha(140) };
    v.popup_shadow = Shadow { offset: [4, 4], blur: 0, spread: 0, color: Color32::from_black_alpha(120) };
    v.extreme_bg_color = DEEP;
    v.faint_bg_color = SURFACE2;
    v.selection.bg_fill = alpha(ACCENT, 0.45);
    v.selection.stroke = stroke(1.0, ACCENT);
    v.hyperlink_color = ACCENT;
    v.slider_trailing_fill = true;
    v.handle_shape = egui::style::HandleShape::Rect { aspect_ratio: 0.5 };
    for w in [&mut v.widgets.noninteractive, &mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active, &mut v.widgets.open] {
        w.corner_radius = CornerRadius::ZERO;
        w.expansion = 0.0;
    }
    v.widgets.noninteractive.bg_stroke = stroke(1.0, LINE);
    v.widgets.noninteractive.fg_stroke = stroke(1.0, TXT);
    v.widgets.inactive.bg_fill = SURFACE2;
    v.widgets.inactive.weak_bg_fill = SURFACE2;
    v.widgets.inactive.bg_stroke = stroke(1.0, LINE_HI);
    v.widgets.inactive.fg_stroke = stroke(1.0, TXT);
    v.widgets.hovered.bg_fill = Color32::from_rgb(0x1f, 0x26, 0x30);
    v.widgets.hovered.weak_bg_fill = Color32::from_rgb(0x1f, 0x26, 0x30);
    v.widgets.hovered.bg_stroke = stroke(1.0, ACCENT);
    v.widgets.hovered.fg_stroke = stroke(1.0, TXT);
    v.widgets.active.bg_fill = alpha(ACCENT, 0.6);
    v.widgets.active.weak_bg_fill = alpha(ACCENT, 0.6);
    v.widgets.active.bg_stroke = stroke(1.0, ACCENT);
    v.widgets.open.bg_fill = SURFACE2;
    v.widgets.open.weak_bg_fill = SURFACE2;
    v.widgets.open.bg_stroke = stroke(1.0, ACCENT);
    ctx.set_visuals(v);
    ctx.style_mut(|s| {
        s.spacing.item_spacing = Vec2::new(8.0, 6.0);
        s.spacing.button_padding = Vec2::new(12.0, 5.0);
        s.spacing.slider_width = 170.0;
        s.spacing.interact_size.y = 26.0;
        s.spacing.scroll = egui::style::ScrollStyle::thin();
        s.spacing.scroll.bar_width = 6.0;
        s.spacing.menu_margin = egui::Margin::same(6);
        s.spacing.window_margin = egui::Margin::same(16);
    });
}

