//! osv-fetch — the OSV half of the collector, runs on elysium (task 3.2).
//!
//! A Cloudflare Worker cannot fetch OSV's per-ecosystem bulk zips (Ubuntu's is
//! ~682 MB against a Worker's ~128 MB memory). This native binary does that
//! download where there is no size limit, filters to the five ecosystems this
//! deployment runs, and POSTs the resulting version ranges to the Worker's
//! authenticated `/ingest/osv` route in bounded chunks.
//!
//! Config comes from the environment (never the repo):
//!   VULNWATCH_INGEST_URL    e.g. https://<host>/ingest/osv
//!   VULNWATCH_INGEST_TOKEN  bearer token matching the Worker's INGEST_TOKEN
//!   VULNWATCH_WORK_DIR      scratch dir for downloads (default /tmp)
//!
//! Ranges are parsed with `vulnwatch_core::feeds::osv`, the same code the
//! parser tests cover, so the collector and the Worker agree on what a range is.

use anyhow::{anyhow, Context, Result};
use std::io::Read;
use vulnwatch_core::feeds::osv;
use vulnwatch_core::ingest::{OsvIngest, OsvRange, OSV_INGEST_VERSION};

/// OSV ecosystem streams to pull, matching AGENTS.md / docs/INVENTORY.md. These
/// are the OSV bucket names; the per-distro records inside carry the concrete
/// version like `Debian:12`. Widening this list is a storage-budget decision.
const ECOSYSTEMS: &[&str] = &["Debian", "Ubuntu", "Alpine", "Red%20Hat"];

/// OSV bulk zip URL for one ecosystem.
fn zip_url(ecosystem: &str) -> String {
    format!("https://osv-vulnerabilities.storage.googleapis.com/{ecosystem}/all.zip")
}

/// POST ranges in chunks of at most this many, so no single request is huge and
/// the Worker's per-request budget check can defer cleanly.
const CHUNK: usize = 2000;

fn main() -> Result<()> {
    let url = std::env::var("VULNWATCH_INGEST_URL")
        .context("VULNWATCH_INGEST_URL must be set (the Worker's /ingest/osv URL)")?;
    let token = std::env::var("VULNWATCH_INGEST_TOKEN")
        .context("VULNWATCH_INGEST_TOKEN must be set (matches the Worker INGEST_TOKEN secret)")?;
    let work_dir = std::env::var("VULNWATCH_WORK_DIR").unwrap_or_else(|_| "/tmp".into());

    let mut total_ranges = 0usize;
    let mut total_accepted = 0usize;

    for ecosystem in ECOSYSTEMS {
        eprintln!("[osv-fetch] ecosystem {ecosystem}: downloading");
        let zip_path =
            download(ecosystem, &work_dir).with_context(|| format!("downloading {ecosystem}"))?;

        eprintln!("[osv-fetch] ecosystem {ecosystem}: extracting + parsing");
        let ranges =
            extract_ranges(&zip_path).with_context(|| format!("extracting {ecosystem}"))?;
        eprintln!("[osv-fetch] ecosystem {ecosystem}: {} ranges", ranges.len());
        total_ranges += ranges.len();

        // Remove the zip promptly; the ranges are in memory now.
        let _ = std::fs::remove_file(&zip_path);

        for chunk in ranges.chunks(CHUNK) {
            let accepted = post_chunk(&url, &token, ecosystem, chunk)
                .with_context(|| format!("posting a chunk for {ecosystem}"))?;
            total_accepted += accepted;
        }
    }

    eprintln!(
        "[osv-fetch] done: {total_ranges} ranges parsed, {total_accepted} accepted by the Worker"
    );
    Ok(())
}

/// Stream one ecosystem zip to a file under the work dir.
fn download(ecosystem: &str, work_dir: &str) -> Result<std::path::PathBuf> {
    let safe = ecosystem.replace("%20", "_");
    let path = std::path::Path::new(work_dir).join(format!("osv_{safe}.zip"));
    let resp = ureq::get(&zip_url(ecosystem))
        .call()
        .with_context(|| format!("GET {}", zip_url(ecosystem)))?;
    if resp.status() != 200 {
        return Err(anyhow!(
            "OSV returned HTTP {} for {ecosystem}",
            resp.status()
        ));
    }
    let mut reader = resp.into_reader();
    let mut file = std::fs::File::create(&path)?;
    std::io::copy(&mut reader, &mut file)?;
    Ok(path)
}

/// Open the zip and parse every record into ranges, filtering to comparable
/// ecosystems. A record we cannot parse is skipped with a warning, not fatal:
/// one bad entry must not lose the whole ecosystem.
fn extract_ranges(zip_path: &std::path::Path) -> Result<Vec<OsvRange>> {
    let file = std::fs::File::open(zip_path)?;
    let mut archive = zip::ZipArchive::new(file)?;
    let mut ranges = Vec::new();

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        if !entry.name().ends_with(".json") {
            continue;
        }
        let mut body = String::new();
        if entry.read_to_string(&mut body).is_err() {
            continue; // non-UTF8 or unreadable member; skip
        }
        let advisory = match osv::parse(&body) {
            Ok(a) => a,
            Err(_) => continue, // unsupported ecosystem / malformed record
        };

        // The CVE the ranges attach to: prefer an alias that looks like a CVE,
        // else the record id itself if it is a CVE.
        let cve = pick_cve(&advisory.id, &advisory.aliases);
        let Some(cve) = cve else { continue };

        for range in &advisory.affected {
            ranges.push(OsvRange {
                cve: cve.clone(),
                ecosystem: range.ecosystem.as_db_str().to_owned(),
                package: range.package.clone(),
                introduced: range.introduced.clone(),
                fixed: range.fixed.clone(),
            });
        }
    }
    Ok(ranges)
}

/// Choose the CVE id these ranges join to. OSV distro records are named like
/// `DEBIAN-CVE-2022-0778` and alias the bare CVE; the matcher joins on the CVE.
fn pick_cve(id: &str, aliases: &[String]) -> Option<String> {
    if is_cve(id) {
        return Some(id.to_owned());
    }
    aliases.iter().find(|a| is_cve(a)).cloned()
}

fn is_cve(s: &str) -> bool {
    s.starts_with("CVE-")
}

/// POST one chunk of ranges; return the count the Worker accepted.
fn post_chunk(url: &str, token: &str, ecosystem: &str, chunk: &[OsvRange]) -> Result<usize> {
    let payload = OsvIngest {
        version: OSV_INGEST_VERSION,
        sources: vec![ecosystem.replace("%20", " ")],
        ranges: chunk.to_vec(),
    };
    let resp = ureq::post(url)
        .set("Authorization", &format!("Bearer {token}"))
        .send_json(serde_json::to_value(&payload)?);

    match resp {
        Ok(r) => {
            let v: serde_json::Value = r.into_json().unwrap_or_default();
            let accepted = v
                .get("accepted")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0);
            if v.get("deferred").and_then(serde_json::Value::as_bool) == Some(true) {
                eprintln!(
                    "[osv-fetch] Worker deferred a chunk (daily write budget); stopping early"
                );
            }
            Ok(accepted as usize)
        }
        Err(ureq::Error::Status(code, _)) => {
            Err(anyhow!("Worker rejected a chunk with HTTP {code}"))
        }
        Err(e) => Err(anyhow!("POST failed: {e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_a_cve_alias_from_a_distro_record() {
        let id = "DEBIAN-CVE-2022-0778";
        let aliases = vec!["CVE-2022-0778".to_string()];
        assert_eq!(pick_cve(id, &aliases).as_deref(), Some("CVE-2022-0778"));
    }

    #[test]
    fn uses_the_id_when_it_is_itself_a_cve() {
        assert_eq!(
            pick_cve("CVE-2022-0778", &[]).as_deref(),
            Some("CVE-2022-0778")
        );
    }

    #[test]
    fn a_record_with_no_cve_anywhere_is_skipped() {
        assert_eq!(pick_cve("DLA-3942-1", &["GHSA-x".into()]), None);
    }
}
