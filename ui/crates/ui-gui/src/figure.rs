//! Anatomical figure viewer: shaded meshes from `ui_model::anatomy`, layers, leader-line labels.

use crate::theme::*;
use eframe::egui::{self, epaint::Vertex, pos2, vec2, Align2, Color32, FontId, Mesh, Pos2, Rect, Sense, Shape, Vec2};
use std::sync::Arc;
use ui_model::anatomy::{self, Geo, Layer, Pulse, Side, P};
use ui_model::Status;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Organs,
    Skeleton,
    XRay,
}

struct Prepared {
    part: anatomy::Part,
    tris: Vec<[u32; 3]>,
    center: P,
    half: (f32, f32),
}

pub struct Figure {
    parts: Vec<Prepared>,
    callouts: Vec<anatomy::Callout>,
}

fn tint(c: [u8; 3], k: f32, a: f32) -> Color32 {
    let f = |v: u8| ((v as f32 * k).clamp(0.0, 255.0)) as u8;
    Color32::from_rgba_unmultiplied(f(c[0]), f(c[1]), f(c[2]), (a.clamp(0.0, 1.0) * 255.0) as u8)
}

impl Figure {
    pub fn new() -> Self {
        let parts = anatomy::parts()
            .into_iter()
            .map(|part| {
                let (tris, center, half) = match &part.geo {
                    Geo::Fill(pts) => {
                        let (lo, hi) = anatomy::bbox(pts);
                        (anatomy::triangulate(pts), P { x: (lo.x + hi.x) / 2.0, y: (lo.y + hi.y) / 2.0 }, (((hi.x - lo.x) / 2.0).max(2.0), ((hi.y - lo.y) / 2.0).max(2.0)))
                    }
                    Geo::Line(pts, _) => {
                        let (lo, hi) = anatomy::bbox(pts);
                        (vec![], P { x: (lo.x + hi.x) / 2.0, y: (lo.y + hi.y) / 2.0 }, (2.0, 2.0))
                    }
                };
                Prepared { part, tris, center, half }
            })
            .collect();
        Figure { parts, callouts: anatomy::callouts() }
    }

    fn layer_alpha(layer: Layer, mode: Mode) -> f32 {
        match (layer, mode) {
            (Layer::Skin, _) => 1.0,
            (Layer::Skeleton, Mode::Organs) => 0.20,
            (Layer::Skeleton, Mode::Skeleton) => 1.0,
            (Layer::Skeleton, Mode::XRay) => 0.62,
            (Layer::Organ, Mode::Organs) => 1.0,
            (Layer::Organ, Mode::Skeleton) => 0.10,
            (Layer::Organ, Mode::XRay) => 0.82,
        }
    }

    /// Draws the figure and returns the clicked body system, if any.
    #[allow(clippy::too_many_arguments)]
    pub fn show(&self, ui: &mut egui::Ui, size: Vec2, status_of: &dyn Fn(&str) -> Status, selected: &str, mode: Mode, time: f64, animate: bool) -> Option<String> {
        let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, egui::CornerRadius::same(10), SURFACE);
        painter.rect_stroke(rect, egui::CornerRadius::same(10), stroke(1.0, LINE), egui::StrokeKind::Inside);

        let s = ((rect.width() - 150.0) / anatomy::BODY_W).min((rect.height() - 30.0) / anatomy::H).max(0.2);
        let origin = pos2(rect.center().x - 120.0 * s, rect.top() + 14.0);
        let to_screen = move |q: P| origin + vec2(q.x, q.y) * s;
        let to_design = move |q: Pos2| P { x: (q.x - origin.x) / s, y: (q.y - origin.y) / s };

        let beat = if animate { (time * std::f64::consts::TAU / 1.1).sin().max(0.0).powi(6) as f32 } else { 0.0 };
        let breathe = if animate { (0.5 + 0.5 * (time * std::f64::consts::TAU / 4.2).sin()) as f32 } else { 0.0 };
        let alert = if animate { (0.5 + 0.5 * (time * std::f64::consts::TAU / 1.4).sin()) as f32 } else { 0.5 };

        // --- hover (parts, then labels)
        let hover = resp.hover_pos();
        let mut hovered: Option<&'static str> = None;
        if let Some(hp) = hover {
            let q = to_design(hp);
            for pr in self.parts.iter().rev() {
                if !pr.part.hit {
                    continue;
                }
                let vis = Self::layer_alpha(pr.part.layer, mode) >= 0.3 || pr.part.sys == selected;
                if !vis {
                    continue;
                }
                let inside = match &pr.part.geo {
                    Geo::Fill(pts) => anatomy::contains(pts, q),
                    Geo::Line(pts, w) => anatomy::dist_to_polyline(pts, q) <= w / 2.0 + 2.5,
                };
                if inside {
                    hovered = Some(pr.part.sys);
                    break;
                }
            }
        }
        let label_w = 70.0;
        let mut label_rects: Vec<(Rect, &'static str)> = vec![];
        for c in &self.callouts {
            let y = to_screen(P { x: 0.0, y: c.label_y }).y;
            let r = match c.side {
                Side::Left => Rect::from_min_size(pos2(rect.left() + 6.0, y - 14.0), vec2(label_w, 28.0)),
                Side::Right => Rect::from_min_size(pos2(rect.right() - 6.0 - label_w, y - 14.0), vec2(label_w, 28.0)),
            };
            label_rects.push((r, c.sys));
            if hover.map_or(false, |h| r.contains(h)) {
                hovered = Some(c.sys);
            }
        }
        if hovered.is_some() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }

        // --- parts, back to front
        for pr in &self.parts {
            let part = &pr.part;
            let st = status_of(part.sys);
            let sel = part.sys == selected;
            let hov = hovered == Some(part.sys);
            let mut a = Self::layer_alpha(part.layer, mode);
            if (sel || hov) && part.layer != Layer::Skin {
                a = a.max(if sel { 0.92 } else { 0.7 });
            }
            if a < 0.04 {
                continue;
            }
            let k = match part.pulse {
                Pulse::Heart => 1.0 + 0.075 * beat,
                Pulse::Lung => 1.0 + 0.035 * breathe,
                Pulse::None => 1.0,
            };
            let xf = |q: P| -> Pos2 {
                let q = if k != 1.0 { P { x: pr.center.x + (q.x - pr.center.x) * k, y: pr.center.y + (q.y - pr.center.y) * k } } else { q };
                to_screen(q)
            };
            // outline colour tells the status; healthy tissue keeps a quiet, darker edge of its own colour
            let status_edge = match st {
                Status::Watch => Some(WARN),
                Status::Act => Some(CRIT),
                _ => None,
            };
            // only organs carry the status colour on their outline; bones stay neutral (the marker shows status)
            let status_edge = if part.layer == Layer::Organ { status_edge } else { None };
            let edge_col = if sel {
                alpha(Color32::WHITE, 0.95)
            } else if let (Some(c), true) = (status_edge, part.layer != Layer::Skin) {
                alpha(c, 0.95)
            } else if part.layer == Layer::Skin {
                Color32::from_rgb(0x3d, 0x50, 0x68)
            } else {
                tint(part.tissue, 0.55, a)
            };
            let mut edge_w = if sel { 1.8 } else if status_edge.is_some() && part.layer != Layer::Skin { 1.3 } else if part.layer == Layer::Skin { 1.3 } else { 0.8 };
            if st == Status::Act && part.layer != Layer::Skin {
                edge_w += 1.2 * alert;
            }
            if hov && !sel {
                edge_w += 0.8;
            }
            match &part.geo {
                Geo::Fill(pts) => {
                    let sp: Vec<Pos2> = pts.iter().map(|&q| xf(q)).collect();
                    if part.layer == Layer::Skin {
                        // skin: vertical gradient, slightly lighter at the chest
                        let cols: Vec<Color32> = pts.iter().map(|q| { let t = (q.y / anatomy::H).clamp(0.0, 1.0); tint(part.tissue, 1.25 - 0.35 * t, 1.0) }).collect();
                        painter.add(mesh(&sp, &pr.tris, &cols));
                    } else {
                        let cols: Vec<Color32> = pts
                            .iter()
                            .map(|q| {
                                let (dx, dy) = ((q.x - pr.center.x) / pr.half.0, (q.y - pr.center.y) / pr.half.1);
                                let shade = 1.12 - 0.42 * (dx * dx + dy * dy).min(1.0);
                                tint(part.tissue, shade, a)
                            })
                            .collect();
                        painter.add(mesh(&sp, &pr.tris, &cols));
                    }
                    painter.add(Shape::closed_line(sp, stroke(edge_w, edge_col)));
                }
                Geo::Line(pts, w) => {
                    let sp: Vec<Pos2> = pts.iter().map(|&q| xf(q)).collect();
                    let wpx = (w * s).max(1.0);
                    let col = if sel { tint(part.tissue, 1.25, a) } else { tint(part.tissue, 1.0, a) };
                    if wpx > 2.6 {
                        // soft edge under the body of the bone / vessel
                        painter.add(Shape::line(sp.clone(), stroke(wpx + 1.6, if sel { alpha(Color32::WHITE, 0.9 * a) } else if let Some(c) = status_edge { alpha(c, 0.85 * a) } else { tint(part.tissue, 0.5, a) })));
                    }
                    painter.add(Shape::line(sp.clone(), stroke(wpx, col)));
                    if wpx > 2.6 {
                        for e in [sp[0], *sp.last().unwrap()] {
                            painter.circle_filled(e, wpx / 2.0, col);
                        }
                    }
                }
            }
        }

        // --- callouts: leader line, status marker, label
        for (c, (lr, _)) in self.callouts.iter().zip(&label_rects) {
            let st = status_of(c.sys);
            let col = status_color(st);
            let sel = c.sys == selected;
            let hov = hovered == Some(c.sys);
            let anchor = to_screen(c.anchor);
            let ly = lr.center().y;
            let edge_x = match c.side {
                Side::Left => lr.right() + 2.0,
                Side::Right => lr.left() - 2.0,
            };
            let elbow_x = match c.side {
                Side::Left => anchor.x - 14.0 * s,
                Side::Right => anchor.x + 14.0 * s,
            };
            let lc = if sel || hov { alpha(TXT, 0.9) } else { alpha(MUT, 0.55) };
            painter.add(Shape::line(vec![anchor, pos2(elbow_x, ly), pos2(edge_x, ly)], stroke(1.0, lc)));
            painter.circle_filled(anchor, 3.4, col);
            painter.circle_stroke(anchor, 5.4, stroke(1.0, alpha(col, 0.5)));
            let (pos, al) = match c.side {
                Side::Left => (pos2(lr.left() + 2.0, ly), Align2::LEFT_CENTER),
                Side::Right => (pos2(lr.right() - 2.0, ly), Align2::RIGHT_CENTER),
            };
            painter.text(pos + vec2(0.0, -6.0), al, c.label.to_uppercase(), FontId::proportional(10.5), if sel { TXT } else { Color32::from_rgb(0xc4, 0xcd, 0xd9) });
            painter.text(pos + vec2(0.0, 7.0), al, st.name().to_uppercase(), FontId::proportional(8.5), col);
        }

        if resp.clicked() {
            hovered.map(|h| h.to_string())
        } else {
            None
        }
    }
}

fn mesh(pts: &[Pos2], tris: &[[u32; 3]], colors: &[Color32]) -> Shape {
    let mut m = Mesh::default();
    m.vertices = pts.iter().zip(colors).map(|(p, c)| Vertex { pos: *p, uv: egui::epaint::WHITE_UV, color: *c }).collect();
    for t in tris {
        m.indices.extend_from_slice(&[t[0], t[1], t[2]]);
    }
    Shape::Mesh(Arc::new(m))
}
