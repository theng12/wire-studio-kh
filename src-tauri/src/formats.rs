//! Per-platform format presets. One place declares sizes, limits and safe zones.

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SafeZone {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Preset {
    pub id: &'static str,
    pub platform: &'static str,
    pub label: &'static str,
    pub width: u32,
    pub height: u32,
    pub ratio: &'static str,
    pub safe_zone: SafeZone,
    pub max_text_length: usize,
    pub media: &'static str,
}

const PRESETS: &[Preset] = &[
    Preset {
        id: "yt-16x9",
        platform: "youtube",
        label: "YouTube landscape",
        width: 1920,
        height: 1080,
        ratio: "16:9",
        safe_zone: SafeZone { top: 0.05, right: 0.05, bottom: 0.05, left: 0.05 },
        max_text_length: 5000,
        media: "video",
    },
    Preset {
        id: "yt-shorts",
        platform: "youtube",
        label: "YouTube Shorts",
        width: 1080,
        height: 1920,
        ratio: "9:16",
        // Shorts overlays occupy the top and bottom of the frame.
        safe_zone: SafeZone { top: 0.10, right: 0.05, bottom: 0.20, left: 0.05 },
        max_text_length: 5000,
        media: "video",
    },
    Preset {
        id: "fb-1x1",
        platform: "facebook_page",
        label: "Facebook square",
        width: 1080,
        height: 1080,
        ratio: "1:1",
        safe_zone: SafeZone { top: 0.05, right: 0.05, bottom: 0.05, left: 0.05 },
        max_text_length: 63206,
        media: "image",
    },
    Preset {
        id: "fb-4x5",
        platform: "facebook_page",
        label: "Facebook portrait",
        width: 1080,
        height: 1350,
        ratio: "4:5",
        safe_zone: SafeZone { top: 0.05, right: 0.05, bottom: 0.05, left: 0.05 },
        max_text_length: 63206,
        media: "image",
    },
    Preset {
        // 1200x630 is Meta's link-card size (40:21), not a true 16:9 frame.
        id: "fb-16x9",
        platform: "facebook_page",
        label: "Facebook link card",
        width: 1200,
        height: 630,
        ratio: "40:21",
        safe_zone: SafeZone { top: 0.05, right: 0.05, bottom: 0.05, left: 0.05 },
        max_text_length: 63206,
        media: "image",
    },
    Preset {
        id: "tg-text",
        platform: "telegram",
        label: "Telegram text",
        width: 0,
        height: 0,
        ratio: "none",
        safe_zone: SafeZone { top: 0.0, right: 0.0, bottom: 0.0, left: 0.0 },
        max_text_length: 4096,
        media: "none",
    },
];

pub fn registry() -> &'static [Preset] {
    PRESETS
}

pub fn presets_for(platform: &str) -> Vec<&'static Preset> {
    PRESETS.iter().filter(|p| p.platform == platform).collect()
}

pub fn get(id: &str) -> Option<&'static Preset> {
    PRESETS.iter().find(|p| p.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn registry_ids_are_unique_and_lookups_match_platforms() {
        let ids: HashSet<&str> = registry().iter().map(|p| p.id).collect();
        assert_eq!(ids.len(), registry().len(), "preset ids must be unique");
        assert_eq!(
            presets_for("youtube").iter().map(|p| p.id).collect::<Vec<_>>(),
            vec!["yt-16x9", "yt-shorts"]
        );
        assert_eq!(
            presets_for("facebook_page").iter().map(|p| p.id).collect::<Vec<_>>(),
            vec!["fb-1x1", "fb-4x5", "fb-16x9"]
        );
        assert_eq!(presets_for("telegram").len(), 1);
        assert!(presets_for("instagram").is_empty(), "Instagram is shelved");
        assert_eq!(get("tg-text").map(|p| p.max_text_length), Some(4096));
        assert_eq!(get("yt-shorts").map(|p| (p.width, p.height)), Some((1080, 1920)));
        assert!(get("nope").is_none());
        assert!(registry().iter().all(|p| {
            let z = p.safe_zone;
            [z.top, z.right, z.bottom, z.left].iter().all(|v| (0.0..=1.0).contains(v))
        }));
    }
}
