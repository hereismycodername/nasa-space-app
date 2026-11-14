//! Tracker-style building blocks: rings, tiles with sparklines, charts with a hover read-out.

use crate::theme::*;
use eframe::egui::{self, pos2, vec2, Align2, Color32, CornerRadius, FontId, Frame, Margin, Painter, Pos2, Rect, RichText, Sense, Shape, StrokeKind, Ui, Vec2};
use ui_model::{fmt_num, IndicatorInfo, Reading, Status, Thresholds};

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

pub fn caption(ui: &mut Ui, t: &str) {
    ui.label(RichText::new(t.to_uppercase()).size(10.5).color(MUT).strong());
}

pub fn pill(ui: &mut Ui, text: &str, col: Color32) {
    Frame::new()
        .fill(alpha(col, 0.14))
        .stroke(stroke(1.0, alpha(col, 0.6)))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin::symmetric(9, 2))
        .show(ui, |ui| {
            ui.label(RichText::new(text.to_uppercase()).color(col).size(10.5).strong());
        });
}

pub fn led(ui: &mut Ui, col: Color32) {
    let (r, _) = ui.allocate_exact_size(vec2(10.0, 10.0), Sense::hover());
    ui.painter().circle_filled(r.center(), 4.0, col);
    ui.painter().circle_stroke(r.center(), 5.0, stroke(1.0, alpha(col, 0.35)));
}

pub fn card() -> Frame {
    Frame::new().fill(SURFACE).stroke(stroke(1.0, LINE)).corner_radius(CornerRadius::same(10)).inner_margin(Margin::same(14))
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
    egui::Button::new(RichText::new(text.to_string()).color(Color32::WHITE).strong()).fill(ACCENT).stroke(egui::Stroke::NONE)
}

pub fn rtt_text(s: f64) -> String {
    if s < 60.0 {
        format!("{} s", fmt_num(s))
    } else {
        format!("{} min", (s / 60.0).round())
    }
}

/// Segmented control. Returns true when the selection changed.
pub fn segmented<T: PartialEq + Copy>(ui: &mut Ui, cur: &mut T, items: &[(T, &str)]) -> bool {
    let mut changed = false;
    Frame::new().fill(DEEP).stroke(stroke(1.0, LINE)).corner_radius(CornerRadius::same(7)).inner_margin(Margin::same(2)).show(ui, |ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        ui.horizontal(|ui| {
            for (v, label) in items {
                let sel = *cur == *v;
                let w = ui.fonts(|f| f.layout_no_wrap(label.to_string(), FontId::proportional(11.5), TXT).size().x) + 22.0;
                let (rect, resp) = ui.allocate_exact_size(vec2(w, 22.0), Sense::click());
                if sel {
                    ui.painter().rect_filled(rect, CornerRadius::same(5), SURFACE2);
                    ui.painter().rect_stroke(rect, CornerRadius::same(5), stroke(1.0, LINE), StrokeKind::Inside);
                }
                ui.painter().text(rect.center(), Align2::CENTER_CENTER, *label, FontId::proportional(11.5), if sel || resp.hovered() { TXT } else { MUT });
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
    let w = ui.fonts(|f| f.layout_no_wrap(text.to_string(), FontId::proportional(13.5), TXT).size().x) + 30.0;
    let (rect, resp) = ui.allocate_exact_size(vec2(w, 32.0), Sense::click());
    let col = if sel || resp.hovered() { TXT } else { MUT };
    ui.painter().text(rect.center(), Align2::CENTER_CENTER, text, FontId::proportional(13.5), col);
    if sel {
        ui.painter().rect_filled(Rect::from_min_size(pos2(rect.left() + 10.0, rect.bottom() - 2.0), vec2(w - 20.0, 2.0)), CornerRadius::ZERO, ACCENT);
    }
    resp.clicked()
}

/// Progress ring (Apple-Activity / Oura style).
pub fn ring(p: &Painter, c: Pos2, r: f32, w: f32, frac: f32, col: Color32) {
    p.circle_stroke(c, r, stroke(w, alpha(col, 0.16)));
    let f = frac.clamp(0.0, 1.0);
    if f <= 0.0 {
        return;
    }
    let n = ((90.0 * f) as usize).max(3);
    let pts: Vec<Pos2> = (0..=n)
        .map(|i| {
            let a = -std::f32::consts::FRAC_PI_2 + (i as f32 / n as f32) * f * std::f32::consts::TAU;
            pos2(c.x + r * a.cos(), c.y + r * a.sin())
        })
        .collect();
    p.circle_filled(pts[0], w / 2.0, col);
    p.circle_filled(*pts.last().unwrap(), w / 2.0, col);
    p.add(Shape::line(pts, stroke(w, col)));
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
    p.rect_filled(Rect::from_min_max(pos2(rect.left(), ya), pos2(rect.right(), yb)), CornerRadius::same(2), alpha(OK, 0.10));
    let pts: Vec<Pos2> = last.iter().enumerate().map(|(i, v)| pos2(x(i), y(*v))).collect();
    p.add(Shape::line(pts.clone(), stroke(1.5, alpha(col, 0.9))));
    p.circle_filled(*pts.last().unwrap(), 2.8, col);
}

/// Metric tile: name, big value, delta vs personal baseline, sparkline. Click target.
#[allow(clippy::too_many_arguments)]
pub fn tile(ui: &mut Ui, size: Vec2, ind: &IndicatorInfo, hist: &[Reading], th: Thresholds, st: Status, due: bool, selected: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let p = ui.painter_at(rect);
    let col = status_color(st);
    p.rect_filled(rect, CornerRadius::same(10), if resp.hovered() { SURFACE2 } else { SURFACE });
    p.rect_stroke(rect, CornerRadius::same(10), stroke(1.0, if selected { ACCENT } else if resp.hovered() { alpha(ACCENT, 0.5) } else { LINE }), StrokeKind::Inside);
    p.text(rect.min + vec2(14.0, 16.0), Align2::LEFT_CENTER, &ind.name, FontId::proportional(11.5), MUT);
    if due {
        p.text(pos2(rect.right() - 12.0, rect.top() + 16.0), Align2::RIGHT_CENTER, "DUE", FontId::proportional(9.0), Color32::from_rgb(0xb8, 0xa4, 0xf0));
    }
    let latest = hist.last();
    let val = latest.map(|r| fmt_num(r.value)).unwrap_or("-".into());
    let vg = ui.fonts(|f| f.layout_no_wrap(val.clone(), FontId::proportional(26.0), TXT));
    let vy = rect.top() + 46.0;
    p.text(pos2(rect.left() + 14.0, vy), Align2::LEFT_CENTER, &val, FontId::proportional(26.0), TXT);
    p.text(pos2(rect.left() + 18.0 + vg.size().x, vy + 5.0), Align2::LEFT_CENTER, &ind.unit, FontId::proportional(11.0), MUT);
    // delta vs baseline
    let (dtxt, dcol) = match (latest, baseline(hist)) {
        (Some(l), Some(b)) => {
            let d = l.value - b;
            (format!("{}{} vs baseline", if d >= 0.0 { "+" } else { "" }, fmt_num((d * 100.0).round() / 100.0)), if st == Status::Normal { MUT } else { col })
        }
        _ => ("no data".into(), MUT),
    };
    p.text(rect.min + vec2(14.0, size.y - 16.0), Align2::LEFT_CENTER, dtxt, FontId::proportional(10.5), dcol);
    p.text(pos2(rect.right() - 12.0, rect.bottom() - 16.0), Align2::RIGHT_CENTER, st.name().to_uppercase(), FontId::proportional(9.0), col);
    // sparkline
    let sr = Rect::from_min_size(pos2(rect.right() - 108.0, rect.top() + 30.0), vec2(94.0, 34.0));
    spark(&p, sr, hist, th, col);
    p.rect_filled(Rect::from_min_size(rect.min + vec2(0.0, 14.0), vec2(3.0, size.y - 28.0)), CornerRadius::same(2), col);
    resp
}

/// History chart with normal / watch / act bands and a hover read-out.
#[allow(clippy::too_many_arguments)]
pub fn chart(ui: &mut Ui, hist: &[Reading], th: Thresholds, min: f64, max: f64, day: u32, height: f32, window: Option<u32>, unit: &str) {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
    let p = ui.painter_at(rect);
    p.rect_filled(rect, CornerRadius::same(8), DEEP);
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
            p.rect_filled(Rect::from_min_max(pos2(plot.left(), y(c)), pos2(plot.right(), y(a))), CornerRadius::ZERO, col);
        }
    };
    band(lo, th.warn.0, alpha(CRIT, 0.12));
    band(th.warn.1, hi, alpha(CRIT, 0.12));
    band(th.warn.0, th.ok.0, alpha(WARN, 0.13));
    band(th.ok.1, th.warn.1, alpha(WARN, 0.13));
    band(th.ok.0, th.ok.1, alpha(OK, 0.14));
    let small = FontId::proportional(9.5);
    let lab = |v: f64| if hi - lo > 20.0 { format!("{:.0}", v) } else { format!("{:.1}", v) };
    for (i, v) in [lo, (lo + hi) / 2.0, hi].into_iter().enumerate() {
        let yy = y(v);
        if i > 0 {
            p.line_segment([pos2(plot.left(), yy), pos2(plot.right(), yy)], stroke(1.0, alpha(LINE, 0.6)));
        }
        p.text(pos2(rect.left() + 33.0, yy), Align2::RIGHT_CENTER, lab(v), small.clone(), MUT);
    }
    p.text(pos2(plot.left(), rect.bottom() - 9.0), Align2::LEFT_CENTER, format!("day {}", d0), small.clone(), MUT);
    p.text(pos2(plot.right(), rect.bottom() - 9.0), Align2::RIGHT_CENTER, format!("day {}", day), small.clone(), MUT);
    let pts: Vec<Pos2> = vis.iter().map(|r| pos2(x(r.day), y(r.value))).collect();
    if pts.len() > 1 {
        p.add(Shape::line(pts.clone(), stroke(1.8, Color32::from_rgb(0xc9, 0xd6, 0xe6))));
    }
    for (i, r) in vis.iter().enumerate() {
        p.circle_filled(pts[i], if i + 1 == vis.len() { 4.5 } else { 3.0 }, status_color(status_of(&th, r.value)));
    }
    // hover read-out
    if let Some(hp) = resp.hover_pos() {
        if let Some((i, r)) = vis.iter().enumerate().min_by(|a, b| (pts[a.0].x - hp.x).abs().total_cmp(&(pts[b.0].x - hp.x).abs())) {
            let pt = pts[i];
            p.line_segment([pos2(pt.x, plot.top()), pos2(pt.x, plot.bottom())], stroke(1.0, alpha(TXT, 0.35)));
            p.circle_stroke(pt, 6.0, stroke(1.5, TXT));
            let txt = format!("day {}   {} {}", r.day, fmt_num(r.value), unit);
            let g = ui.fonts(|f| f.layout_no_wrap(txt.clone(), FontId::proportional(11.5), TXT));
            let w = g.size().x + 16.0;
            let tx = (pt.x - w / 2.0).clamp(rect.left() + 4.0, rect.right() - w - 4.0);
            let box_r = Rect::from_min_size(pos2(tx, (pt.y - 34.0).max(rect.top() + 2.0)), vec2(w, 22.0));
            p.rect_filled(box_r, CornerRadius::same(5), SURFACE2);
            p.rect_stroke(box_r, CornerRadius::same(5), stroke(1.0, LINE), StrokeKind::Inside);
            p.text(box_r.center(), Align2::CENTER_CENTER, txt, FontId::proportional(11.5), TXT);
        }
    }
}
