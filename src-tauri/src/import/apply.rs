//! Idempotent apply of a scan plan into the Wire data dir. Never touches `wire.db`
//! in this slice: recipes carry the post id and the queue is left alone.

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use super::plan::{
    safe_filename, sha256_hex, ImportPlan, PlannedPanel, PlannedPost, Skipped, SourceSnapshot,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Imported {
    pub id: String,
    pub files_copied: usize,
    pub bytes: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportCounts {
    pub posts_total: usize,
    pub imported: usize,
    pub already_present: usize,
    pub skipped: usize,
    pub files_copied: usize,
    pub bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportReport {
    pub channel_slug: String,
    pub imported: Vec<Imported>,
    pub skipped: Vec<Skipped>,
    pub already_present: Vec<String>,
    pub source_unchanged: bool,
    pub counts: ImportCounts,
}

struct Asset {
    filename: String,
    path: PathBuf,
    sha256: String,
    kind: &'static str,
    aspect: Option<String>,
    rendered_at: Option<String>,
    model_id: Option<String>,
    model_label: Option<String>,
    prompt: Option<String>,
    character_ids: Vec<String>,
    character_names: Vec<String>,
    style_hash: Option<String>,
    font_id: Option<String>,
    text: Option<String>,
}

fn assets(post: &PlannedPost) -> Vec<Asset> {
    let default_aspect = post.aspects.first().cloned();
    let mut out = Vec::new();

    for panel in &post.panels {
        for image in &panel.images {
            out.push(Asset {
                filename: image.filename.clone(),
                path: image.path.clone(),
                sha256: image.sha256.clone(),
                kind: "panel",
                aspect: image.aspect.clone().or_else(|| default_aspect.clone()),
                rendered_at: image.rendered_at.clone(),
                model_id: image.model_id.clone(),
                model_label: image.model_label.clone(),
                prompt: image.prompt.clone(),
                character_ids: image.reference_character_ids.clone(),
                character_names: image.reference_names.clone(),
                style_hash: None,
                font_id: post.font_id.clone(),
                text: Some(panel.text.clone()),
            });
        }
    }

    for final_ in &post.finals {
        let panel: Option<&PlannedPanel> = post.panels.iter().find(|candidate| {
            final_
                .panel_id
                .as_ref()
                .map_or(true, |id| id == &candidate.id)
        });
        let image = panel.and_then(|panel| panel.images.first());
        out.push(Asset {
            filename: final_.filename.clone(),
            path: final_.path.clone(),
            sha256: final_.sha256.clone(),
            kind: "final",
            aspect: Some(final_.aspect.clone()),
            rendered_at: final_
                .rendered_at
                .clone()
                .or_else(|| image.and_then(|image| image.rendered_at.clone())),
            model_id: image.and_then(|image| image.model_id.clone()),
            model_label: image.and_then(|image| image.model_label.clone()),
            prompt: image.and_then(|image| image.prompt.clone()),
            character_ids: image.map_or_else(Vec::new, |image| image.reference_character_ids.clone()),
            character_names: image.map_or_else(Vec::new, |image| image.reference_names.clone()),
            style_hash: final_.style_hash.clone(),
            font_id: final_.font_id.clone().or_else(|| post.font_id.clone()),
            text: Some(final_.text.clone()),
        });
    }
    out
}

fn load_json(path: &Path, fallback: Value) -> Result<Value, String> {
    match fs::read_to_string(path) {
        Ok(raw) if raw.trim().is_empty() => Ok(fallback),
        Ok(raw) => serde_json::from_str(&raw)
            .map_err(|_| format!("{} cannot be parsed", path.display())),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(fallback),
        Err(_) => Err(format!("{} cannot be read", path.display())),
    }
}

fn as_array(value: Value, path: &Path) -> Result<Vec<Value>, String> {
    match value {
        Value::Array(items) => Ok(items),
        Value::Null => Ok(Vec::new()),
        _ => Err(format!("{} is not an array", path.display())),
    }
}

fn as_object(value: Value, path: &Path) -> Result<Map<String, Value>, String> {
    match value {
        Value::Object(items) => Ok(items),
        Value::Null => Ok(Map::new()),
        _ => Err(format!("{} is not an object", path.display())),
    }
}

fn write_json(path: &Path, value: &Value) -> Result<(), String> {
    let raw = serde_json::to_vec_pretty(value)
        .map_err(|error| format!("cannot encode {}: {error}", path.display()))?;
    fs::write(path, raw).map_err(|_| format!("{} cannot be written", path.display()))
}

/// Re-stat and re-hash exactly the files `scan` read. A difference names the file.
fn verify_sources(plan: &ImportPlan) -> Result<Vec<SourceSnapshot>, String> {
    if plan.sources.is_empty() {
        return Err("no source files to verify".to_string());
    }
    let mut current = Vec::with_capacity(plan.sources.len());
    for expected in &plan.sources {
        let path = Path::new(expected.path.as_str());
        let bytes = fs::read(path)
            .map_err(|_| format!("source {} cannot be read", expected.path))?;
        let meta = fs::metadata(path)
            .map_err(|_| format!("source {} cannot be read", expected.path))?;
        let modified = meta
            .modified()
            .map_err(|_| format!("source {} has no modification time", expected.path))?;
        let mtime_ns = modified
            .duration_since(UNIX_EPOCH)
            .map_err(|_| format!("source {} has an invalid modification time", expected.path))?
            .as_nanos() as u64;
        let snapshot = SourceSnapshot {
            path: expected.path.clone(),
            mtime_ns,
            size: bytes.len() as u64,
            sha256: sha256_hex(&bytes),
        };
        if &snapshot != expected {
            return Err(format!(
                "source changed: {} (source_unchanged: false)",
                expected.path
            ));
        }
        current.push(snapshot);
    }
    Ok(current)
}

/// Copy originals and opaque metadata only; skipped rows come straight from the plan.
pub fn apply(plan: &ImportPlan, data_dir: &Path) -> Result<ImportReport, String> {
    let start = verify_sources(plan)?;

    let recipes_dir = data_dir.join("recipes");
    let images_dir = data_dir.join("pool/images");
    let pool_index_path = data_dir.join("pool/index.json");
    let fonts_path = data_dir.join("fonts.json");
    let ledger_path = data_dir.join("imports/ledger.json");
    let imports_dir = data_dir.join("imports");
    for dir in [&recipes_dir, &images_dir, &imports_dir] {
        fs::create_dir_all(dir)
            .map_err(|_| format!("{} cannot be created", dir.display()))?;
    }

    let mut pool_index = as_array(
        load_json(&pool_index_path, json!([]))?,
        &pool_index_path,
    )?;
    let mut fonts = as_array(load_json(&fonts_path, json!([]))?, &fonts_path)?;
    let mut ledger = as_object(load_json(&ledger_path, json!({}))?, &ledger_path)?;
    let entries_value = ledger
        .entry("entries".to_string())
        .or_insert_with(|| json!({}));
    let entries = entries_value
        .as_object_mut()
        .ok_or_else(|| "imports/ledger.json entries is not an object".to_string())?;

    let mut imported = Vec::new();
    let mut already_present = Vec::new();
    let mut pool_index_dirty = false;
    let mut fonts_dirty = false;
    let mut ledger_dirty = false;
    let mut counts = ImportCounts {
        posts_total: plan.posts.len(),
        skipped: plan.skipped.len(),
        ..ImportCounts::default()
    };

    for post in &plan.posts {
        if !safe_filename(&post.id) {
            return Err(format!("unsafe post id: {}", post.id));
        }
        let key = format!(
            "{}|{}|{}",
            plan.channel_slug, post.id, post.content_hash
        );
        if entries.contains_key(&key) {
            already_present.push(format!("{}: already imported (unchanged)", post.id));
            continue;
        }

        let mut files_copied = 0usize;
        let mut bytes = 0u64;
        let mut new_entries = Vec::new();
        for asset in assets(post) {
            let extension = Path::new(asset.filename.as_str())
                .extension()
                .and_then(|part| part.to_str())
                .unwrap_or("png");
            let name = format!("{}.{}", asset.sha256, extension);
            let destination = images_dir.join(&name);
            let source_bytes = fs::read(&asset.path)
                .map_err(|_| format!("{} cannot be read", asset.path.display()))?;
            let sha256 = sha256_hex(&source_bytes);
            if !sha256.eq_ignore_ascii_case(&asset.sha256) {
                return Err(format!("sha256 mismatch for {}", asset.path.display()));
            }
            fs::write(&destination, &source_bytes)
                .map_err(|_| format!("{} cannot be written", destination.display()))?;
            let written = fs::read(&destination)
                .map_err(|_| format!("{} cannot be read", destination.display()))?;
            if sha256_hex(&written) != asset.sha256 {
                return Err(format!(
                    "written image sha256 mismatch: {}",
                    destination.display()
                ));
            }
            new_entries.push(json!({
                "post_id": post.id,
                "channel_slug": plan.channel_slug,
                "provenance": "imported:story",
                "path": format!("images/{name}"),
                "filename": asset.filename,
                "kind": asset.kind,
                "sha256": asset.sha256,
                "rendered_at": asset.rendered_at,
                "model_id": asset.model_id,
                "model_label": asset.model_label,
                "prompt": asset.prompt,
                "character_ids": asset.character_ids,
                "character_names": asset.character_names,
                "aspect": asset.aspect,
                "style_hash": asset.style_hash,
                "font_id": asset.font_id,
                "text": asset.text,
            }));
            files_copied += 1;
            bytes += source_bytes.len() as u64;
        }

        let mut recipe = post.raw.clone();
        let object = recipe
            .as_object_mut()
            .ok_or_else(|| format!("{} is not an object", post.source_dir.join("post.json").display()))?;
        object.insert("source".to_string(), json!("story"));
        object.insert("renderable".to_string(), json!(false));
        object.insert(
            "render_note".to_string(),
            json!("Wire 0.2.0 has no Story layer renderer; original PNGs were copied"),
        );
        write_json(&recipes_dir.join(format!("{}.json", post.id)), &recipe)?;

        pool_index_dirty |= !new_entries.is_empty();
        pool_index.extend(new_entries);
        entries.insert(
            key,
            json!({
                "source": "story",
                "post_id": post.id,
                "content_hash": post.content_hash,
                "imported_at": Utc::now().to_rfc3339(),
            }),
        );
        ledger_dirty = true;
        imported.push(Imported {
            id: post.id.clone(),
            files_copied,
            bytes,
        });
        counts.imported += 1;
        counts.files_copied += files_copied;
        counts.bytes += bytes;
    }

    for font in &plan.fonts {
        let source_key = format!("{}|{}", plan.channel_slug, font.script);
        if fonts.iter().any(|entry| {
            entry.get("source_key").and_then(Value::as_str) == Some(source_key.as_str())
        }) {
            continue;
        }
        fonts.push(json!({
            "source_key": source_key,
            "provenance": "imported:story",
            "channel_slug": plan.channel_slug,
            "script": font.script,
            "scripts": font.scripts,
            "usage_mode": font.usage_mode,
            "allow_repeat": font.allow_repeat,
            "fonts": font.fonts,
            "weights": font.weights,
        }));
        fonts_dirty = true;
    }

    if pool_index_dirty {
        write_json(&pool_index_path, &Value::Array(pool_index))?;
    }
    if fonts_dirty {
        write_json(&fonts_path, &Value::Array(fonts))?;
    }
    if ledger_dirty {
        write_json(&ledger_path, &Value::Object(ledger))?;
    }

    let end = verify_sources(plan)?;
    for (before, after) in start.iter().zip(end.iter()) {
        if before != after {
            return Err(format!(
                "source changed during apply: {} (source_unchanged: false)",
                before.path
            ));
        }
    }

    counts.already_present = already_present.len();
    Ok(ImportReport {
        channel_slug: plan.channel_slug.clone(),
        imported,
        skipped: plan.skipped.clone(),
        already_present,
        source_unchanged: true,
        counts,
    })
}
