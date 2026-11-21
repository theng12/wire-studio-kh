//! Read-only scan of a Story Studio KH channel into an import plan.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Skipped {
    pub path: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceSnapshot {
    pub path: String,
    pub mtime_ns: u64,
    pub size: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlannedImage {
    pub filename: String,
    pub sha256: String,
    pub aspect: Option<String>,
    #[serde(skip)]
    pub path: PathBuf,
    pub rendered_at: Option<String>,
    pub model_id: Option<String>,
    pub model_label: Option<String>,
    pub prompt: Option<String>,
    pub reference_character_ids: Vec<String>,
    pub reference_names: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlannedPanel {
    pub id: String,
    pub text: String,
    pub images: Vec<PlannedImage>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlannedFinal {
    pub aspect: String,
    pub panel_id: Option<String>,
    pub filename: String,
    pub font_id: Option<String>,
    pub style_hash: Option<String>,
    pub text: String,
    pub rendered_at: Option<String>,
    #[serde(skip)]
    pub path: PathBuf,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlannedPost {
    pub id: String,
    pub text: String,
    pub created_at: String,
    pub format_id: String,
    pub option_id: String,
    pub language: String,
    pub art_mode: String,
    pub aspects: Vec<String>,
    pub font_id: Option<String>,
    pub character_ids: Vec<String>,
    pub panels: Vec<PlannedPanel>,
    pub finals: Vec<PlannedFinal>,
    pub content_hash: String,
    /// The original record, kept verbatim for `recipes/<id>.json`.
    pub raw: Value,
    #[serde(skip)]
    pub source_dir: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlannedFontChoice {
    pub font_id: String,
    pub weight: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlannedFont {
    pub script: String,
    pub scripts: Vec<String>,
    pub usage_mode: String,
    pub allow_repeat: bool,
    pub fonts: Vec<PlannedFontChoice>,
    pub weights: Vec<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImportPlan {
    pub channel_slug: String,
    pub channel_name: String,
    pub posts: Vec<PlannedPost>,
    pub fonts: Vec<PlannedFont>,
    pub skipped: Vec<Skipped>,
    pub source_files: usize,
    /// Exact files read by `scan`, used as the read-only proof in `apply`.
    pub sources: Vec<SourceSnapshot>,
}

pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(digest.len() * 2);
    for &byte in digest.iter() {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

#[derive(Default)]
struct Scanner {
    sources: Vec<SourceSnapshot>,
    seen: BTreeSet<String>,
}

impl Scanner {
    fn read(&mut self, path: &Path) -> Result<Vec<u8>, String> {
        let bytes = fs::read(path).map_err(|_| format!("{} cannot be read", path.display()))?;
        let key = path.to_string_lossy().into_owned();
        if self.seen.insert(key.clone()) {
            let meta = fs::metadata(path).map_err(|_| format!("{} cannot be read", path.display()))?;
            let modified = meta
                .modified()
                .map_err(|_| format!("{} has no modification time", path.display()))?;
            let mtime_ns = modified
                .duration_since(UNIX_EPOCH)
                .map_err(|_| format!("{} has an invalid modification time", path.display()))?
                .as_nanos() as u64;
            self.sources.push(SourceSnapshot {
                path: key,
                mtime_ns,
                size: bytes.len() as u64,
                sha256: sha256_hex(&bytes),
            });
        }
        Ok(bytes)
    }
}

fn parse_json(bytes: &[u8], label: &str) -> Result<Value, String> {
    serde_json::from_slice(bytes).map_err(|_| format!("{label} cannot be parsed"))
}

fn str_field<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(Value::as_str)
}

fn opt_str(value: &Value, key: &str) -> Option<String> {
    str_field(value, key).map(str::to_string)
}

fn str_array(value: &Value, key: &str) -> Vec<String> {
    value
        .get(key)
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

pub(crate) fn safe_filename(name: &str) -> bool {
    !name.is_empty()
        && !name.contains('/')
        && !name.contains('\\')
        && Path::new(name).file_name().and_then(|part| part.to_str()) == Some(name)
}

fn aspect_from_filename(name: &str) -> Option<String> {
    let suffix = name.strip_suffix(".png")?.rsplit_once('-')?.1;
    let (width, height) = suffix.split_once('x')?;
    if matches!(suffix, "1x1" | "9x16" | "16x9") {
        Some(format!("{width}:{height}"))
    } else {
        None
    }
}

/// Read `project.json`, `config.json`, every `post-*/post.json` and each declared art file.
/// Nothing in the channel is written, renamed or moved. The font cursor is not channel state
/// in Production, so it is never read.
pub fn scan(channel_dir: &Path) -> Result<ImportPlan, String> {
    let mut scanner = Scanner::default();

    // Production channels carry config.json; project.json and the font cursor are
    // not guaranteed (verified on story-mini 2026-11-21). A thin channel still imports.
    let dir_slug = channel_dir
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "channel".to_string());

    let project = match scanner.read(&channel_dir.join("project.json")) {
        Ok(raw) => parse_json(&raw, "project.json").ok(),
        Err(_) => None,
    };
    let config = match scanner.read(&channel_dir.join("config.json")) {
        Ok(raw) => parse_json(&raw, "config.json").ok(),
        Err(_) => None,
    };

    let channel_slug = project
        .as_ref()
        .and_then(|p| str_field(p, "slug"))
        .map(str::to_string)
        .unwrap_or_else(|| dir_slug.clone());
    let channel_name = project
        .as_ref()
        .and_then(|p| str_field(p, "name"))
        .map(str::to_string)
        .unwrap_or_else(|| channel_slug.clone());

    let fonts = match &config {
        Some(config) => plan_fonts(config),
        None => Vec::new(),
    };

    let posts_dir = channel_dir.join("still-posts/posts");
    let entries = fs::read_dir(&posts_dir)
        .map_err(|_| format!("{} cannot be read", posts_dir.display()))?;
    let mut post_dirs = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|_| format!("{} cannot be read", posts_dir.display()))?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with("post-") && entry.path().is_dir() {
            post_dirs.push(entry.path());
        }
    }
    post_dirs.sort();

    let mut posts = Vec::new();
    let mut skipped = Vec::new();
    for dir in post_dirs {
        match plan_post(&dir, &mut scanner) {
            Ok(planned) => {
                posts.push(planned.post);
                skipped.extend(planned.skipped);
            }
            Err(reason) => skipped.push(Skipped {
                path: dir.join("post.json").to_string_lossy().into_owned(),
                reason,
            }),
        }
    }

    let source_files = scanner.sources.len();
    Ok(ImportPlan {
        channel_slug,
        channel_name,
        posts,
        fonts,
        skipped,
        source_files,
        sources: scanner.sources,
    })
}

struct PostPlan {
    post: PlannedPost,
    skipped: Vec<Skipped>,
}

fn plan_post(dir: &Path, scanner: &mut Scanner) -> Result<PostPlan, String> {
    let mut file_skips = Vec::new();
    let raw_bytes = scanner
        .read(&dir.join("post.json"))
        .map_err(|_| "post.json cannot be read".to_string())?;
    let raw = parse_json(&raw_bytes, "post.json")?;

    let id = str_field(&raw, "id")
        .ok_or_else(|| "post id missing".to_string())?
        .to_string();
    let caption = str_field(&raw, "caption").unwrap_or_default();
    let panels_raw = raw
        .get("panels")
        .and_then(Value::as_array)
        .ok_or_else(|| "no panels".to_string())?;
    if panels_raw.is_empty() {
        return Err("no panels".to_string());
    }
    let first_panel_text = str_field(&panels_raw[0], "text").unwrap_or_default();
    let text = if !caption.trim().is_empty() {
        caption.to_string()
    } else if !first_panel_text.trim().is_empty() {
        first_panel_text.to_string()
    } else {
        return Err("post.json has no text: caption and panel text are empty".to_string());
    };

    let finals_raw = raw
        .get("finals")
        .and_then(Value::as_array)
        .filter(|items| !items.is_empty())
        .ok_or_else(|| "no final image in post.json".to_string())?;

    let mut panels = Vec::new();
    for (index, panel) in panels_raw.iter().enumerate() {
        let panel_id = str_field(panel, "id").unwrap_or_default().to_string();
        let mut declared: Vec<Value> = Vec::new();
        if let Some(image) = panel.get("image").and_then(Value::as_object) {
            declared.push(Value::Object(image.clone()));
        }
        if let Some(images) = panel.get("images").and_then(Value::as_object) {
            for (aspect, image) in images {
                if let Some(object) = image.as_object() {
                    let mut value = Value::Object(object.clone());
                    if str_field(&value, "aspect").is_none() {
                        if let Some(map) = value.as_object_mut() {
                            map.insert("aspect".to_string(), Value::String(aspect.clone()));
                        }
                    }
                    declared.push(value);
                }
            }
        }

        let mut candidates: Vec<(String, Option<Value>)> = Vec::new();
        for image in &declared {
            let Some(filename) = str_field(image, "filename") else {
                file_skips.push(Skipped {
                    path: dir.join("post.json").to_string_lossy().into_owned(),
                    reason: "image has no filename in post.json".to_string(),
                });
                continue;
            };
            if !safe_filename(filename) {
                return Err(format!("unsafe image filename in post.json: {filename}"));
            }
            let path = dir.join(filename);
            if !path.is_file() {
                file_skips.push(Skipped {
                    path: path.to_string_lossy().into_owned(),
                    reason: format!("image {filename} missing"),
                });
                continue;
            }
            candidates.push((filename.to_string(), Some(image.clone())));
        }

        let panel_index = panel_id
            .strip_prefix("panel-")
            .and_then(|suffix| suffix.parse::<usize>().ok())
            .unwrap_or(index);
        let mut names = vec![format!("p{panel_index}.png")];
        for aspect in ["1:1", "9:16", "16:9"] {
            names.push(format!("p{panel_index}-{}.png", aspect.replace(':', "x")));
        }
        for name in names {
            if candidates.iter().any(|(existing, _)| existing == &name) {
                continue;
            }
            if dir.join(&name).is_file() {
                candidates.push((name, None));
            }
        }

        let fallback = declared.first();
        let mut images = Vec::new();
        for (filename, declaration) in candidates {
            let path = dir.join(&filename);
            let bytes = match scanner.read(&path) {
                Ok(bytes) => bytes,
                Err(_) => {
                    file_skips.push(Skipped {
                        path: path.to_string_lossy().into_owned(),
                        reason: format!("image {filename} cannot be read"),
                    });
                    continue;
                }
            };
            let actual = sha256_hex(&bytes);
            if let Some(expected) = declaration
                .as_ref()
                .and_then(|image| str_field(image, "sha256"))
            {
                if !actual.eq_ignore_ascii_case(expected) {
                    file_skips.push(Skipped {
                        path: path.to_string_lossy().into_owned(),
                        reason: format!("sha256 mismatch for {filename}"),
                    });
                    continue;
                }
            }
            let source = declaration.as_ref().or(fallback);
            images.push(PlannedImage {
                aspect: declaration
                    .as_ref()
                    .and_then(|image| str_field(image, "aspect"))
                    .map(|value| value.to_string())
                    .or_else(|| aspect_from_filename(&filename)),
                filename,
                sha256: actual,
                path,
                rendered_at: source.and_then(|image| opt_str(image, "rendered_at")),
                model_id: source.and_then(|image| opt_str(image, "model_id")),
                model_label: source.and_then(|image| opt_str(image, "model_label")),
                prompt: source.and_then(|image| opt_str(image, "prompt")),
                reference_character_ids: source
                    .map(|image| str_array(image, "reference_character_ids"))
                    .unwrap_or_default(),
                reference_names: source
                    .map(|image| str_array(image, "reference_names"))
                    .unwrap_or_default(),
            });
        }
        panels.push(PlannedPanel {
            id: panel_id,
            text: str_field(panel, "text").unwrap_or_default().to_string(),
            images,
        });
    }

    let mut finals = Vec::new();
    for final_value in finals_raw {
        let aspect = str_field(final_value, "aspect")
            .ok_or_else(|| "final has no aspect in post.json".to_string())?
            .to_string();
        let filename = str_field(final_value, "filename")
            .ok_or_else(|| "final has no filename in post.json".to_string())?;
        if !safe_filename(filename) {
            return Err(format!("unsafe final filename in post.json: {filename}"));
        }
        let path = dir.join(filename);
        if !path.is_file() {
            file_skips.push(Skipped {
                path: path.to_string_lossy().into_owned(),
                reason: format!("final {filename} missing"),
            });
            continue;
        }
        let Ok(bytes) = scanner.read(&path) else {
            file_skips.push(Skipped {
                path: path.to_string_lossy().into_owned(),
                reason: format!("final {filename} cannot be read"),
            });
            continue;
        };
        finals.push(PlannedFinal {
            aspect,
            panel_id: opt_str(final_value, "panel_id"),
            filename: filename.to_string(),
            font_id: opt_str(final_value, "font_id"),
            style_hash: opt_str(final_value, "style_hash"),
            text: str_field(final_value, "text").unwrap_or_default().to_string(),
            rendered_at: opt_str(final_value, "rendered_at"),
            path,
            sha256: sha256_hex(&bytes),
        });
    }
    if finals.is_empty() {
        return Err("no final image in post.json".to_string());
    }

    let mut hash_input = raw_bytes.clone();
    hash_input.push(0);
    for final_value in &finals {
        hash_input.extend_from_slice(final_value.filename.as_bytes());
        hash_input.push(0);
        hash_input.extend_from_slice(final_value.sha256.as_bytes());
        hash_input.push(0);
    }

    Ok(PostPlan {
        post: PlannedPost {
            id,
            text,
            created_at: str_field(&raw, "created_at")
                .ok_or_else(|| "created_at missing in post.json".to_string())?
                .to_string(),
            format_id: str_field(&raw, "format_id")
                .ok_or_else(|| "format_id missing in post.json".to_string())?
                .to_string(),
            option_id: str_field(&raw, "option_id")
                .ok_or_else(|| "option_id missing in post.json".to_string())?
                .to_string(),
            language: str_field(&raw, "language")
                .ok_or_else(|| "language missing in post.json".to_string())?
                .to_string(),
            art_mode: str_field(&raw, "art_mode")
                .unwrap_or("single")
                .to_string(),
            aspects: str_array(&raw, "aspects"),
            font_id: opt_str(&raw, "font_id"),
            character_ids: str_array(&raw, "character_ids"),
            panels,
            finals,
            content_hash: sha256_hex(&hash_input),
            raw,
            source_dir: dir.to_path_buf(),
        },
        skipped: file_skips,
    })
}

fn plan_fonts(config: &Value) -> Vec<PlannedFont> {
    let presets = config
        .get("still_post_presets")
        .and_then(Value::as_array);
    let last = str_field(config, "still_post_last_preset").unwrap_or_default();
    let preset = presets
        .and_then(|list| list.iter().find(|entry| str_field(entry, "id") == Some(last)))
        .or_else(|| presets.and_then(|list| list.first()));

    let Some(preset) = preset else {
        return Vec::new();
    };
    let Some(pools) = preset
        .get("data")
        .and_then(|data| data.get("font_pools"))
        .and_then(Value::as_object)
    else {
        return Vec::new();
    };

    pools
        .iter()
        .map(|(script, pool)| {
            let fonts: Vec<_> = pool
                .get("fonts")
                .and_then(Value::as_array)
                .map(|choices| {
                    choices
                        .iter()
                        .filter_map(|choice| {
                            let font_id = choice
                                .as_str()
                                .map(|id| id.to_string())
                                .or_else(|| opt_str(choice, "font_id"))?;
                            let weight = choice.get("weight").and_then(Value::as_f64).unwrap_or(0.0);
                            Some(PlannedFontChoice { font_id, weight })
                        })
                        .collect()
                })
                .unwrap_or_default();
            let weights = fonts.iter().map(|choice| choice.weight).collect();
            PlannedFont {
                script: script.clone(),
                scripts: vec![script.clone()],
                usage_mode: str_field(pool, "usage_mode")
                    .unwrap_or("serial")
                    .to_string(),
                allow_repeat: pool
                    .get("allow_repeat")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                fonts,
                weights,
            }
        })
        .collect()
}
