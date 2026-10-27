use std::{fs, path::Path, time::Duration as StdDuration};

use chrono::{Duration, Local, Utc};
use rusqlite::{params, Connection, OptionalExtension, Row, TransactionBehavior};
use serde::Serialize;

use crate::{config::Limits, error::AppError};

const BUSY_TIMEOUT_MS: u64 = 5_000;
/// Pending rows older than this belong to crashed processes and stop counting against the budget.
const STALE_PENDING_MINUTES: i64 = 30;
const TIMESTAMP_FORMAT: &str = "%Y-%m-%dT%H:%M:%SZ";
const DAY_FORMAT: &str = "%Y-%m-%d";

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS calls (
    id               INTEGER PRIMARY KEY AUTOINCREMENT,
    created_at       TEXT    NOT NULL,
    day              TEXT    NOT NULL,
    project          TEXT    NOT NULL,
    capability       TEXT    NOT NULL,
    provider         TEXT    NOT NULL,
    model            TEXT    NOT NULL,
    prompt           TEXT    NOT NULL,
    output_path      TEXT,
    status           TEXT    NOT NULL,
    estimated_usd    REAL    NOT NULL,
    cost_usd         REAL,
    cost_is_estimate INTEGER NOT NULL DEFAULT 1,
    error            TEXT,
    duration_ms      INTEGER
);
CREATE INDEX IF NOT EXISTS ix_calls_day_project ON calls (day, project);
CREATE INDEX IF NOT EXISTS ix_calls_status_created ON calls (status, created_at);
";

/// Pending calls count at their estimate so parallel agents cannot all pass the same budget check.
const SPENT_EXPR: &str =
    "COALESCE(SUM(CASE WHEN status = 'pending' THEN estimated_usd ELSE COALESCE(cost_usd, 0) END), 0)";

pub struct NewCall<'a> {
    pub project: &'a str,
    pub capability: &'a str,
    pub provider: &'a str,
    pub model: &'a str,
    pub prompt: &'a str,
    pub estimated_usd: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct CallRecord {
    pub id: i64,
    pub created_at: String,
    pub project: String,
    pub capability: String,
    pub provider: String,
    pub model: String,
    pub prompt: String,
    pub output_path: Option<String>,
    pub status: String,
    pub estimated_usd: f64,
    pub cost_usd: Option<f64>,
    pub cost_is_estimate: bool,
    pub error: Option<String>,
    pub duration_ms: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DayUsage {
    pub day: String,
    pub calls: i64,
    pub cost_usd: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProviderUsage {
    pub provider: String,
    pub calls: i64,
    pub cost_usd: f64,
}

const CALL_COLUMNS: &str = "id, created_at, project, capability, provider, model, prompt, output_path, status,
    estimated_usd, cost_usd, cost_is_estimate, error, duration_ms";

pub struct Store {
    conn: Connection,
}

impl Store {
    pub fn open(path: &Path) -> Result<Self, AppError> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        Self::init(Connection::open(path)?)
    }

    pub fn open_in_memory() -> Result<Self, AppError> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Self, AppError> {
        conn.busy_timeout(StdDuration::from_millis(BUSY_TIMEOUT_MS))?;
        conn.query_row("PRAGMA journal_mode = WAL", [], |_| Ok(()))?;
        conn.execute_batch(SCHEMA)?;
        Ok(Self { conn })
    }

    /// Checks every limit and records a pending call in one write transaction.
    pub fn reserve(&mut self, call: &NewCall, limits: &Limits) -> Result<i64, AppError> {
        let estimate = call.estimated_usd;
        if estimate > limits.max_call_usd {
            return Err(AppError::Budget(format!(
                "estimated cost ${estimate:.3} exceeds the per-call limit ${:.2} (limits.max_call_usd)",
                limits.max_call_usd
            )));
        }

        let now = Utc::now();
        let day = today();
        let tx = self.conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let stale_cutoff = (now - Duration::minutes(STALE_PENDING_MINUTES)).format(TIMESTAMP_FORMAT).to_string();
        tx.execute(
            "UPDATE calls SET status = 'abandoned', cost_usd = 0 WHERE status = 'pending' AND created_at < ?1",
            params![stale_cutoff],
        )?;

        let day_spent = spent(&tx, &day, None)?;
        if day_spent + estimate > limits.daily_usd {
            return Err(AppError::Budget(format!(
                "daily limit reached: spent ${day_spent:.3} of ${:.2}, this call needs ~${estimate:.3} (limits.daily_usd)",
                limits.daily_usd
            )));
        }
        let project_spent = spent(&tx, &day, Some(call.project))?;
        if project_spent + estimate > limits.project_daily_usd {
            return Err(AppError::Budget(format!(
                "project daily limit reached: spent ${project_spent:.3} of ${:.2}, this call needs ~${estimate:.3} (limits.project_daily_usd)",
                limits.project_daily_usd
            )));
        }

        tx.execute(
            "INSERT INTO calls (created_at, day, project, capability, provider, model, prompt, status, estimated_usd)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'pending', ?8)",
            params![
                now.format(TIMESTAMP_FORMAT).to_string(),
                day,
                call.project,
                call.capability,
                call.provider,
                call.model,
                call.prompt,
                estimate
            ],
        )?;
        let id = tx.last_insert_rowid();
        tx.commit()?;
        Ok(id)
    }

    pub fn finish_success(
        &self,
        id: i64,
        cost_usd: f64,
        cost_is_estimate: bool,
        output_path: Option<&str>,
        duration_ms: i64,
    ) -> Result<(), AppError> {
        self.conn.execute(
            "UPDATE calls SET status = 'success', cost_usd = ?2, cost_is_estimate = ?3, output_path = ?4, duration_ms = ?5
             WHERE id = ?1",
            params![id, cost_usd, cost_is_estimate, output_path, duration_ms],
        )?;
        Ok(())
    }

    /// Failed calls cost nothing: providers do not bill rejected or errored requests.
    pub fn finish_failure(&self, id: i64, error: &str, duration_ms: i64) -> Result<(), AppError> {
        self.conn.execute(
            "UPDATE calls SET status = 'failed', cost_usd = 0, cost_is_estimate = 0, error = ?2, duration_ms = ?3
             WHERE id = ?1",
            params![id, error, duration_ms],
        )?;
        Ok(())
    }

    pub fn spent_today(&self, project: Option<&str>) -> Result<f64, AppError> {
        spent(&self.conn, &today(), project)
    }

    pub fn recent(&self, limit: u32) -> Result<Vec<CallRecord>, AppError> {
        let sql = format!("SELECT {CALL_COLUMNS} FROM calls ORDER BY id DESC LIMIT ?1");
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params![limit], call_record)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    /// Successful media generations that produced a file, newest first.
    pub fn assets(&self, limit: u32) -> Result<Vec<CallRecord>, AppError> {
        let sql = format!(
            "SELECT {CALL_COLUMNS} FROM calls
             WHERE status = 'success' AND output_path IS NOT NULL AND capability IN ('image', 'audio', 'speech')
             ORDER BY id DESC LIMIT ?1"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params![limit], call_record)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn get(&self, id: i64) -> Result<Option<CallRecord>, AppError> {
        let sql = format!("SELECT {CALL_COLUMNS} FROM calls WHERE id = ?1");
        Ok(self.conn.query_row(&sql, params![id], call_record).optional()?)
    }

    pub fn usage_by_day(&self, days: u32, project: Option<&str>) -> Result<Vec<DayUsage>, AppError> {
        let sql = format!(
            "SELECT day, COUNT(*), {SPENT_EXPR} FROM calls
             WHERE day >= ?1 AND (?2 IS NULL OR project = ?2)
             GROUP BY day ORDER BY day DESC"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params![first_day(days), project], |row| {
            Ok(DayUsage { day: row.get(0)?, calls: row.get(1)?, cost_usd: row.get(2)? })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn usage_by_provider(&self, days: u32) -> Result<Vec<ProviderUsage>, AppError> {
        let sql = format!(
            "SELECT provider, COUNT(*), {SPENT_EXPR} FROM calls
             WHERE day >= ?1
             GROUP BY provider ORDER BY 3 DESC"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params![first_day(days)], |row| {
            Ok(ProviderUsage { provider: row.get(0)?, calls: row.get(1)?, cost_usd: row.get(2)? })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }
}

fn today() -> String {
    Local::now().format(DAY_FORMAT).to_string()
}

/// First day of a window of `days` days ending today.
fn first_day(days: u32) -> String {
    let first = Local::now().date_naive() - Duration::days(i64::from(days.max(1)) - 1);
    first.format(DAY_FORMAT).to_string()
}

fn spent(conn: &Connection, day: &str, project: Option<&str>) -> Result<f64, AppError> {
    let sql = format!("SELECT {SPENT_EXPR} FROM calls WHERE day = ?1 AND (?2 IS NULL OR project = ?2)");
    Ok(conn.query_row(&sql, params![day, project], |row| row.get(0))?)
}

fn call_record(row: &Row) -> rusqlite::Result<CallRecord> {
    Ok(CallRecord {
        id: row.get(0)?,
        created_at: row.get(1)?,
        project: row.get(2)?,
        capability: row.get(3)?,
        provider: row.get(4)?,
        model: row.get(5)?,
        prompt: row.get(6)?,
        output_path: row.get(7)?,
        status: row.get(8)?,
        estimated_usd: row.get(9)?,
        cost_usd: row.get(10)?,
        cost_is_estimate: row.get(11)?,
        error: row.get(12)?,
        duration_ms: row.get(13)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(project: &str, estimated_usd: f64) -> NewCall<'_> {
        NewCall { project, capability: "image", provider: "openai", model: "m", prompt: "p", estimated_usd }
    }

    fn limits(daily: f64, project: f64, per_call: f64) -> Limits {
        Limits { daily_usd: daily, project_daily_usd: project, max_call_usd: per_call }
    }

    #[test]
    fn pending_reservations_count_against_budget() {
        let mut store = Store::open_in_memory().unwrap();
        let limits = limits(1.0, 1.0, 1.0);
        store.reserve(&call("a", 0.6), &limits).unwrap();
        let err = store.reserve(&call("a", 0.6), &limits).unwrap_err();
        assert_eq!(err.code(), "budget_exceeded");
    }

    #[test]
    fn failed_calls_release_their_reservation() {
        let mut store = Store::open_in_memory().unwrap();
        let limits = limits(1.0, 1.0, 1.0);
        let id = store.reserve(&call("a", 0.6), &limits).unwrap();
        store.finish_failure(id, "boom", 10).unwrap();
        store.reserve(&call("a", 0.6), &limits).unwrap();
    }

    #[test]
    fn project_limit_is_per_project() {
        let mut store = Store::open_in_memory().unwrap();
        let limits = limits(10.0, 0.5, 1.0);
        store.reserve(&call("a", 0.4), &limits).unwrap();
        assert!(store.reserve(&call("a", 0.4), &limits).is_err());
        store.reserve(&call("b", 0.4), &limits).unwrap();
    }

    #[test]
    fn per_call_limit_rejects_before_writing() {
        let mut store = Store::open_in_memory().unwrap();
        assert!(store.reserve(&call("a", 2.0), &limits(10.0, 10.0, 1.0)).is_err());
        assert!(store.recent(10).unwrap().is_empty());
    }

    #[test]
    fn success_uses_actual_cost() {
        let mut store = Store::open_in_memory().unwrap();
        let id = store.reserve(&call("a", 0.5), &limits(10.0, 10.0, 1.0)).unwrap();
        store.finish_success(id, 0.1, false, Some("out.png"), 5).unwrap();
        assert!((store.spent_today(Some("a")).unwrap() - 0.1).abs() < 1e-9);
        let usage = store.usage_by_day(7, None).unwrap();
        assert_eq!(usage.len(), 1);
        assert_eq!(usage[0].calls, 1);
        let by_provider = store.usage_by_provider(7).unwrap();
        assert_eq!(by_provider[0].provider, "openai");
        assert!((by_provider[0].cost_usd - 0.1).abs() < 1e-9);
    }

    #[test]
    fn assets_lists_only_successful_files() {
        let mut store = Store::open_in_memory().unwrap();
        let limits = limits(10.0, 10.0, 1.0);
        let ok = store.reserve(&call("a", 0.1), &limits).unwrap();
        store.finish_success(ok, 0.1, true, Some("a.png"), 5).unwrap();
        let failed = store.reserve(&call("a", 0.1), &limits).unwrap();
        store.finish_failure(failed, "boom", 5).unwrap();

        let assets = store.assets(10).unwrap();
        assert_eq!(assets.len(), 1);
        assert_eq!(assets[0].id, ok);
        assert_eq!(store.get(ok).unwrap().unwrap().output_path.as_deref(), Some("a.png"));
        assert!(store.get(9_999).unwrap().is_none());
    }
}
