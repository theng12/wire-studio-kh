//! SQLite queue. One post has one delivery per target; retries reuse that row.

use std::collections::HashSet;
use std::path::Path;

use chrono::{DateTime, Duration, SecondsFormat, Utc};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use uuid::Uuid;

use super::models::{Claim, DeliveryRow, Failure, ATTENTION, CANCELLED, FAILED, PENDING, SENDING, SENT};
use super::scheduler::{backoff_delay, MAX_ATTEMPTS};
#[cfg(test)]
use super::scheduler::backoff_base_secs;

const SCHEMA_VERSION: i64 = 1;

const SCHEMA_V1: &str = "
CREATE TABLE posts (
    id          TEXT PRIMARY KEY,
    text        TEXT NOT NULL,
    created_at  TEXT NOT NULL
);
CREATE TABLE deliveries (
    id                TEXT PRIMARY KEY,
    post_id           TEXT NOT NULL REFERENCES posts(id) ON DELETE CASCADE,
    target            TEXT NOT NULL,
    state             TEXT NOT NULL DEFAULT 'pending',
    attempts          INTEGER NOT NULL DEFAULT 0,
    next_attempt_at   TEXT NOT NULL,
    scheduled_at      TEXT NOT NULL,
    claimed_at        TEXT,
    last_error        TEXT,
    attention         INTEGER NOT NULL DEFAULT 0,
    remote_id         TEXT,
    remote_url        TEXT,
    sent_at           TEXT,
    sent_late_minutes INTEGER,
    retried_by        TEXT,
    retried_at        TEXT,
    created_at        TEXT NOT NULL,
    UNIQUE(post_id, target)
);
CREATE INDEX idx_deliveries_due ON deliveries(state, next_attempt_at);
";

pub struct Store {
    conn: Connection,
}

fn iso(at: DateTime<Utc>) -> String {
    at.to_rfc3339_opts(SecondsFormat::Secs, true)
}

fn parse_ts(value: &str) -> rusqlite::Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .map(|d| d.with_timezone(&Utc))
        .map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(
                0,
                rusqlite::types::Type::Text,
                Box::new(e),
            )
        })
}

fn bad_data(message: &str) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        0,
        rusqlite::types::Type::Text,
        Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            message.to_string(),
        )),
    )
}

impl Store {
    pub fn open(path: &Path) -> rusqlite::Result<Store> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent).map_err(|e| {
                    rusqlite::Error::FromSqlConversionFailure(
                        0,
                        rusqlite::types::Type::Text,
                        Box::new(e),
                    )
                })?;
            }
        }
        let mut store = Store { conn: Connection::open(path)? };
        store.setup()?;
        Ok(store)
    }

    pub fn open_in_memory() -> rusqlite::Result<Store> {
        let mut store = Store { conn: Connection::open_in_memory()? };
        store.setup()?;
        Ok(store)
    }

    fn setup(&mut self) -> rusqlite::Result<()> {
        self.conn.execute_batch(
            "PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON; PRAGMA busy_timeout=5000;",
        )?;
        self.migrate()
    }

    /// Forward-only: an app older than its database refuses to open it.
    fn migrate(&mut self) -> rusqlite::Result<()> {
        let tx = self.conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute_batch("CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);")?;
        let raw: Option<String> = tx
            .query_row("SELECT value FROM meta WHERE key = 'schema_version'", [], |r| r.get(0))
            .optional()?;
        let version: i64 = match raw {
            Some(v) => v.parse().map_err(|_| bad_data("invalid schema version"))?,
            None => 0,
        };
        if version > SCHEMA_VERSION {
            return Err(bad_data("database schema is newer than this app"));
        }
        if version < 1 {
            tx.execute_batch(SCHEMA_V1)?;
            tx.execute(
                "INSERT INTO meta (key, value) VALUES ('schema_version', '1')",
                [],
            )?;
        }
        tx.commit()
    }

    pub fn create_post(
        &mut self,
        text: &str,
        targets: &[String],
    ) -> rusqlite::Result<(String, Vec<String>)> {
        let now = iso(Utc::now());
        let post_id = Uuid::new_v4().to_string();
        let mut delivery_ids = Vec::new();
        let mut seen = HashSet::new();
        let tx = self.conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute(
            "INSERT INTO posts (id, text, created_at) VALUES (?1, ?2, ?3)",
            params![post_id, text, now],
        )?;
        for target in targets {
            if !seen.insert(target) {
                continue;
            }
            let id = Uuid::new_v4().to_string();
            tx.execute(
                "INSERT INTO deliveries (id, post_id, target, state, attempts, \
                 next_attempt_at, scheduled_at, created_at) \
                 VALUES (?1, ?2, ?3, 'pending', 0, ?4, ?4, ?4)",
                params![id, post_id, target, now],
            )?;
            delivery_ids.push(id);
        }
        tx.commit()?;
        Ok((post_id, delivery_ids))
    }

    /// Claim the oldest due row, flipping it to sending and counting the attempt.
    pub fn claim_due(&mut self, now: DateTime<Utc>) -> rusqlite::Result<Option<Claim>> {
        let now_s = iso(now);
        let tx = self.conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let due: Option<(String, String, String, String, i64)> = tx
            .query_row(
                "SELECT d.id, d.post_id, d.target, p.text, \
                 CAST(MAX(0, (julianday(?1) - julianday(d.scheduled_at)) * 1440) AS INTEGER) \
                 FROM deliveries d JOIN posts p ON p.id = d.post_id \
                 WHERE d.state = 'pending' AND d.next_attempt_at <= ?1 \
                 ORDER BY d.next_attempt_at, d.rowid LIMIT 1",
                params![now_s],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
            )
            .optional()?;
        let Some((id, post_id, target, text, late_minutes)) = due else {
            return Ok(None);
        };
        tx.execute(
            "UPDATE deliveries SET state = 'sending', attempts = attempts + 1, \
             claimed_at = ?1, sent_late_minutes = ?2 \
             WHERE id = ?3 AND state = 'pending'",
            params![now_s, late_minutes, id],
        )?;
        tx.commit()?;
        Ok(Some(Claim { delivery_id: id, post_id, target, text }))
    }

    pub fn mark_sent(
        &mut self,
        delivery_id: &str,
        remote_id: Option<&str>,
        remote_url: Option<&str>,
    ) -> rusqlite::Result<()> {
        let tx = self.conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute(
            "UPDATE deliveries SET state = 'sent', sent_at = ?1, remote_id = ?2, \
             remote_url = ?3, last_error = NULL, attention = 0 \
             WHERE id = ?4 AND state = 'sending'",
            params![iso(Utc::now()), remote_id, remote_url, delivery_id],
        )?;
        tx.commit()
    }

    pub fn mark_failed(
        &mut self,
        delivery_id: &str,
        failure: Failure,
        reason: &str,
    ) -> rusqlite::Result<()> {
        let now_dt = Utc::now();
        let now = iso(now_dt);
        let reason: String = reason.chars().take(1000).collect();
        let tx = self.conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let attempts: Option<i64> = tx
            .query_row(
                "SELECT attempts FROM deliveries WHERE id = ?1 AND state = 'sending'",
                params![delivery_id],
                |r| r.get(0),
            )
            .optional()?;
        let Some(attempts) = attempts else { return Ok(()) };
        let (state, next_at, attention) = match failure {
            Failure::AuthPermission => (ATTENTION, now, 1),
            Failure::Permanent => (FAILED, now, 0),
            Failure::Transient if attempts >= MAX_ATTEMPTS => (FAILED, now, 0),
            Failure::Transient => (PENDING, iso(now_dt + backoff_delay(attempts)), 0),
        };
        tx.execute(
            "UPDATE deliveries SET state = ?1, next_attempt_at = ?2, attention = ?3, \
             last_error = ?4 WHERE id = ?5 AND state = 'sending'",
            params![state, next_at, attention, reason, delivery_id],
        )?;
        tx.commit()
    }

    /// Only a queued row may be cancelled; anything already worked is terminal.
    pub fn cancel(&mut self, delivery_id: &str) -> rusqlite::Result<()> {
        let tx = self.conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let changed = tx.execute(
            "UPDATE deliveries SET state = 'cancelled', last_error = 'cancelled by user', \
             attention = 0, next_attempt_at = ?1 \
             WHERE id = ?2 AND state = 'pending'",
            params![iso(Utc::now()), delivery_id],
        )?;
        if changed == 0 {
            return Err(rusqlite::Error::QueryReturnedNoRows);
        }
        tx.commit()
    }

    /// Explicit replay of a terminal row; sent rows are never retried.
    pub fn retry(&mut self, delivery_id: &str) -> rusqlite::Result<()> {
        let now = iso(Utc::now());
        let tx = self.conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let changed = tx.execute(
            "UPDATE deliveries SET state = 'pending', attempts = 0, next_attempt_at = ?1, \
             claimed_at = NULL, sent_late_minutes = NULL, last_error = NULL, attention = 0, \
             retried_by = 'user', retried_at = ?1 \
             WHERE id = ?2 AND state IN ('failed', 'attention', 'cancelled')",
            params![now, delivery_id],
        )?;
        if changed == 0 {
            return Err(rusqlite::Error::QueryReturnedNoRows);
        }
        tx.commit()
    }

    pub fn release_stale_sending(&mut self, max_age_secs: i64) -> rusqlite::Result<usize> {
        let cutoff = iso(Utc::now() - Duration::seconds(max_age_secs));
        let tx = self.conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let changed = tx.execute(
            "UPDATE deliveries SET state = 'pending', claimed_at = NULL \
             WHERE state = 'sending' AND COALESCE(claimed_at, next_attempt_at) <= ?1",
            params![cutoff],
        )?;
        tx.commit()?;
        Ok(changed)
    }

    /// Move a post's queued deliveries into the future (UI scheduling).
    pub fn set_next_attempt_for_post(
        &mut self,
        post_id: &str,
        at: DateTime<Utc>,
    ) -> rusqlite::Result<usize> {
        let tx = self.conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let changed = tx.execute(
            "UPDATE deliveries SET next_attempt_at = ?1 \
             WHERE post_id = ?2 AND state = 'pending'",
            params![iso(at), post_id],
        )?;
        tx.commit()?;
        Ok(changed)
    }

    pub fn targets_seen(&self) -> rusqlite::Result<Vec<String>> {
        let mut stmt = self.conn.prepare("SELECT DISTINCT target FROM deliveries ORDER BY target")?;
        let rows = stmt.query_map([], |r| r.get(0))?;
        rows.collect()
    }

    pub fn queue_view(&self) -> rusqlite::Result<Vec<DeliveryRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT d.id, d.post_id, p.text, d.target, d.state, d.attempts, d.next_attempt_at, \
             d.last_error, d.attention, d.remote_id, d.remote_url, d.sent_late_minutes \
             FROM deliveries d JOIN posts p ON p.id = d.post_id ORDER BY d.rowid DESC",
        )?;
        let rows = stmt.query_map([], |r| {
            let state: String = r.get(4)?;
            let shown_state = match state.as_str() {
                PENDING => "queued",
                SENDING => "sending",
                SENT => "sent",
                FAILED => "dead",
                ATTENTION => "attention",
                CANCELLED => "cancelled",
                _ => state.as_str(),
            }
            .to_string();
            Ok(DeliveryRow {
                id: r.get(0)?,
                post_id: r.get(1)?,
                text: r.get(2)?,
                target: r.get(3)?,
                state,
                shown_state,
                attempts: r.get(5)?,
                next_attempt_at: r.get(6)?,
                last_error: r.get(7)?,
                attention: r.get::<_, i64>(8)? != 0,
                remote_id: r.get(9)?,
                remote_url: r.get(10)?,
                sent_late_minutes: r.get(11)?,
            })
        })?;
        rows.collect()
    }

    pub fn next_due(&self) -> rusqlite::Result<Option<DateTime<Utc>>> {
        let value: Option<String> = self.conn.query_row(
            "SELECT MIN(next_attempt_at) FROM deliveries WHERE state = 'pending'",
            [],
            |r| r.get(0),
        )?;
        value.map(|s| parse_ts(&s)).transpose()
    }

    pub fn in_flight_count(&self) -> rusqlite::Result<usize> {
        let n: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM deliveries WHERE state = 'sending'",
            [],
            |r| r.get(0),
        )?;
        Ok(n as usize)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh() -> Store {
        Store::open_in_memory().expect("in-memory store")
    }

    fn row_for(store: &Store, id: &str) -> DeliveryRow {
        store
            .queue_view()
            .unwrap()
            .into_iter()
            .find(|r| r.id == id)
            .unwrap_or_else(|| panic!("no row {id}"))
    }

    #[test]
    fn duplicate_targets_produce_one_delivery() {
        let mut store = fresh();
        let targets = vec!["chat".to_string(), "chat".to_string(), "chat".to_string()];
        let (_, ids) = store.create_post("hello", &targets).unwrap();
        assert_eq!(ids.len(), 1);
        let count: i64 = store
            .conn
            .query_row("SELECT COUNT(*) FROM deliveries", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);
        assert_eq!(store.targets_seen().unwrap(), vec!["chat".to_string()]);
    }

    #[test]
    fn backoff_follows_one_two_four_eight_sixteen_minutes() {
        assert_eq!(
            (1..=5).map(backoff_base_secs).collect::<Vec<_>>(),
            vec![60, 120, 240, 480, 960]
        );
        for attempt in 1..=5 {
            let ms = backoff_delay(attempt).num_milliseconds();
            let base = backoff_base_secs(attempt) as f64 * 1000.0;
            assert!(
                ms >= (base * 0.9) as i64 && ms <= (base * 1.1) as i64,
                "attempt {attempt}: {ms}ms"
            );
        }
    }

    #[test]
    fn five_transient_attempts_end_dead() {
        let mut store = fresh();
        let (_, ids) = store.create_post("hello", &["chat".to_string()]).unwrap();
        for expected in 1..=MAX_ATTEMPTS {
            let due = store.next_due().unwrap().expect("due time");
            assert!(store.claim_due(due).unwrap().is_some());
            assert_eq!(row_for(&store, &ids[0]).attempts, expected);
            assert_eq!(store.in_flight_count().unwrap(), 1);
            store.mark_failed(&ids[0], Failure::Transient, "network").unwrap();
        }
        let row = row_for(&store, &ids[0]);
        assert_eq!(row.state, FAILED);
        assert_eq!(row.shown_state, "dead");
        assert_eq!(row.attempts, 5);
        assert_eq!(store.next_due().unwrap(), None);
        assert!(store.claim_due(Utc::now()).unwrap().is_none());
    }

    #[test]
    fn auth_failure_needs_attention_and_never_requeues() {
        let mut store = fresh();
        let (_, ids) = store.create_post("hello", &["chat".to_string()]).unwrap();
        assert_eq!(Failure::classify("403 forbidden"), Failure::AuthPermission);
        assert_eq!(Failure::classify("timeout"), Failure::Transient);
        assert_eq!(Failure::classify("permanent"), Failure::Permanent);
        assert!(store.claim_due(Utc::now()).unwrap().is_some());
        store
            .mark_failed(&ids[0], Failure::classify("permission_denied"), "no scope")
            .unwrap();
        let row = row_for(&store, &ids[0]);
        assert_eq!(row.state, ATTENTION);
        assert!(row.attention);
        assert_eq!(store.next_due().unwrap(), None);
        assert!(store.claim_due(Utc::now()).unwrap().is_none());
        store.retry(&ids[0]).unwrap();
        assert_eq!(row_for(&store, &ids[0]).attempts, 0);
        assert!(store.next_due().unwrap().is_some());
    }

    #[test]
    fn cancel_only_affects_pending_and_retry_replays_terminal_rows() {
        let mut store = fresh();
        let (_, ids) = store.create_post("hello", &["a".to_string(), "b".to_string()]).unwrap();
        let (a, b) = (&ids[0], &ids[1]);
        store.cancel(a).unwrap();
        store.cancel(b).unwrap();
        assert_eq!(row_for(&store, a).state, CANCELLED);
        assert_eq!(row_for(&store, b).state, CANCELLED);
        assert!(store.cancel(a).is_err());
        assert_eq!(store.next_due().unwrap(), None);

        store.retry(a).unwrap();
        let claim = store.claim_due(Utc::now()).unwrap().expect("a is queued");
        assert_eq!(claim.delivery_id, a.to_string());
        store.mark_sent(a, Some("remote-1"), Some("https://example.test/1")).unwrap();
        assert!(store.cancel(a).is_err());
        assert!(store.retry(a).is_err());
        assert_eq!(row_for(&store, a).state, SENT);

        store.retry(b).unwrap();
        assert!(store.claim_due(Utc::now()).unwrap().is_some());
        store.mark_failed(b, Failure::Permanent, "bad text").unwrap();
        assert!(store.cancel(b).is_err());
        assert_eq!(row_for(&store, b).state, FAILED);
    }

    #[test]
    fn terminal_rows_are_never_claimed() {
        let mut store = fresh();
        let (_, ids) = store.create_post(
            "hello",
            &["sent".to_string(), "dead".to_string(), "attention".to_string()],
        )
        .unwrap();
        for _ in 0..3 {
            assert!(store.claim_due(Utc::now()).unwrap().is_some());
        }
        store.mark_sent(&ids[0], None, None).unwrap();
        store.mark_failed(&ids[1], Failure::Permanent, "nope").unwrap();
        store.mark_failed(&ids[2], Failure::AuthPermission, "no token").unwrap();
        assert!(store.claim_due(Utc::now()).unwrap().is_none());
        assert_eq!(store.in_flight_count().unwrap(), 0);
    }

    #[test]
    fn stale_due_rows_record_lateness_on_claim() {
        let mut store = fresh();
        let (post_id, ids) = store.create_post("hello", &["chat".to_string()]).unwrap();
        let when = Utc::now() - Duration::minutes(30);
        assert_eq!(store.set_next_attempt_for_post(&post_id, when).unwrap(), 1);
        store
            .conn
            .execute(
                "UPDATE deliveries SET scheduled_at = ?1 WHERE id = ?2",
                params![iso(when), &ids[0]],
            )
            .unwrap();
        assert!(store.claim_due(Utc::now()).unwrap().is_some());
        assert_eq!(row_for(&store, &ids[0]).sent_late_minutes, Some(30));
        store.mark_sent(&ids[0], None, None).unwrap();
        assert_eq!(row_for(&store, &ids[0]).sent_late_minutes, Some(30));
    }
}
