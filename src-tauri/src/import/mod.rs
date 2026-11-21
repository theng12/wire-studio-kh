//! Read-only Story Still Posts importer: scan a channel, then apply it to a Wire data dir.

pub mod apply;
pub mod plan;

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicUsize, Ordering};

    use serde_json::Value;

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

    #[test]
    fn scan_reads_one_post_and_skips_the_missing_image() {
        let plan = scan(&fixture()).unwrap();

        assert_eq!(plan.channel_slug, "test-channel-kh");
        assert_eq!(plan.posts.len(), 1);
        assert_eq!(plan.skipped.len(), 1);
        assert!(
            plan.skipped[0].reason.contains("image p0.png missing"),
            "{:?}",
            plan.skipped
        );
        assert_eq!(plan.fonts.len(), 2);
        assert!(plan.source_files >= 8);

        let post = &plan.posts[0];
        assert_eq!(post.id, "post-0001");
        assert_eq!(post.text, post.raw["caption"].as_str().unwrap());
        assert_eq!(post.panels.len(), 1);
        assert_eq!(post.finals.len(), 2);
        assert_eq!(post.aspects, vec!["1:1", "9:16"]);
        assert_eq!(post.format_id, "quiet-ink");
        assert_eq!(post.option_id, "centred-quote");
        assert_eq!(post.language, "km");
    }

    #[test]
    fn apply_copies_three_verified_images_and_is_idempotent() {
        let data = temp_dir("apply");
        let plan = scan(&fixture()).unwrap();

        let report = apply(&plan, &data).unwrap();
        assert!(report.source_unchanged);
        assert_eq!(report.skipped.len(), 1);
        assert_eq!(report.imported.len(), 1);
        assert_eq!(report.imported[0].id, "post-0001");
        assert_eq!(report.imported[0].files_copied, 3);
        assert_eq!(report.counts.files_copied, 3);
        assert!(report.counts.bytes > 0);

        for post in &plan.posts {
            for panel in &post.panels {
                let dest = data
                    .join("pool/images")
                    .join(format!("{}.png", panel.image.sha256));
                assert_eq!(sha256_hex(&fs::read(dest).unwrap()), panel.image.sha256);
            }
            for final_ in &post.finals {
                let dest = data
                    .join("pool/images")
                    .join(format!("{}.png", final_.sha256));
                assert_eq!(sha256_hex(&fs::read(dest).unwrap()), final_.sha256);
            }
        }

        let recipe: Value = serde_json::from_slice(
            &fs::read(data.join("recipes/post-0001.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(recipe["id"], "post-0001");
        assert_eq!(recipe["source"], "story");
        assert_eq!(recipe["renderable"], false);
        assert_eq!(recipe["caption"], plan.posts[0].raw["caption"]);

        let pool_index: Value =
            serde_json::from_slice(&fs::read(data.join("pool/index.json")).unwrap()).unwrap();
        let rows = pool_index.as_array().unwrap();
        assert_eq!(rows.len(), 3);
        assert!(rows.iter().all(|row| row["provenance"] == "imported:story"
            && row["sha256"].as_str().unwrap().len() == 64));
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
            .any(|row| row["script"] == "khmr" && row["last_font_id"] == "kantumruy-pro"));

        let ledger: Value = serde_json::from_slice(
            &fs::read(data.join("imports/ledger.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(ledger["entries"].as_object().unwrap().len(), 1);

        let again = apply(&plan, &data).unwrap();
        assert!(again.source_unchanged);
        assert!(again.imported.is_empty());
        assert_eq!(again.already_present.len(), 1);
        assert!(again.already_present[0].contains("already imported (unchanged)"));
        assert_eq!(again.counts.files_copied, 0);
        let pool_again: Value =
            serde_json::from_slice(&fs::read(data.join("pool/index.json")).unwrap()).unwrap();
        assert_eq!(pool_again.as_array().unwrap().len(), 3);

        fs::remove_dir_all(&data).unwrap();
    }

    #[test]
    fn tampered_image_is_skipped_with_sha_mismatch() {
        let root = temp_dir("tamper");
        let source = root.join("channel");
        copy_tree(&fixture(), &source);

        let image = source.join("still-posts/posts/post-0001/p0.png");
        let mut bytes = fs::read(&image).unwrap();
        let last = bytes.len() - 1;
        bytes[last] ^= 0xff;
        fs::write(&image, bytes).unwrap();

        let plan = scan(&source).unwrap();
        assert!(plan.posts.is_empty());
        assert!(plan
            .skipped
            .iter()
            .any(|row| row.reason.contains("sha256 mismatch for p0.png")),
            "{:?}",
            plan.skipped
        );

        fs::remove_dir_all(&root).unwrap();
    }
}
