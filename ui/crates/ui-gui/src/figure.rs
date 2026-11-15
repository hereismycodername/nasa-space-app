//! Anatomical figure viewer.
//!
//! The figure is a shaded frontal render of the real BodyParts3D meshes, one sprite per part (skin, bone
//! groups, organs; see `tools/render_anatomy.py`). Each sprite carries light, rim, specular and coverage, so
//! the colours, the layer looks (organs / skeleton / X-ray), the status tint and the outlines are made here.
//! Hit-testing reads the coverage of the sprites, so a click lands on exactly the organ under the cursor.

use crate::anatomy_sprites::{Sprite, ATLAS_PNG, SPRITES, TEXELS_PER_UNIT as TPU};
use crate::theme::*;
use eframe::egui::{self, pos2, vec2, Align2, Color32, ColorImage, FontId, Pos2, Rect, Sense, Shape, TextureFilter, TextureHandle, TextureOptions, TextureWrapMode, Vec2};
use ui_model::anatomy::{self, Side, P};
use ui_model::Status;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Organs,
    Skeleton,
    XRay,
    /// infrared: the skin surface in an ironbow palette, driven by the latest body temperature
    Thermal,
}

/// Main arteries in design units: control points of smooth curves. The right half is listed; the left half
/// is its mirror image. A branch starts on a control point of an earlier path, so its distance from the heart
/// (which times the pulse wave) continues from there.
const VESSELS: &[&[(f32, f32)]] = &[
    // aortic arch and descending aorta
    &[(126.0, 118.0), (125.0, 108.0), (122.0, 101.0), (117.0, 104.0), (119.0, 116.0), (120.0, 150.0), (120.0, 180.0), (120.0, 212.0)],
    // renal artery
    &[(120.0, 180.0), (112.0, 182.0), (104.0, 187.0)],
    // common carotid, then internal (into the skull) and superficial temporal (the side of the head)
    &[(122.0, 101.0), (117.0, 88.0), (114.0, 72.0), (113.0, 58.0)],
    &[(113.0, 58.0), (113.0, 46.0), (116.0, 32.0)],
    &[(113.0, 58.0), (107.0, 50.0), (104.0, 38.0), (106.0, 22.0)],
    // subclavian, axillary and brachial down to the elbow
    &[(117.0, 104.0), (106.0, 96.0), (90.0, 95.0), (76.0, 100.0), (66.0, 112.0), (61.0, 135.0), (59.0, 165.0), (56.0, 185.0)],
    // radial (thumb side) and ulnar
    &[(56.0, 185.0), (50.0, 205.0), (43.0, 230.0), (38.0, 255.0), (36.0, 270.0)],
    &[(56.0, 185.0), (58.0, 205.0), (54.0, 230.0), (48.0, 255.0), (45.0, 272.0)],
    // iliac, femoral and popliteal down to the knee
    &[(120.0, 212.0), (112.0, 226.0), (105.0, 240.0), (101.0, 258.0), (99.0, 290.0), (98.0, 330.0), (97.0, 372.0)],
    // anterior and posterior tibial
    &[(97.0, 372.0), (101.0, 400.0), (102.0, 440.0), (100.0, 480.0), (96.0, 497.0), (90.0, 506.0)],
    &[(97.0, 372.0), (95.0, 400.0), (94.0, 440.0), (93.0, 478.0), (92.0, 500.0)],
];
const MIRROR_X: f32 = 239.0;
/// visual pulse-wave speed, design units per second (slowed down from ~6 m/s so the eye can follow it)
const WAVE_SPEED: f32 = 380.0;
/// how far behind the wave front the glow fades, design units
const WAVE_TAIL: f32 = 42.0;
/// curve samples per control-point span
const SAMPLES: usize = 6;

/// A smooth artery: sampled points and each point's distance from the heart along the vessels.
struct Vessel {
    pts: Vec<P>,
    dist: Vec<f32>,
}

/// Catmull-Rom curve through the control points.
fn smooth_curve(c: &[(f32, f32)]) -> Vec<P> {
    let at = |i: isize| {
        let i = i.clamp(0, c.len() as isize - 1) as usize;
        c[i]
    };
    let mut out = vec![];
    for i in 0..c.len() - 1 {
        let (p0, p1, p2, p3) = (at(i as isize - 1), at(i as isize), at(i as isize + 1), at(i as isize + 2));
        for k in 0..SAMPLES {
            let t = k as f32 / SAMPLES as f32;
            let (t2, t3) = (t * t, t * t * t);
            let f = |a: f32, b: f32, c: f32, d: f32| 0.5 * (2.0 * b + (c - a) * t + (2.0 * a - 5.0 * b + 4.0 * c - d) * t2 + (3.0 * b - a - 3.0 * c + d) * t3);
            out.push(P { x: f(p0.0, p1.0, p2.0, p3.0), y: f(p0.1, p1.1, p2.1, p3.1) });
        }
    }
    let last = c[c.len() - 1];
    out.push(P { x: last.0, y: last.1 });
    out
}

/// All arteries, both halves.
fn arteries() -> Vec<Vessel> {
    let mut right: Vec<Vessel> = vec![];
    for (n, ctrl) in VESSELS.iter().enumerate() {
        let pts = smooth_curve(ctrl);
        // where the branch leaves an earlier path: that control point's distance from the heart
        let start = VESSELS[..n]
            .iter()
            .zip(&right)
            .find_map(|(parent, v)| parent.iter().position(|&q| q == ctrl[0]).map(|i| v.dist[i * SAMPLES]))
            .unwrap_or(0.0);
        let mut dist = vec![start];
        for w in pts.windows(2) {
            let d = dist[dist.len() - 1] + ((w[1].x - w[0].x).powi(2) + (w[1].y - w[0].y).powi(2)).sqrt();
            dist.push(d);
        }
        right.push(Vessel { pts, dist });
    }
    // the arch belongs to the midline; everything else also runs on the left
    let left: Vec<Vessel> = right.iter().skip(1).map(|v| Vessel { pts: v.pts.iter().map(|q| P { x: MIRROR_X - q.x, y: q.y }).collect(), dist: v.dist.clone() }).collect();
    right.into_iter().chain(left).collect()
}

/// A tapered, anti-aliased band through `pts` with a half width and a colour per point.
fn ribbon(painter: &egui::Painter, pts: &[Pos2], half: &[f32], col: &[Color32], feather: f32) {
    if pts.len() < 2 {
        return;
    }
    let mut mesh = egui::Mesh::default();
    for i in 0..pts.len() {
        let a = pts[i.saturating_sub(1)];
        let b = pts[(i + 1).min(pts.len() - 1)];
        let n = (b - a).normalized().rot90();
        let (p, h) = (pts[i], half[i]);
        mesh.colored_vertex(p - n * (h + feather), Color32::TRANSPARENT);
        mesh.colored_vertex(p - n * h, col[i]);
        mesh.colored_vertex(p + n * h, col[i]);
        mesh.colored_vertex(p + n * (h + feather), Color32::TRANSPARENT);
        if i > 0 {
            let (o, c) = ((i as u32 - 1) * 4, i as u32 * 4);
            for k in 0..3 {
                mesh.add_triangle(o + k, o + k + 1, c + k);
                mesh.add_triangle(o + k + 1, c + k + 1, c + k);
            }
        }
    }
    painter.add(Shape::mesh(mesh));
}

/// Ironbow thermal palette, `t` 0 (cold) .. 1 (hot).
fn ironbow(t: f32) -> [f32; 3] {
    const STOPS: [(f32, [f32; 3]); 7] = [
        (0.0, [0.02, 0.01, 0.06]),
        (0.18, [0.16, 0.02, 0.40]),
        (0.38, [0.55, 0.04, 0.55]),
        (0.56, [0.88, 0.25, 0.14]),
        (0.74, [0.98, 0.60, 0.02]),
        (0.90, [1.0, 0.88, 0.32]),
        (1.0, [1.0, 0.99, 0.90]),
    ];
    let t = t.clamp(0.0, 1.0);
    for w in STOPS.windows(2) {
        if t <= w[1].0 {
            return mix(w[0].1, w[1].1, (t - w[0].0) / (w[1].0 - w[0].0));
        }
    }
    STOPS[6].1
}

/// Thermal scale shown on the figure, degrees Celsius.
pub const THERMAL_MIN: f32 = 24.0;
pub const THERMAL_MAX: f32 = 41.0;

/// Skin surface temperature for a body (core) temperature and a heat factor `h` (0 cold extremity .. 1 warm core).
fn surface_temp(core: f32, h: f32) -> f32 {
    core - 0.5 - (1.0 - h) * 10.0
}

/// How warm a point of the skin runs relative to the core: trunk and head warm, limbs cooling towards the
/// hands and feet, edges (where the surface turns away) cooler. `rim` is 1 - n.view.
fn heat_factor(x: f32, y: f32, rim: f32) -> f32 {
    let smooth = |e0: f32, e1: f32, v: f32| {
        let t = ((v - e0) / (e1 - e0)).clamp(0.0, 1.0);
        t * t * (3.0 - 2.0 * t)
    };
    let dx = (x - MIRROR_X / 2.0).abs();
    // away from the midline the surface cools (shoulders, arms, hands)
    let lateral = (-(dx - 38.0).max(0.0) / 62.0).exp();
    // the neck and head a little cooler than the chest, the legs cooling down to the feet
    let vertical = 1.0 - 0.08 * smooth(86.0, 60.0, y) - 0.65 * smooth(225.0, 515.0, y);
    // on the arms, cooler down towards the hands
    let arm = smooth(46.0, 66.0, dx) * smooth(120.0, 300.0, y);
    let trunk = lateral * vertical * (1.0 - 0.45 * arm);
    let core = (1.0 - rim).clamp(0.0, 1.0).powf(0.7);
    (0.4 * core + 0.6 * trunk.clamp(0.0, 1.0)).clamp(0.0, 1.0)
}

struct Gpu {
    /// normal look (skin: the body behind everything)
    main: TextureHandle,
    /// X-ray look (skin: the glass surface in front of everything)
    alt: TextureHandle,
    /// white x coverage: status tint, hover / selection lift (organs only)
    mask: Option<TextureHandle>,
}

/// One outline around all organs of a body system (heart + great vessels, both lungs + airways, ...).
struct Outline {
    sys: &'static str,
    origin: P,
    w: usize,
    h: usize,
    /// the part that sets the pulse (the largest one), its index
    lead: usize,
    cov: Vec<u8>,
    /// status outline (2 texels)
    tex: Option<TextureHandle>,
    /// selection / hover outline (1 texel): the dimmed surroundings already make the organ stand out
    thin: Option<TextureHandle>,
}

struct Part {
    s: &'static Sprite,
    w: usize,
    h: usize,
    /// coverage for hit-testing (0..255), row by row
    cov: Vec<u8>,
    gpu: Option<Gpu>,
}

impl Part {
    fn origin(&self) -> P {
        P { x: self.s.origin[0], y: self.s.origin[1] }
    }
    fn size(&self) -> (f32, f32) {
        (self.w as f32 / TPU, self.h as f32 / TPU)
    }
    fn covers(&self, q: P) -> bool {
        let tx = ((q.x - self.s.origin[0]) * TPU).floor();
        let ty = ((q.y - self.s.origin[1]) * TPU).floor();
        if tx < 0.0 || ty < 0.0 || tx >= self.w as f32 || ty >= self.h as f32 {
            return false;
        }
        self.cov[ty as usize * self.w + tx as usize] > 110
    }
}

/// Long, thin parts that belong to a system but would drag its outline far away from the organ
/// (the aorta and the vena cava run down to the pelvis, the oesophagus up into the neck).
const NO_OUTLINE: &[&str] = &["Aorta", "Inferior vena cava", "Esophagus"];

pub struct Figure {
    parts: Vec<Part>,
    outlines: Vec<Outline>,
    atlas: Option<(usize, Vec<u8>)>,
    callouts: Vec<anatomy::Callout>,
    /// vertical extent of the body in design units
    top: f32,
    bottom: f32,
    live_labels: bool,
    /// Russian callouts and frame marks (live window only)
    russian: bool,
    /// latest body temperature (degrees C) and heart rate (bpm) from the server
    temp_c: Option<f32>,
    heart_rate: Option<f32>,
    /// skin sprite: index, heat factor per texel
    skin: Option<(usize, Vec<f32>)>,
    /// thermal skin texture and the temperature it was made for (tenths of a degree; i32::MIN = no data)
    thermal: Option<(i32, TextureHandle)>,
    arteries: Vec<Vessel>,
    /// heart beats so far (fractional), advanced each frame by the current rate so a new rate never jumps
    beat_phase: f64,
    last_time: Option<f64>,
    /// temperature drawn on the infrared view, easing towards the measured one
    shown_temp: Option<f32>,
}

fn rgb(c: [u8; 3]) -> [f32; 3] {
    [c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0]
}
fn mix(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}
fn px(c: [f32; 3], a: f32) -> Color32 {
    let f = |v: f32| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
    Color32::from_rgba_unmultiplied(f(c[0]), f(c[1]), f(c[2]), f(a))
}

/// Normal look. `sh` diffuse light (0..1.25), `rim` 1 - n.view, `sp` specular, `cv` coverage.
fn look_main(s: &Sprite, sh: f32, _rim: f32, sp: f32, cv: f32) -> Color32 {
    if s.layer == 0 {
        // the body: a dark, softly lit volume that the organs sit in
        let base = [0x18 as f32 / 255.0, 0x1f as f32 / 255.0, 0x29 as f32 / 255.0];
        let k = 0.55 + 0.7 * sh;
        return px([base[0] * k, base[1] * k, base[2] * k], cv * 0.96);
    }
    let t = rgb(s.tissue);
    let c = [t[0] * sh + sp * 0.28, t[1] * sh + sp * 0.28, t[2] * sh + sp * 0.28];
    px(c, cv)
}

/// X-ray look; for the skin the glass surface drawn over everything.
fn look_alt(s: &Sprite, sh: f32, rim: f32, _sp: f32, cv: f32) -> Color32 {
    match s.layer {
        0 => px([0.64, 0.72, 0.81], cv * (0.03 + 0.62 * rim.powi(3)).min(1.0)),
        1 => {
            // bone on a radiograph: bright, brightest at the cortical edge
            let i = 0.25 + 0.75 * rim.powf(1.3);
            let c = mix([0.24, 0.33, 0.44], [0.80, 0.87, 0.94], i);
            let k = 0.80 + 0.20 * (sh / 1.25);
            px([c[0] * k, c[1] * k, c[2] * k], cv * (0.24 + 0.50 * rim))
        }
        _ => {
            let t = rgb(s.tissue);
            let g = 0.3 * t[0] + 0.55 * t[1] + 0.15 * t[2];
            let c = mix(t, [g * 0.9, g * 0.95, g * 1.08], 0.35);
            let k = 0.6 + 0.6 * rim;
            px([c[0] * k, c[1] * k, c[2] * k], cv * (0.34 + 0.5 * rim.powf(1.5)))
        }
    }
}

impl Figure {
    pub fn new() -> Self {
        let img = image::load_from_memory_with_format(ATLAS_PNG, image::ImageFormat::Png).expect("anatomy atlas").to_rgba8();
        let aw = img.width() as usize;
        let raw = img.into_raw();
        let parts: Vec<Part> = SPRITES
            .iter()
            .map(|s| {
                let [ax, ay, w, h] = s.atlas.map(|v| v as usize);
                let mut cov = Vec::with_capacity(w * h);
                for y in 0..h {
                    for x in 0..w {
                        cov.push(raw[((ay + y) * aw + ax + x) * 4 + 3]);
                    }
                }
                Part { s, w, h, cov, gpu: None }
            })
            .collect();
        let (top, bottom) = parts.iter().find(|p| p.s.layer == 0).map(|p| (p.origin().y, p.origin().y + p.size().1)).unwrap_or((0.0, anatomy::H));
        let mut outlines: Vec<Outline> = vec![];
        for p in parts.iter().filter(|p| p.s.layer == 2 && p.s.hit) {
            if !outlines.iter().any(|o| o.sys == p.s.sys) {
                outlines.push(Outline { sys: p.s.sys, origin: p.origin(), w: 0, h: 0, lead: 0, cov: vec![], tex: None, thin: None });
            }
        }
        for o in outlines.iter_mut() {
            let members: Vec<&Part> = parts.iter().filter(|p| p.s.layer == 2 && p.s.sys == o.sys && !NO_OUTLINE.contains(&p.s.name)).collect();
            let pad = 3.0 / TPU;
            let x0 = members.iter().map(|p| p.origin().x).fold(f32::MAX, f32::min) - pad;
            let y0 = members.iter().map(|p| p.origin().y).fold(f32::MAX, f32::min) - pad;
            let x1 = members.iter().map(|p| p.origin().x + p.size().0).fold(f32::MIN, f32::max) + pad;
            let y1 = members.iter().map(|p| p.origin().y + p.size().1).fold(f32::MIN, f32::max) + pad;
            o.origin = P { x: x0, y: y0 };
            o.w = ((x1 - x0) * TPU).round() as usize;
            o.h = ((y1 - y0) * TPU).round() as usize;
            o.cov = vec![0u8; o.w * o.h];
            for p in &members {
                let ox = ((p.origin().x - x0) * TPU).round() as usize;
                let oy = ((p.origin().y - y0) * TPU).round() as usize;
                for y in 0..p.h {
                    for x in 0..p.w {
                        let d = &mut o.cov[(oy + y) * o.w + ox + x];
                        *d = (*d).max(p.cov[y * p.w + x]);
                    }
                }
            }
            fill_holes(&mut o.cov, o.w, o.h);
            o.lead = parts.iter().enumerate().filter(|(_, p)| p.s.layer == 2 && p.s.sys == o.sys).max_by_key(|(_, p)| p.w * p.h).map(|(i, _)| i).unwrap_or(0);
        }
        Figure { parts, outlines, atlas: Some((aw, raw)), callouts: anatomy::callouts(), top, bottom, live_labels: false, russian: false, temp_c: None, heart_rate: None, skin: None, thermal: None, arteries: arteries(), beat_phase: 0.0, last_time: None, shown_temp: None }
    }

    /// The shared-server view reports metric-rule alerts, not organ diagnoses.
    pub fn with_live_labels(mut self) -> Self {
        self.live_labels = true;
        self
    }

    /// Russian or English callouts in the live window; the demo keeps its English labels.
    pub fn set_russian(&mut self, russian: bool) {
        self.russian = russian;
    }

    fn callout_labels<'a>(&self, system: &str, original: &'a str, status: Status) -> (&'a str, &'a str) {
        if self.live_labels && self.russian {
            (
                match system {
                    "brain" => "Мозг",
                    "eyes" => "Зрение",
                    "lungs" => "Дыхание",
                    "heart" => "Сердце",
                    "gut" => "Обмен",
                    "msk" => "Кости",
                    "immune" => "Всё тело",
                    _ => original,
                },
                match status {
                    Status::Normal => "НЕТ СИГНАЛОВ",
                    Status::Watch => "ВНИМАНИЕ",
                    Status::Act => "СРОЧНО",
                    Status::Unassessed | Status::NoData => "НЕ ОЦЕНЕНО",
                },
            )
        } else if self.live_labels {
            (if system == "immune" { "BODY / TEMP" } else { original }, match status {
                Status::Normal => "NO ALERTS",
                Status::Watch => "ATTENTION",
                Status::Act => "URGENT",
                Status::Unassessed | Status::NoData => "NOT ASSESSED",
            })
        } else {
            (original, status.name())
        }
    }

    /// Latest temperature and heart rate: the infrared view and the pulse follow them.
    pub fn set_vitals(&mut self, temp_c: Option<f32>, heart_rate: Option<f32>) {
        self.temp_c = temp_c.filter(|t| t.is_finite());
        self.heart_rate = heart_rate.filter(|h| h.is_finite() && *h > 0.0).map(|h| h.clamp(20.0, 240.0));
    }

    /// Seconds per heart beat: from the measured heart rate, otherwise a resting 1.1 s.
    fn beat_period(&self) -> f64 {
        self.heart_rate.map_or(1.1, |h| 60.0 / h as f64)
    }

    /// The row under the figure: the infrared scale in the thermal view, the pulse otherwise.
    pub fn legend(&self, ui: &mut egui::Ui, mode: Mode, time: f64, animate: bool) {
        let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 22.0), Sense::hover());
        let p = ui.painter_at(rect);
        let ru = self.russian;
        let y = rect.center().y;
        let no_data = if ru { "НЕТ ДАННЫХ" } else { "NO DATA" };
        if mode == Mode::Thermal {
            p.text(pos2(rect.left(), y), Align2::LEFT_CENTER, if ru { "ИК" } else { "IR" }, mono(10.0), MUT);
            let bar = Rect::from_min_max(pos2(rect.left() + 26.0, y - 5.0), pos2(rect.right() - 92.0, y + 2.0));
            let n = 48;
            for i in 0..n {
                let t0 = i as f32 / n as f32;
                let r = Rect::from_min_max(pos2(bar.left() + bar.width() * t0, bar.top()), pos2(bar.left() + bar.width() * (t0 + 1.0 / n as f32) + 0.5, bar.bottom()));
                let c = if self.temp_c.is_some() { ironbow(t0) } else { [0.10 + 0.32 * t0; 3] };
                p.rect_filled(r, egui::CornerRadius::ZERO, px(c, 1.0));
            }
            p.rect_stroke(bar, egui::CornerRadius::ZERO, stroke(1.0, LINE_HI), egui::StrokeKind::Outside);
            p.text(pos2(bar.left(), bar.bottom() + 1.0), Align2::LEFT_TOP, format!("{THERMAL_MIN:.0}°"), mono(8.0), alpha(MUT, 0.8));
            p.text(pos2(bar.right(), bar.bottom() + 1.0), Align2::RIGHT_TOP, format!("{THERMAL_MAX:.0}°"), mono(8.0), alpha(MUT, 0.8));
            let text = match self.temp_c {
                Some(t) => {
                    let k = ((t - THERMAL_MIN) / (THERMAL_MAX - THERMAL_MIN)).clamp(0.0, 1.0);
                    let x = bar.left() + bar.width() * k;
                    p.line_segment([pos2(x, bar.top() - 3.0), pos2(x, bar.bottom() + 3.0)], stroke(2.0, TXT));
                    let v = format!("{t:.1}");
                    format!("{} °C", if ru { v.replace('.', ",") } else { v })
                }
                None => no_data.to_owned(),
            };
            p.text(pos2(rect.right(), y), Align2::RIGHT_CENTER, text, mono(12.0), if self.temp_c.is_some() { TXT } else { MUT });
        } else {
            let _ = time;
            let on = animate && self.heart_rate.is_some() && (self.beat_phase - 0.2).rem_euclid(1.0) < 0.16;
            let col = Color32::from_rgb(0xe8, 0x4a, 0x52);
            let m = Rect::from_center_size(pos2(rect.left() + 5.0, y), Vec2::splat(8.0));
            if on {
                p.rect_filled(m, egui::CornerRadius::ZERO, col);
            } else {
                p.rect_stroke(m, egui::CornerRadius::ZERO, stroke(1.0, alpha(col, 0.8)), egui::StrokeKind::Inside);
            }
            p.text(pos2(rect.left() + 18.0, y), Align2::LEFT_CENTER, if ru { "ПУЛЬС · АРТЕРИИ" } else { "PULSE · ARTERIES" }, mono(10.0), MUT);
            let text = match self.heart_rate {
                Some(h) => format!("{h:.0} {}", if ru { "уд/мин" } else { "bpm" }),
                None => no_data.to_owned(),
            };
            p.text(pos2(rect.right(), y), Align2::RIGHT_CENTER, text, mono(12.0), if self.heart_rate.is_some() { TXT } else { MUT });
        }
    }

    /// Rebuilds the infrared skin texture when the temperature changes by a tenth of a degree or more.
    fn thermal_texture(&mut self, ctx: &egui::Context) -> Option<egui::TextureId> {
        // ease the picture towards a new reading instead of switching at once
        self.shown_temp = match (self.temp_c, self.shown_temp) {
            (Some(t), Some(shown)) => {
                let dt = ctx.input(|i| i.stable_dt).min(0.1);
                let next = shown + (t - shown) * (1.0 - (-dt * 2.5).exp());
                if (next - t).abs() < 0.05 {
                    Some(t)
                } else {
                    ctx.request_repaint();
                    Some(next)
                }
            }
            (t, _) => t,
        };
        let key = self.shown_temp.map_or(i32::MIN, |t| (t * 10.0).round() as i32);
        if self.thermal.as_ref().is_none_or(|(k, _)| *k != key) {
            let (i, heat) = self.skin.as_ref()?;
            let part = &self.parts[*i];
            let pixels = heat
                .iter()
                .zip(&part.cov)
                .map(|(&h, &c)| {
                    let a = c as f32 / 255.0;
                    match self.shown_temp {
                        Some(t) => px(ironbow((surface_temp(t, h) - THERMAL_MIN) / (THERMAL_MAX - THERMAL_MIN)), a),
                        None => px([0.10 + 0.32 * h; 3], a),
                    }
                })
                .collect();
            let opts = TextureOptions { magnification: TextureFilter::Linear, minification: TextureFilter::Linear, wrap_mode: TextureWrapMode::ClampToEdge, mipmap_mode: Some(TextureFilter::Linear) };
            let tex = ctx.load_texture("anatomy-thermal", ColorImage { size: [part.w, part.h], pixels }, opts);
            self.thermal = Some((key, tex));
        }
        self.thermal.as_ref().map(|(_, t)| t.id())
    }

    /// Builds the textures on first use (needs the egui context).
    fn upload(&mut self, ctx: &egui::Context) {
        let Some((aw, raw)) = self.atlas.take() else { return };
        let opts = TextureOptions { magnification: TextureFilter::Linear, minification: TextureFilter::Linear, wrap_mode: TextureWrapMode::ClampToEdge, mipmap_mode: Some(TextureFilter::Linear) };
        for (i, p) in self.parts.iter_mut().enumerate() {
            let [ax, ay, w, h] = p.s.atlas.map(|v| v as usize);
            let n = w * h;
            let (mut main, mut alt) = (Vec::with_capacity(n), Vec::with_capacity(n));
            for y in 0..h {
                for x in 0..w {
                    let o = ((ay + y) * aw + ax + x) * 4;
                    let (sh, rim, sp, cv) = (raw[o] as f32 / 255.0 * 1.25, raw[o + 1] as f32 / 255.0, raw[o + 2] as f32 / 255.0, raw[o + 3] as f32 / 255.0);
                    main.push(look_main(p.s, sh, rim, sp, cv));
                    alt.push(look_alt(p.s, sh, rim, sp, cv));
                }
            }
            let load = |tag: &str, pixels: Vec<Color32>| ctx.load_texture(format!("anatomy-{i}-{tag}"), ColorImage { size: [w, h], pixels }, opts);
            if p.s.layer == 0 && self.skin.is_none() {
                let mut heat = Vec::with_capacity(n);
                for y in 0..h {
                    for x in 0..w {
                        let rim = raw[((ay + y) * aw + ax + x) * 4 + 1] as f32 / 255.0;
                        heat.push(heat_factor(p.s.origin[0] + x as f32 / TPU, p.s.origin[1] + y as f32 / TPU, rim));
                    }
                }
                self.skin = Some((i, heat));
            }
            let mask = (p.s.layer == 2 && p.s.hit).then(|| load("mask", p.cov.iter().map(|&c| white_a(c)).collect()));
            p.gpu = Some(Gpu { main: load("main", main), alt: load("alt", alt), mask });
        }
        for o in self.outlines.iter_mut() {
            let edge = |r: usize| -> Vec<Color32> { dilate(&o.cov, o.w, o.h, r).iter().zip(&o.cov).map(|(&g, &c)| white_a(g.saturating_sub(c))).collect() };
            o.tex = Some(ctx.load_texture(format!("anatomy-outline-{}", o.sys), ColorImage { size: [o.w, o.h], pixels: edge(2) }, opts));
            o.thin = Some(ctx.load_texture(format!("anatomy-outline-thin-{}", o.sys), ColorImage { size: [o.w, o.h], pixels: edge(1) }, opts));
        }
    }

    fn layer_alpha(layer: u8, mode: Mode) -> f32 {
        match (layer, mode) {
            (0, _) => 1.0,
            (1, Mode::Organs) => 0.16,
            (1, Mode::Skeleton) => 1.0,
            (1, Mode::XRay) => 0.9,
            (_, Mode::Organs) => 1.0,
            (_, Mode::Skeleton) => 0.10,
            (_, Mode::XRay) => 1.0,
            (_, Mode::Thermal) => 0.0,
        }
    }

    /// Draws the figure and returns the clicked body system, if any.
    #[allow(clippy::too_many_arguments)]
    pub fn show(&mut self, ui: &mut egui::Ui, size: Vec2, status_of: &dyn Fn(&str) -> Status, selected: &str, mode: Mode, time: f64, animate: bool) -> Option<String> {
        self.upload(ui.ctx());
        let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
        let painter = ui.painter_at(rect);
        painter.add(panel_shape(rect, CUT, SURFACE, stroke(1.0, LINE)));

        // fit: the body fills the height; the shoulders must clear the label columns
        let label_w = 66.0;
        let body_h = self.bottom - self.top;
        let s = ((rect.height() - 34.0) / body_h).min((rect.width() / 2.0 - label_w - 14.0) / 78.0).max(0.2);
        let origin = pos2(rect.center().x - 120.0 * s, rect.top() + 10.0 - self.top * s);
        let to_screen = move |q: P| origin + vec2(q.x, q.y) * s;
        let to_design = move |q: Pos2| P { x: (q.x - origin.x) / s, y: (q.y - origin.y) / s };

        frame_marks(&painter, rect, to_screen(P { x: 0.0, y: self.top }).y, to_screen(P { x: 0.0, y: self.bottom }).y, self.live_labels && self.russian);

        let period = self.beat_period();
        let dt = self.last_time.map_or(0.0, |last| (time - last).clamp(0.0, 0.1));
        self.last_time = Some(time);
        self.beat_phase += dt / period;
        let thermal_tex = if mode == Mode::Thermal { self.thermal_texture(ui.ctx()) } else { None };
        let beat = if animate { (self.beat_phase * std::f64::consts::TAU).sin().max(0.0).powi(6) as f32 } else { 0.0 };
        let breathe = if animate { (0.5 + 0.5 * (time * std::f64::consts::TAU / 4.2).sin()) as f32 } else { 0.0 };
        let alert = if animate { (0.5 + 0.5 * (time * std::f64::consts::TAU / 1.4).sin()) as f32 } else { 0.5 };

        // drawing order for the mode: skin body, the quiet layer, the emphasised layer, then the skin glass.
        // A selection in the quiet layer (bones while looking at organs, an organ in the skeleton view) goes on top.
        let sel_layer = self.parts.iter().filter(|p| p.s.layer != 0 && p.s.sys == selected).map(|p| p.s.layer).max();
        let order: [u8; 2] = match (sel_layer, mode) {
            (Some(1), _) => [2, 1],
            (Some(_), _) => [1, 2],
            (None, Mode::Organs) => [1, 2],
            (None, _) => [2, 1],
        };
        let alpha_of = |p: &Part, sel: bool, hov: bool| -> f32 {
            let mut a = Self::layer_alpha(p.s.layer, mode);
            if (sel || hov) && p.s.layer != 0 {
                a = a.max(if sel { 0.9 } else { 0.6 });
            }
            a
        };

        // --- hover: labels first, then the front-most visible part under the cursor
        let hover = resp.hover_pos();
        let mut label_rects: Vec<Rect> = vec![];
        let mut hovered: Option<&'static str> = None;
        for c in &self.callouts {
            let y = to_screen(P { x: 0.0, y: c.label_y }).y;
            let r = match c.side {
                Side::Left => Rect::from_min_size(pos2(rect.left() + 12.0, y - 13.0), vec2(label_w, 26.0)),
                Side::Right => Rect::from_min_size(pos2(rect.right() - 12.0 - label_w, y - 13.0), vec2(label_w, 26.0)),
            };
            if hover.is_some_and(|h| r.contains(h)) {
                hovered = Some(c.sys);
            }
            label_rects.push(r);
        }
        if hovered.is_none() {
            if let Some(hp) = hover {
                let q = to_design(hp);
                'outer: for layer in order.iter().rev() {
                    for p in self.parts.iter().rev().filter(|p| p.s.layer == *layer && p.s.hit) {
                        if alpha_of(p, p.s.sys == selected, false) >= 0.3 && p.covers(q) {
                            hovered = Some(p.s.sys);
                            break 'outer;
                        }
                    }
                }
            }
        }
        if hovered.is_some() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }

        // focus 1 = full light; with a selection the other systems fade to DIM (a hovered one comes half back)
        const DIM: f32 = 0.28;
        let ctx = ui.ctx().clone();
        let any = ctx.animate_bool_with_time(egui::Id::new("figure-any-selected"), !selected.is_empty(), 0.18);
        let mut focus: Vec<(&'static str, f32)> = vec![];
        for p in &self.parts {
            if p.s.layer == 0 || focus.iter().any(|f| f.0 == p.s.sys) {
                continue;
            }
            let target = if selected.is_empty() || p.s.sys == selected {
                1.0
            } else if hovered == Some(p.s.sys) {
                0.65
            } else {
                0.0
            };
            focus.push((p.s.sys, ctx.animate_value_with_time(egui::Id::new(("figure-focus", p.s.sys)), target, 0.18)));
        }
        let focus_of = |sys: &str| focus.iter().find(|f| f.0 == sys).map_or(1.0, |f| f.1);
        let light_of = |sys: &str| DIM + (1.0 - DIM) * focus_of(sys);

        // --- sprites
        let full = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
        let place = |p: &Part| -> Rect {
            let (w, h) = p.size();
            let o = p.origin();
            let k = match p.s.pulse {
                1 => 1.0 + 0.035 * beat,
                2 => 1.0 + 0.018 * breathe,
                _ => 1.0,
            };
            let c = P { x: o.x + w / 2.0, y: o.y + h / 2.0 };
            Rect::from_center_size(to_screen(c), vec2(w * s * k, h * s * k))
        };
        let tint = |a: f32| white_a((a.clamp(0.0, 1.0) * 255.0) as u8);
        // colour x k at opacity a (premultiplied)
        let lit = |k: f32, a: f32| {
            let (k, a) = (k.clamp(0.0, 1.0), a.clamp(0.0, 1.0));
            let v = (k * a * 255.0 + 0.5) as u8;
            Color32::from_rgba_premultiplied(v, v, v, (a * 255.0 + 0.5) as u8)
        };
        let xray = mode == Mode::XRay;
        for p in self.parts.iter().filter(|p| p.s.layer == 0) {
            let g = p.gpu.as_ref().unwrap();
            painter.image(g.main.id(), place(p), full, lit(1.0 - 0.3 * any, if xray { 0.75 } else { 1.0 }));
        }
        if let Some(id) = thermal_tex {
            for p in self.parts.iter().filter(|p| p.s.layer == 0) {
                painter.image(id, place(p), full, lit(1.0 - 0.15 * any, 1.0));
            }
        }
        let layers: &[u8] = if thermal_tex.is_some() { &[] } else { &order };
        for &layer in layers {
            for p in self.parts.iter().filter(|p| p.s.layer == layer) {
                let sel = p.s.sys == selected;
                let hov = hovered == Some(p.s.sys);
                let a = alpha_of(p, sel, hov);
                if a < 0.03 {
                    continue;
                }
                let g = p.gpu.as_ref().unwrap();
                let r = place(p);
                let k = light_of(p.s.sys);
                painter.image(if xray { g.alt.id() } else { g.main.id() }, r, full, lit(k, a));
                if let Some(m) = &g.mask {
                    // organs carry their status as a light tint; bones stay neutral (their marker shows it)
                    match status_of(p.s.sys) {
                        Status::Watch => {
                            painter.image(m.id(), r, full, alpha(WARN, 0.12 * a * k));
                        }
                        Status::Act => {
                            painter.image(m.id(), r, full, alpha(CRIT, (0.12 + 0.10 * alert) * a * k));
                        }
                        _ => {}
                    }
                    if sel {
                        painter.image(m.id(), r, full, alpha(Color32::WHITE, 0.05 * a));
                    }
                }
            }
        }
        // arteries: a dim network; with each heart beat a bright pulse wave runs out from the heart
        let vessel_a = match mode {
            Mode::Organs => 1.0,
            Mode::XRay => 0.85,
            Mode::Skeleton => 0.35,
            Mode::Thermal => 0.0,
        } * light_of("heart");
        if vessel_a > 0.0 {
            let red = Color32::from_rgb(0xd8, 0x3c, 0x46);
            let bright = Color32::from_rgb(0xff, 0x9c, 0x94);
            // beats counted from systole; the wave front leaves the heart at phase 0
            let ph = (animate && self.heart_rate.is_some()).then_some((self.beat_phase - 0.2) as f32);
            let span = WAVE_SPEED * period as f32;
            for v in &self.arteries {
                let pts: Vec<Pos2> = v.pts.iter().map(|&q| to_screen(q)).collect();
                // wide near the heart, fine at the hands and feet
                let half: Vec<f32> = v.dist.iter().map(|&d| ((0.32 + 0.95 * (-d / 150.0).exp()) * s).max(0.45)).collect();
                let glow: Vec<f32> = v
                    .dist
                    .iter()
                    .map(|&d| {
                        ph.map_or(0.0, |ph| {
                            let behind = (ph - d / span).rem_euclid(1.0) * span;
                            // soft rise at the front, slow fade behind it, weaker far from the heart
                            (1.0 - (-behind / 5.0).exp()) * (-behind / WAVE_TAIL).exp() * (-d / 1100.0).exp()
                        })
                    })
                    .collect();
                if glow.iter().any(|&g| g > 0.02) {
                    let halo_w: Vec<f32> = half.iter().map(|h| h * 2.4 + 1.0).collect();
                    let halo_c: Vec<Color32> = glow.iter().map(|&g| alpha(red, 0.22 * g * vessel_a)).collect();
                    ribbon(&painter, &pts, &halo_w, &halo_c, 2.5);
                }
                let core: Vec<Color32> = glow
                    .iter()
                    .map(|&g| {
                        let c = Color32::from_rgb(
                            (red.r() as f32 + (bright.r() as f32 - red.r() as f32) * g) as u8,
                            (red.g() as f32 + (bright.g() as f32 - red.g() as f32) * g) as u8,
                            (red.b() as f32 + (bright.b() as f32 - red.b() as f32) * g) as u8,
                        );
                        alpha(c, (0.38 + 0.55 * g) * vessel_a)
                    })
                    .collect();
                ribbon(&painter, &pts, &half, &core, 0.8);
            }
        }
        if thermal_tex.is_none() {
            for p in self.parts.iter().filter(|p| p.s.layer == 0) {
                let g = p.gpu.as_ref().unwrap();
                painter.image(g.alt.id(), place(p), full, tint(1.0 - 0.45 * any));
            }
        }
        // one outline per system: thin white when selected / hovered, otherwise the status colour
        for o in &self.outlines {
            let st = status_of(o.sys);
            let sel = o.sys == selected;
            let hov = hovered == Some(o.sys);
            let f = 0.25 + 0.75 * focus_of(o.sys);
            let (col, thin) = if sel {
                (alpha(Color32::WHITE, 0.95), true)
            } else if hov {
                (alpha(Color32::WHITE, 0.6), true)
            } else {
                match st {
                    Status::Watch => (alpha(WARN, 0.9 * f), false),
                    Status::Act => (alpha(CRIT, (0.6 + 0.4 * alert) * f), false),
                    _ => continue,
                }
            };
            // follow the pulse of the system's largest part (heart beat, breathing)
            let lead = &self.parts[o.lead];
            let lr = place(lead);
            let k = lr.width() / (lead.size().0 * s);
            let c = lr.center();
            let r = Rect::from_min_size(to_screen(o.origin), vec2(o.w as f32 / TPU * s, o.h as f32 / TPU * s));
            let r = Rect::from_min_max(c + (r.min - c) * k, c + (r.max - c) * k);
            let tex = if thin { o.thin.as_ref() } else { o.tex.as_ref() };
            painter.image(tex.unwrap().id(), r, full, col);
        }

        // --- callouts: square marker on the organ, 45-degree leader, tracked mono label
        for (c, lr) in self.callouts.iter().zip(&label_rects) {
            let st = status_of(c.sys);
            let col = if self.live_labels { match st {
                Status::Normal => Color32::from_rgb(89, 199, 139),
                Status::Unassessed | Status::NoData => MUT,
                _ => status_color(st),
            }} else { status_color(st) };
            let sel = c.sys == selected;
            let hov = hovered == Some(c.sys);
            let a = to_screen(c.anchor);
            let ly = lr.center().y;
            let (edge_x, dir) = match c.side {
                Side::Left => (lr.right() + 4.0, -1.0),
                Side::Right => (lr.left() - 4.0, 1.0),
            };
            let run = (edge_x - a.x).abs();
            let diag = (ly - a.y).abs().min((run - 10.0).max(0.0));
            let elbow = pos2(a.x + dir * diag, ly);
            let back = 0.35 + 0.65 * focus_of(c.sys);
            let lc = if sel || hov { alpha(TXT, 0.85) } else { alpha(MUT, 0.7 * back) };
            // the leader starts at the marker's bracket, not under it
            let d = elbow - a;
            let start = if d.length() > 8.0 { a + d.normalized() * 7.0 } else { a + vec2(dir * 7.0, 0.0) };
            let mut path = vec![start];
            if d.length() > 8.0 {
                path.push(elbow);
            }
            path.push(pos2(edge_x, ly));
            painter.add(Shape::line(path, stroke(1.0, lc)));
            painter.line_segment([pos2(edge_x, ly - 3.0), pos2(edge_x, ly + 3.0)], stroke(1.0, lc));
            // marker: square with a bracket around it
            let pulse = if st == Status::Act { 1.5 * alert } else { 0.0 };
            painter.rect_filled(Rect::from_center_size(a, vec2(6.0, 6.0)), egui::CornerRadius::ZERO, col);
            painter.rect_stroke(Rect::from_center_size(a, Vec2::splat(11.0 + pulse * 2.0)), egui::CornerRadius::ZERO, stroke(1.0, alpha(col, if sel || hov { 0.9 } else { 0.5 * back })), egui::StrokeKind::Middle);
            let (pos, al) = match c.side {
                Side::Left => (pos2(lr.left(), ly), Align2::LEFT_CENTER),
                Side::Right => (pos2(lr.right(), ly), Align2::RIGHT_CENTER),
            };
            let (label, status) = self.callout_labels(c.sys, c.label, st);
            let name = egui::RichText::new(label.to_uppercase()).font(mono(10.5)).extra_letter_spacing(0.6).color(if sel || hov { TXT } else { alpha(Color32::from_rgb(0xc0, 0xc8, 0xd3), back) });
            let g = egui::WidgetText::from(name).into_galley(ui, Some(egui::TextWrapMode::Extend), f32::INFINITY, FontId::default());
            let tp = match al {
                Align2::LEFT_CENTER => pos + vec2(0.0, -6.0 - g.size().y / 2.0),
                _ => pos + vec2(-g.size().x, -6.0 - g.size().y / 2.0),
            };
            painter.galley(tp, g, TXT);
            painter.text(pos + vec2(0.0, 7.0), al, status.to_uppercase(), mono(9.0), if sel || hov { col } else { alpha(col, back) });
        }

        if resp.clicked() {
            hovered.map(|h| h.to_string())
        } else {
            None
        }
    }
}

/// Measurement marks: a height scale on the left edge (10 cm ticks), corner brackets, R / L and the view name.
fn frame_marks(p: &egui::Painter, rect: Rect, y_top: f32, y_bottom: f32, russian: bool) {
    let c = alpha(LINE_HI, 0.9);
    // scale: the body spans 0..~174 cm from the soles up; 10 cm ticks, 50 cm long ticks
    let cm = (y_bottom - y_top) / 174.0;
    let x = rect.left() + 1.0;
    let mut k = 0;
    while (k as f32) * 10.0 <= 174.0 {
        let y = y_bottom - k as f32 * 10.0 * cm;
        let long = k % 5 == 0;
        p.line_segment([pos2(x, y), pos2(x + if long { 7.0 } else { 4.0 }, y)], stroke(1.0, c));
        k += 1;
    }
    let b = 10.0;
    for (corner, dx, dy) in [(rect.left_bottom() + vec2(5.0, -5.0), 1.0, -1.0), (rect.right_bottom() + vec2(-5.0, -5.0), -1.0, -1.0)] {
        p.line_segment([corner, corner + vec2(dx * b, 0.0)], stroke(1.0, c));
        p.line_segment([corner, corner + vec2(0.0, dy * b)], stroke(1.0, c));
    }
    // radiology convention: the patient's right is on the viewer's left
    let (right, left, view) = if russian { ("П", "Л", "ВИД СПЕРЕДИ") } else { ("R", "L", "ANTERIOR") };
    p.text(rect.left_bottom() + vec2(14.0, -12.0), Align2::LEFT_BOTTOM, right, mono(10.0), MUT);
    p.text(rect.right_bottom() + vec2(-14.0, -12.0), Align2::RIGHT_BOTTOM, left, mono(10.0), MUT);
    p.text(pos2(rect.center().x, rect.bottom() - 12.0), Align2::CENTER_BOTTOM, view, mono(9.0), alpha(MUT, 0.7));
}

/// Fills enclosed gaps (between intestine loops, ...) so an outline only runs around the outside.
fn fill_holes(c: &mut [u8], w: usize, h: usize) {
    let mut outside = vec![false; w * h];
    let mut stack: Vec<usize> = (0..w).flat_map(|x| [x, (h - 1) * w + x]).chain((0..h).flat_map(|y| [y * w, y * w + w - 1])).collect();
    while let Some(i) = stack.pop() {
        if outside[i] || c[i] >= 128 {
            continue;
        }
        outside[i] = true;
        let (x, y) = (i % w, i / w);
        if x > 0 {
            stack.push(i - 1);
        }
        if x + 1 < w {
            stack.push(i + 1);
        }
        if y > 0 {
            stack.push(i - w);
        }
        if y + 1 < h {
            stack.push(i + w);
        }
    }
    for (v, out) in c.iter_mut().zip(&outside) {
        if !out && *v < 128 {
            *v = 255;
        }
    }
}

/// Grows the coverage by `r` texels (square max filter, separable).
fn dilate(c: &[u8], w: usize, h: usize, r: usize) -> Vec<u8> {
    let mut tmp = vec![0u8; w * h];
    for y in 0..h {
        for x in 0..w {
            let (a, b) = (x.saturating_sub(r), (x + r).min(w - 1));
            tmp[y * w + x] = c[y * w + a..=y * w + b].iter().copied().max().unwrap_or(0);
        }
    }
    let mut out = vec![0u8; w * h];
    for y in 0..h {
        let (a, b) = (y.saturating_sub(r), (y + r).min(h - 1));
        for x in 0..w {
            out[y * w + x] = (a..=b).map(|yy| tmp[yy * w + x]).max().unwrap_or(0);
        }
    }
    out
}

/// White with alpha `a` (premultiplied, gamma space). `Color32::from_white_alpha` treats `a` as linear
/// and would make half-transparent layers brighter instead of fainter.
fn white_a(a: u8) -> Color32 {
    Color32::from_rgba_premultiplied(a, a, a, a)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atlas_decodes_and_every_sprite_fits() {
        let img = image::load_from_memory_with_format(ATLAS_PNG, image::ImageFormat::Png).unwrap();
        for s in SPRITES {
            let [x, y, w, h] = s.atlas.map(u32::from);
            assert!(w > 0 && h > 0 && x + w <= img.width() && y + h <= img.height(), "{}", s.name);
        }
        let f = Figure::new();
        assert_eq!(f.parts.len(), SPRITES.len());
        assert!(f.bottom - f.top > 480.0, "the body spans head to soles: {}..{}", f.top, f.bottom);
    }

    #[test]
    fn every_system_has_a_clickable_sprite_under_its_marker() {
        // the leader-line anchors come from ui-model; the sprites must sit under them (same projection)
        let f = Figure::new();
        for c in anatomy::callouts() {
            assert!(f.parts.iter().any(|p| p.s.sys == c.sys && p.s.hit && p.covers(c.anchor)), "nothing of '{}' under its marker {:?}", c.sys, c.anchor);
        }
    }

    #[test]
    fn hit_test_picks_the_organ_not_its_neighbour() {
        let f = Figure::new();
        let heart = f.parts.iter().find(|p| p.s.name == "Heart").unwrap();
        let liver = f.parts.iter().find(|p| p.s.name == "Liver").unwrap();
        let at = |p: &Part| {
            // a covered texel near the middle of the sprite
            let (w, h) = (p.w, p.h);
            let (mut bx, mut by, mut best) = (0, 0, usize::MAX);
            for y in 0..h {
                for x in 0..w {
                    let d = x.abs_diff(w / 2) + y.abs_diff(h / 2);
                    if p.cov[y * w + x] == 255 && d < best {
                        (bx, by, best) = (x, y, d);
                    }
                }
            }
            P { x: p.s.origin[0] + (bx as f32 + 0.5) / TPU, y: p.s.origin[1] + (by as f32 + 0.5) / TPU }
        };
        assert!(heart.covers(at(heart)) && !liver.covers(at(heart)));
        assert!(liver.covers(at(liver)) && !heart.covers(at(liver)));
        assert!(!heart.covers(P { x: 5.0, y: 5.0 }));
    }

    #[test]
    fn outlines_leave_out_the_long_vessels() {
        let f = Figure::new();
        let heart = f.outlines.iter().find(|o| o.sys == "heart").unwrap();
        // the aorta and the vena cava reach the pelvis (y > 250); the heart outline must stop at the chest
        assert!(heart.origin.y + heart.h as f32 / TPU < 200.0, "{:?} {}", heart.origin, heart.h);
    }
}

#[cfg(test)]
mod live_label_tests {
    use super::*;
    #[test]
    fn live_temperature_and_missing_data_do_not_claim_organ_health() {
        let demo = Figure::new();
        assert_eq!(demo.callout_labels("immune", "Immune", Status::Normal), ("Immune", "Normal"));
        let live = demo.with_live_labels();
        assert_eq!(live.callout_labels("immune", "Immune", Status::Normal), ("BODY / TEMP", "NO ALERTS"));
        assert_eq!(live.callout_labels("eyes", "Eyes", Status::NoData), ("Eyes", "NOT ASSESSED"));
        assert_eq!(live.callout_labels("heart", "Heart", Status::Unassessed), ("Heart", "NOT ASSESSED"));
        assert_eq!(live.callout_labels("heart", "Heart", Status::Act), ("Heart", "URGENT"));
        let mut russian = live;
        russian.set_russian(true);
        assert_eq!(russian.callout_labels("heart", "Heart", Status::Watch), ("Сердце", "ВНИМАНИЕ"));
        assert_eq!(russian.callout_labels("immune", "Immune", Status::NoData), ("Всё тело", "НЕ ОЦЕНЕНО"));
    }
}
