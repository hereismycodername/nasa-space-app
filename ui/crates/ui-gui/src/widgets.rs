//! Instrument-panel building blocks: chamfered panels, segmented gauges, tiles with sparklines,
//! charts with a hover read-out. Square corners and square markers throughout.

use crate::theme::*;
use eframe::egui::{self, pos2, vec2, Align2, Color32, CornerRadius, FontId, Frame, Margin, Painter, Pos2, Rect, RichText, Sense, Shape, StrokeKind, Ui, Vec2, WidgetText};
use ui_model::{fmt_num, IndicatorInfo, Reading, Status, Thresholds};

const SQ: CornerRadius = CornerRadius::ZERO;

pub fn status_of(th: &Thresholds, v: f64) -> Status {
    if v >= th.ok.0 && v <= th.ok.1 {
        Status::Normal
    } else if v >= th.warn.0 && v <= th.warn.1 {
        Status::Watch
    } else {
        Status::Act
    }
}

/// Personal baseline = mean of the first (up to) 7 readings, as the rules use for the crew member.
pub fn baseline(hist: &[Reading]) -> Option<f64> {
    if hist.is_empty() {
        return None;
    }
    let n = hist.len().min(7);
    Some(hist[..n].iter().map(|r| r.value).sum::<f64>() / n as f64)
}

fn galley(ui: &Ui, t: RichText) -> std::sync::Arc<egui::Galley> {
    WidgetText::from(t).into_galley(ui, Some(egui::TextWrapMode::Extend), f32::INFINITY, FontId::default())
}

/// Tracked mono text for technical labels (upper case).
pub fn tracked(t: &str, size: f32, col: Color32) -> RichText {
    tracked_raw(&t.to_uppercase(), size, col)
}

/// Tracked mono text, case kept (values with units: "RTT 1 s").
pub fn tracked_raw(t: &str, size: f32, col: Color32) -> RichText {
    RichText::new(t).font(mono(size)).extra_letter_spacing(size * 0.09).color(col)
}

/// Section caption: a small accent square and a tracked mono label.
pub fn caption(ui: &mut Ui, t: &str) {
    let g = galley(ui, tracked(t, 10.0, MUT));
    let (rect, _) = ui.allocate_exact_size(vec2(g.size().x + 11.0, g.size().y.max(12.0)), Sense::hover());
    ui.painter().rect_filled(Rect::from_min_size(pos2(rect.left(), rect.center().y - 2.0), vec2(4.0, 4.0)), SQ, alpha(ACCENT, 0.9));
    ui.painter().galley(pos2(rect.left() + 11.0, rect.center().y - g.size().y / 2.0), g, MUT);
}

/// Status tag: square box, solid block on the left, tracked mono upper-case text.
pub fn pill(ui: &mut Ui, text: &str, col: Color32) {
    tag(ui, &text.to_uppercase(), col)
}

/// [`pill`] with the text as given.
pub fn tag(ui: &mut Ui, text: &str, col: Color32) {
    let r = Frame::new()
        .fill(alpha(col, 0.12))
        .stroke(stroke(1.0, alpha(col, 0.55)))
        .corner_radius(SQ)
        .inner_margin(Margin { left: 10, right: 7, top: 2, bottom: 2 })
        .show(ui, |ui| {
            ui.label(tracked_raw(text, 10.0, col));
        });
    let rr = r.response.rect;
    ui.painter().rect_filled(Rect::from_min_size(rr.min + vec2(1.0, 1.0), vec2(3.0, rr.height() - 2.0)), SQ, col);
}

/// Square status lamp.
pub fn led(ui: &mut Ui, col: Color32) {
    let (r, _) = ui.allocate_exact_size(vec2(10.0, 10.0), Sense::hover());
    ui.painter().rect_filled(Rect::from_center_size(r.center(), vec2(7.0, 7.0)), SQ, col);
    ui.painter().rect_stroke(Rect::from_center_size(r.center(), vec2(10.0, 10.0)), SQ, stroke(1.0, alpha(col, 0.35)), StrokeKind::Inside);
}

/// Panel with one chamfered corner. Use like `egui::Frame`: `card().show(ui, |ui| ...)`.
#[derive(Clone, Copy)]
pub struct Card {
    fill: Color32,
    line: Color32,
    margin: Margin,
    cut: f32,
}

pub fn card() -> Card {
    Card { fill: SURFACE, line: LINE, margin: Margin::same(14), cut: CUT }
}

impl Card {
    pub fn fill(mut self, c: Color32) -> Self {
        self.fill = c;
        self
    }
    pub fn line(mut self, c: Color32) -> Self {
        self.line = c;
        self
    }
    pub fn margin(mut self, m: Margin) -> Self {
        self.margin = m;
        self
    }
    pub fn show<R>(self, ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> egui::InnerResponse<R> {
        let bg = ui.painter().add(Shape::Noop);
        let inner = Frame::new().inner_margin(self.margin).show(ui, add);
        ui.painter().set(bg, panel_shape(inner.response.rect, self.cut, self.fill, stroke(1.0, self.line)));
        inner
    }
}

/// Card of a fixed outer width (and optional minimum height) whose content is always laid out vertically,
/// even when the card itself sits in a horizontal row.
pub fn vcard(ui: &mut Ui, outer_w: f32, outer_h: f32, content: impl FnOnce(&mut Ui)) {
    card().show(ui, |ui| {
        ui.vertical(|ui| {
            ui.set_width(outer_w - 30.0);
            if outer_h > 0.0 {
                ui.set_min_height(outer_h - 30.0);
            }
            content(ui)
        });
    });
}

pub fn primary(text: &str) -> egui::Button<'static> {
    egui::Button::new(tracked(text, 11.0, Color32::WHITE)).fill(ACCENT).stroke(egui::Stroke::NONE)
}

pub fn rtt_text(s: f64) -> String {
    if s < 60.0 {
        format!("{} s", fmt_num(s))
    } else {
        format!("{} min", (s / 60.0).round())
    }
}

/// Segmented control: square cells, the selected one is lit and underlined. Returns true when the selection changed.
pub fn segmented<T: PartialEq + Copy>(ui: &mut Ui, cur: &mut T, items: &[(T, &str)]) -> bool {
    let mut changed = false;
    Frame::new().fill(DEEP).stroke(stroke(1.0, LINE)).corner_radius(SQ).inner_margin(Margin::same(1)).show(ui, |ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        ui.horizontal(|ui| {
            for (i, (v, label)) in items.iter().enumerate() {
                let sel = *cur == *v;
                let g = galley(ui, tracked(label, 10.0, TXT));
                let (rect, resp) = ui.allocate_exact_size(vec2(g.size().x + 20.0, 22.0), Sense::click());
                let p = ui.painter();
                if sel {
                    p.rect_filled(rect, SQ, SURFACE2);
                    p.rect_filled(Rect::from_min_size(pos2(rect.left(), rect.bottom() - 2.0), vec2(rect.width(), 2.0)), SQ, ACCENT);
                } else if resp.hovered() {
                    p.rect_filled(rect, SQ, alpha(SURFACE2, 0.6));
                }
                if i > 0 && !sel {
                    p.line_segment([pos2(rect.left(), rect.top() + 5.0), pos2(rect.left(), rect.bottom() - 5.0)], stroke(1.0, LINE));
                }
                let col = if sel || resp.hovered() { TXT } else { MUT };
                let g = galley(ui, tracked(label, 10.0, col));
                ui.painter().galley(rect.center() - g.size() / 2.0, g, col);
                if resp.clicked() && !sel {
                    *cur = *v;
                    changed = true;
                }
            }
        });
    });
    changed
}

/// Flat tab with an accent underline. Returns true when clicked.
pub fn tab_btn(ui: &mut Ui, text: &str, sel: bool) -> bool {
    let probe = galley(ui, RichText::new(text.to_uppercase()).font(display(12.0)).extra_letter_spacing(1.0));
    let w = probe.size().x + 28.0;
    let (rect, resp) = ui.allocate_exact_size(vec2(w, 32.0), Sense::click());
    let col = if sel || resp.hovered() { TXT } else { MUT };
    let g = galley(ui, RichText::new(text.to_uppercase()).font(display(12.0)).extra_letter_spacing(1.0).color(col));
    ui.painter().galley(rect.center() - g.size() / 2.0, g, col);
    if sel {
        ui.painter().rect_filled(Rect::from_min_size(pos2(rect.left() + 8.0, rect.bottom() - 2.0), vec2(w - 16.0, 2.0)), SQ, ACCENT);
    }
    resp.clicked()
}

/// Segmented gauge: `n` square cells, the first `frac * n` lit.
pub fn seg_bar(p: &Painter, rect: Rect, frac: f32, n: usize, col: Color32) {
    let gap = 2.0;
    let w = (rect.width() - gap * (n as f32 - 1.0)) / n as f32;
    let lit = (frac.clamp(0.0, 1.0) * n as f32).round() as usize;
    for i in 0..n {
        let r = Rect::from_min_size(pos2(rect.left() + i as f32 * (w + gap), rect.top()), vec2(w, rect.height()));
        p.rect_filled(r, SQ, if i < lit { col } else { alpha(OK, 0.20) });
    }
}

/// Scale under a gauge: ticks at 0, 25, 50, 75, 100.
pub fn scale_ticks(p: &Painter, rect: Rect) {
    for i in 0..=4 {
        let x = rect.left() + rect.width() * i as f32 / 4.0;
        p.line_segment([pos2(x, rect.top()), pos2(x, rect.top() + 4.0)], stroke(1.0, LINE_HI));
        let al = match i {
            0 => Align2::LEFT_TOP,
            4 => Align2::RIGHT_TOP,
            _ => Align2::CENTER_TOP,
        };
        p.text(pos2(x, rect.top() + 6.0), al, (i * 25).to_string(), mono(9.0), alpha(MUT, 0.8));
    }
}

pub fn spark(p: &Painter, rect: Rect, hist: &[Reading], th: Thresholds, col: Color32) {
    let last: Vec<f64> = hist.iter().rev().take(14).rev().map(|r| r.value).collect();
    if last.len() < 2 {
        return;
    }
    let lo = last.iter().cloned().fold(f64::MAX, f64::min).min(th.ok.0);
    let hi = last.iter().cloned().fold(f64::MIN, f64::max).max(th.ok.1);
    let pad = ((hi - lo) * 0.12).max(0.01);
    let (lo, hi) = (lo - pad, hi + pad);
    let y = |v: f64| rect.bottom() - (((v - lo) / (hi - lo)) as f32) * rect.height();
    let x = |i: usize| rect.left() + rect.width() * i as f32 / (last.len() - 1) as f32;
    let (ya, yb) = (y(th.ok.1.min(hi)), y(th.ok.0.max(lo)));
    p.rect_filled(Rect::from_min_max(pos2(rect.left(), ya), pos2(rect.right(), yb)), SQ, alpha(OK, 0.12));
    p.line_segment([pos2(rect.left(), ya), pos2(rect.right(), ya)], stroke(1.0, alpha(OK, 0.30)));
    p.line_segment([pos2(rect.left(), yb), pos2(rect.right(), yb)], stroke(1.0, alpha(OK, 0.30)));
    let pts: Vec<Pos2> = last.iter().enumerate().map(|(i, v)| pos2(x(i), y(*v))).collect();
    p.add(Shape::line(pts.clone(), stroke(1.3, alpha(col, 0.9))));
    p.rect_filled(Rect::from_center_size(*pts.last().unwrap(), vec2(5.0, 5.0)), SQ, col);
}

/// Metric tile: name, big value, delta vs personal baseline, sparkline. Click target.
#[allow(clippy::too_many_arguments)]
pub fn tile(ui: &mut Ui, size: Vec2, ind: &IndicatorInfo, hist: &[Reading], th: Thresholds, st: Status, due: bool, selected: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let p = ui.painter_at(rect.expand(1.0));
    let col = status_color(st);
    let border = if selected { ACCENT } else if resp.hovered() { alpha(ACCENT, 0.6) } else { LINE };
    p.add(panel_shape(rect, CUT, if resp.hovered() { SURFACE2 } else { SURFACE }, stroke(1.0, border)));
    p.text(rect.min + vec2(14.0, 16.0), Align2::LEFT_CENTER, &ind.name, FontId::proportional(11.5), MUT);
    if due {
        let g = galley(ui, tracked("due", 8.5, Color32::from_rgb(0xb8, 0xa4, 0xf0)));
        let r = Rect::from_min_size(pos2(rect.right() - 16.0 - g.size().x - 8.0, rect.top() + 9.0), vec2(g.size().x + 8.0, 14.0));
        p.rect_stroke(r, SQ, stroke(1.0, alpha(Color32::from_rgb(0xb8, 0xa4, 0xf0), 0.45)), StrokeKind::Inside);
        p.galley(r.center() - g.size() / 2.0, g, TXT);
    }
    let latest = hist.last();
    let val = latest.map(|r| fmt_num(r.value)).unwrap_or("-".into());
    let vg = ui.fonts(|f| f.layout_no_wrap(val.clone(), mono(26.0), TXT));
    let vy = rect.top() + 46.0;
    p.text(pos2(rect.left() + 14.0, vy), Align2::LEFT_CENTER, &val, mono(26.0), TXT);
    p.text(pos2(rect.left() + 18.0 + vg.size().x, vy + 5.0), Align2::LEFT_CENTER, &ind.unit, mono(10.5), MUT);
    // delta vs baseline
    let (dtxt, dcol) = match (latest, baseline(hist)) {
        (Some(l), Some(b)) => {
            let d = l.value - b;
            (format!("{}{} vs baseline", if d >= 0.0 { "+" } else { "" }, fmt_num((d * 100.0).round() / 100.0)), if st == Status::Normal { MUT } else { col })
        }
        _ => ("no data".into(), MUT),
    };
    p.text(rect.min + vec2(14.0, size.y - 16.0), Align2::LEFT_CENTER, dtxt, mono(10.0), dcol);
    if st != Status::Normal {
        let g = galley(ui, tracked(st.name(), 9.0, col));
        p.galley(pos2(rect.right() - 12.0 - g.size().x, rect.bottom() - 16.0 - g.size().y / 2.0), g, col);
    }
    // sparkline
    let sr = Rect::from_min_size(pos2(rect.right() - 108.0, rect.top() + 32.0), vec2(94.0, 32.0));
    spark(&p, sr, hist, th, col);
    if st != Status::Normal {
        // quiet display: the status bar appears only when something departs from normal
        p.rect_filled(Rect::from_min_size(rect.min + vec2(1.0, 1.0), vec2(3.0, size.y - 2.0)), SQ, col);
    }
    resp
}

/// History chart with normal / watch / act bands and a hover read-out.
#[allow(clippy::too_many_arguments)]
pub fn chart(ui: &mut Ui, hist: &[Reading], th: Thresholds, min: f64, max: f64, day: u32, height: f32, window: Option<u32>, unit: &str) {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
    let p = ui.painter_at(rect);
    p.rect_filled(rect, SQ, DEEP);
    let d0 = match window {
        Some(n) => day.saturating_sub(n - 1).max(1),
        None => hist.first().map(|r| r.day).unwrap_or(1),
    };
    let vis: Vec<Reading> = hist.iter().filter(|r| r.day >= d0).copied().collect();
    if vis.is_empty() {
        return;
    }
    let vals: Vec<f64> = vis.iter().map(|r| r.value).collect();
    let mut lo = vals.iter().cloned().fold(f64::MAX, f64::min).min(th.warn.0).min(th.ok.0);
    let mut hi = vals.iter().cloned().fold(f64::MIN, f64::max).max(th.warn.1).max(th.ok.1);
    let pad = ((hi - lo) * 0.12).max(0.5);
    lo = (lo - pad).max(min);
    hi = (hi + pad).min(max);
    let plot = Rect::from_min_max(pos2(rect.left() + 38.0, rect.top() + 10.0), pos2(rect.right() - 12.0, rect.bottom() - 20.0));
    let y = |v: f64| plot.bottom() - (((v - lo) / (hi - lo)) as f32) * plot.height();
    let span = day.saturating_sub(d0).max(1) as f32;
    let x = |d: u32| plot.left() + plot.width() * (d.saturating_sub(d0)) as f32 / span;
    let band = |a: f64, c: f64, col: Color32| {
        let (a, c) = (a.max(lo), c.min(hi));
        if c > a {
            p.rect_filled(Rect::from_min_max(pos2(plot.left(), y(c)), pos2(plot.right(), y(a))), SQ, col);
        }
    };
    band(lo, th.warn.0, alpha(CRIT, 0.16));
    band(th.warn.1, hi, alpha(CRIT, 0.16));
    band(th.warn.0, th.ok.0, alpha(WARN, 0.14));
    band(th.ok.1, th.warn.1, alpha(WARN, 0.14));
    band(th.ok.0, th.ok.1, alpha(OK, 0.12));
    let small = mono(9.5);
    let lab = |v: f64| if hi - lo > 20.0 { format!("{:.0}", v) } else { format!("{:.1}", v) };
    for (i, v) in [lo, (lo + hi) / 2.0, hi].into_iter().enumerate() {
        let yy = y(v);
        if i > 0 {
            p.line_segment([pos2(plot.left(), yy), pos2(plot.right(), yy)], stroke(1.0, alpha(LINE, 0.7)));
        }
        p.line_segment([pos2(plot.left() - 4.0, yy), pos2(plot.left(), yy)], stroke(1.0, LINE_HI));
        p.text(pos2(rect.left() + 31.0, yy), Align2::RIGHT_CENTER, lab(v), small.clone(), MUT);
    }
    // day ticks along the bottom
    let days = day.saturating_sub(d0);
    for d in 0..=days {
        let xx = x(d0 + d);
        p.line_segment([pos2(xx, plot.bottom()), pos2(xx, plot.bottom() + 3.0)], stroke(1.0, LINE_HI));
    }
    p.text(pos2(plot.left(), rect.bottom() - 8.0), Align2::LEFT_CENTER, format!("DAY {}", d0), small.clone(), MUT);
    p.text(pos2(plot.right(), rect.bottom() - 8.0), Align2::RIGHT_CENTER, format!("DAY {}", day), small.clone(), MUT);
    let pts: Vec<Pos2> = vis.iter().map(|r| pos2(x(r.day), y(r.value))).collect();
    if pts.len() > 1 {
        p.add(Shape::line(pts.clone(), stroke(1.5, Color32::from_rgb(0xc9, 0xd6, 0xe6))));
    }
    for (i, r) in vis.iter().enumerate() {
        let sz = if i + 1 == vis.len() { 7.0 } else { 4.5 };
        p.rect_filled(Rect::from_center_size(pts[i], vec2(sz, sz)), SQ, status_color(status_of(&th, r.value)));
    }
    // hover read-out
    if let Some(hp) = resp.hover_pos() {
        if let Some((i, r)) = vis.iter().enumerate().min_by(|a, b| (pts[a.0].x - hp.x).abs().total_cmp(&(pts[b.0].x - hp.x).abs())) {
            let pt = pts[i];
            p.line_segment([pos2(pt.x, plot.top()), pos2(pt.x, plot.bottom())], stroke(1.0, alpha(TXT, 0.35)));
            p.rect_stroke(Rect::from_center_size(pt, vec2(12.0, 12.0)), SQ, stroke(1.0, TXT), StrokeKind::Middle);
            let txt = format!("DAY {}   {} {}", r.day, fmt_num(r.value), unit);
            let g = ui.fonts(|f| f.layout_no_wrap(txt.clone(), mono(11.0), TXT));
            let w = g.size().x + 16.0;
            let tx = (pt.x - w / 2.0).clamp(rect.left() + 4.0, rect.right() - w - 4.0);
            let box_r = Rect::from_min_size(pos2(tx, (pt.y - 34.0).max(rect.top() + 2.0)), vec2(w, 22.0));
            p.rect_filled(box_r, SQ, SURFACE2);
            p.rect_stroke(box_r, SQ, stroke(1.0, LINE_HI), StrokeKind::Inside);
            p.text(box_r.center(), Align2::CENTER_CENTER, txt, mono(11.0), TXT);
        }
    }
}
