#!/usr/bin/env python3
"""The owner's pick: option A, the dispatch line, vermilion ground.

Drawn at every size rather than scaled down — the stroke, the arrowhead and the
start node are re-tuned per size tier, because 16 px is where a thick line either
holds or smears. Writes the Tauri PNGs, an .icns, and the in-app mark SVG.
Run with: python3 -I scripts/make-icon.py
"""
import math
import struct
import subprocess
import tempfile
import zlib
from pathlib import Path

N = 1024
CREAM = (250, 246, 238)
VERMILION = (166, 64, 44)
PALE = (240, 226, 214)
MARGIN = 92
RADIUS = 224


def rrect_inside(x, y, n, margin, radius):
    if x < margin or y < margin or x > n - margin or y > n - margin:
        return False
    cx = min(max(x, margin + radius), n - margin - radius)
    cy = min(max(y, margin + radius), n - margin - radius)
    return (x - cx) ** 2 + (y - cy) ** 2 <= radius * radius or (
        margin + radius <= x <= n - margin - radius
        or margin + radius <= y <= n - margin - radius
    )


def seg_dist(px, py, ax, ay, bx, by):
    dx, dy = bx - ax, by - ay
    length2 = dx * dx + dy * dy
    if length2 == 0:
        return math.hypot(px - ax, py - ay)
    t = max(0.0, min(1.0, ((px - ax) * dx + (py - ay) * dy) / length2))
    return math.hypot(px - (ax + t * dx), py - (ay + t * dy))


def capsule(px, py, a, b, w):
    return seg_dist(px, py, a[0], a[1], b[0], b[1]) <= w / 2


# Geometry per tier: (polyline, stroke, node radius, barb length, barb spread°).
# The arrow's barbs are computed from the last segment, so every tier aims alike.
TIERS = {
    "large": ([(268, 560), (470, 560), (610, 420), (748, 420)], 62, 78, 150, 42),
    "medium": ([(268, 560), (470, 560), (610, 420), (748, 420)], 76, 0, 170, 42),
    "small": ([(300, 640), (520, 640), (720, 470)], 120, 0, 210, 45),
    "tiny": ([(290, 660), (720, 450)], 170, 0, 280, 45),
}


def tier_for(size):
    if size >= 128:
        return TIERS["large"]
    if size >= 48:
        return TIERS["medium"]
    if size >= 24:
        return TIERS["small"]
    return TIERS["tiny"]


def barbs(points, length, spread_deg):
    tip = points[-1]
    prev = points[-2]
    dx, dy = tip[0] - prev[0], tip[1] - prev[1]
    d = math.hypot(dx, dy) or 1.0
    ux, uy = dx / d, dy / d
    out = []
    for sign in (1, -1):
        a = math.radians(spread_deg) * sign
        rx = ux * math.cos(a) - uy * math.sin(a)
        ry = ux * math.sin(a) + uy * math.cos(a)
        out.append(((tip[0] - rx * length, tip[1] - ry * length), tip))
    return out


def draw(x, y, size):
    """RGBA for one sample at design coords x,y for an icon of `size` px."""
    if not rrect_inside(x, y, N, MARGIN, RADIUS):
        return (0, 0, 0, 0)
    points, stroke, node_r, barb_len, spread = tier_for(size)
    for a, b in zip(points, points[1:]):
        if capsule(x, y, a, b, stroke):
            return CREAM + (255,)
    for a, b in barbs(points, barb_len, spread):
        if capsule(x, y, a, b, stroke):
            return CREAM + (255,)
    if node_r and (x - points[0][0]) ** 2 + (y - points[0][1]) ** 2 <= node_r * node_r:
        return PALE + (255,)
    return VERMILION + (255,)


def render(size):
    supersample = 4 if size <= 256 else 2
    step = N / size
    rows = []
    for py in range(size):
        row = bytearray([0])
        for px in range(size):
            ar = ag = ab = aa = 0
            for sy in range(supersample):
                for sx in range(supersample):
                    x = (px + (sx + 0.5) / supersample) * step
                    y = (py + (sy + 0.5) / supersample) * step
                    r, g, b, a = draw(x, y, size)
                    ar += r * a
                    ag += g * a
                    ab += b * a
                    aa += a
            n = supersample * supersample
            alpha = aa // n
            if alpha == 0:
                row += bytes((0, 0, 0, 0))
            else:
                row += bytes((ar // aa, ag // aa, ab // aa, alpha))
        rows.append(bytes(row))
    raw = b"".join(rows)

    def chunk(tag, data):
        return (struct.pack(">I", len(data)) + tag + data
                + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF))

    return (b"\x89PNG\r\n\x1a\n"
            + chunk(b"IHDR", struct.pack(">IIBBBBB", size, size, 8, 6, 0, 0, 0))
            + chunk(b"IDAT", zlib.compress(raw, 9))
            + chunk(b"IEND", b""))


def mark_svg():
    """The same mark, vector, for the window header, About and empty states."""
    return f"""<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {N} {N}" role="img" aria-label="Wire Studio mark">
  <rect x="{MARGIN}" y="{MARGIN}" width="{N - 2 * MARGIN}" height="{N - 2 * MARGIN}" rx="{RADIUS}" fill="#a6402c"/>
  <path d="M268 560 H470 L610 420 H748" fill="none" stroke="#faf6ee" stroke-width="62" stroke-linecap="round" stroke-linejoin="round"/>
  <path d="M748 420 L668 352 M748 420 L676 488" fill="none" stroke="#faf6ee" stroke-width="62" stroke-linecap="round"/>
  <circle cx="268" cy="560" r="78" fill="#f0e2d6"/>
</svg>
"""


def main():
    root = Path(__file__).resolve().parent.parent
    icons = root / "src-tauri" / "icons"
    icons.mkdir(parents=True, exist_ok=True)

    rendered = {}
    for size in (16, 32, 64, 128, 256, 512, 1024):
        rendered[size] = render(size)
        print(f"rendered {size}px")

    for name, size in [
        ("icon.png", 1024),
        ("128x128@2x.png", 256),
        ("128x128.png", 128),
        ("32x32.png", 32),
    ]:
        (icons / name).write_bytes(rendered[size])
        print(f"wrote {icons / name}")

    # .icns: every slot gets a render of its own size, never an upscale of another.
    with tempfile.TemporaryDirectory() as tmp:
        iconset = Path(tmp) / "icon.iconset"
        iconset.mkdir()
        for slot in (16, 32, 128, 256, 512):
            (iconset / f"icon_{slot}x{slot}.png").write_bytes(rendered[slot])
            (iconset / f"icon_{slot}x{slot}@2x.png").write_bytes(rendered[slot * 2])
        subprocess.run(["iconutil", "-c", "icns", str(iconset), "-o", str(icons / "icon.icns")], check=True)
        print(f"wrote {icons / 'icon.icns'}")

    mark = root / "ui" / "logo.svg"
    mark.write_text(mark_svg())
    print(f"wrote {mark}")


if __name__ == "__main__":
    main()
