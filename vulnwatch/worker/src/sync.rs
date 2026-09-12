//! Feed sync into D1.
//!
//! Task 3.4: KEV, the smallest feed, proves the pipeline end to end. The KEV
//! catalog is a full snapshot, so a run mirrors it exactly — one `DELETE`
//! then one `INSERT` per entry — inside a single D1 batch, which D1 applies
//! atomically so the table is never observed empty or half-updated.
//! `sync_state` records the catalog version as the cursor and stamps an
//! RFC 3339 `last_success` on completion.
//!
//! KEV is a few thousand rows against a single-column table with no index of
//! its own, so it fits comfortably under the daily write budget in one run.
//! The budget guard (task 3.5) still wraps the large feeds; KEV does not need
//! to defer, but it reports `rows_written` so the accounting stays honest.

use vulnwatch_core::feeds::kev;
use vulnwatch_core::health;
use worker::*;

/// The public CISA Known Exploited Vulnerabilities catalog (JSON).
const KEV_URL: &str =
    "https://www.cisa.gov/sites/default/files/feeds/known_exploited_vulnerabilities.json";

/// Outcome of a sync run, returned to the caller (route or cron) for logging.
#[derive(Debug)]
pub struct SyncReport {
    pub feed: &'static str,
    pub catalog_version: String,
    pub rows_written: usize,
    pub rows_pruned: usize,
    /// True when the run wrote nothing because the daily budget was exhausted.
    /// A normal outcome, not a failure.
    pub deferred: bool,
}

/// Fetch, parse, and mirror the KEV catalog into D1.
pub async fn sync_kev(env: &Env) -> Result<SyncReport> {
    let body = fetch_catalog().await?;

    let catalog = kev::parse(&body).map_err(|e| Error::RustError(format!("KEV parse: {e}")))?;

    let db = env.d1(super::DB_BINDING)?;
    // RFC 3339 (`...Z`), the one format `core::health` parses. `Date::to_string`
    // gives a human string the freshness check rightly rejects, so format the
    // epoch millis ourselves through the tested core helper.
    let now = health::format_epoch_millis(Date::now().as_millis() as i64);

    // Refuse to mirror an empty catalog: the real feed never sends one, so an
    // empty parse means a broken fetch, and clearing KEV against it would wipe
    // the highest-signal table in the database.
    if catalog.entries.is_empty() {
        return Err(Error::RustError(
            "refusing to sync KEV against an empty catalog".into(),
        ));
    }

    // Write-budget guard (task 3.5). KEV is a single atomic snapshot rebuild
    // that always fits under the daily cap, so it writes all-or-nothing rather
    // than deferring a partial catalog. But it still checks and records against
    // the shared daily counter, so the accounting stays honest and the large
    // feeds (OSV/NVD) inherit a proven mechanism. Cost is priced as one write
    // per catalog row (the table carries no secondary index).
    let cost = catalog.entries.len() as u32;
    let mut daily = super::budget::open(&db, super::budget::day_key(&now)).await?;
    let mut run = daily.run_budget();
    if !run.take(cost) {
        // Not enough budget left today for a full KEV snapshot. Leave the table
        // and cursor untouched and try again after the daily reset. This is a
        // normal outcome, not an error.
        return Ok(SyncReport {
            feed: "kev",
            catalog_version: catalog.version,
            rows_written: 0,
            rows_pruned: 0,
            deferred: true,
        });
    }
    daily = daily.record(cost);

    // KEV is a full snapshot. Rebuild the table to match the catalog exactly:
    // a DELETE followed by one INSERT per entry, all in a single D1 batch.
    // D1 applies a batch atomically, so the table is never observed empty, and
    // this avoids the 100-bound-parameter ceiling a `NOT IN (...)` prune hits
    // once the catalog exceeds 100 CVEs (it always does).
    let mut statements = Vec::with_capacity(catalog.entries.len() + 2);
    statements.push(db.prepare("DELETE FROM kev"));

    for (cve_id, version, date_added) in catalog.rows() {
        statements.push(
            db.prepare("INSERT INTO kev (cve_id, catalog_version, date_added) VALUES (?1, ?2, ?3)")
                .bind(&[cve_id.into(), version.into(), json_or_null(date_added)])?,
        );
    }

    // Cursor + freshness bookkeeping, in the same batch as the data so the two
    // can never disagree.
    statements.push(
        db.prepare(
            "UPDATE sync_state \
             SET cursor = ?1, last_success = ?2, last_error = NULL, rows_written = ?3 \
             WHERE feed = 'kev'",
        )
        .bind(&[
            catalog.version.as_str().into(),
            now.as_str().into(),
            (catalog.entries.len() as f64).into(),
        ])?,
    );

    // Persist the updated daily counter in the same batch, so the write count
    // and the writes it accounts for commit together.
    statements.push(super::budget::persist_statement(&db, &daily)?);

    let before = count_kev(&db).await?;
    db.batch(statements).await?;
    let after = count_kev(&db).await?;
    let rows_pruned = before.saturating_sub(after.min(before));

    Ok(SyncReport {
        feed: "kev",
        catalog_version: catalog.version,
        rows_written: catalog.entries.len(),
        rows_pruned,
        deferred: false,
    })
}

async fn count_kev(db: &D1Database) -> Result<usize> {
    let row: Option<serde_json::Value> = db
        .prepare("SELECT COUNT(*) AS n FROM kev")
        .first(None)
        .await?;
    let n = row
        .and_then(|v| v.get("n").and_then(serde_json::Value::as_u64))
        .unwrap_or(0);
    Ok(n as usize)
}

/// Fetch the catalog over HTTPS, failing loudly on a non-200 rather than
/// treating an error page as an empty catalog.
async fn fetch_catalog() -> Result<String> {
    let mut resp = Fetch::Url(
        KEV_URL
            .parse()
            .map_err(|_| Error::RustError("bad KEV URL".into()))?,
    )
    .send()
    .await?;
    let status = resp.status_code();
    if status != 200 {
        return Err(Error::RustError(format!(
            "KEV fetch returned HTTP {status}"
        )));
    }
    resp.text().await
}

fn json_or_null(v: Option<&str>) -> wasm_bindgen::JsValue {
    match v {
        Some(s) => s.into(),
        None => wasm_bindgen::JsValue::NULL,
    }
}
