//! Read-only Story Still Posts importer: scan a channel, then apply it to a Wire data dir.

pub mod apply;
pub mod plan;

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicUsize, Ordering};

    use serde_json::{json, Value};

    use super::apply::apply;
    use super::plan::{scan, sha256_hex};

    static NEXT_DIR: AtomicUsize = AtomicUsize::new(0);

    fn fixture() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/story-channel")
    }

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "wire-import-{tag}-{}-{}",
            std::process::id(),
            NEXT_DIR.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn copy_tree(from: &Path, to: &Path) {
        fs::create_dir_all(to).unwrap();
        for entry in fs::read_dir(from).unwrap() {
            let entry = entry.unwrap();
            let target = to.join(entry.file_name());
            if entry.file_type().unwrap().is_dir() {
                copy_tree(&entry.path(), &target);
            } else {
                fs::copy(entry.path(), target).unwrap();
            }
        }
    }

    fn channel_with_only(root: &Path, post_id: &str) -> PathBuf {
        let channel = root.join("channel");
        copy_tree(&fixture(), &channel);
        let posts = channel.join("still-posts/posts");
        for entry in fs::read_dir(&posts).unwrap() {
            let entry = entry.unwrap();
            if entry.file_name().to_string_lossy() == post_id {
                continue;
            }
            let path = entry.path();
            if path.is_dir() {
                fs::remove_dir_all(path).unwrap();
            } else {
                fs::remove_file(path).unwrap();
            }
        }
        channel
    }

    #[test]
    fn scan_plans_per_size_art_and_skips_zero_finals() {
        let plan = scan(&fixture()).unwrap();

        assert_eq!(plan.channel_slug, "test-channel-kh");
        assert_eq!(plan.posts.len(), 2);
        assert_eq!(plan.skipped.len(), 1);
        assert!(
            plan.skipped[0].reason.contains("no final image"),
            "{:?}",
            plan.skipped
        );
        assert!(plan.skipped[0].path.ends_with("post-0003/post.json"));
        assert_eq!(plan.fonts.len(), 2);
        // project.json + config.json + post-0001 (record, 3 art, 2 finals)
        // + post-0002 (record, 1 art, 1 final) + post-0003 (record).
        assert_eq!(plan.source_files, 12);

        let per_size = &plan.posts[0];
        assert_eq!(per_size.id, "post-0001");
        assert_eq!(per_size.text, per_size.raw["caption"].as_str().unwrap());
        assert_eq!(per_size.panels.len(), 1);
        assert_eq!(per_size.panels[0].images.len(), 3);
        assert_eq!(per_size.finals.len(), 2);
        let names: Vec<&str> = per_size.panels[0]
            .images
            .iter()
            .map(|image| image.filename.as_str())
            .collect();
        assert!(names.contains(&"p0.png"));
        assert!(names.contains(&"p0-1x1.png"));
        assert!(names.contains(&"p0-9x16.png"));
        assert_eq!(per_size.aspects, vec!["1:1", "9:16"]);
        assert_eq!(per_size.art_mode, "per-size");

        let simple = &plan.posts[1];
        assert_eq!(simple.id, "post-0002");
        assert_eq!(simple.panels[0].images.len(), 1);
        assert_eq!(simple.finals.len(), 1);
    }

    #[test]
    fn apply_copies_all_verified_images_and_is_idempotent() {
        let data = temp_dir("apply");
        let plan = scan(&fixture()).unwrap();

        let report = apply(&plan, &data).unwrap();
        assert!(report.source_unchanged);
        assert_eq!(report.skipped.len(), 1);
        assert_eq!(report.imported.len(), 2);
        assert_eq!(report.imported[0].files_copied, 5);
        assert_eq!(report.imported[1].files_copied, 2);
        assert_eq!(report.counts.files_copied, 7);
        assert!(report.counts.bytes > 0);

        for post in &plan.posts {
            let mut shas: Vec<&str> = post
                .panels
                .iter()
                .flat_map(|panel| panel.images.iter().map(|image| image.sha256.as_str()))
                .collect();
            shas.extend(post.finals.iter().map(|final_| final_.sha256.as_str()));
            assert_eq!(shas.len(), report_imported_files(&report, &post.id));
            for sha in shas {
                let dest = data.join("pool/images").join(format!("{sha}.png"));
                assert_eq!(sha256_hex(&fs::read(dest).unwrap()), sha);
            }
        }

        for id in ["post-0001", "post-0002"] {
            let recipe: Value =
                serde_json::from_slice(&fs::read(data.join(format!("recipes/{id}.json"))).unwrap())
                    .unwrap();
            assert_eq!(recipe["source"], "story");
            assert_eq!(recipe["renderable"], false);
        }

        let pool_index: Value =
            serde_json::from_slice(&fs::read(data.join("pool/index.json")).unwrap()).unwrap();
        let rows = pool_index.as_array().unwrap();
        assert_eq!(rows.len(), 7);
        assert!(rows.iter().all(|row| row["provenance"] == "imported:story"
            && row["sha256"].as_str().unwrap().len() == 64));
        assert!(rows.iter().any(|row| row["filename"] == "p0-9x16.png"
            && row["aspect"] == "9:16"
            && row["kind"] == "panel"));
        assert!(rows.iter().any(|row| row["aspect"] == "9:16"
            && row["style_hash"] == "style-fixture-9x16"
            && row["font_id"] == "kantumruy-pro"));

        let fonts: Value =
            serde_json::from_slice(&fs::read(data.join("fonts.json")).unwrap()).unwrap();
        let font_rows = fonts.as_array().unwrap();
        assert_eq!(font_rows.len(), 2);
        assert!(font_rows
            .iter()
            .any(|row| row["script"] == "latn" && row["weights"][1] == 2.0));
        assert!(font_rows
            .iter()
            .any(|row| row["script"] == "khmr" && row.get("last_font_id").is_none()));

        let ledger: Value =
            serde_json::from_slice(&fs::read(data.join("imports/ledger.json")).unwrap()).unwrap();
        assert_eq!(ledger["entries"].as_object().unwrap().len(), 2);

        let again = apply(&plan, &data).unwrap();
        assert!(again.source_unchanged);
        assert!(again.imported.is_empty());
        assert_eq!(again.already_present.len(), 2);
        assert!(again
            .already_present
            .iter()
            .all(|row| row.contains("already imported (unchanged)")));
        assert_eq!(again.counts.files_copied, 0);
        let pool_again: Value =
            serde_json::from_slice(&fs::read(data.join("pool/index.json")).unwrap()).unwrap();
        assert_eq!(pool_again.as_array().unwrap().len(), 7);

        fs::remove_dir_all(&data).unwrap();
    }

    fn report_imported_files(report: &super::apply::ImportReport, id: &str) -> usize {
        report
            .imported
            .iter()
            .find(|row| row.id == id)
            .unwrap()
            .files_copied
    }

    #[test]
    fn tampered_image_is_skipped_without_a_silent_copy() {
        let root = temp_dir("tamper");
        let source = root.join("channel");
        copy_tree(&fixture(), &source);

        let image = source.join("still-posts/posts/post-0001/p0.png");
        let mut bytes = fs::read(&image).unwrap();
        let last = bytes.len() - 1;
        bytes[last] ^= 0xff;
        fs::write(&image, bytes).unwrap();
        let flipped_sha = sha256_hex(&fs::read(&image).unwrap());

        let plan = scan(&source).unwrap();
        assert_eq!(plan.posts.len(), 2);
        assert!(plan
            .skipped
            .iter()
            .any(|row| row.reason.contains("sha256 mismatch for p0.png")),
            "{:?}",
            plan.skipped
        );

        let data = root.join("data");
        let report = apply(&plan, &data).unwrap();
        assert!(report.source_unchanged);
        assert_eq!(report.counts.files_copied, 6);
        assert!(!data.join("pool/images").join(format!("{flipped_sha}.png")).exists());

        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn three_file_post_imports_without_the_9x16_pair() {
        let root = temp_dir("three-file");
        let source = channel_with_only(&root, "post-0002");
        let plan = scan(&source).unwrap();

        assert_eq!(plan.posts.len(), 1);
        assert_eq!(plan.posts[0].id, "post-0002");
        assert_eq!(plan.posts[0].finals.len(), 1);
        assert!(plan.posts[0].finals.iter().all(|final_| final_.aspect == "1:1"));

        let report = apply(&plan, &root.join("data")).unwrap();
        assert_eq!(report.imported.len(), 1);
        assert_eq!(report.imported[0].files_copied, 2);
        assert!(root.join("data/recipes/post-0002.json").exists());

        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn zero_final_post_is_skipped_with_a_plain_reason() {
        let root = temp_dir("no-final");
        let source = channel_with_only(&root, "post-0003");
        let plan = scan(&source).unwrap();

        assert!(plan.posts.is_empty());
        assert_eq!(plan.skipped.len(), 1);
        assert!(
            plan.skipped[0].reason.contains("no final image"),
            "{:?}",
            plan.skipped
        );

        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn no_cursor_and_no_font_pools_still_imports_posts() {
        let root = temp_dir("no-fonts");
        let source = channel_with_only(&root, "post-0002");
        let cursor = source.join("still-posts/font-pool-cursor.json");
        assert!(!cursor.exists());
        fs::write(
            source.join("config.json"),
            serde_json::to_vec_pretty(&json!({"still_posts": {"handle": "@fixture"}})).unwrap(),
        )
        .unwrap();

        let plan = scan(&source).unwrap();
        assert!(plan.fonts.is_empty());
        assert_eq!(plan.posts.len(), 1);
        let report = apply(&plan, &root.join("data")).unwrap();
        assert_eq!(report.imported.len(), 1);
        assert_eq!(report.imported[0].files_copied, 2);
        assert!(!root.join("data/fonts.json").exists());

        fs::remove_dir_all(&root).unwrap();
    }

    /// The real Production channel, staged read-only on this Mac. Ignored by default
    /// so a normal run needs no external data. Run explicitly:
    ///   cargo test --lib real_channel_report -- --ignored --nocapture
    #[test]
    #[ignore]
    fn real_channel_report() {
        let root = std::env::var("STORY_REAL_CHANNEL").unwrap_or_else(|_| {
            "/build/Developer/_orchestra/youtube-stack/wire-still-posts-fixture-20261121/little-weather-within".to_string()
        });
        let plan = scan(std::path::Path::new(&root)).expect("scan the real channel");
        assert_eq!(plan.posts.len(), 9, "nine posts in little-weather-within");
        assert!(plan.skipped.is_empty(), "no skips expected: {:?}", plan.skipped);

        let data = std::env::temp_dir().join(format!("wire-real-import-{}", std::process::id()));
        let _ = fs::remove_dir_all(&data);
        let report = apply(&plan, &data).expect("apply the real channel");
        assert_eq!(report.counts.imported, 9);
        assert_eq!(report.counts.skipped, 0);
        assert!(report.source_unchanged, "Story's bytes must not change");

        let again = apply(&plan, &data).expect("re-apply");
        assert_eq!(again.counts.imported, 0, "second run copies nothing");
        assert_eq!(again.counts.already_present, 9);

        let summary = json!({
            "channel": report.channel_slug,
            "posts": plan.posts.len(),
            "imported": report.counts.imported,
            "already_present_second_run": again.counts.already_present,
            "skipped": report.counts.skipped,
            "files_copied": report.counts.files_copied,
            "bytes": report.counts.bytes,
            "source_unchanged": report.source_unchanged,
            "skips": report.skipped,
        });
        println!("REPORT_JSON={}", serde_json::to_string_pretty(&summary).unwrap());

        fs::remove_dir_all(&data).unwrap();
    }
}
