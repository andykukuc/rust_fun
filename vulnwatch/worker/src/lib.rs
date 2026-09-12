//! Cloudflare Worker entry point.
//!
//! Task 3.1: bind D1 and make `/health` report each feed's cursor age, so a
//! sync that has quietly stopped is visible without opening the database. The
//! freshness arithmetic lives in `vulnwatch_core::health` (pure, tested); this
//! file only reads rows and shapes the response.

mod budget;
mod nvd_sync;
mod sync;

use serde_json::json;
use vulnwatch_core::health::Freshness;
use worker::*;

/// The D1 binding name, matching `d1_databases[].binding` in wrangler.jsonc.
pub(crate) const DB_BINDING: &str = "DB";

/// Per-feed staleness thresholds, in seconds, aligned to the planned cron
/// cadence in task 3.6 with generous slack for a run that stopped on the write
/// budget and has not yet caught up. A feed older than this needs attention.
///
/// KEV daily, OSV daily -> two days. NVD every six hours -> one day.
fn stale_after_secs(feed: &str) -> i64 {
    match feed {
        "nvd" => 86_400,          // 24h
        "kev" | "osv" => 172_800, // 48h
        _ => 172_800,
    }
}

#[event(fetch)]
async fn fetch(request: Request, env: Env, _context: Context) -> Result<Response> {
    match request.path().as_str() {
        "/health" => health(&env).await,
        // Manual sync trigger for KEV (task 3.4). Cron will call the same code
        // in task 3.6. NOTE: this is a write path with no auth yet — it must
        // be gated (Cloudflare Access / task 7.1) before the hostname is used
        // for anything beyond /health.
        "/sync/kev" => run_sync(&env, "kev", sync::sync_kev(&env).await).await,
        "/sync/nvd" => run_sync(&env, "nvd", nvd_sync::sync_nvd(&env).await).await,
        _ => Response::error("Not found", 404),
    }
}

/// Shape a sync outcome into a JSON response, logging success and recording
/// failure to `sync_state` so `/health` surfaces it. Shared by every feed.
async fn run_sync(env: &Env, feed: &str, result: Result<sync::SyncReport>) -> Result<Response> {
    match result {
        Ok(report) => {
            console_log!(
                "sync {}: version={} written={} pruned={} deferred={}",
                report.feed,
                report.catalog_version,
                report.rows_written,
                report.rows_pruned,
                report.deferred
            );
            Response::from_json(&json!({
                "feed": report.feed,
                "catalog_version": report.catalog_version,
                "rows_written": report.rows_written,
                "rows_pruned": report.rows_pruned,
                "deferred": report.deferred,
            }))
        }
        Err(e) => {
            console_error!("sync {feed} failed: {e}");
            record_sync_error(env, feed, &e.to_string()).await;
            Response::error("Sync failed", 502)
        }
    }
}

/// Best-effort write of a sync failure to `sync_state.last_error`, so a broken
/// feed is visible in `/health` rather than only in the logs.
async fn record_sync_error(env: &Env, feed: &str, message: &str) {
    let attempt = || async {
        let db = env.d1(DB_BINDING)?;
        db.prepare("UPDATE sync_state SET last_error = ?1 WHERE feed = ?2")
            .bind(&[message.into(), feed.into()])?
            .run()
            .await?;
        Ok::<(), Error>(())
    };
    if let Err(e) = attempt().await {
        console_error!("could not record sync error for {feed}: {e}");
    }
}

/// Report each feed's freshness from `sync_state`. Returns 200 when every feed
/// is fresh, 503 when any feed has never synced or has gone stale, so an
/// uptime check alone surfaces a silently stalled pipeline.
async fn health(env: &Env) -> Result<Response> {
    let now_secs = (Date::now().as_millis() / 1000) as i64;

    let rows = match read_sync_state(env).await {
        Ok(rows) => rows,
        Err(e) => {
            // Detail server-side only; the client sees a generic message.
            console_error!("health: reading sync_state failed: {e}");
            return Response::error("Service unavailable", 503);
        }
    };

    let mut feeds = serde_json::Map::new();
    let mut all_ok = true;

    for row in &rows {
        let freshness = match Freshness::evaluate(
            row.last_success.as_deref(),
            now_secs,
            stale_after_secs(&row.feed),
        ) {
            Ok(f) => f,
            Err(e) => {
                // A malformed timestamp is a writer bug; treat the feed as
                // needing attention rather than reporting it healthy.
                console_error!("health: bad last_success for {}: {e}", row.feed);
                all_ok = false;
                feeds.insert(row.feed.clone(), json!({ "status": "invalid_timestamp" }));
                continue;
            }
        };

        if freshness.needs_attention() {
            all_ok = false;
        }

        let entry = match freshness {
            Freshness::NeverSynced => json!({
                "status": "never_synced",
                "rows_written": row.rows_written,
            }),
            Freshness::Synced { age_secs, stale } => json!({
                "status": if stale { "stale" } else { "fresh" },
                "age_secs": age_secs,
                "last_success": row.last_success,
                "last_error": row.last_error,
                "rows_written": row.rows_written,
            }),
        };
        feeds.insert(row.feed.clone(), entry);
    }

    let body = json!({
        "ok": all_ok,
        "now": now_secs,
        "feeds": feeds,
    });

    let status = if all_ok { 200 } else { 503 };
    let mut resp = Response::from_json(&body)?;
    resp = resp.with_status(status);
    Ok(resp)
}

/// One `sync_state` row, as read for the health report.
struct SyncRow {
    feed: String,
    last_success: Option<String>,
    last_error: Option<String>,
    rows_written: i64,
}

/// Read every feed's cursor bookkeeping. No user input reaches this query, but
/// it uses the typed statement API rather than string building on principle.
async fn read_sync_state(env: &Env) -> Result<Vec<SyncRow>> {
    let db = env.d1(DB_BINDING)?;
    let stmt = db.prepare(
        "SELECT feed, last_success, last_error, rows_written \
         FROM sync_state ORDER BY feed",
    );
    let result = stmt.all().await?;

    #[derive(serde::Deserialize)]
    struct Raw {
        feed: String,
        last_success: Option<String>,
        last_error: Option<String>,
        rows_written: i64,
    }

    let raw: Vec<Raw> = result.results()?;
    Ok(raw
        .into_iter()
        .map(|r| SyncRow {
            feed: r.feed,
            last_success: r.last_success,
            last_error: r.last_error,
            rows_written: r.rows_written,
        })
        .collect())
}
