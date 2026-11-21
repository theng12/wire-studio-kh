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
GOOD = ROOT / "still-posts/posts/post-0001"
BAD = ROOT / "still-posts/posts/post-0002"


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


write(GOOD / "p0.png", png("still-posts-p0"))
write(GOOD / "final-p0-1x1.png", png("still-posts-final-1x1"))
write(GOOD / "final-p0-9x16.png", png("still-posts-final-9x16"))
panel_sha = hashlib.sha256((GOOD / "p0.png").read_bytes()).hexdigest()

write(
    GOOD / "post.json",
    {
        "id": "post-0001",
        "batch_id": "batch-20261121-FIXTURE",
        "created_at": "2026-11-21T01:02:03.000Z",
        "updated_at": "2026-11-21T02:03:04.000Z",
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
                "image": {
                    "filename": "p0.png",
                    "rendered_at": "2026-11-21T01:30:00.000Z",
                    "sha256": panel_sha,
                    "model_id": "fixture-model-1",
                    "model_label": "Fixture Model 1",
                    "prompt": "A quiet desk in soft morning light; no lettering.",
                    "reference_character_ids": ["char-fixture"],
                    "reference_names": ["Fixture Character"],
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
        "art_mode": "single",
        "font_id": "kantumruy-pro",
        "published_at": None,
    },
)

write(
    BAD / "post.json",
    {
        "id": "post-0002",
        "batch_id": "batch-20261121-FIXTURE",
        "created_at": "2026-11-21T03:00:00.000Z",
        "format_id": "quiet-ink",
        "option_id": "centred-quote",
        "language": "km",
        "caption": "រូបភាពនេះបាត់ហើយ។",
        "panels": [
            {
                "id": "panel-0",
                "text": "រូបភាពនេះបាត់ហើយ។",
                "image": {"filename": "p0.png", "sha256": "0" * 64},
            }
        ],
        "finals": [
            {
                "aspect": "1:1",
                "panel_id": "panel-0",
                "filename": "final-p0-1x1.png",
                "text": "រូបភាពនេះបាត់ហើយ។",
                "style_hash": "style-fixture-missing",
                "font_id": "kantumruy-pro",
            }
        ],
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

write(
    ROOT / "still-posts/font-pool-cursor.json",
    {
        "cursors": {
            "latn": {"last_font_id": "inter", "last_index": 0},
            "khmr": {"last_font_id": "kantumruy-pro", "last_index": 0},
        }
    },
)

print(f"wrote {ROOT}")
