//! Ingest routes: authenticated write paths that take data from the collector
//! on elysium (tasks 4.4, 5.1 groundwork). This is the first authenticated
//! route in the Worker (task 7.1), so the bearer-token check lives here and is
//! reused as more ingest routes land.
//!
//! Everything crossing this boundary is untrusted: the token is checked first,
//! the payload size is bounded, the contract version is verified, and each
//! range is validated before it becomes a row. A range whose CVE is not already
//! a known advisory is dropped, not invented — `advisory_ranges.advisory_id`
//! has a foreign key to `advisories`.

use crate::budget;
use vulnwatch_core::health;
use vulnwatch_core::ingest::OsvIngest;
use vulnwatch_core::package::Ecosystem;
use worker::*;

/// Largest ingest body accepted, in bytes. The collector chunks its POSTs, so
/// no single request should approach this; anything larger is refused rather
/// than buffered.
const MAX_BODY_BYTES: usize = 4 * 1024 * 1024;

/// Check the `Authorization: Bearer <token>` header against the `INGEST_TOKEN`
/// secret. Returns `Ok(())` only on an exact match. Missing secret => closed:
/// with no token configured, every write is refused rather than allowed.
pub fn check_auth(req: &Request, env: &Env) -> Result<()> {
    let configured = env
        .secret("INGEST_TOKEN")
        .map(|s| s.to_string())
        .map_err(|_| Error::RustError("ingest token not configured".into()))?;

    let presented = req
        .headers()
        .get("Authorization")?
        .and_then(|h| h.strip_prefix("Bearer ").map(str::to_owned));

    match presented {
        Some(token) if constant_time_eq(token.as_bytes(), configured.as_bytes()) => Ok(()),
        _ => Err(Error::RustError("unauthorized".into())),
    }
}

/// Return the subset of the payload's CVE ids that already exist in
/// `advisories`. Queried in chunks well under D1's 100-bound-parameter limit,
/// so a range for a CVE we do not track is dropped before it costs any budget.
async fn known_advisories(
    db: &D1Database,
    ranges: &[&vulnwatch_core::ingest::OsvRange],
) -> Result<std::collections::HashSet<String>> {
    use std::collections::HashSet;

    // Distinct CVE ids in this batch.
    let mut ids: Vec<&str> = ranges.iter().map(|r| r.cve.as_str()).collect();
    ids.sort_unstable();
    ids.dedup();

    let mut known = HashSet::new();
    // 90 keeps each IN(...) safely under the 100-param ceiling.
    for chunk in ids.chunks(90) {
        let placeholders = (1..=chunk.len())
            .map(|i| format!("?{i}"))
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!("SELECT id FROM advisories WHERE id IN ({placeholders})");
        let binds: Vec<wasm_bindgen::JsValue> = chunk.iter().map(|id| (*id).into()).collect();
        let rows: Vec<serde_json::Value> = db.prepare(&sql).bind(&binds)?.all().await?.results()?;
        for row in rows {
            if let Some(id) = row.get("id").and_then(serde_json::Value::as_str) {
                known.insert(id.to_owned());
            }
        }
    }
    Ok(known)
}

/// Length-checked, branch-independent byte comparison, so a wrong token cannot
/// be recovered a character at a time from response timing.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// Handle `POST /ingest/osv`: validate the payload and write the ranges,
/// budget-guarded. Returns a JSON summary of what was accepted, dropped, and
/// deferred.
pub async fn ingest_osv(mut req: Request, env: &Env) -> Result<Response> {
    // Bound the body before reading it as JSON.
    let raw = req.text().await?;
    if raw.len() > MAX_BODY_BYTES {
        return Response::error("Payload too large", 413);
    }

    let payload: OsvIngest = match serde_json::from_str(&raw) {
        Ok(p) => p,
        Err(e) => {
            console_error!("ingest osv: bad JSON: {e}");
            return Response::error("Bad request", 400);
        }
    };
    if !payload.is_supported() {
        return Response::error("Unsupported ingest version", 409);
    }

    let db = env.d1(super::DB_BINDING)?;
    let now = health::format_epoch_millis(Date::now().as_millis() as i64);

    // Validate every range at the boundary. A bad ecosystem or empty field is a
    // dropped range, not a rejected batch — one malformed record should not
    // discard thousands of good ones. Count drops so the collector can notice.
    let mut valid: Vec<&vulnwatch_core::ingest::OsvRange> = Vec::new();
    let mut dropped_invalid = 0usize;
    for r in &payload.ranges {
        let ok = Ecosystem::from_db_str(&r.ecosystem).is_some()
            && !r.cve.is_empty()
            && !r.package.is_empty()
            && !r.introduced.is_empty();
        if ok {
            valid.push(r);
        } else {
            dropped_invalid += 1;
        }
    }

    // Keep only ranges whose CVE we actually track. Budget is scarce, so we
    // must NOT spend it on ranges that the foreign key would discard anyway —
    // check existence in `advisories` up front and drop the rest for free. This
    // is what keeps a 3.6M-range OSV feed from burning the daily cap on rows
    // that never land (only CVEs NVD has synced can match).
    let known = known_advisories(&db, &valid).await?;
    let mut writable: Vec<&vulnwatch_core::ingest::OsvRange> = Vec::new();
    let mut dropped_unknown = 0usize;
    for r in valid {
        if known.contains(&r.cve) {
            writable.push(r);
        } else {
            dropped_unknown += 1;
        }
    }

    // Budget: one row per range that will actually be written.
    let cost = writable.len() as u32;
    let mut daily = budget::open(&db, budget::day_key(&now)).await?;
    let mut run = daily.run_budget();
    if !run.take(cost) {
        return Response::from_json(&serde_json::json!({
            "accepted": 0,
            "dropped_invalid": dropped_invalid,
            "dropped_unknown": dropped_unknown,
            "deferred": true,
        }));
    }
    daily = daily.record(cost);

    let mut statements = Vec::with_capacity(writable.len() + 1);
    for r in &writable {
        statements.push(
            db.prepare(
                "INSERT INTO advisory_ranges (advisory_id, ecosystem, package, introduced, fixed) \
                 VALUES (?1, ?2, ?3, ?4, ?5) \
                 ON CONFLICT(advisory_id, ecosystem, package, introduced) DO UPDATE SET \
                   fixed = excluded.fixed",
            )
            .bind(&[
                r.cve.as_str().into(),
                r.ecosystem.as_str().into(),
                r.package.as_str().into(),
                r.introduced.as_str().into(),
                match &r.fixed {
                    Some(f) => f.as_str().into(),
                    None => wasm_bindgen::JsValue::NULL,
                },
            ])?,
        );
    }
    statements.push(budget::persist_statement(&db, &daily)?);

    db.batch(statements).await?;

    console_log!(
        "ingest osv: accepted={} dropped_invalid={} dropped_unknown={} sources={:?}",
        writable.len(),
        dropped_invalid,
        dropped_unknown,
        payload.sources
    );

    Response::from_json(&serde_json::json!({
        "accepted": writable.len(),
        "dropped_invalid": dropped_invalid,
        "dropped_unknown": dropped_unknown,
        "deferred": false,
    }))
}
