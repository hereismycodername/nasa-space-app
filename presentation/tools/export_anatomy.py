"""Turns the raw anatomy atlas (R shade, G rim, B specular, A coverage) into ready-to-show sprites for the pitch
deck, with the same looks as ui/crates/ui-gui/src/figure.rs (organs / skeleton / X-ray).

Usage: python export_anatomy.py <atlas.png> <sprites.rs> <out dir>
"""
import json, os, re, sys
import numpy as np, cv2

ATLAS, RS, OUT = sys.argv[1], sys.argv[2], sys.argv[3]
os.makedirs(OUT, exist_ok=True)

src = open(RS, encoding="utf-8").read()
TPU = int(float(re.search(r"TEXELS_PER_UNIT: f32 = ([\d.]+)", src).group(1)))
pat = re.compile(r'Sprite \{ sys: "(\w+)", name: "([^"]+)", layer: (\d), tissue: \[(\d+), (\d+), (\d+)\], pulse: (\d), hit: (\w+), '
                 r'atlas: \[(\d+), (\d+), (\d+), (\d+)\], origin: \[([-\d.]+), ([-\d.]+)\] \}')
sprites = []
for m in pat.finditer(src):
    sprites.append(dict(sys=m[1], name=m[2], layer=int(m[3]), tissue=np.array([int(m[4]), int(m[5]), int(m[6])], np.float32) / 255.0,
                        pulse=int(m[7]), atlas=tuple(int(m[i]) for i in range(9, 13)), origin=(float(m[13]), float(m[14]))))
atlas = cv2.cvtColor(cv2.imread(ATLAS, cv2.IMREAD_UNCHANGED), cv2.COLOR_BGRA2RGBA).astype(np.float32) / 255.0
print("sprites", len(sprites), "tpu", TPU, "atlas", atlas.shape)


def raw(s):
    ax, ay, w, h = s["atlas"]
    t = atlas[ay:ay + h, ax:ax + w]
    return t[..., 0] * 1.25, t[..., 1], t[..., 2], t[..., 3]


def look_main(s):
    sh, rim, sp, cv = raw(s)
    if s["layer"] == 0:
        base = np.array([0x18, 0x1f, 0x29], np.float32) / 255.0
        k = 0.55 + 0.7 * sh
        rgb = base[None, None, :] * k[..., None]
        return np.dstack([np.clip(rgb, 0, 1), cv * 0.96])
    rgb = s["tissue"][None, None, :] * sh[..., None] + (sp * 0.28)[..., None]
    return np.dstack([np.clip(rgb, 0, 1), cv])


def mix(a, b, t):
    return a + (b - a) * t


def look_alt(s):
    sh, rim, sp, cv = raw(s)
    if s["layer"] == 0:
        rgb = np.broadcast_to(np.array([0.64, 0.72, 0.81], np.float32), sh.shape + (3,))
        return np.dstack([rgb, cv * np.minimum(0.03 + 0.62 * rim ** 3, 1.0)])
    if s["layer"] == 1:
        i = 0.25 + 0.75 * rim ** 1.3
        c = mix(np.array([0.24, 0.33, 0.44], np.float32), np.array([0.80, 0.87, 0.94], np.float32), i[..., None])
        k = 0.80 + 0.20 * (sh / 1.25)
        return np.dstack([np.clip(c * k[..., None], 0, 1), cv * (0.24 + 0.50 * rim)])
    t = s["tissue"]
    g = 0.3 * t[0] + 0.55 * t[1] + 0.15 * t[2]
    c = mix(t, np.array([g * 0.9, g * 0.95, g * 1.08], np.float32), 0.35)
    k = 0.6 + 0.6 * rim
    return np.dstack([np.clip(c[None, None, :] * k[..., None], 0, 1), cv * (0.34 + 0.5 * rim ** 1.5)])


def pos(s):
    return int(round(s["origin"][0] * TPU)), int(round(s["origin"][1] * TPU))


def over(dst, img, x, y):
    h, w = img.shape[:2]
    d = dst[y:y + h, x:x + w]
    sa, da = img[..., 3:4], d[..., 3:4]
    oa = sa + da * (1 - sa)
    d[..., :3] = (img[..., :3] * sa + d[..., :3] * da * (1 - sa)) / np.maximum(oa, 1e-6)
    d[..., 3:4] = oa


def save(name, img, q=92):
    """img: float RGBA straight alpha. Lossy colour, lossless alpha."""
    u8 = (np.clip(img, 0, 1) * 255 + 0.5).astype(np.uint8)
    path = os.path.join(OUT, name)
    if name.endswith(".png"):
        cv2.imwrite(path, cv2.cvtColor(u8, cv2.COLOR_RGBA2BGRA), [cv2.IMWRITE_PNG_COMPRESSION, 9])
    else:
        cv2.imwrite(path, cv2.cvtColor(u8, cv2.COLOR_RGBA2BGRA), [cv2.IMWRITE_WEBP_QUALITY, q])
    return os.path.getsize(path)


def composite(items, look):
    x0 = min(pos(s)[0] for s in items)
    y0 = min(pos(s)[1] for s in items)
    x1 = max(pos(s)[0] + s["atlas"][2] for s in items)
    y1 = max(pos(s)[1] + s["atlas"][3] for s in items)
    canvas = np.zeros((y1 - y0, x1 - x0, 4), np.float32)
    for s in items:
        x, y = pos(s)
        over(canvas, look(s), x - x0, y - y0)
    return canvas, (x0, y0, x1 - x0, y1 - y0)


def slug(n):
    return re.sub(r"[^a-z0-9]+", "-", n.lower()).strip("-")


total = 0
man = {"tpu": TPU, "w": 240 * TPU, "h": 530 * TPU}
skin = [s for s in sprites if s["layer"] == 0][0]
x, y = pos(skin)
total += save("body.webp", look_main(skin), 95)
total += save("glass.webp", look_alt(skin), 92)
man["skin"] = {"x": x, "y": y, "w": skin["atlas"][2], "h": skin["atlas"][3]}

bones = [s for s in sprites if s["layer"] == 1]
organs = [s for s in sprites if s["layer"] == 2]
img, box = composite(bones, look_main)
total += save("skeleton.webp", img)
man["skeleton"] = dict(zip("xywh", box))
img, box2 = composite(bones, look_alt)
total += save("skeleton-xray.webp", img)
img, box = composite(organs, look_alt)
total += save("organs-xray.webp", img)
man["organsXray"] = dict(zip("xywh", box))

man["organs"] = []
seen = {}
for i, s in enumerate(organs):
    base = slug(s["name"])
    seen[base] = seen.get(base, 0) + 1
    fn = "org-%s%s.webp" % (base, "" if seen[base] == 1 else "-%d" % seen[base])
    total += save(fn, look_main(s))
    x, y = pos(s)
    man["organs"].append({"name": s["name"], "sys": s["sys"], "pulse": s["pulse"], "src": fn, "x": x, "y": y, "w": s["atlas"][2], "h": s["atlas"][3]})

# one outline around all organs of a system, as in figure.rs
NO_OUTLINE = {"Aorta", "Inferior vena cava", "Esophagus"}
COLORS = {"warn": (0xf2, 0xb6, 0x32), "crit": (0xff, 0x5b, 0x4d), "white": (0xff, 0xff, 0xff), "accent": (0x3a, 0x7b, 0xef)}
man["outlines"] = {}
for sysname in sorted({s["sys"] for s in organs}):
    members = [s for s in organs if s["sys"] == sysname and s["name"] not in NO_OUTLINE]
    pad = 3 * TPU
    x0 = min(pos(s)[0] for s in members) - pad
    y0 = min(pos(s)[1] for s in members) - pad
    x1 = max(pos(s)[0] + s["atlas"][2] for s in members) + pad
    y1 = max(pos(s)[1] + s["atlas"][3] for s in members) + pad
    cov = np.zeros((y1 - y0, x1 - x0), np.float32)
    for s in members:
        x, y = pos(s)
        c = raw(s)[3]
        d = cov[y - y0:y - y0 + c.shape[0], x - x0:x - x0 + c.shape[1]]
        np.maximum(d, c, out=d)
    # fill enclosed holes so the outline only runs around the outside
    solid = (cov >= 0.5).astype(np.uint8)
    ff = np.zeros((solid.shape[0] + 2, solid.shape[1] + 2), np.uint8)
    flood = solid.copy()
    cv2.floodFill(flood, ff, (0, 0), 2)
    cov[(flood != 2) & (solid == 0)] = 1.0
    r = TPU  # 2 texels at 2 texels/unit in the app
    k = np.ones((2 * r + 1, 2 * r + 1), np.uint8)
    edge = np.clip(cv2.dilate(cov, k) - cov, 0, 1)
    edge = cv2.GaussianBlur(edge, (0, 0), 0.6)
    for cname, col in COLORS.items():
        rgba = np.dstack([np.full_like(edge, col[0] / 255.0), np.full_like(edge, col[1] / 255.0), np.full_like(edge, col[2] / 255.0), edge])
        total += save("outline-%s-%s.png" % (sysname, cname), rgba)
    man["outlines"][sysname] = {"x": x0, "y": y0, "w": x1 - x0, "h": y1 - y0}

with open(os.path.join(OUT, "manifest.js"), "w", encoding="utf-8", newline="\n") as o:
    o.write("// Generated for the pitch deck from the BodyParts3D sprite atlas (see presentation/README.md).\n")
    o.write("// Coordinates are texels: %d per anatomy design unit (240 x 530 units).\n" % TPU)
    o.write("window.ANATOMY = " + json.dumps(man, indent=1) + ";\n")
print("files", len(os.listdir(OUT)), "total bytes", total)
print(json.dumps({k: v for k, v in man.items() if k not in ("organs",)}, indent=0)[:900])
