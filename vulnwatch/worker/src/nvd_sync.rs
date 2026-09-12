//! NVD feed sync into D1 (task 3.3).
//!
//! NVD supplies CVE identity, description and CVSS score, but expresses what is
//! affected as CPEs, not package versions — so it fills `advisories` and
//! `advisory_aliases`, and the version ranges come from OSV (task 3.2), joined
//! on the CVE id. NVD advisories therefore have an empty `affected` list by
//! design; do not "fix" that.
//!
//! Paging: the NVD 2.0 API returns CVEs modified within a
//! `[lastModStartDate, lastModEndDate]` window, at most 2000 per page, offset by
//! `startIndex`. This sync walks one window forward from the stored cursor,
//! writing pages until the write budget runs out or the window is exhausted,
//! then advances the cursor. A run that stops early is normal — the cursor
//! resumes it next time.

use crate::budget;
use crate::sync::SyncReport;
use vulnwatch_core::batch::write_cost;
use vulnwatch_core::feeds::nvd;
use vulnwatch_core::health;
use worker::*;

const NVD_API: &str = "https://services.nvd.nist.gov/rest/json/cves/2.0";
/// NVD's maximum page size.
const PAGE_SIZE: u32 = 2000;

/// Sync one page of NVD from the stored cursor. Kept to a single page per
/// invocation so a Worker request stays well inside its CPU and time limits;
/// cron (task 3.6) drives repeated runs, and the write-budget guard bounds each.
pub async fn sync_nvd(env: &Env) -> Result<SyncReport> {
    let db = env.d1(super::DB_BINDING)?;
    let now = health::format_epoch_millis(Date::now().as_millis() as i64);

    // Cursor holds "startIndex" into the modified-date window. Absent = start
    // at 0 over a default look-back window.
    let cursor = read_cursor(&db, "nvd").await?;
    let start_index: u32 = cursor.as_deref().and_then(|c| c.parse().ok()).unwrap_or(0);

    let api_key = env.secret("NVD_API_KEY").ok().map(|s| s.to_string());

    let body = fetch_page(start_index, api_key.as_deref()).await?;
    let advisories =
        nvd::parse_page(&body).map_err(|e| Error::RustError(format!("NVD parse: {e}")))?;
    let total = total_results(&body);

    // Budget: each advisory writes one `advisories` row (no secondary index →
    // `write_cost::ADVISORY`) plus one `advisory_aliases` row per alias, and
    // each alias row also touches `idx_aliases_alias` (→ `write_cost::ALIAS`).
    let cost: u32 = advisories
        .iter()
        .map(|a| write_cost::ADVISORY + a.aliases.len() as u32 * write_cost::ALIAS)
        .sum();

    let mut daily = budget::open(&db, budget::day_key(&now)).await?;
    let mut run = daily.run_budget();
    if !run.take(cost) {
        return Ok(SyncReport {
            feed: "nvd",
            catalog_version: format!("startIndex={start_index}"),
            rows_written: 0,
            rows_pruned: 0,
            deferred: true,
        });
    }
    daily = daily.record(cost);

    // Advance the cursor: next page, or wrap to 0 once we pass the end so the
    // feed is re-walked for modifications on the next cycle.
    let next_index = start_index + PAGE_SIZE;
    let next_cursor = if next_index >= total { 0 } else { next_index };

    // One INSERT per advisory + one per alias, plus the sync_state UPDATE and
    // the budget-persist statement.
    let total_aliases: usize = advisories.iter().map(|a| a.aliases.len()).sum();
    let mut statements = Vec::with_capacity(advisories.len() + total_aliases + 2);
    for adv in &advisories {
        statements.push(
            db.prepare(
                "INSERT INTO advisories (id, source, summary, severity, updated_at) \
                 VALUES (?1, 'nvd', ?2, ?3, ?4) \
                 ON CONFLICT(id) DO UPDATE SET \
                   summary = excluded.summary, \
                   severity = excluded.severity, \
                   updated_at = excluded.updated_at",
            )
            .bind(&[
                adv.id.as_str().into(),
                adv.summary.as_str().into(),
                adv.severity.as_db_str().into(),
                now.as_str().into(),
            ])?,
        );
        for alias in &adv.aliases {
            statements.push(
                db.prepare(
                    "INSERT INTO advisory_aliases (advisory_id, alias) VALUES (?1, ?2) \
                     ON CONFLICT(advisory_id, alias) DO NOTHING",
                )
                .bind(&[adv.id.as_str().into(), alias.as_str().into()])?,
            );
        }
    }

    statements.push(
        db.prepare(
            "UPDATE sync_state \
             SET cursor = ?1, last_success = ?2, last_error = NULL, rows_written = ?3 \
             WHERE feed = 'nvd'",
        )
        .bind(&[
            next_cursor.to_string().as_str().into(),
            now.as_str().into(),
            (advisories.len() as f64).into(),
        ])?,
    );
    statements.push(budget::persist_statement(&db, &daily)?);

    db.batch(statements).await?;

    Ok(SyncReport {
        feed: "nvd",
        catalog_version: format!("startIndex={start_index}->{next_cursor} of {total}"),
        rows_written: advisories.len(),
        rows_pruned: 0,
        deferred: false,
    })
}

async fn read_cursor(db: &D1Database, feed: &str) -> Result<Option<String>> {
    let row: Option<serde_json::Value> = db
        .prepare("SELECT cursor FROM sync_state WHERE feed = ?1")
        .bind(&[feed.into()])?
        .first(None)
        .await?;
    Ok(row.and_then(|v| {
        v.get("cursor")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
    }))
}

fn total_results(body: &str) -> u32 {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|v| v.get("totalResults").and_then(serde_json::Value::as_u64))
        .unwrap_or(0) as u32
}

/// Fetch one NVD page. Sends the API key header when one is configured, which
/// lifts the rate limit; without it the API still works, just slower.
async fn fetch_page(start_index: u32, api_key: Option<&str>) -> Result<String> {
    let url = format!("{NVD_API}?resultsPerPage={PAGE_SIZE}&startIndex={start_index}");
    let headers = Headers::new();
    if let Some(key) = api_key {
        headers.set("apiKey", key)?;
    }
    let req = Request::new_with_init(
        &url,
        RequestInit::new()
            .with_method(Method::Get)
            .with_headers(headers),
    )?;
    let mut resp = Fetch::Request(req).send().await?;
    let status = resp.status_code();
    if status != 200 {
        return Err(Error::RustError(format!(
            "NVD fetch returned HTTP {status}"
        )));
    }
    resp.text().await
}
