#!/usr/bin/env python
"""Renders the shaded anatomy sprites for the window UI from BodyParts3D (real CT-based meshes).

Every part (skin, each bone group, each organ) is rasterised on its own with a z-buffer in the same frontal,
orthographic projection as tools/gen_anatomy.py (so the vector data, the anchors and these sprites line up),
lit by a key light (upper left, front), a fill light and a little specular, with screen-space ambient
occlusion from the other parts of the same layer. 2x2 supersampling gives the anti-aliased edge.

Output
  crates/ui-gui/assets/anatomy_atlas.png  RGBA atlas, one sprite per part:
        R = diffuse shade (with occlusion, 255 = 1.25), G = rim (1 - n.view), B = specular, A = coverage
  crates/ui-gui/src/anatomy_sprites.rs    table: part -> body system, layer, tissue colour, atlas and design rects

BodyParts3D, (c) The Database Center for Life Science, licensed under CC Attribution 4.0 International
(mesh files: CC Attribution-Share Alike 2.1 Japan). https://dbarchive.biosciencedbc.jp/en/bodyparts3d/lic.html

Usage: python render_anatomy.py <dir with obj/partof_BP3D_4.0_obj_99/ and elem.txt> <atlas.png> <out.rs> [preview.png]
Needs numpy + opencv-python-headless.
"""
import sys, os, collections, re
import numpy as np, cv2

D, OUT_PNG, OUT_RS = sys.argv[1], sys.argv[2], sys.argv[3]
PREV = sys.argv[4] if len(sys.argv) > 4 else None
CACHE = os.environ.get("BP3D_CACHE")  # optional folder for parsed meshes (.npz), speeds up re-runs

el = collections.defaultdict(list)
cname = {}
for l in open(os.path.join(D, "elem.txt"), encoding="utf-8").read().splitlines()[1:]:
    a = l.split("\t")
    if len(a) >= 3:
        el[a[0]].append(a[2])
        cname[a[0]] = a[1]


def load(fj):
    if CACHE:
        cp = os.path.join(CACHE, fj + ".npz")
        if os.path.exists(cp):
            z = np.load(cp)
            return z["v"], z["f"]
    v, f = [], []
    for l in open(os.path.join(D, "obj", "partof_BP3D_4.0_obj_99", fj + ".obj")):
        if l.startswith("v "):
            v.append(l.split()[1:4])
        elif l.startswith("f "):
            f.append([x.split("/")[0] for x in l.split()[1:4]])
    v = np.array(v, np.float64)
    f = np.array(f, np.int64) - 1
    if CACHE:
        os.makedirs(CACHE, exist_ok=True)
        np.savez(os.path.join(CACHE, fj + ".npz"), v=v, f=f)
    return v, f


HMM = 1641.4  # body height in the data set, mm
S = 490.0 / HMM  # design units per mm
R = 2  # atlas texels per design unit
SS = 2  # supersampling
RR = R * SS


def cen(c):
    return np.vstack([load(x)[0] for x in el[c]]).mean(0)


heart, spine, sternum = cen("FMA7088"), cen("FMA13478"), cen("FMA7485")
XS = 1.0 if heart[0] > 0 else -1.0
ANT = 1.0 if sternum[1] > spine[1] else -1.0


def to_view(v):
    """mm -> design units: x right, y down, z toward the viewer."""
    return np.stack([120.0 + XS * v[:, 0] * S, 5.0 + (HMM - v[:, 2]) * S, ANT * v[:, 1] * S], 1)


def vnormals(v, f):
    a, b, c = v[f[:, 0]], v[f[:, 1]], v[f[:, 2]]
    fn = np.cross(b - a, c - a)
    n = np.zeros_like(v)
    for k in range(3):
        np.add.at(n, f[:, k], fn)
    n /= np.maximum(np.linalg.norm(n, axis=1, keepdims=True), 1e-12)
    return n


def raster(V, F, N, x0, y0, w, h):
    """Z-buffer rasterisation at RR px / unit of triangles (V in design units). Returns z (front-most), normal."""
    P = np.empty_like(V)
    P[:, 0] = (V[:, 0] - x0) * RR
    P[:, 1] = (V[:, 1] - y0) * RR
    P[:, 2] = V[:, 2]
    a, b, c = P[F[:, 0]], P[F[:, 1]], P[F[:, 2]]
    area = (b[:, 0] - a[:, 0]) * (c[:, 1] - a[:, 1]) - (b[:, 1] - a[:, 1]) * (c[:, 0] - a[:, 0])
    keep = np.abs(area) > 1e-9
    F, a, b, c, area = F[keep], a[keep], b[keep], c[keep], area[keep]
    lo = np.floor(np.minimum(np.minimum(a, b), c)[:, :2] - 0.5).astype(np.int64)
    hi = np.ceil(np.maximum(np.maximum(a, b), c)[:, :2] - 0.5).astype(np.int64)
    span = np.maximum(hi[:, 0] - lo[:, 0], hi[:, 1] - lo[:, 1]) + 1
    zbuf = np.full(w * h, -1e9)
    nbuf = np.zeros((w * h, 3))
    bucket = np.ceil(np.log2(np.maximum(span, 1))).astype(np.int64)
    for bk in np.unique(bucket):
        B = 1 << int(bk)
        idx = np.nonzero(bucket == bk)[0]
        oy, ox = np.mgrid[0:B, 0:B]
        ox, oy = ox.ravel(), oy.ravel()
        chunk = max(1, 3_000_000 // (B * B))
        for s in range(0, len(idx), chunk):
            t = idx[s:s + chunk]
            px = lo[t, 0:1] + ox[None, :]
            py = lo[t, 1:2] + oy[None, :]
            cx, cy = px + 0.5, py + 0.5
            A, Bv, C = a[t], b[t], c[t]
            w0 = (Bv[:, 0:1] - cx) * (C[:, 1:2] - cy) - (Bv[:, 1:2] - cy) * (C[:, 0:1] - cx)
            w1 = (C[:, 0:1] - cx) * (A[:, 1:2] - cy) - (C[:, 1:2] - cy) * (A[:, 0:1] - cx)
            w2 = (A[:, 0:1] - cx) * (Bv[:, 1:2] - cy) - (A[:, 1:2] - cy) * (Bv[:, 0:1] - cx)
            ar = area[t][:, None]
            l0, l1, l2 = w0 / ar, w1 / ar, w2 / ar
            m = (l0 >= -1e-7) & (l1 >= -1e-7) & (l2 >= -1e-7) & (px >= 0) & (py >= 0) & (px < w) & (py < h)
            if not m.any():
                continue
            ti, si = np.nonzero(m)
            tri = t[ti]
            L0, L1, L2 = l0[ti, si], l1[ti, si], l2[ti, si]
            z = L0 * a[tri, 2] + L1 * b[tri, 2] + L2 * c[tri, 2]
            pix = py[ti, si] * w + px[ti, si]
            order = np.lexsort((-z, pix))
            pix, z, tri, L0, L1, L2 = pix[order], z[order], tri[order], L0[order], L1[order], L2[order]
            first = np.ones(len(pix), bool)
            first[1:] = pix[1:] != pix[:-1]
            pix, z, tri, L0, L1, L2 = pix[first], z[first], tri[first], L0[first], L1[first], L2[first]
            better = z > zbuf[pix]
            pix, z, tri, L0, L1, L2 = pix[better], z[better], tri[better], L0[better], L1[better], L2[better]
            zbuf[pix] = z
            nbuf[pix] = L0[:, None] * N[F[tri, 0]] + L1[:, None] * N[F[tri, 1]] + L2[:, None] * N[F[tri, 2]]
    return zbuf.reshape(h, w), nbuf.reshape(h, w, 3)


def unit(x):
    x = np.array(x, float)
    return x / np.linalg.norm(x)


KEY = unit([-0.55, -0.65, 0.75])  # upper left, in front (x right, y down, z to viewer)
FILL = unit([0.7, 0.1, 0.55])
HALF = unit(KEY + np.array([0, 0, 1.0]))

# ---------------------------------------------------------------------------------------------- parts
BONE = (0xD8, 0xD2, 0xC4)
CART = (0xB8, 0xC3, 0xC9)
spec = []


def add(sys_, name, layer, rgb, concepts, pulse=0, hit=True, gloss=0.0, lobes=None, bias=0.0):
    """lobes: the part is only a vessel / airway tree (BodyParts3D lungs); build its outer surface from it,
    with fissures between these lobe concepts. bias: added to the depth used for the drawing order."""
    spec.append(dict(sys=sys_, name=name, layer=layer, rgb=rgb, concepts=concepts, pulse=pulse, hit=hit, gloss=gloss, lobes=lobes, bias=bias))


def by_name(rx):
    r = re.compile(rx)
    return sorted(c for c, n in cname.items() if r.search(n))


carpals = r"^(right|left) (scaphoid|lunate|triquetral|pisiform|trapezium|trapezoid|capitate|hamate)$"
add("immune", "Skin", 0, (0x5A, 0x6A, 0x7C), ["FMA7163"], hit=False)
add("msk", "Skull", 1, BONE, ["FMA46565"])
add("msk", "Hyoid", 1, BONE, ["FMA52749"])
add("msk", "Vertebral column", 1, BONE, ["FMA13478"])
add("msk", "Rib cage", 1, BONE, ["FMA7480"])
add("msk", "Costal cartilage", 1, CART, by_name(r"^(right|left) \w+ costal cartilage$"))
add("msk", "Sternum", 1, BONE, ["FMA7485"])
add("msk", "Pelvis", 1, BONE, ["FMA16580"])
for side in ("right", "left"):
    sid = {"right": 0, "left": 1}[side]
    add("msk", "Clavicle", 1, BONE, [["FMA13322", "FMA13323"][sid]])
    add("msk", "Scapula", 1, BONE, [["FMA13395", "FMA13396"][sid]])
    add("msk", "Humerus", 1, BONE, [["FMA23130", "FMA23131"][sid]])
    add("msk", "Radius", 1, BONE, [["FMA23464", "FMA23465"][sid]])
    add("msk", "Ulna", 1, BONE, [["FMA23467", "FMA23468"][sid]])
    add("msk", "Hand", 1, BONE, [["FMA9713", "FMA9714"][sid]] + [c for c in by_name(carpals) if cname[c].startswith(side)])
    add("msk", "Femur", 1, BONE, [["FMA24474", "FMA24475"][sid]])
    add("msk", "Patella", 1, BONE, [["FMA24486", "FMA24487"][sid]])
    add("msk", "Tibia", 1, BONE, [["FMA24477", "FMA24478"][sid]])
    add("msk", "Fibula", 1, BONE, [["FMA24480", "FMA24481"][sid]])
    add("msk", "Foot", 1, BONE, [["FMA11343", "FMA11344"][sid]])

add("gut", "Kidney", 2, (0x8E, 0x45, 0x45), ["FMA7204"], gloss=0.5)
add("gut", "Kidney", 2, (0x8E, 0x45, 0x45), ["FMA7205"], gloss=0.5)
add("heart", "Inferior vena cava", 2, (0x4E, 0x62, 0x8A), ["FMA10951"], gloss=0.5)
add("heart", "Aorta", 2, (0xA8, 0x4A, 0x4E), ["FMA3734"], gloss=0.5)
add("gut", "Pancreas", 2, (0xC9, 0xA4, 0x78), ["FMA7198"], gloss=0.3)
add("gut", "Large intestine", 2, (0xB4, 0x8A, 0x78), ["FMA7201"], gloss=0.4)
add("gut", "Small intestine", 2, (0xC6, 0x98, 0x88), ["FMA7200"], gloss=0.5)
add("gut", "Urinary bladder", 2, (0xC0, 0x98, 0x90), ["FMA15900"], gloss=0.5)
add("gut", "Esophagus", 2, (0xB0, 0x7E, 0x76), ["FMA7131"], gloss=0.4, bias=-60)
add("gut", "Stomach", 2, (0xC2, 0x8C, 0x80), ["FMA7148"], gloss=0.6)
add("gut", "Liver", 2, (0x7E, 0x3E, 0x36), ["FMA7197"], gloss=0.6)
add("gut", "Gallbladder", 2, (0x6A, 0x7E, 0x4C), ["FMA7202"], gloss=0.6)
add("lungs", "Airways", 2, (0xC4, 0xC9, 0xCE), ["FMA7393"], gloss=0.3, bias=-40)
add("lungs", "Right lung", 2, (0xB6, 0x8C, 0x92), ["FMA7309"], pulse=2, gloss=0.3, lobes=["FMA7333", "FMA7383", "FMA7337"])
add("lungs", "Left lung", 2, (0xB6, 0x8C, 0x92), ["FMA7310"], pulse=2, gloss=0.3, lobes=["FMA7370", "FMA7371"])
add("heart", "Superior vena cava", 2, (0x4E, 0x62, 0x8A), ["FMA4720"], gloss=0.5)
add("heart", "Pulmonary trunk", 2, (0x5E, 0x6E, 0x98), ["FMA8612"], gloss=0.5)
add("immune", "Thymus", 2, (0xC4, 0xAE, 0x86), ["FMA9607"], gloss=0.3)
add("heart", "Heart", 2, (0x9A, 0x3A, 0x40), ["FMA7088"], pulse=1, gloss=0.7, bias=40)
add("brain", "Brain", 2, (0xC8, 0xA6, 0xA4), ["FMA50801"], gloss=0.35)
add("eyes", "Eye", 2, (0xB4, 0xB8, 0xBC), ["FMA12514"], gloss=0.45)
add("eyes", "Eye", 2, (0xB4, 0xB8, 0xBC), ["FMA12515"], gloss=0.45)

# ---------------------------------------------------------------------------------------------- geometry
meshes = []
for sp in spec:
    fjs = sorted({x for c in sp["concepts"] for x in el.get(c, [])})
    if not fjs:
        print("MISSING", sp["name"], sp["concepts"], file=sys.stderr)
        continue
    Vs, Fs, FN, off = [], [], [], 0
    for fj in fjs:
        v, f = load(fj)
        a, b, c = v[f[:, 0]], v[f[:, 1]], v[f[:, 2]]
        fn = np.cross(b - a, c - a)
        vol = np.einsum("ij,ij->i", a, np.cross(b, c)).sum()
        FN.append(fn * (1.0 if vol >= 0 else -1.0))  # outward
        Vs.append(v)
        Fs.append(f + off)
        off += len(v)
    Vmm, F = np.vstack(Vs), np.vstack(Fs)
    # weld coincident vertices across files so the seams between segments (liver, heart, brain) shade smoothly
    _, inv = np.unique(np.round(Vmm * 100).astype(np.int64), axis=0, return_inverse=True)
    inv = inv.ravel()
    nw = np.zeros((inv.max() + 1, 3))
    fn = np.vstack(FN)
    for k in range(3):
        np.add.at(nw, inv[F[:, k]], fn)
    n = nw[inv]
    n /= np.maximum(np.linalg.norm(n, axis=1, keepdims=True), 1e-12)
    sp["V"], sp["F"], sp["N"] = to_view(Vmm), F, np.stack([XS * n[:, 0], -n[:, 2], ANT * n[:, 1]], 1)
    if sp["lobes"]:
        sp["lobe_mesh"] = []
        for c in sp["lobes"]:
            Vl, Fl, Nl, o2 = [], [], [], 0
            for fj in sorted(set(el[c])):
                v, f = load(fj)
                Vl.append(to_view(v))
                Nl.append(np.zeros((len(v), 3)))
                Fl.append(f + o2)
                o2 += len(v)
            sp["lobe_mesh"].append((np.vstack(Vl), np.vstack(Fl), np.vstack(Nl)))
    meshes.append(sp)
    print("%-20s %3d files %7d tris" % (sp["name"], len(fjs), len(sp["F"])), file=sys.stderr)

# every layer is rendered on one canvas so occlusion between parts of the same layer can be measured
CW, CH = 240 * RR, 530 * RR
def envelope(sp, z, x0, y0, w, h):
    """Outer surface of a lung from its airway / vessel tree: close the projected tree into a solid outline,
    raise it as a dome (height field) and groove the fissures between the lobes."""
    px = lambda u: max(1, int(round(u * RR)))
    ell = lambda r: cv2.getStructuringElement(cv2.MORPH_ELLIPSE, (2 * px(r) + 1, 2 * px(r) + 1))
    m = (z > -1e8).astype(np.uint8)
    m = cv2.morphologyEx(m, cv2.MORPH_CLOSE, ell(7.0))
    ff = m.copy()
    cv2.floodFill(ff, np.zeros((h + 2, w + 2), np.uint8), (0, 0), 2)
    m[ff == 0] = 1
    m = cv2.dilate(m, ell(1.6))
    m = (cv2.GaussianBlur(m.astype(np.float32), (0, 0), px(2.6)) > 0.5).astype(np.uint8)
    # one lung = one piece: drop stray bits of the tree that hang below the base
    n_lab, lab_cc, stats, _ = cv2.connectedComponentsWithStats(m, connectivity=8)
    if n_lab > 2:
        m = (lab_cc == 1 + int(np.argmax(stats[1:, cv2.CC_STAT_AREA]))).astype(np.uint8)
    d = cv2.distanceTransform(m, cv2.DIST_L2, 5) / RR
    rd = 11.0
    t = np.clip(d / rd, 0, 1)
    hf = cv2.GaussianBlur((rd * 0.85 * np.sqrt(1 - (1 - t) ** 2)).astype(np.float32), (0, 0), px(0.8))
    gx = cv2.Sobel(hf, cv2.CV_32F, 1, 0, ksize=3) / 8.0 * RR
    gy = cv2.Sobel(hf, cv2.CV_32F, 0, 1, ksize=3) / 8.0 * RR
    n = np.dstack([-gx, -gy, np.ones_like(gx)]).astype(np.float64)
    n /= np.linalg.norm(n, axis=2, keepdims=True)
    zc = float(np.median(z[z > -1e8]))
    zz = np.where(m > 0, zc + hf, -1e9)
    # lobes: front-most lobe tree wins, else the nearest one; smooth the labels, then groove their borders
    fiss = np.zeros((h, w), np.float32)
    if sp.get("lobe_mesh"):
        score = []
        for (Vl, Fl, Nl) in sp["lobe_mesh"]:
            zl, _ = raster(Vl, Fl, Nl, x0, y0, w, h)
            cov = (zl > -1e8).astype(np.uint8)
            dist = cv2.distanceTransform(1 - cov, cv2.DIST_L2, 5) / RR
            score.append(cv2.GaussianBlur((-dist + np.where(cov > 0, 0.02 * zl, 0)).astype(np.float32), (0, 0), px(1.5)))
        lab = np.argmax(np.stack(score), 0)
        oh = [cv2.GaussianBlur((lab == i).astype(np.float32), (0, 0), px(3.0)) for i in range(len(score))]
        lab = np.argmax(np.stack(oh), 0).astype(np.uint8)
        edge = np.zeros((h, w), np.uint8)
        edge[:, 1:] |= lab[:, 1:] != lab[:, :-1]
        edge[1:, :] |= lab[1:, :] != lab[:-1, :]
        inner = cv2.erode(m, ell(2.0))
        fiss = cv2.GaussianBlur((edge * inner).astype(np.float32), (0, 0), px(0.55))
        fiss = np.clip(fiss / max(float(fiss.max()), 1e-6) * 1.6, 0, 1)
    return zz, n, fiss


for sp in meshes:
    lo = sp["V"][:, :2].min(0)
    hi = sp["V"][:, :2].max(0)
    mg = 8 if sp["lobes"] else 3
    x0 = np.floor(lo[0] - mg) // 1
    y0 = np.floor(lo[1] - mg) // 1
    w = int(np.ceil((hi[0] + mg - x0) * RR / SS)) * SS
    h = int(np.ceil((hi[1] + mg - y0) * RR / SS)) * SS
    z, n = raster(sp["V"], sp["F"], sp["N"], x0, y0, w, h)
    sp["fiss"] = None
    if sp["lobes"]:
        z, n, sp["fiss"] = envelope(sp, z, x0, y0, w, h)
    sp["box"] = (x0, y0, w, h)
    sp["z"], sp["n"] = z, n
    sp["depth"] = (float(np.median(z[z > -1e8])) if (z > -1e8).any() else 0.0) + sp["bias"]

layer_z = {}
for L in (0, 1, 2):
    zc = np.full((CH, CW), -1e9)
    for sp in meshes:
        if sp["layer"] != L:
            continue
        x0, y0, w, h = sp["box"]
        X, Y = int(round(x0 * RR)), int(round(y0 * RR))
        xa, ya, xb, yb = max(X, 0), max(Y, 0), min(X + w, CW), min(Y + h, CH)
        sub = sp["z"][ya - Y:yb - Y, xa - X:xb - X]
        zc[ya:yb, xa:xb] = np.maximum(zc[ya:yb, xa:xb], sub)
    layer_z[L] = zc


def shade(sp):
    z, n = sp["z"], sp["n"].copy()
    m = z > -1e8
    n /= np.maximum(np.linalg.norm(n, axis=2, keepdims=True), 1e-9)
    n[n[:, :, 2] < 0] *= -1  # two-sided
    nz = np.clip(n[:, :, 2], 0, 1)
    diff = np.clip(n @ KEY, 0, 1)
    fill = np.clip(n @ FILL, 0, 1)
    sh = 0.20 + 0.78 * diff + 0.20 * fill
    spc = np.clip(n @ HALF, 0, 1) ** 28 * sp["gloss"]
    rim = 1.0 - nz
    # occlusion from the layer (other parts in front of this pixel nearby, and the part's own folds)
    x0, y0, w, h = sp["box"]
    X, Y = int(round(x0 * RR)), int(round(y0 * RR))
    zc = layer_z[sp["layer"]]
    pad = 24
    big = np.full((h + 2 * pad, w + 2 * pad), -1e9, np.float32)
    ya, yb, xa, xb = max(Y - pad, 0), min(Y + h + pad, CH), max(X - pad, 0), min(X + w + pad, CW)
    big[ya - (Y - pad):yb - (Y - pad), xa - (X - pad):xb - (X - pad)] = zc[ya:yb, xa:xb]
    floor = float(z[m].min()) - 20 if m.any() else 0.0
    big = np.maximum(big, floor).astype(np.float32)
    k = cv2.getStructuringElement(cv2.MORPH_ELLIPSE, (2 * pad - 1, 2 * pad - 1))
    near = cv2.dilate(big, k)[pad:pad + h, pad:pad + w]
    zz = np.where(m, z, floor).astype(np.float32)
    occ = np.clip((near - zz - 0.5) / 7.0, 0, 1)
    occ = cv2.GaussianBlur(occ.astype(np.float32), (0, 0), 3.0)
    sh = sh * (1.0 - 0.42 * occ)
    if sp["fiss"] is not None:
        sh = sh * (1.0 - 0.55 * sp["fiss"])
        spc = spc * (1.0 - sp["fiss"])
    out = np.zeros((h, w, 4), np.float64)
    out[:, :, 0] = np.where(m, np.clip(sh / 1.25, 0, 1), 0)
    out[:, :, 1] = np.where(m, rim, 0)
    out[:, :, 2] = np.where(m, np.clip(spc, 0, 1), 0)
    out[:, :, 3] = m
    # 2x2 supersampling -> atlas resolution (colour channels averaged over covered samples)
    out = out.reshape(h // SS, SS, w // SS, SS, 4)
    cov = out[:, :, :, :, 3].mean((1, 3))
    rgb = (out[:, :, :, :, :3] * out[:, :, :, :, 3:4]).sum((1, 3)) / np.maximum(out[:, :, :, :, 3:4].sum((1, 3)), 1e-9)
    img = np.concatenate([rgb, cov[:, :, None]], 2)
    return (np.clip(img, 0, 1) * 255 + 0.5).astype(np.uint8)


for sp in meshes:
    sp["img"] = shade(sp)

# drawing order: by layer, then back to front
meshes.sort(key=lambda s: (s["layer"], s["depth"]))

# ---------------------------------------------------------------------------------------------- atlas
PADT = 4
AW = 1024
x = y = row = 0
for sp in sorted(meshes, key=lambda s: -s["img"].shape[0]):
    h, w = sp["img"].shape[:2]
    if x + w + 2 * PADT > AW:
        x, y, row = 0, y + row, 0
    sp["at"] = (x + PADT, y + PADT, w, h)
    x += w + 2 * PADT
    row = max(row, h + 2 * PADT)
AH = y + row
AH = (AH + 3) // 4 * 4
atlas = np.zeros((AH, AW, 4), np.uint8)
for sp in meshes:
    ax, ay, w, h = sp["at"]
    atlas[ay:ay + h, ax:ax + w] = sp["img"]
cv2.imwrite(OUT_PNG, cv2.cvtColor(atlas, cv2.COLOR_RGBA2BGRA), [cv2.IMWRITE_PNG_COMPRESSION, 9])

with open(OUT_RS, "w", encoding="utf-8", newline="\n") as o:
    o.write("// @generated by tools/render_anatomy.py - do not edit.\n")
    o.write("// Shaded frontal renders of real human anatomy: BodyParts3D, (c) The Database Center for Life Science,\n")
    o.write("// licensed under CC Attribution 4.0 International (mesh files: CC Attribution-Share Alike 2.1 Japan).\n")
    o.write("// https://dbarchive.biosciencedbc.jp/en/bodyparts3d/lic.html\n")
    o.write("// Atlas channels: R diffuse shade (255 = 1.25), G rim, B specular, A coverage. %d texels per design unit.\n\n" % R)
    o.write("pub const ATLAS_PNG: &[u8] = include_bytes!(\"../assets/anatomy_atlas.png\");\n")
    o.write("pub const TEXELS_PER_UNIT: f32 = %d.0;\n\n" % R)
    o.write("pub struct Sprite {\n    pub sys: &'static str,\n    pub name: &'static str,\n    /// 0 skin, 1 skeleton, 2 organ\n    pub layer: u8,\n"
            "    pub tissue: [u8; 3],\n    /// 0 none, 1 heart beat, 2 breathing\n    pub pulse: u8,\n    pub hit: bool,\n"
            "    /// x, y, w, h in the atlas (texels)\n    pub atlas: [u16; 4],\n    /// top-left corner in design units\n    pub origin: [f32; 2],\n}\n\n")
    o.write("/// Back to front within each layer.\npub static SPRITES: &[Sprite] = &[\n")
    for sp in meshes:
        x0, y0, w, h = sp["box"]
        r = sp["rgb"]
        o.write('    Sprite { sys: "%s", name: "%s", layer: %d, tissue: [%d, %d, %d], pulse: %d, hit: %s, atlas: [%d, %d, %d, %d], origin: [%.2f, %.2f] },\n'
                % (sp["sys"], sp["name"], sp["layer"], r[0], r[1], r[2], sp["pulse"], "true" if sp["hit"] else "false",
                   *sp["at"], x0, y0))
    o.write("];\n")
print("sprites", len(meshes), "atlas", AW, "x", AH, file=sys.stderr)

if PREV:
    # quick composite (organs mode) to judge the look without building the app
    PR = R
    canvas = np.zeros((530 * PR, 240 * PR, 3), np.float64) + np.array([0x11, 0x15, 0x1B]) / 255.0

    def over(sp, col_fn, alpha):
        x0, y0, w, h = sp["box"]
        img = sp["img"].astype(np.float64) / 255.0
        X, Y = int(round(x0 * PR)), int(round(y0 * PR))
        hh, ww = img.shape[:2]
        rgb, a = col_fn(sp, img)
        a = a * alpha
        ys, xs = slice(max(Y, 0), min(Y + hh, canvas.shape[0])), slice(max(X, 0), min(X + ww, canvas.shape[1]))
        sy, sx = slice(ys.start - Y, ys.stop - Y), slice(xs.start - X, xs.stop - X)
        canvas[ys, xs] = canvas[ys, xs] * (1 - a[sy, sx, None]) + rgb[sy, sx] * a[sy, sx, None]

    def lit(sp, img):
        t = np.array(sp["rgb"]) / 255.0
        rgb = t[None, None, :] * img[:, :, 0:1] * 1.25 + img[:, :, 2:3] * 0.35
        return np.clip(rgb, 0, 1), img[:, :, 3]

    def skin_back(sp, img):
        t = np.array([0x1C, 0x24, 0x30]) / 255.0
        rgb = t[None, None, :] * (0.6 + 0.5 * img[:, :, 0:1] * 1.25)
        return rgb, img[:, :, 3] * 0.95

    def skin_front(sp, img):
        t = np.array([0x9F, 0xB4, 0xCC]) / 255.0
        rim = img[:, :, 1:2]
        return np.broadcast_to(t, img[:, :, :3].shape), img[:, :, 3] * np.clip(0.03 + 0.55 * rim[:, :, 0] ** 3, 0, 1)

    for sp in meshes:
        if sp["layer"] == 0:
            over(sp, skin_back, 1.0)
    for sp in meshes:
        if sp["layer"] == 1:
            over(sp, lit, 0.22)
    for sp in meshes:
        if sp["layer"] == 2:
            over(sp, lit, 1.0)
    for sp in meshes:
        if sp["layer"] == 0:
            over(sp, skin_front, 1.0)
    a = (np.clip(canvas, 0, 1) * 255).astype(np.uint8)
    b = np.zeros((530 * PR, 240 * PR, 3), np.float64) + np.array([0x11, 0x15, 0x1B]) / 255.0
    canvas = b
    for sp in meshes:
        if sp["layer"] == 0:
            over(sp, skin_back, 1.0)
    for sp in meshes:
        if sp["layer"] == 2:
            over(sp, lit, 0.12)
    for sp in meshes:
        if sp["layer"] == 1:
            over(sp, lit, 1.0)
    for sp in meshes:
        if sp["layer"] == 0:
            over(sp, skin_front, 1.0)
    b2 = (np.clip(canvas, 0, 1) * 255).astype(np.uint8)
    cv2.imwrite(PREV, cv2.cvtColor(np.concatenate([a, b2], 1), cv2.COLOR_RGB2BGR))
