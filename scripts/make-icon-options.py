#!/usr/bin/env python3
"""Three icon options for Wire Studio KH, rendered to PNG at 256 and 32 px.

House language, taken from the siblings: rounded square, one flat ground, one
thick single-colour glyph, generous margin, no text. Motif is a dispatch line.
Run with: python3 -I scripts/make-icon-options.py
"""
import math
import struct
import zlib
from pathlib import Path

S = 4                      # supersample factor
N = 1024                   # design size
CREAM = (250, 246, 238)
INK = (42, 33, 28)
VERMILION = (166, 64, 44)
PALE = (240, 226, 214)


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


def disc(px, py, c, r):
    return (px - c[0]) ** 2 + (py - c[1]) ** 2 <= r * r


def render(draw, size):
    """draw(u, v) -> (r,g,b) for design coords u,v in 0..1024."""
    step = N / size
    out = bytearray()
    for y in range(size):
        out.append(0)  # filter byte
        for x in range(size):
            acc = [0, 0, 0]
            for sy in range(S):
                for sx in range(S):
                    u = (x + (sx + 0.5) / S) * step
                    v = (y + (sy + 0.5) / S) * step
                    r, g, b = draw(u, v)
                    acc[0] += r
                    acc[1] += g
                    acc[2] += b
            n = S * S
            out += bytes((acc[0] // n, acc[1] // n, acc[2] // n, 255))
    raw = bytes(out)

    def chunk(tag, data):
        return (struct.pack(">I", len(data)) + tag + data
                + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF))

    return (b"\x89PNG\r\n\x1a\n"
            + chunk(b"IHDR", struct.pack(">IIBBBBB", size, size, 8, 6, 0, 0, 0))
            + chunk(b"IDAT", zlib.compress(raw, 9))
            + chunk(b"IEND", b""))


def ground(colour):
    def draw(x, y):
        if not rrect_inside(x, y, N, 92, 224):
            return (0, 0, 0)
        return colour
    return draw


def option_a(x, y):
    """Vermilion ground, cream dispatch line into an arrowhead, node at the start."""
    base = ground(VERMILION)
    bg = base(x, y)
    if bg == (0, 0, 0):
        return (0, 0, 0)
    w = 62
    pts = [(268, 560), (470, 560), (610, 420), (748, 420)]
    for a, b in zip(pts, pts[1:]):
        if capsule(x, y, a, b, w):
            return CREAM
    if capsule(x, y, (748, 420), (668, 352), w) or capsule(x, y, (748, 420), (676, 488), w):
        return CREAM
    if disc(x, y, (268, 560), 78):
        return PALE
    return bg


def option_b(x, y):
    """Ink ground, cream paper-plane from three strokes with a dashed trail."""
    base = ground(INK)
    bg = base(x, y)
    if bg == (0, 0, 0):
        return (0, 0, 0)
    w = 54
    tip, left, right = (772, 352), (300, 470), (612, 620)
    if capsule(x, y, tip, left, w) or capsule(x, y, tip, right, w) or capsule(x, y, left, right, w):
        return CREAM
    if capsule(x, y, (330, 640), (420, 610), 40) or capsule(x, y, (452, 600), (542, 570), 40):
        return VERMILION
    return bg


def option_c(x, y):
    """Warm ink ground, cream wire between two nodes, one node vermilion."""
    base = ground(INK)
    bg = base(x, y)
    if bg == (0, 0, 0):
        return (0, 0, 0)
    w = 58
    pts = [(292, 640), (292, 420), (700, 420), (700, 598)]
    for a, b in zip(pts, pts[1:]):
        if capsule(x, y, a, b, w):
            return CREAM
    if disc(x, y, (292, 690), 74):
        return CREAM
    if disc(x, y, (700, 648), 74):
        return VERMILION
    return bg


def main():
    out = Path(__file__).resolve().parent.parent / "docs" / "icon-options"
    out.mkdir(parents=True, exist_ok=True)
    for name, fn in (("a-dispatch-line", option_a),
                     ("b-paper-plane", option_b),
                     ("c-node-route", option_c)):
        for size in (256, 32):
            p = out / f"{name}-{size}.png"
            p.write_bytes(render(fn, size))
            print(f"wrote {p}")


if __name__ == "__main__":
    main()
