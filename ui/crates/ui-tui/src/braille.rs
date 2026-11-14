//! Real anatomy in a terminal: the vector figure from `ui_model::anatomy` rasterised into
//! braille dots (2 x 4 dots per character cell). Falls back to the ASCII sketch when the
//! console has no braille glyphs (Linux framebuffer console): set ASTROCHECK_ASCII=1.

use ratatui::style::Color;
use ui_model::anatomy::{self, Geo, Layer, P};
use ui_model::Status;

pub struct Cell {
    pub ch: char,
    pub color: Color,
    pub bold: bool,
}

fn bright(s: Status) -> Color {
    match s {
        Status::Normal => Color::White,
        Status::Watch => Color::LightYellow,
        Status::Act => Color::LightRed,
        Status::NoData => Color::LightCyan,
        Status::Unassessed => Color::White,
    }
}
fn normal(s: Status) -> Color {
    match s {
        Status::Normal => Color::Gray,
        Status::Watch => Color::Yellow,
        Status::Act => Color::Red,
        Status::NoData => Color::Cyan,
        Status::Unassessed => Color::Gray,
    }
}

/// Braille bit for the dot at (dx, dy) inside a cell.
fn bit(dx: usize, dy: usize) -> u32 {
    match (dx, dy) {
        (0, 0) => 0x01,
        (0, 1) => 0x02,
        (0, 2) => 0x04,
        (1, 0) => 0x08,
        (1, 1) => 0x10,
        (1, 2) => 0x20,
        (0, 3) => 0x40,
        _ => 0x80,
    }
}

pub fn use_braille() -> bool {
    std::env::var_os("ASTROCHECK_ASCII").is_none() && std::env::var("TERM").map_or(true, |t| t != "linux")
}

pub fn render(wc: usize, hc: usize, status_of: &dyn Fn(&str) -> Status, selected: &str) -> Vec<Vec<Cell>> {
    let (wd, hd) = (wc * 2, hc * 4);
    let s = (wd as f32 / (anatomy::BODY_W + 4.0)).min(hd as f32 / anatomy::H);
    let ox = (wd as f32 - anatomy::BODY_W * s) / 2.0 - anatomy::BODY_X0 * s;
    let parts = anatomy::parts();
    // owner[y][x][layer] = part index + 1 (0 = nothing)
    let mut owner = vec![vec![[0u16; 3]; wd]; hd];
    for (pi, part) in parts.iter().enumerate() {
        let li = match part.layer {
            Layer::Skin => 0,
            Layer::Skeleton => 1,
            Layer::Organ => 2,
        };
        let erase = part.tissue[0] < 0x20; // orbits, nasal cavity: holes in the skull
        let (poly, half): (&[P], f32) = match &part.geo {
            Geo::Fill(p) => (p, 0.0),
            Geo::Line(p, w) => (p, (w / 2.0).max(0.7 / s)),
        };
        let (lo, hi) = anatomy::bbox(poly);
        let x0 = (((lo.x - half) * s + ox).floor().max(0.0)) as usize;
        let x1 = (((hi.x + half) * s + ox).ceil().min(wd as f32 - 1.0).max(0.0)) as usize;
        let y0 = (((lo.y - half) * s).floor().max(0.0)) as usize;
        let y1 = (((hi.y + half) * s).ceil().min(hd as f32 - 1.0).max(0.0)) as usize;
        for y in y0..=y1.min(hd - 1) {
            for x in x0..=x1.min(wd - 1) {
                let q = P { x: (x as f32 + 0.5 - ox) / s, y: (y as f32 + 0.5) / s };
                let inside = match &part.geo {
                    Geo::Fill(p) => anatomy::contains(p, q),
                    Geo::Line(p, _) => anatomy::dist_to_polyline(p, q) <= half,
                };
                if inside {
                    if erase {
                        owner[y][x][1] = 0;
                    } else {
                        owner[y][x][li] = pi as u16 + 1;
                    }
                }
            }
        }
    }
    let skin = |x: isize, y: isize| -> bool { x >= 0 && y >= 0 && (x as usize) < wd && (y as usize) < hd && owner[y as usize][x as usize][0] != 0 };
    let mut out = Vec::with_capacity(hc);
    for cy in 0..hc {
        let mut row = Vec::with_capacity(wc);
        for cx in 0..wc {
            let (mut m0, mut m1, mut m2) = (0u32, 0u32, 0u32);
            let (mut top1, mut top2) = (0u16, 0u16);
            for dy in 0..4 {
                for dx in 0..2 {
                    let (x, y) = (cx * 2 + dx, cy * 4 + dy);
                    let o = owner[y][x];
                    if o[2] != 0 {
                        m2 |= bit(dx, dy);
                        top2 = top2.max(o[2]);
                    }
                    if o[1] != 0 {
                        m1 |= bit(dx, dy);
                        top1 = top1.max(o[1]);
                    }
                    if o[0] != 0 {
                        let (xi, yi) = (x as isize, y as isize);
                        if !(skin(xi - 1, yi) && skin(xi + 1, yi) && skin(xi, yi - 1) && skin(xi, yi + 1)) {
                            m0 |= bit(dx, dy);
                        }
                    }
                }
            }
            let cell = if m2 != 0 {
                let part = &parts[top2 as usize - 1];
                let st = status_of(part.sys);
                let sel = part.sys == selected;
                Cell { ch: char::from_u32(0x2800 + m2).unwrap_or(' '), color: if sel { bright(st) } else { normal(st) }, bold: sel }
            } else if m1 != 0 {
                let part = &parts[top1 as usize - 1];
                let sel = part.sys == selected;
                Cell { ch: char::from_u32(0x2800 + m1).unwrap_or(' '), color: if sel { Color::White } else { Color::Gray }, bold: sel }
            } else if m0 != 0 {
                Cell { ch: char::from_u32(0x2800 + m0).unwrap_or(' '), color: if selected == "immune" { Color::White } else { Color::DarkGray }, bold: selected == "immune" }
            } else {
                Cell { ch: ' ', color: Color::Reset, bold: false }
            };
            row.push(cell);
        }
        out.push(row);
    }
    out
}
