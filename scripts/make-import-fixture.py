#!/usr/bin/env python3
"""Regenerate the Story Still Posts import fixture. No third-party packages."""

import binascii
import hashlib
import json
import random
import struct
import zlib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1] / "src-tauri/tests/fixtures/story-channel"
POSTS = ROOT / "still-posts/posts"
PER_SIZE = POSTS / "post-0001"
SIMPLE = POSTS / "post-0002"
NO_FINAL = POSTS / "post-0003"


def png(seed, size=20):
    rng = random.Random(seed)
    rows = bytearray()
    for _ in range(size):
        rows.append(0)  # PNG filter: none
        for _ in range(size):
            rows.extend(rng.randrange(256) for _ in range(3))  # RGB, no alpha

    def chunk(kind, payload):
        body = kind + payload
        return (
            struct.pack(">I", len(payload))
            + body
            + struct.pack(">I", binascii.crc32(body) & 0xFFFFFFFF)
        )

    header = struct.pack(">IIBBBBB", size, size, 8, 2, 0, 0, 0)
    return (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", header)
        + chunk(b"IDAT", zlib.compress(bytes(rows), 9))
        + chunk(b"IEND", b"")
    )


def write(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    if isinstance(value, bytes):
        path.write_bytes(value)
    else:
        path.write_text(
            json.dumps(value, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
        )


def image_meta(directory, filename, aspect=None):
    meta = {
        "filename": filename,
        "rendered_at": "2026-11-21T01:30:00.000Z",
        "sha256": hashlib.sha256((directory / filename).read_bytes()).hexdigest(),
        "model_id": "fixture-model-1",
        "model_label": "Fixture Model 1",
        "prompt": "A quiet desk in soft morning light; no lettering.",
        "reference_character_ids": ["char-fixture"],
        "reference_names": ["Fixture Character"],
    }
    if aspect:
        meta["aspect"] = aspect
    return meta


for directory, filename, seed in [
    (PER_SIZE, "p0.png", "still-posts-p0"),
    (PER_SIZE, "p0-1x1.png", "still-posts-p0-1x1"),
    (PER_SIZE, "p0-9x16.png", "still-posts-p0-9x16"),
    (PER_SIZE, "final-p0-1x1.png", "still-posts-final-1x1"),
    (PER_SIZE, "final-p0-9x16.png", "still-posts-final-9x16"),
    (SIMPLE, "p0.png", "still-posts-simple-p0"),
    (SIMPLE, "final-p0-1x1.png", "still-posts-simple-final"),
    (NO_FINAL, "p0.png", "still-posts-no-final-p0"),
]:
    write(directory / filename, png(seed))

write(
    PER_SIZE / "post.json",
    {
        "id": "post-0001",
        "batch_id": "batch-20261121-FIXTURE",
        "created_at": "2026-11-21T01:02:03.000Z",
        "format_id": "quiet-ink",
        "option_id": "centred-quote",
        "language": "km",
        "character_ids": ["char-fixture"],
        "caption": "ចំណេះដឹងតូចមួយ សម្រាប់ថ្ងៃនេះ។",
        "panels": [
            {
                "id": "panel-0",
                "role": "single",
                "text": "អានឱ្យយឺតៗ ហើយគិតមួយភ្លែត។",
                "scene_prompt": "A quiet desk, soft morning light, no text in the image.",
                "image": image_meta(PER_SIZE, "p0.png"),
                "images": {
                    "1:1": image_meta(PER_SIZE, "p0-1x1.png", "1:1"),
                    "9:16": image_meta(PER_SIZE, "p0-9x16.png", "9:16"),
                },
            }
        ],
        "finals": [
            {
                "aspect": "1:1",
                "panel_id": "panel-0",
                "filename": "final-p0-1x1.png",
                "rendered_at": "2026-11-21T01:40:00.000Z",
                "text": "អានឱ្យយឺតៗ ហើយគិតមួយភ្លែត។",
                "style_hash": "style-fixture-1x1",
                "font_id": "kantumruy-pro",
            },
            {
                "aspect": "9:16",
                "panel_id": "panel-0",
                "filename": "final-p0-9x16.png",
                "rendered_at": "2026-11-21T01:41:00.000Z",
                "text": "អានឱ្យយឺតៗ ហើយគិតមួយភ្លែត។",
                "style_hash": "style-fixture-9x16",
                "font_id": "kantumruy-pro",
            },
        ],
        "aspects": ["1:1", "9:16"],
        "art_mode": "per-size",
        "font_id": "kantumruy-pro",
        "published_at": None,
    },
)

write(
    SIMPLE / "post.json",
    {
        "id": "post-0002",
        "batch_id": "batch-20261121-FIXTURE",
        "created_at": "2026-11-21T03:00:00.000Z",
        "format_id": "quiet-ink",
        "option_id": "centred-quote",
        "language": "km",
        "caption": "ថ្ងៃនេះ សូមឈប់មួយភ្លែត។",
        "panels": [
            {
                "id": "panel-0",
                "text": "ឈប់មួយភ្លែត ហើយដកដង្ហើមធំ។",
                "image": image_meta(SIMPLE, "p0.png"),
            }
        ],
        "finals": [
            {
                "aspect": "1:1",
                "panel_id": "panel-0",
                "filename": "final-p0-1x1.png",
                "text": "ឈប់មួយភ្លែត ហើយដកដង្ហើមធំ។",
                "style_hash": "style-fixture-simple",
                "font_id": "kantumruy-pro",
            }
        ],
        "aspects": ["1:1"],
        "font_id": "kantumruy-pro",
    },
)

write(
    NO_FINAL / "post.json",
    {
        "id": "post-0003",
        "batch_id": "batch-20261121-FIXTURE",
        "created_at": "2026-11-21T04:00:00.000Z",
        "format_id": "quiet-ink",
        "option_id": "centred-quote",
        "language": "km",
        "caption": "គ្មានរូបចុងក្រោយទេ។",
        "panels": [
            {
                "id": "panel-0",
                "text": "គ្មានរូបចុងក្រោយទេ។",
                "image": image_meta(NO_FINAL, "p0.png"),
            }
        ],
        "finals": [],
        "aspects": ["1:1"],
        "font_id": "kantumruy-pro",
    },
)

write(
    ROOT / "project.json",
    {"slug": "test-channel-kh", "name": "Test Channel KH", "niche_id": "still-posts"},
)

write(
    ROOT / "config.json",
    {
        "still_post_last_preset": "quiet-ink",
        "still_post_presets": [
            {
                "id": "quiet-ink",
                "name": "Quiet ink",
                "data": {
                    "font_pools": {
                        "latn": {
                            "usage_mode": "serial",
                            "allow_repeat": False,
                            "fonts": [
                                {"font_id": "inter", "weight": 1},
                                {"font_id": "noto-sans", "weight": 2},
                            ],
                        },
                        "khmr": {
                            "usage_mode": "weighted",
                            "allow_repeat": True,
                            "fonts": [
                                {"font_id": "kantumruy-pro", "weight": 3},
                                {"font_id": "noto-sans-khmer", "weight": 1},
                            ],
                        },
                    }
                },
            },
            {
                "id": "bold-ink",
                "name": "Bold ink",
                "data": {
                    "font_pools": {
                        "latn": {
                            "usage_mode": "weighted",
                            "allow_repeat": True,
                            "fonts": [{"font_id": "inter-bold", "weight": 1}],
                        }
                    }
                },
            },
        ],
        "still_posts": {"character_ids": ["char-fixture"], "handle": "@fixture"},
    },
)

# Production has no font-pool cursor file; do not ship one in the fixture.
cursor = ROOT / "still-posts/font-pool-cursor.json"
if cursor.exists():
    cursor.unlink()

print(f"wrote {ROOT}")
