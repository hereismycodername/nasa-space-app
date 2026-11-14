//! Human anatomy (frontal view, anatomical position) as plain vector data.
//!
//! Design space is 240 x 500 units, body axis at x = 120. Everything is authored as smooth
//! splines (skin, skeleton, organs) so any front end can draw it: the window UI fills it with
//! shaded meshes, the terminal UI rasterises it into braille dots.
//!
//! This is a schematic medical illustration, not a licensed atlas. To use official artwork,
//! replace `parts()` - nothing else depends on how the shapes were made.

use std::f32::consts::TAU;

pub const W: f32 = 240.0;
pub const H: f32 = 500.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct P {
    pub x: f32,
    pub y: f32,
}
pub const fn p(x: f32, y: f32) -> P {
    P { x, y }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Layer {
    Skin,
    Skeleton,
    Organ,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Pulse {
    None,
    Heart,
    Lung,
}

#[derive(Clone, Debug)]
pub enum Geo {
    /// closed polygon, filled
    Fill(Vec<P>),
    /// open polyline with a stroke width (design units)
    Line(Vec<P>, f32),
}

#[derive(Clone, Debug)]
pub struct Part {
    /// body system this part belongs to: brain, eyes, heart, lungs, gut, msk, immune
    pub sys: &'static str,
    pub name: &'static str,
    pub layer: Layer,
    /// natural tissue colour (r, g, b)
    pub tissue: [u8; 3],
    pub geo: Geo,
    pub pulse: Pulse,
    /// can be clicked
    pub hit: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Side {
    Left,
    Right,
}

/// Label with a leader line pointing at the organ.
#[derive(Clone, Copy, Debug)]
pub struct Callout {
    pub sys: &'static str,
    pub label: &'static str,
    pub anchor: P,
    pub side: Side,
    pub label_y: f32,
}

pub fn callouts() -> Vec<Callout> {
    vec![
        Callout { sys: "brain", label: "Brain", anchor: p(136.0, 24.0), side: Side::Right, label_y: 24.0 },
        Callout { sys: "eyes", label: "Eyes", anchor: p(106.0, 43.0), side: Side::Left, label_y: 43.0 },
        Callout { sys: "lungs", label: "Lungs", anchor: p(96.0, 140.0), side: Side::Left, label_y: 128.0 },
        Callout { sys: "heart", label: "Heart", anchor: p(136.0, 158.0), side: Side::Right, label_y: 150.0 },
        Callout { sys: "gut", label: "Metabolism", anchor: p(98.0, 208.0), side: Side::Left, label_y: 214.0 },
        Callout { sys: "msk", label: "Skeletal", anchor: p(153.0, 330.0), side: Side::Right, label_y: 318.0 },
        Callout { sys: "immune", label: "Immune", anchor: p(81.0, 300.0), side: Side::Left, label_y: 304.0 },
    ]
}

// ------------------------------------------------------------------------------ geometry helpers

fn catmull(p0: P, p1: P, p2: P, p3: P, t: f32) -> P {
    let (t2, t3) = (t * t, t * t * t);
    let f = |a: f32, b: f32, c: f32, d: f32| 0.5 * (2.0 * b + (-a + c) * t + (2.0 * a - 5.0 * b + 4.0 * c - d) * t2 + (-a + 3.0 * b - 3.0 * c + d) * t3);
    p(f(p0.x, p1.x, p2.x, p3.x), f(p0.y, p1.y, p2.y, p3.y))
}

/// Smooth curve through the control points (Catmull-Rom).
pub fn spline(pts: &[P], closed: bool, steps: usize) -> Vec<P> {
    let n = pts.len();
    if n < 3 {
        return pts.to_vec();
    }
    let get = |i: isize| -> P {
        if closed {
            pts[((i % n as isize) + n as isize) as usize % n]
        } else {
            pts[i.clamp(0, n as isize - 1) as usize]
        }
    };
    let segs = if closed { n } else { n - 1 };
    let mut out = Vec::with_capacity(segs * steps + 1);
    for i in 0..segs as isize {
        for s in 0..steps {
            out.push(catmull(get(i - 1), get(i), get(i + 1), get(i + 2), s as f32 / steps as f32));
        }
    }
    if !closed {
        out.push(pts[n - 1]);
    }
    out
}

fn pts(v: &[(f32, f32)]) -> Vec<P> {
    v.iter().map(|&(x, y)| p(x, y)).collect()
}
fn mir(v: &[P]) -> Vec<P> {
    v.iter().map(|q| p(W - q.x, q.y)).collect()
}
/// Closed outline from the right half of a symmetric shape given top -> bottom on the axis.
fn sym(half: &[(f32, f32)]) -> Vec<P> {
    let r = pts(half);
    let mut left = mir(&r);
    left.reverse();
    let mut all = r.clone();
    all.extend(left[1..left.len() - 1].iter().copied());
    all
}
fn ell(cx: f32, cy: f32, rx: f32, ry: f32) -> Vec<P> {
    (0..28).map(|i| { let a = i as f32 / 28.0 * TAU; p(cx + rx * a.cos(), cy + ry * a.sin()) }).collect()
}
fn rect(cx: f32, cy: f32, w: f32, h: f32) -> Vec<P> {
    vec![p(cx - w / 2.0, cy - h / 2.0), p(cx + w / 2.0, cy - h / 2.0), p(cx + w / 2.0, cy + h / 2.0), p(cx - w / 2.0, cy + h / 2.0)]
}

pub fn bbox(poly: &[P]) -> (P, P) {
    let mut lo = p(f32::MAX, f32::MAX);
    let mut hi = p(f32::MIN, f32::MIN);
    for q in poly {
        lo = p(lo.x.min(q.x), lo.y.min(q.y));
        hi = p(hi.x.max(q.x), hi.y.max(q.y));
    }
    (lo, hi)
}

pub fn area(poly: &[P]) -> f32 {
    let n = poly.len();
    (0..n).map(|i| { let (a, b) = (poly[i], poly[(i + 1) % n]); a.x * b.y - b.x * a.y }).sum::<f32>() * 0.5
}

/// Point inside polygon (ray casting).
pub fn contains(poly: &[P], q: P) -> bool {
    let mut inside = false;
    let n = poly.len();
    let mut j = n - 1;
    for i in 0..n {
        let (a, b) = (poly[i], poly[j]);
        if (a.y > q.y) != (b.y > q.y) && q.x < (b.x - a.x) * (q.y - a.y) / (b.y - a.y) + a.x {
            inside = !inside;
        }
        j = i;
    }
    inside
}

pub fn dist_to_polyline(line: &[P], q: P) -> f32 {
    let mut best = f32::MAX;
    for w in line.windows(2) {
        let (a, b) = (w[0], w[1]);
        let (abx, aby) = (b.x - a.x, b.y - a.y);
        let l2 = (abx * abx + aby * aby).max(1e-6);
        let t = (((q.x - a.x) * abx + (q.y - a.y) * aby) / l2).clamp(0.0, 1.0);
        let (dx, dy) = (a.x + abx * t - q.x, a.y + aby * t - q.y);
        best = best.min((dx * dx + dy * dy).sqrt());
    }
    best
}

fn cross(a: P, b: P, c: P) -> f32 {
    (b.x - a.x) * (c.y - b.y) - (b.y - a.y) * (c.x - b.x)
}
fn in_tri(q: P, a: P, b: P, c: P) -> bool {
    let d1 = cross(a, b, q);
    let d2 = cross(b, c, q);
    let d3 = cross(c, a, q);
    let neg = d1 < -1e-5 || d2 < -1e-5 || d3 < -1e-5;
    let pos = d1 > 1e-5 || d2 > 1e-5 || d3 > 1e-5;
    !(neg && pos)
}

/// Ear-clipping triangulation: lets a front end fill concave outlines exactly.
pub fn triangulate(poly: &[P]) -> Vec<[u32; 3]> {
    let n = poly.len();
    if n < 3 {
        return vec![];
    }
    let mut idx: Vec<usize> = (0..n).collect();
    if area(poly) < 0.0 {
        idx.reverse();
    }
    let mut out = Vec::with_capacity(n - 2);
    while idx.len() > 3 {
        let m = idx.len();
        let mut clipped = false;
        for i in 0..m {
            let (a, b, c) = (idx[(i + m - 1) % m], idx[i], idx[(i + 1) % m]);
            let (pa, pb, pc) = (poly[a], poly[b], poly[c]);
            if cross(pa, pb, pc) <= 1e-6 {
                continue;
            }
            if idx.iter().any(|&j| j != a && j != b && j != c && in_tri(poly[j], pa, pb, pc)) {
                continue;
            }
            out.push([a as u32, b as u32, c as u32]);
            idx.remove(i);
            clipped = true;
            break;
        }
        if !clipped {
            // degenerate input: clip the first vertex anyway so we always terminate
            let m = idx.len();
            out.push([idx[m - 1] as u32, idx[0] as u32, idx[1] as u32]);
            idx.remove(0);
        }
    }
    if idx.len() == 3 {
        out.push([idx[0] as u32, idx[1] as u32, idx[2] as u32]);
    }
    out
}

// ------------------------------------------------------------------------------ the figure

const BONE: [u8; 3] = [0xd9, 0xd4, 0xc6];
const BONE_DK: [u8; 3] = [0xbd, 0xb7, 0xa6];
const HOLE: [u8; 3] = [0x0e, 0x14, 0x1c];

/// All parts in drawing order (back to front).
pub fn parts() -> Vec<Part> {
    let mut v: Vec<Part> = Vec::new();
    let mut add = |sys: &'static str, name: &'static str, layer: Layer, tissue: [u8; 3], geo: Geo, pulse: Pulse, hit: bool| {
        v.push(Part { sys, name, layer, tissue, geo, pulse, hit });
    };
    let fill = |g: Vec<P>| Geo::Fill(g);
    let line = |g: Vec<P>, w: f32| Geo::Line(g, w);
    let none = Pulse::None;

    // ---- skin: right half of the silhouette from crown to crotch (top -> bottom)
    let skin = sym(&[
        (120.0, 8.0), (131.0, 9.5), (140.0, 17.0), (144.0, 30.0), (143.0, 45.0), (139.0, 58.0), (132.0, 68.0), (125.0, 75.0),
        (124.0, 82.0), (124.0, 90.0), (134.0, 92.0), (150.0, 96.0), (165.0, 100.0), (176.0, 108.0), (181.0, 122.0),
        (182.0, 148.0), (185.0, 172.0), (188.0, 194.0), (190.0, 216.0), (193.0, 238.0), (196.0, 254.0), (201.0, 268.0),
        (205.0, 282.0), (203.0, 293.0), (196.0, 292.0), (192.0, 278.0), (188.0, 262.0), (184.0, 252.0), (179.0, 232.0),
        (174.0, 210.0), (170.0, 192.0), (166.0, 168.0), (162.0, 142.0), (158.0, 120.0), (153.0, 128.0), (150.0, 150.0),
        (148.0, 178.0), (146.0, 204.0), (145.0, 228.0), (153.0, 248.0), (160.0, 268.0), (162.0, 292.0), (160.0, 325.0),
        (156.0, 360.0), (154.0, 376.0), (156.0, 398.0), (158.0, 420.0), (155.0, 448.0), (150.0, 474.0), (153.0, 488.0),
        (152.0, 497.0), (141.0, 499.0), (131.0, 495.0), (130.0, 480.0), (129.0, 455.0), (127.0, 425.0), (125.0, 398.0),
        (124.0, 376.0), (124.0, 350.0), (124.0, 318.0), (122.0, 290.0), (120.0, 268.0),
    ]);
    add("immune", "Skin", Layer::Skin, [0x22, 0x2e, 0x3d], fill(spline(&skin, true, 5)), none, true);

    // ---- skeleton
    let skull = sym(&[(120.0, 11.0), (131.0, 12.5), (139.0, 20.0), (142.0, 32.0), (141.0, 46.0), (138.0, 56.0), (133.0, 65.0), (126.0, 72.0), (120.0, 75.0)]);
    add("msk", "Skull", Layer::Skeleton, BONE, fill(spline(&skull, true, 5)), none, true);
    add("msk", "Orbit", Layer::Skeleton, HOLE, fill(ell(108.0, 43.0, 6.8, 6.0)), none, false);
    add("msk", "Orbit", Layer::Skeleton, HOLE, fill(ell(132.0, 43.0, 6.8, 6.0)), none, false);
    add("msk", "Nasal cavity", Layer::Skeleton, HOLE, fill(pts(&[(120.0, 47.0), (116.0, 58.0), (124.0, 58.0)])), none, false);
    add("msk", "Jaw", Layer::Skeleton, BONE_DK, line(spline(&pts(&[(110.0, 63.0), (115.0, 69.0), (120.0, 71.0), (125.0, 69.0), (130.0, 63.0)]), false, 4), 1.6), none, false);
    // spine: cervical, thoracic, lumbar vertebrae, sacrum
    for i in 0..7 {
        add("msk", "Cervical vertebra", Layer::Skeleton, if i % 2 == 0 { BONE } else { BONE_DK }, fill(rect(120.0, 79.0 + i as f32 * 2.7, 9.0 + i as f32 * 0.4, 2.2)), none, true);
    }
    for i in 0..12 {
        add("msk", "Thoracic vertebra", Layer::Skeleton, if i % 2 == 0 { BONE } else { BONE_DK }, fill(rect(120.0, 99.0 + i as f32 * 5.7, 12.5 + i as f32 * 0.35, 4.2)), none, true);
    }
    for i in 0..5 {
        add("msk", "Lumbar vertebra", Layer::Skeleton, if i % 2 == 0 { BONE } else { BONE_DK }, fill(rect(120.0, 170.0 + i as f32 * 11.0, 18.0, 8.2)), none, true);
    }
    add("msk", "Sacrum", Layer::Skeleton, BONE, fill(pts(&[(111.0, 225.0), (129.0, 225.0), (125.0, 247.0), (120.0, 255.0), (115.0, 247.0)])), none, true);
    add("msk", "Sternum", Layer::Skeleton, BONE, fill(spline(&pts(&[(115.5, 100.0), (124.5, 100.0), (125.5, 118.0), (124.0, 148.0), (120.0, 158.0), (116.0, 148.0), (114.5, 118.0)]), true, 4)), none, true);
    // clavicles
    let clav = spline(&pts(&[(117.0, 98.0), (130.0, 94.5), (146.0, 96.0), (163.0, 101.5)]), false, 8);
    add("msk", "Clavicle", Layer::Skeleton, BONE, line(clav.clone(), 3.4), none, true);
    add("msk", "Clavicle", Layer::Skeleton, BONE, line(mir(&clav), 3.4), none, true);
    // ribs (10 pairs drawn)
    for i in 0..10 {
        let y0 = 103.0 + i as f32 * 6.6;
        let xo = 147.5 - i as f32 * 0.55;
        let r = spline(&pts(&[(126.0, y0), (136.0, y0 + 1.0), (xo, y0 + 9.0 + i as f32 * 0.6), (xo - 1.0, y0 + 19.0 + i as f32), (xo - 7.0, y0 + 28.0 + i as f32 * 1.2), (131.0 - i as f32 * 0.4, y0 + 33.0 + i as f32 * 1.4)]), false, 6);
        let w = 2.5 - i as f32 * 0.09;
        add("msk", "Rib", Layer::Skeleton, BONE, line(r.clone(), w), none, true);
        add("msk", "Rib", Layer::Skeleton, BONE, line(mir(&r), w), none, true);
    }
    // pelvis: iliac wings and pubic rami as bone arcs, so the pelvic inlet stays open like on a real X-ray
    let wing = spline(&pts(&[(124.0, 223.0), (138.0, 219.0), (152.0, 222.0), (160.0, 234.0), (158.0, 250.0), (150.0, 264.0), (141.0, 273.0), (131.0, 277.0), (122.0, 271.0)]), false, 7);
    let inlet = spline(&pts(&[(133.0, 233.0), (127.0, 246.0), (126.0, 260.0), (122.0, 270.0)]), false, 6);
    for (l, w) in [(wing, 6.2), (inlet, 3.4)] {
        add("msk", "Pelvis", Layer::Skeleton, BONE, line(mir(&l), w), none, true);
        add("msk", "Pelvis", Layer::Skeleton, BONE, line(l, w), none, true);
    }
    // arm bones
    let arm = |v: &mut dyn FnMut(Vec<P>, f32)| {
        v(spline(&pts(&[(167.0, 108.0), (172.0, 140.0), (178.0, 170.0), (183.0, 192.0)]), false, 6), 7.8); // humerus
        v(spline(&pts(&[(185.0, 196.0), (189.0, 224.0), (194.0, 250.0)]), false, 5), 3.8); // radius
        v(spline(&pts(&[(180.0, 196.0), (182.0, 224.0), (190.0, 250.0)]), false, 5), 3.5); // ulna
        for tip in [(197.0, 286.0), (202.0, 284.0), (206.0, 278.0), (207.0, 270.0), (189.0, 270.0)] {
            v(pts(&[(193.0, 253.0), tip]), 2.1); // hand
        }
    };
    arm(&mut |g, w| add("msk", "Arm bone", Layer::Skeleton, BONE, line(g, w), none, true));
    arm(&mut |g, w| add("msk", "Arm bone", Layer::Skeleton, BONE, line(mir(&g), w), none, true));
    // leg bones
    let leg = |v: &mut dyn FnMut(Geo)| {
        v(line(spline(&pts(&[(145.0, 262.0), (150.0, 300.0), (153.0, 340.0), (154.0, 372.0)]), false, 6), 9.5)); // femur
        v(fill(ell(143.0, 260.0, 6.8, 6.8))); // femoral head
        v(fill(ell(154.0, 378.0, 5.2, 6.4))); // patella
        v(line(spline(&pts(&[(154.0, 384.0), (152.0, 425.0), (150.0, 470.0)]), false, 6), 6.2)); // tibia
        v(line(spline(&pts(&[(160.0, 389.0), (158.0, 430.0), (155.0, 466.0)]), false, 6), 3.4)); // fibula
        v(fill(spline(&pts(&[(145.0, 474.0), (153.0, 486.0), (153.0, 496.0), (141.0, 498.0), (135.0, 490.0)]), true, 4)));
    };
    leg(&mut |g| add("msk", "Leg bone", Layer::Skeleton, BONE, g, none, true));
    leg(&mut |g| {
        let m = match g {
            Geo::Fill(p) => Geo::Fill(mir(&p)),
            Geo::Line(p, w) => Geo::Line(mir(&p), w),
        };
        add("msk", "Leg bone", Layer::Skeleton, BONE, m, none, true)
    });

    // ---- organs
    let liver = [0x8a, 0x4a, 0x3c];
    add("gut", "Liver", Layer::Organ, liver, fill(spline(&pts(&[(88.0, 200.0), (100.0, 194.0), (118.0, 194.0), (130.0, 200.0), (128.0, 210.0), (118.0, 220.0), (104.0, 224.0), (92.0, 220.0), (86.0, 210.0)]), true, 6)), none, true);
    add("gut", "Stomach", Layer::Organ, [0xc4, 0x7c, 0x6c], fill(spline(&pts(&[(130.0, 202.0), (142.0, 198.0), (152.0, 206.0), (154.0, 220.0), (146.0, 234.0), (136.0, 236.0), (130.0, 228.0), (134.0, 216.0)]), true, 6)), none, true);
    add("immune", "Spleen", Layer::Organ, [0x6e, 0x4d, 0x70], fill(spline(&pts(&[(157.0, 200.0), (163.0, 206.0), (163.0, 218.0), (157.0, 223.0), (153.0, 212.0)]), true, 5)), none, true);
    add("gut", "Large intestine", Layer::Organ, [0xb4, 0x7f, 0x6d],
        line(spline(&pts(&[(100.0, 270.0), (97.0, 250.0), (98.0, 237.0), (110.0, 232.0), (130.0, 232.0), (142.0, 237.0), (143.0, 250.0), (142.0, 270.0), (136.0, 279.0), (124.0, 277.0), (120.0, 270.0)]), false, 6), 6.4), none, true);
    add("gut", "Small intestine", Layer::Organ, [0xd9, 0xa3, 0x92], fill(spline(&pts(&[(104.0, 243.0), (136.0, 243.0), (138.0, 260.0), (128.0, 270.0), (112.0, 270.0), (102.0, 260.0)]), true, 5)), none, true);
    for k in 0..5 {
        let y = 248.0 + k as f32 * 5.2;
        add("gut", "Intestinal coil", Layer::Organ, [0xa4, 0x68, 0x5a], line(spline(&pts(&[(106.0, y), (112.0, y + 3.0), (118.0, y), (124.0, y + 3.0), (130.0, y), (136.0, y + 3.0)]), false, 5), 1.3), none, false);
    }
    // lungs: viewer-left = patient's right lung (3 lobes), viewer-right = left lung (2 lobes)
    let lung = [0xc2, 0x8a, 0x96];
    let fis = [0x8c, 0x58, 0x65];
    add("lungs", "Right lung", Layer::Organ, lung, fill(spline(&pts(&[(108.0, 104.0), (98.0, 110.0), (90.0, 126.0), (86.0, 152.0), (88.0, 178.0), (96.0, 192.0), (108.0, 194.0), (116.0, 188.0), (118.0, 168.0), (117.0, 134.0), (114.0, 112.0)]), true, 6)), Pulse::Lung, true);
    add("lungs", "Fissure", Layer::Organ, fis, line(spline(&pts(&[(87.0, 150.0), (104.0, 145.0), (117.0, 140.0)]), false, 5), 1.1), Pulse::Lung, false);
    add("lungs", "Fissure", Layer::Organ, fis, line(spline(&pts(&[(94.0, 119.0), (108.0, 152.0), (117.0, 182.0)]), false, 5), 1.1), Pulse::Lung, false);
    add("lungs", "Left lung", Layer::Organ, lung, fill(spline(&pts(&[(132.0, 104.0), (142.0, 110.0), (150.0, 126.0), (154.0, 152.0), (152.0, 178.0), (144.0, 192.0), (132.0, 194.0), (126.0, 186.0), (124.0, 170.0), (121.0, 150.0), (122.0, 128.0), (126.0, 112.0)]), true, 6)), Pulse::Lung, true);
    add("lungs", "Fissure", Layer::Organ, fis, line(spline(&pts(&[(147.0, 118.0), (138.0, 152.0), (130.0, 184.0)]), false, 5), 1.1), Pulse::Lung, false);
    let air = [0xa6, 0xb8, 0xcc];
    add("lungs", "Trachea", Layer::Organ, air, line(pts(&[(120.0, 77.0), (120.0, 96.0), (120.0, 112.0)]), 6.0), none, true);
    add("lungs", "Bronchus", Layer::Organ, air, line(spline(&pts(&[(120.0, 112.0), (113.0, 123.0), (107.0, 135.0)]), false, 4), 3.8), none, true);
    add("lungs", "Bronchus", Layer::Organ, air, line(spline(&pts(&[(120.0, 112.0), (127.0, 122.0), (135.0, 133.0)]), false, 4), 3.8), none, true);
    // heart + great vessels
    add("heart", "Vena cava", Layer::Organ, [0x4b, 0x6f, 0xa8], line(spline(&pts(&[(115.0, 112.0), (113.0, 134.0), (115.0, 154.0)]), false, 5), 4.0), Pulse::Heart, true);
    add("heart", "Aorta", Layer::Organ, [0xd2, 0x5a, 0x62], line(spline(&pts(&[(124.0, 134.0), (122.0, 120.0), (128.0, 112.0), (136.0, 116.0), (138.0, 134.0)]), false, 6), 5.0), Pulse::Heart, true);
    add("heart", "Pulmonary trunk", Layer::Organ, [0x6d, 0x8c, 0xc0], line(spline(&pts(&[(131.0, 132.0), (135.0, 124.0), (142.0, 128.0)]), false, 5), 3.8), Pulse::Heart, true);
    add("heart", "Heart", Layer::Organ, [0xb0, 0x3e, 0x4a], fill(spline(&pts(&[(118.0, 134.0), (127.0, 128.0), (137.0, 132.0), (143.0, 146.0), (140.0, 164.0), (132.0, 180.0), (125.0, 193.0), (119.0, 184.0), (114.0, 166.0), (113.0, 148.0)]), true, 6)), Pulse::Heart, true);
    // brain (inside the skull) with gyri, cerebellum
    let brain = sym(&[(120.0, 13.0), (131.0, 14.0), (138.0, 21.0), (141.0, 31.0), (138.0, 40.0), (131.0, 44.0), (124.0, 45.0), (120.0, 43.0)]);
    add("brain", "Brain", Layer::Organ, [0xc9, 0xa4, 0xa6], fill(spline(&brain, true, 5)), none, true);
    add("brain", "Cerebellum", Layer::Organ, [0xb8, 0x92, 0x95], fill(spline(&pts(&[(110.0, 44.0), (120.0, 47.0), (130.0, 44.0), (128.0, 50.0), (120.0, 52.0), (112.0, 50.0)]), true, 4)), none, true);
    let gy = [0x96, 0x6f, 0x73];
    for path in [
        &[(108.0, 22.0), (114.0, 19.0), (120.0, 23.0), (126.0, 19.0), (132.0, 22.0)][..],
        &[(106.0, 30.0), (112.0, 27.0), (118.0, 31.0), (124.0, 27.0), (130.0, 30.0), (135.0, 27.0)][..],
        &[(110.0, 37.0), (116.0, 34.0), (122.0, 38.0), (128.0, 35.0), (134.0, 37.0)][..],
        &[(120.0, 14.0), (120.0, 42.0)][..],
    ] {
        add("brain", "Gyri", Layer::Organ, gy, line(spline(&pts(path), false, 5), 1.1), none, false);
    }
    // eyes
    for cx in [108.0, 132.0] {
        add("eyes", "Eye", Layer::Organ, [0xe2, 0xe8, 0xf0], fill(ell(cx, 43.0, 5.4, 3.5)), none, true);
        add("eyes", "Iris", Layer::Organ, [0x4a, 0x78, 0xa6], fill(ell(cx, 43.0, 2.7, 2.7)), none, true);
        add("eyes", "Pupil", Layer::Organ, [0x10, 0x16, 0x1e], fill(ell(cx, 43.0, 1.1, 1.1)), none, false);
    }
    // lymph nodes
    for (x, y) in [(113.0, 91.0), (127.0, 91.0), (158.0, 123.0), (82.0, 123.0), (113.0, 268.0), (127.0, 268.0)] {
        add("immune", "Lymph node", Layer::Organ, [0xb9, 0xa0, 0xd8], fill(ell(x, y, 2.0, 2.0)), none, true);
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_part_is_finite_and_inside_the_canvas() {
        for part in parts() {
            let pts: &[P] = match &part.geo {
                Geo::Fill(p) | Geo::Line(p, _) => p,
            };
            assert!(pts.len() >= 2, "{}", part.name);
            for q in pts {
                assert!(q.x.is_finite() && q.y.is_finite(), "{}", part.name);
                assert!(q.x > -5.0 && q.x < W + 5.0 && q.y > -5.0 && q.y < H + 5.0, "{} out of canvas: {:?}", part.name, q);
            }
        }
    }

    #[test]
    fn triangulation_covers_the_polygon_area() {
        for part in parts() {
            if let Geo::Fill(poly) = &part.geo {
                let tris = triangulate(poly);
                let a: f32 = tris.iter().map(|t| area(&[poly[t[0] as usize], poly[t[1] as usize], poly[t[2] as usize]]).abs()).sum();
                let want = area(poly).abs();
                assert!((a - want).abs() / want.max(1.0) < 0.02, "{}: triangles {} vs polygon {}", part.name, a, want);
            }
        }
    }

    #[test]
    fn hit_testing_finds_the_heart_and_the_skin() {
        let ps = parts();
        let heart = ps.iter().find(|p| p.name == "Heart").unwrap();
        if let Geo::Fill(poly) = &heart.geo {
            assert!(contains(poly, p(128.0, 160.0)));
            assert!(!contains(poly, p(60.0, 160.0)));
        }
        let skin = ps.iter().find(|p| p.name == "Skin").unwrap();
        if let Geo::Fill(poly) = &skin.geo {
            assert!(contains(poly, p(120.0, 160.0)));
            assert!(!contains(poly, p(20.0, 160.0)));
        }
    }

    #[test]
    fn every_system_has_a_clickable_part_and_a_callout() {
        let ps = parts();
        for sys in ["brain", "eyes", "heart", "lungs", "gut", "msk", "immune"] {
            assert!(ps.iter().any(|p| p.sys == sys && p.hit), "{sys}");
            assert!(callouts().iter().any(|c| c.sys == sys), "{sys}");
        }
    }
}
