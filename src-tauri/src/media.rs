//! Read-only reader for Media Studio's catalogue (its own SQLite store).
//! Nothing here writes: no WAL checkpoint, no pragma that persists, no file created.

use std::path::{Path, PathBuf};
use std::time::Duration;

use rusqlite::{Connection, OpenFlags};
use serde::Serialize;

/// The headless and windowed paths share this default page size.
pub const DEFAULT_LIMIT: usize = 200;

#[derive(Debug, Clone, Serialize)]
pub struct Product {
    pub id: String,
    pub sku: String,
    pub name: String,
    pub price_retail: Option<f64>,
    pub status: String,
    pub updated_at: i64,
}

/// Open Media's store read-only. No journal is created or checkpointed.
pub fn open_read_only(path: &Path) -> Result<Connection, String> {
    let conn = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|e| format!("cannot open {}: {e}", path.display()))?;
    conn.busy_timeout(Duration::from_secs(5))
        .map_err(|e| e.to_string())?;
    Ok(conn)
}

/// Active products, name-ordered. A store that answers is read; nothing else.
pub fn products(conn: &Connection, limit: usize) -> Result<Vec<Product>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, sku, name, price_retail, status, updated_at FROM products \
             WHERE status = 'active' ORDER BY name COLLATE NOCASE LIMIT ?1",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([limit as i64], |row| {
            Ok(Product {
                id: row.get(0)?,
                sku: row.get(1)?,
                name: row.get(2)?,
                price_retail: row.get(3)?,
                status: row.get(4)?,
                updated_at: row.get(5)?,
            })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())
}

/// The packaged app's store first, then the dev one; an empty answer means "not found".
pub fn default_store_path() -> Option<PathBuf> {
    let home = std::env::var("HOME").ok()?;
    for name in ["Business Studio KH", "business-studio-kh"] {
        let path = PathBuf::from(&home)
            .join("Library/Application Support")
            .join(name)
            .join("data")
            .join("database.sqlite");
        if path.is_file() {
            return Some(path);
        }
    }
    None
}

/// One read shared by the window, the headless route and `state`, so they cannot drift.
/// A missing or unreadable store is an honest answer (`found: false` plus a reason),
/// never an error page.
pub struct MediaRead {
    pub configured: bool,
    pub path: String,
    pub found: bool,
    pub state: &'static str,
    pub reason: String,
    pub products: Vec<Product>,
}

fn resolve(data_dir: &Path) -> (String, Option<PathBuf>) {
    let cfg = crate::settings::load(&crate::settings::path_in(data_dir));
    let configured = cfg.media_store_path.trim().to_string();
    // Empty means discover; a saved path is used as saved. Nothing is created or written.
    let store = if configured.is_empty() {
        default_store_path()
    } else {
        Some(PathBuf::from(&configured))
    };
    (configured, store)
}

pub fn read(data_dir: &Path, limit: usize) -> MediaRead {
    let (configured, store) = resolve(data_dir);
    let path_text = store
        .as_ref()
        .map(|p| p.display().to_string())
        .unwrap_or_default();
    let mut rows_out = Vec::new();
    let (found, reason) = match store {
        None => (
            false,
            "No Media Studio store was found in the usual places.".to_string(),
        ),
        Some(path) if !path.is_file() => (
            false,
            format!("No Media Studio store at {}.", path.display()),
        ),
        Some(path) => match open_read_only(&path).and_then(|conn| products(&conn, limit)) {
            Ok(rows) => {
                rows_out = rows;
                (true, format!("{} active products.", rows_out.len()))
            }
            Err(error) => (
                false,
                format!("Could not read {}: {error}", path.display()),
            ),
        },
    };
    MediaRead {
        configured: !configured.is_empty(),
        path: path_text,
        found,
        state: crate::settings::media_status(&configured, found),
        reason,
        products: rows_out,
    }
}

/// The `state` doorway's small block — the same read, no rows attached.
pub fn status_json(data_dir: &Path) -> serde_json::Value {
    // A path check only: `state` refreshes often and must never open Media's store for nothing.
    let (configured, store) = resolve(data_dir);
    let path_text = store.as_ref().map(|p| p.display().to_string()).unwrap_or_default();
    let found = store.as_ref().map(|p| p.is_file()).unwrap_or(false);
    serde_json::json!({
        "configured": !configured.is_empty(),
        "path": path_text,
        "found": found,
        "state": crate::settings::media_status(&configured, found),
    })
}

/// One product as it is right now: the compose path records its price, the send path re-reads it.
pub fn product(data_dir: &Path, id: &str) -> Result<Product, String> {
    let (_, store) = resolve(data_dir);
    let path = store.ok_or_else(|| "No Media Studio store was found.".to_string())?;
    if !path.is_file() {
        return Err(format!("No Media Studio store at {}.", path.display()));
    }
    let conn = open_read_only(&path)?;
    conn.query_row(
        "SELECT id, sku, name, price_retail, status, updated_at FROM products WHERE id = ?1",
        [id],
        |row| {
            Ok(Product {
                id: row.get(0)?,
                sku: row.get(1)?,
                name: row.get(2)?,
                price_retail: row.get(3)?,
                status: row.get(4)?,
                updated_at: row.get(5)?,
            })
        },
    )
    .map_err(|error| format!("Could not read {}: {error}", path.display()))
}

/// The full answer for `media_products` and `save_media_path`.
pub fn products_json(data_dir: &Path, limit: usize) -> serde_json::Value {
    let read = read(data_dir, limit);
    serde_json::json!({
        "configured": read.configured,
        "path": read.path,
        "found": read.found,
        "state": read.state,
        "reason": read.reason,
        "as_of": chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        "products": read.products,
    })
}

/// Save the Media store path (empty = discover again) and answer like `media_products`.
/// Only Wire's own settings file is written.
pub fn save_path(data_dir: &Path, path: &str) -> Result<serde_json::Value, String> {
    let sp = crate::settings::path_in(data_dir);
    let mut cfg = crate::settings::load(&sp);
    cfg.media_store_path = path.trim().to_string();
    crate::settings::save(&sp, &cfg)?;
    Ok(products_json(data_dir, DEFAULT_LIMIT))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Reading must leave the store byte-identical: same sha256, same mtime.
    #[test]
    fn read_only_reading_changes_no_byte() {
        let dir = std::env::temp_dir().join(format!("wire-media-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let db = dir.join("database.sqlite");
        {
            let conn = Connection::open(&db).unwrap();
            conn.execute_batch(
                "CREATE TABLE products (id TEXT, sku TEXT, name TEXT, description TEXT, \
                 price_retail REAL, price_wholesale REAL, status TEXT, updated_at INTEGER);",
            )
            .unwrap();
            conn.execute(
                "INSERT INTO products (id, sku, name, price_retail, status, updated_at) \
                 VALUES ('p1', 'S1', 'Banana', 1.5, 'active', 1000)",
                [],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO products (id, sku, name, price_retail, status, updated_at) \
                 VALUES ('p2', 'S2', 'Apple', 2.0, 'retired', 1001)",
                [],
            )
            .unwrap();
        }
        let sha = |path: &Path| {
            use sha2::{Digest, Sha256};
            let mut hasher = Sha256::new();
            hasher.update(std::fs::read(path).unwrap());
            format!("{:x}", hasher.finalize())
        };
        let before_mtime = std::fs::metadata(&db).unwrap().modified().unwrap();
        let before_sha = sha(&db);

        let conn = open_read_only(&db).unwrap();
        let rows = products(&conn, 50).unwrap();
        assert_eq!(rows.len(), 1, "only the active row is read");
        assert_eq!(rows[0].name, "Banana");
        assert_eq!(rows[0].price_retail, Some(1.5));
        drop(conn);

        assert_eq!(
            before_mtime,
            std::fs::metadata(&db).unwrap().modified().unwrap(),
            "mtime must not change"
        );
        assert_eq!(before_sha, sha(&db), "bytes must not change");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
