#!/usr/bin/env python3
"""Draws the Wire Studio KH icon: warm paper square, one vermilion seal ring and a
dispatch line. No dependencies beyond zlib/struct. Run with -I from scripts/.."""

import struct
import zlib
from pathlib import Path

PAPER = (250, 248, 244, 255)
SEAL = (166, 64, 44, 255)
DIM = (196, 140, 128, 255)


def pixel(x, y, size):
    s = size / 1024
    cx = cy = size / 2
    inset = 54 * s
    if x < inset or y < inset or x > size - inset or y > size - inset:
        return (0, 0, 0, 0)
    dx, dy = x - cx, y - cy
    d = (dx * dx + dy * dy) ** 0.5
    ring = 300 * s
    if abs(d - ring) <= 17 * s:  # seal ring
        return SEAL
    if abs(dy) <= 26 * s and abs(dx) <= ring * 0.78:  # dispatch line
        return SEAL
    if (x - (cx + ring * 0.9)) ** 2 + (y - cy) ** 2 <= (26 * s) ** 2:  # dot
        return DIM
    return PAPER


def write_png(path, size):
    rows = []
    for y in range(size):
        row = bytearray([0])
        for x in range(size):
            row += bytes(pixel(x, y, size))
        rows.append(bytes(row))
    raw = b"".join(rows)

    def chunk(tag, data):
        return (
            struct.pack(">I", len(data))
            + tag
            + data
            + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)
        )

    png = (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", struct.pack(">IIBBBBB", size, size, 8, 6, 0, 0, 0))
        + chunk(b"IDAT", zlib.compress(raw, 9))
        + chunk(b"IEND", b"")
    )
    path.write_bytes(png)


def main():
    here = Path(__file__).resolve().parent.parent
    out = here / "src-tauri" / "icons"
    out.mkdir(parents=True, exist_ok=True)
    for name, size in [
        ("icon.png", 1024),
        ("128x128@2x.png", 256),
        ("128x128.png", 128),
        ("32x32.png", 32),
    ]:
        write_png(out / name, size)
        print(f"wrote {out / name}")


if __name__ == "__main__":
    main()
