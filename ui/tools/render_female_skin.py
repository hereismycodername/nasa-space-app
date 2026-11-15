#!/usr/bin/env python
"""Renders the female skin sprite and the female body-shape map for the window UI.

The female body is the MakeHuman base mesh with its female macro target (caucasian-female-young), both CC0
(makehumancommunity/makehuman, released as CC0 in 2020). Its arms are lowered with the MakeHuman default rig
weights to the pose of the BodyParts3D figure, then it is scaled to the same height and rendered with the
same frontal projection, lights and channels as tools/render_anatomy.py:
    R diffuse shade (255 = 1.25), G rim (1 - n.view), B specular, A coverage; 2 texels per design unit.

The organs, bones and vessels stay BodyParts3D (male only). The shape map tells the app, row by row, where
each silhouette edge of the male figure lands on the female one, so the inner layers follow the new outline.

Usage: python render_female_skin.py <makehuman dir> <male atlas.png> <out skin.png> <out shape.rs> [preview.png]
  makehuman dir: base.obj, caucasian-female-young.target, default.mhskel, default_weights.mhw
Needs numpy + opencv-python-headless.
"""
import sys, os, json
import numpy as np, cv2

MH, ATLAS, OUT_PNG, OUT_RS = sys.argv[1:5]
PREV = sys.argv[5] if len(sys.argv) > 5 else None
R, SS = 2, 2
RR = R * SS
CX = 119.5
# the male skin sprite (anatomy_sprites.rs: "Skin")
MALE_ATLAS = (4, 4, 411, 1039)
MALE_ORIGIN = (17.0, 2.0)
# arm pose of the BodyParts3D figure: shoulder joint -> wrist, in design units (x right, y down)
MALE_ARM = ((72.0, 107.0), (50.0, 258.0))

# ------------------------------------------------------------------ mesh
v, faces, group = [], [], None
for line in open(os.path.join(MH, "base.obj")):
    if line.startswith("v "):
        v.append([float(x) for x in line.split()[1:4]])
    elif line.startswith("g "):
        group = line.split()[1]
    elif line.startswith("f ") and group == "body":
        idx = [int(x.split("/")[0]) - 1 for x in line.split()[1:]]
        for k in range(1, len(idx) - 1):
            faces.append([idx[0], idx[k], idx[k + 1]])
V = np.array(v)
F = np.array(faces)
for line in open(os.path.join(MH, "caucasian-female-young.target")):
    if line.strip() and not line.startswith("#"):
        a = line.split()
        V[int(a[0])] += [float(a[1]), float(a[2]), float(a[3])]

skel = json.load(open(os.path.join(MH, "default.mhskel")))
weights = json.load(open(os.path.join(MH, "default_weights.mhw")))["weights"]
joint = lambda name: V[skel["joints"][name]].mean(0)


def descendants(root):
    out = {root}
    grew = True
    while grew:
        grew = False
        for b, d in skel["bones"].items():
            if d["parent"] in out and b not in out:
                out.add(b)
                grew = True
    return out


def to_design(p, scale, ox, oy):
    """MakeHuman (x to the figure's left, y up, z forward) -> design units (x right, y down)."""
    return np.stack([ox + p[..., 0] * scale, oy - p[..., 1] * scale], -1)


# ------------------------------------------------------------------ fit to the male figure: same top, soles, midline
male = cv2.cvtColor(cv2.imread(ATLAS, cv2.IMREAD_UNCHANGED), cv2.COLOR_BGRA2RGBA)
ax, ay, aw, ah = MALE_ATLAS
mskin = male[ay:ay + ah, ax:ax + aw].astype(np.float64) / 255.0
mcov = mskin[:, :, 3] > 0.5
rows = np.nonzero(mcov.any(1))[0]
male_top = MALE_ORIGIN[1] + rows[0] / R
male_bottom = MALE_ORIGIN[1] + (rows[-1] + 1) / R
ymax, ymin = V[:, 1].max(), V[:, 1].min()
scale = (male_bottom - male_top) / (ymax - ymin)
xmid = 0.5 * (V[:, 0].max() + V[:, 0].min())
ox = CX - xmid * scale
oy = male_top + ymax * scale
# lower each arm about its shoulder in the frontal plane until shoulder -> wrist matches the male figure
target = np.arctan2(MALE_ARM[1][0] - MALE_ARM[0][0], MALE_ARM[1][1] - MALE_ARM[0][1])  # screen angle, right arm
for side, sign in (("R", 1.0), ("L", -1.0)):
    bones = descendants("upperarm01." + side)
    w = np.zeros(len(V))
    for b in bones:
        for vi, wt in weights.get(b, []):
            w[vi] += wt
    w = np.clip(w, 0, 1)
    J = joint(skel["bones"]["upperarm01." + side]["head"])
    W = joint(skel["bones"]["wrist." + side]["head"]) if "wrist." + side in skel["bones"] else joint(skel["bones"]["lowerarm02." + side]["tail"])
    # MakeHuman x points to the figure's left = screen right; y up = screen up
    d = W - J
    cur = np.arctan2(d[0], -d[1])  # screen-space angle of the arm (x right, y down)
    want = target if side == "R" else -target
    th = want - cur
    # rotation in the x/y plane of MakeHuman space; screen y is -y, so the sense flips
    c, s = np.cos(th), np.sin(th)
    rel = V - J
    rot = np.stack([c * rel[:, 0] - s * rel[:, 1], s * rel[:, 0] + c * rel[:, 1], rel[:, 2]], 1) + J
    V = V + w[:, None] * (rot - V)
    # stretch the arm along its new axis so the wrist lands where the BodyParts3D wrist bones are
    Wn = J + np.array([c * d[0] - s * d[1], s * d[0] + c * d[1], d[2]])
    axis = (Wn - J) / np.linalg.norm(Wn - J)
    male_len = np.hypot(MALE_ARM[1][0] - MALE_ARM[0][0], MALE_ARM[1][1] - MALE_ARM[0][1])
    k = male_len / (np.linalg.norm(Wn - J) * scale)
    along = (V - J) @ axis
    V = V + (w * (k - 1.0) * np.maximum(along, 0.0))[:, None] * axis[None, :]
    print("arm", side, "turned %.1f deg, stretched x%.2f" % (np.degrees(th), k), file=sys.stderr)

P2 = to_design(V, scale, ox, oy)
Vd = np.stack([P2[:, 0], P2[:, 1], V[:, 2] * scale], 1)


def vnormals(v, f):
    a, b, c = v[f[:, 0]], v[f[:, 1]], v[f[:, 2]]
    fn = np.cross(b - a, c - a)
    n = np.zeros_like(v)
    for k in range(3):
        np.add.at(n, f[:, k], fn)
    return n / np.maximum(np.linalg.norm(n, axis=1, keepdims=True), 1e-12)


# a medical figure, not a nude: flatten the nipples and the vulva by smoothing the mesh there
edges = np.concatenate([F[:, [0, 1]], F[:, [1, 2]], F[:, [2, 0]]])
edges = np.concatenate([edges, edges[:, ::-1]])
deg = np.bincount(edges[:, 0], minlength=len(Vd)).astype(float)


def flatten(center, radii, iterations=60):
    global Vd
    d = (((Vd - center) / radii) ** 2).sum(1)
    k = np.clip(1.0 - d, 0, 1) ** 0.6 * 0.7 * (deg > 0)
    for _ in range(iterations):
        acc = np.zeros_like(Vd)
        np.add.at(acc, edges[:, 0], Vd[edges[:, 1]])
        avg = acc / np.maximum(deg, 1)[:, None]
        Vd = Vd + k[:, None] * (avg - Vd)


for side in ("L", "R"):
    ids = [vi for vi, wt in weights["breast." + side] if wt > 0.3]
    tip = Vd[ids[int(np.argmax(Vd[ids, 2]))]]
    # the nipple itself: the most forward point of the chest around the breast
    near = (np.abs(Vd[:, 1] - tip[1]) < 15) & (np.abs(Vd[:, 0] - tip[0]) < 18) & (deg > 0)
    cand = np.nonzero(near)[0]
    tip = Vd[cand[int(np.argmax(Vd[cand, 2]))]].copy()
    print("nipple", side, tip, file=sys.stderr)
    flatten(tip, np.array([6.0, 6.0, 6.0]))
mid = np.abs(Vd[:, 0] - CX) < 1.5
low = Vd[:, 1] < Vd[:, 1].min() + 0.6 * (Vd[:, 1].max() - Vd[:, 1].min())
crotch_y = Vd[mid & low, 1].max()
near = mid & (np.abs(Vd[:, 1] - (crotch_y - 6)) < 4)
front = Vd[near, 2].max() if near.any() else Vd[mid & low, 2].max()
print("crotch", crotch_y, "front", front, file=sys.stderr)
flatten(np.array([CX, crotch_y - 6.0, front]), np.array([9.0, 12.0, 8.0]), 80)

# screen y is flipped against MakeHuman y, which flips the winding; normals still point outwards after the flip
N = vnormals(Vd, F)
if N[:, 2][Vd[:, 2] > np.median(Vd[:, 2])].mean() < 0:
    N = -N

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))


def raster(V, F, N, x0, y0, w, h):
    """Z-buffer rasterisation, as in render_anatomy.py."""
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
        oy_, ox_ = np.mgrid[0:B, 0:B]
        ox_, oy_ = ox_.ravel(), oy_.ravel()
        chunk = max(1, 3_000_000 // (B * B))
        for s in range(0, len(idx), chunk):
            t = idx[s:s + chunk]
            px = lo[t, 0:1] + ox_[None, :]
            py = lo[t, 1:2] + oy_[None, :]
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


KEY = unit([-0.55, -0.65, 0.75])
FILL = unit([0.7, 0.1, 0.55])
HALF = unit(KEY + np.array([0, 0, 1.0]))

lo, hi = Vd[:, :2].min(0), Vd[:, :2].max(0)
x0, y0 = np.floor(lo[0] - 3), np.floor(lo[1] - 3)
w = int(np.ceil((hi[0] + 3 - x0) * RR / SS)) * SS
h = int(np.ceil((hi[1] + 3 - y0) * RR / SS)) * SS
z, n = raster(Vd, F, N, x0, y0, w, h)
m = z > -1e8
n /= np.maximum(np.linalg.norm(n, axis=2, keepdims=True), 1e-9)
n[n[:, :, 2] < 0] *= -1
# a medical figure, not a nude: nipples and the vulva are smoothed into the surrounding surface
# (normals there blend into heavily blurred ones, so the forms read flat and soft)
def soft_region(cx, cy, rx, ry, sigma):
    global n
    yy, xx = np.mgrid[0:h, 0:w]
    d = ((xx / RR + x0 - cx) / rx) ** 2 + ((yy / RR + y0 - cy) / ry) ** 2
    k = np.clip(1.0 - d, 0, 1) ** 0.7
    blur = np.dstack([cv2.GaussianBlur(np.where(m, n[:, :, c], 0).astype(np.float32), (0, 0), sigma * RR) for c in range(3)])
    wgt = cv2.GaussianBlur(m.astype(np.float32), (0, 0), sigma * RR)[:, :, None]
    blur = blur / np.maximum(wgt, 1e-6)
    n = n * (1 - k[:, :, None]) + blur * k[:, :, None]
    n /= np.maximum(np.linalg.norm(n, axis=2, keepdims=True), 1e-9)


for side in ("L", "R"):
    # the nipple: the most forward vertex the breast bone moves
    ids = [vi for vi, wt in weights["breast." + side] if wt > 0.3]
    tip = Vd[ids[int(np.argmax(Vd[ids, 2]))], :2]
    soft_region(tip[0], tip[1], 10.0, 10.0, 2.8)
# crotch: where the legs part on the midline, below the pelvis
pel = to_design(joint("joint-pelvis")[None, :] if "joint-pelvis" in skel["joints"] else V[skel["joints"][skel["bones"]["pelvis.L"]["head"]]].mean(0)[None, :], scale, ox, oy)[0]
col = int(round((CX - x0) * RR))
ys = np.nonzero(~m[:, col])[0]
ys = ys[ys > (pel[1] - y0) * RR]
crotch = (ys[0] / RR + y0) if len(ys) else pel[1] + 25
soft_region(CX, crotch - 5.0, 9.0, 11.0, 2.4)

nz = np.clip(n[:, :, 2], 0, 1)
sh = 0.20 + 0.78 * np.clip(n @ KEY, 0, 1) + 0.20 * np.clip(n @ FILL, 0, 1)
rim = 1.0 - nz
# self-occlusion in folds (armpits, between the legs), as in render_anatomy.py
pad = 24
floor = float(z[m].min()) - 20
zz = np.where(m, z, floor).astype(np.float32)
big = cv2.copyMakeBorder(zz, pad, pad, pad, pad, cv2.BORDER_CONSTANT, value=floor)
near = cv2.dilate(big, cv2.getStructuringElement(cv2.MORPH_ELLIPSE, (2 * pad - 1, 2 * pad - 1)))[pad:pad + h, pad:pad + w]
occ = cv2.GaussianBlur(np.clip((near - zz - 0.5) / 7.0, 0, 1).astype(np.float32), (0, 0), 3.0)
sh = sh * (1.0 - 0.42 * occ)
out = np.zeros((h, w, 4))
out[:, :, 0] = np.where(m, np.clip(sh / 1.25, 0, 1), 0)
out[:, :, 1] = np.where(m, rim, 0)
out[:, :, 3] = m
out = out.reshape(h // SS, SS, w // SS, SS, 4)
cov = out[:, :, :, :, 3].mean((1, 3))
rgb = (out[:, :, :, :, :3] * out[:, :, :, :, 3:4]).sum((1, 3)) / np.maximum(out[:, :, :, :, 3:4].sum((1, 3)), 1e-9)
img = (np.clip(np.concatenate([rgb, cov[:, :, None]], 2), 0, 1) * 255 + 0.5).astype(np.uint8)
cv2.imwrite(OUT_PNG, cv2.cvtColor(img, cv2.COLOR_RGBA2BGRA), [cv2.IMWRITE_PNG_COMPRESSION, 9])
fcov = img[:, :, 3] > 127
forigin = (float(x0), float(y0))

# ------------------------------------------------------------------ shape map: male silhouette edges -> female ones, per row
def runs(row):
    """Covered runs of one sprite row as edge positions (x0, x1, x0, x1, ...) in texel units."""
    r = row.astype(np.int8)
    d = np.diff(np.concatenate([[0], r, [0]]))
    starts, ends = np.nonzero(d == 1)[0], np.nonzero(d == -1)[0]
    keep = (ends - starts) >= 3  # ignore single-texel slivers
    return starts[keep], ends[keep]


rows_out = []
for y in range(int(np.ceil(male_top)), int(np.floor(male_bottom)) + 1):
    my = int((y - MALE_ORIGIN[1]) * R)
    fy = int((y - forigin[1]) * R)
    if not (0 <= my < mcov.shape[0] and 0 <= fy < fcov.shape[0]):
        continue
    ms, me = runs(mcov[my])
    fs, fe = runs(fcov[fy])
    if len(ms) == 0 or len(ms) != len(fs):
        rows_out.append((y, None))
        continue
    pairs = []
    for a, b, c, d in zip(ms, me, fs, fe):
        pairs.append((MALE_ORIGIN[0] + a / R, forigin[0] + c / R))
        pairs.append((MALE_ORIGIN[0] + b / R, forigin[0] + d / R))
    rows_out.append((y, pairs))

# rows without a match take the nearest matched row; then smooth each edge over neighbouring rows
good = [i for i, (_, p) in enumerate(rows_out) if p]
for i, (y, p) in enumerate(rows_out):
    if p is None:
        j = min(good, key=lambda g: abs(g - i))
        rows_out[i] = (y, rows_out[j][1])
sm = []
for i, (y, p) in enumerate(rows_out):
    acc = [list(q) for q in p]
    cnt = 1
    for j in range(max(0, i - 3), min(len(rows_out), i + 4)):
        if j != i and len(rows_out[j][1]) == len(p):
            for k, q in enumerate(rows_out[j][1]):
                acc[k][0] += q[0]
                acc[k][1] += q[1]
            cnt += 1
    sm.append((y, [(a / cnt, b / cnt) for a, b in acc]))

with open(OUT_RS, "w", encoding="utf-8", newline="\n") as o:
    o.write("// @generated by tools/render_female_skin.py - do not edit.\n")
    o.write("// Female skin: MakeHuman base mesh + caucasian-female-young target (CC0, makehumancommunity/makehuman).\n")
    o.write("// Same channels as the anatomy atlas: R shade (255 = 1.25), G rim, B specular, A coverage. 2 texels per unit.\n\n")
    o.write('pub const FEMALE_SKIN_PNG: &[u8] = include_bytes!("../assets/female_skin.png");\n')
    o.write("pub const FEMALE_SKIN_ORIGIN: [f32; 2] = [%.2f, %.2f];\n\n" % forigin)
    o.write("/// Per design row: (male x, female x) of each silhouette edge, left to right.\n")
    o.write("pub static FEMALE_SHAPE: &[(f32, &[(f32, f32)])] = &[\n")
    for y, p in sm:
        o.write("    (%.1f, &[%s]),\n" % (y, ", ".join("(%.2f, %.2f)" % q for q in p)))
    o.write("];\n")
print("female skin", img.shape, "origin", forigin, "rows", len(sm), file=sys.stderr)

if PREV:
    a = (mskin[:, :, 0] * mskin[:, :, 3] * 255).astype(np.uint8)
    b = (img[:, :, 0].astype(np.float64) * img[:, :, 3] / 255).astype(np.uint8)
    hh = max(a.shape[0], b.shape[0])
    pa = np.zeros((hh, a.shape[1]), np.uint8); pa[:a.shape[0]] = a
    pb = np.zeros((hh, b.shape[1]), np.uint8); pb[:b.shape[0]] = b
    cv2.imwrite(PREV, np.concatenate([pa, pb], 1))
