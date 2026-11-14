#!/usr/bin/env python
"""Generates crates/ui-model/src/anatomy_data.rs from BodyParts3D reference meshes (frontal projection).

BodyParts3D, (c) The Database Center for Life Science, licensed under CC Attribution 4.0 International
(the mesh files themselves state CC Attribution-Share Alike 2.1 Japan; this derived data is shared under the same terms).
https://dbarchive.biosciencedbc.jp/en/bodyparts3d/lic.html

Usage: python gen_anatomy.py <dir with obj/partof_BP3D_4.0_obj_99/ and elem.txt> <out.rs> [preview.png]
elem.txt = partof_element_parts.txt from the same site. Needs numpy + opencv-python-headless.
"""
import sys, os, collections
import numpy as np, cv2

D, OUT = sys.argv[1], sys.argv[2]
PREV = sys.argv[3] if len(sys.argv) > 3 else None
el = collections.defaultdict(list)
for l in open(os.path.join(D, "elem.txt"), encoding="utf-8").read().splitlines()[1:]:
    a = l.split("\t")
    if len(a) >= 3:
        el[a[0]].append(a[2])


def load(fj):
    v = []
    f = []
    for l in open(os.path.join(D, "obj", "partof_BP3D_4.0_obj_99", fj + ".obj")):
        if l.startswith("v "):
            v.append([float(x) for x in l.split()[1:4]])
        elif l.startswith("f "):
            f.append([int(x.split("/")[0]) - 1 for x in l.split()[1:4]])
    return np.array(v), np.array(f)


HMM = 1641.4  # body height in the data set, mm
S = 490.0 / HMM  # design units per mm
R = 4  # raster scale for contouring
CW, CH = 240 * R, 500 * R


def cen(c):
    return np.vstack([load(x)[0] for x in el[c]]).mean(0)


# orientation: the patient's left must be on the viewer's right; find the anterior axis
heart, spine, sternum = cen("FMA7088"), cen("FMA13478"), cen("FMA7485")
XS = 1.0 if heart[0] > 0 else -1.0
ANT = 1.0 if sternum[1] > spine[1] else -1.0
print("heart x", heart[0], "XS", XS, "anterior sign", ANT, file=sys.stderr)


def proj(v):
    x = 120.0 + XS * v[:, 0] * S
    y = 5.0 + (HMM - v[:, 2]) * S
    return np.stack([x, y], 1)


def contours(fjs, keep_frac=0.04, eps=0.35):
    img = np.zeros((CH, CW), np.uint8)
    depth = []
    for fj in fjs:
        v, f = load(fj)
        p = proj(v) * R
        depth.append(ANT * v[:, 1].mean())
        for t in f:
            cv2.fillConvexPoly(img, np.round(p[t]).astype(np.int32), 255)
    img = cv2.morphologyEx(img, cv2.MORPH_CLOSE, np.ones((3, 3), np.uint8))
    cs, _ = cv2.findContours(img, cv2.RETR_EXTERNAL, cv2.CHAIN_APPROX_NONE)
    if not cs:
        return [], float(np.mean(depth))
    areas = [cv2.contourArea(c) for c in cs]
    amax = max(areas)
    out = []
    for c, a in zip(cs, areas):
        if a < amax * keep_frac or a < 6 * R * R:
            continue
        c = cv2.approxPolyDP(c, eps * R, True)[:, 0, :].astype(np.float32) / R
        if len(c) >= 3:
            out.append(c)
    return out, float(np.mean(depth))


SK = (0xE4, 0xDA, 0xC4)
spec = []


def add(sys_, name, layer, rgb, concepts, per=False, pulse=0, hit=True):
    spec.append((sys_, name, layer, rgb, concepts, per, pulse, hit))


add("immune", "Skin", 0, (0x22, 0x2E, 0x3D), ["FMA7163"], hit=False)
add("msk", "Skull", 1, SK, ["FMA46565"], per=True)
add("msk", "Vertebral column", 1, SK, ["FMA13478"], per=True)
add("msk", "Rib cage", 1, SK, ["FMA7480"], per=True)
add("msk", "Sternum", 1, SK, ["FMA7485"], per=True)
add("msk", "Pelvis", 1, SK, ["FMA16580"], per=True)
for c, n in [("FMA13322", "Clavicle"), ("FMA13323", "Clavicle"), ("FMA13395", "Scapula"), ("FMA13396", "Scapula"),
             ("FMA23130", "Humerus"), ("FMA23131", "Humerus"), ("FMA23464", "Radius"), ("FMA23465", "Radius"),
             ("FMA23467", "Ulna"), ("FMA23468", "Ulna"), ("FMA24474", "Femur"), ("FMA24475", "Femur"),
             ("FMA24477", "Tibia"), ("FMA24478", "Tibia"), ("FMA24480", "Fibula"), ("FMA24481", "Fibula")]:
    add("msk", n, 1, SK, [c])
add("gut", "Large intestine", 2, (0xC8, 0x8A, 0x78), ["FMA7201"])
add("gut", "Small intestine", 2, (0xD9, 0x9F, 0x8C), ["FMA7200"])
add("gut", "Kidney", 2, (0x8E, 0x3B, 0x3F), ["FMA7204"])
add("gut", "Kidney", 2, (0x8E, 0x3B, 0x3F), ["FMA7205"])
add("gut", "Urinary bladder", 2, (0xC9, 0x8E, 0x8E), ["FMA15900"])
add("gut", "Pancreas", 2, (0xD8, 0xA6, 0x6E), ["FMA7198"])
add("gut", "Liver", 2, (0x7A, 0x2E, 0x2A), ["FMA7197"])
add("gut", "Gallbladder", 2, (0x4F, 0x7A, 0x3A), ["FMA7202"])
add("gut", "Stomach", 2, (0xCF, 0x8B, 0x7B), ["FMA7148"])
add("gut", "Esophagus", 2, (0xC4, 0x80, 0x78), ["FMA7131"])
add("lungs", "Trachea", 2, (0xA8, 0xB4, 0xC0), ["FMA7394"])
add("lungs", "Right lung", 2, (0xC4, 0x8A, 0x92), ["FMA7309"], pulse=2)
add("lungs", "Left lung", 2, (0xC4, 0x8A, 0x92), ["FMA7310"], pulse=2)
add("immune", "Thymus", 2, (0xD9, 0xC2, 0x8E), ["FMA9607"])
add("heart", "Heart", 2, (0xB0, 0x2E, 0x3A), ["FMA7088"], pulse=1)
add("brain", "Brain", 2, (0xD7, 0xA3, 0xA0), ["FMA50801"])
add("eyes", "Eye", 2, (0xE8, 0xEA, 0xEE), ["FMA12514"])
add("eyes", "Eye", 2, (0xE8, 0xEA, 0xEE), ["FMA12515"])

parts = []
for sys_, name, layer, rgb, concepts, per, pulse, hit in spec:
    fjs = [x for c in concepts for x in el.get(c, [])]
    if not fjs:
        print("MISSING", concepts, file=sys.stderr)
        continue
    if per:
        for fj in fjs:
            cs, dep = contours([fj], keep_frac=0.3)
            for c in cs:
                parts.append([sys_, name, layer, rgb, pulse, hit, c, dep])
    else:
        cs, dep = contours(fjs, keep_frac=0.25 if layer == 2 else 0.5, eps=0.3 if layer != 0 else 0.5)
        for c in cs:
            parts.append([sys_, name, layer, rgb, pulse, hit, c, dep])
sk = [p for p in parts if p[2] == 1]
sk.sort(key=lambda p: p[7])
parts = [p for p in parts if p[2] == 0] + sk + [p for p in parts if p[2] == 2]


def cent(c):
    return (float(c[:, 0].mean()), float(c[:, 1].mean()))


def first(name_):
    return [p[6] for p in parts if p[1] == name_]


anch = {}
anch["brain"] = cent(first("Brain")[0])
anch["heart"] = cent(first("Heart")[0])
anch["gut"] = cent(first("Stomach")[0])
anch["immune"] = cent(first("Thymus")[0])
anch["eyes"] = cent(sorted(first("Eye"), key=lambda c: c[:, 0].mean())[0])
anch["lungs"] = cent(sorted(first("Right lung"), key=lambda c: c[:, 0].mean())[0])
anch["msk"] = cent(sorted(first("Femur"), key=lambda c: -c[:, 0].mean())[0])

with open(OUT, "w", encoding="utf-8", newline="\n") as o:
    o.write("// @generated by tools/gen_anatomy.py - do not edit.\n")
    o.write("// Frontal projection of real human anatomy: BodyParts3D, (c) The Database Center for Life Science,\n")
    o.write("// licensed under CC Attribution 4.0 International (mesh files: CC Attribution-Share Alike 2.1 Japan).\n")
    o.write("// https://dbarchive.biosciencedbc.jp/en/bodyparts3d/lic.html\n")
    o.write("// Coordinates are design units x 10 (i16), flat x,y pairs.\n\n")
    o.write("pub struct Raw {\n    pub sys: &'static str,\n    pub name: &'static str,\n    pub layer: u8,\n    pub tissue: [u8; 3],\n    pub pulse: u8,\n    pub hit: bool,\n    pub pts: &'static [i16],\n}\n\n")
    o.write("pub const ANCHORS: &[(&str, f32, f32)] = &[\n")
    for k, (x, y) in anch.items():
        o.write('    ("%s", %.1f, %.1f),\n' % (k, x, y))
    o.write("];\n\npub static PARTS: &[Raw] = &[\n")
    for sys_, name, layer, rgb, pulse, hit, c, _ in parts:
        flat = ",".join("%d,%d" % (round(x * 10), round(y * 10)) for x, y in c)
        o.write('    Raw { sys: "%s", name: "%s", layer: %d, tissue: [%d, %d, %d], pulse: %d, hit: %s, pts: &[%s] },\n'
                % (sys_, name, layer, rgb[0], rgb[1], rgb[2], pulse, "true" if hit else "false", flat))
    o.write("];\n")
print("parts", len(parts), "points", sum(len(p[6]) for p in parts), file=sys.stderr)

if PREV:
    img = np.full((1000, 480, 3), 16, np.uint8)
    for sys_, name, layer, rgb, pulse, hit, c, _ in parts:
        col = (rgb[2], rgb[1], rgb[0])
        pts = np.round(c * 2).astype(np.int32)
        if layer == 0:
            cv2.polylines(img, [pts], True, (110, 110, 110), 1)
        else:
            cv2.fillPoly(img, [pts], col)
            cv2.polylines(img, [pts], True, (30, 30, 30), 1)
    cv2.imwrite(PREV, img)

