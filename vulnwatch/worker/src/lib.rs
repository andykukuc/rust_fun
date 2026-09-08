//! Cloudflare Worker entry point.
//!
//! Task 0.2 only needs this to prove the Rust -> WASM -> Workers toolchain
//! works end to end. It deliberately exercises `vulnwatch-core` rather than
//! returning a bare string, so a successful build also proves the domain
//! crate links into the Worker target.

use vulnwatch_core::package::Ecosystem;
use vulnwatch_core::version;
use worker::*;

#[event(fetch)]
async fn fetch(request: Request, _env: Env, _context: Context) -> Result<Response> {
    match request.path().as_str() {
        "/health" => health(),
        _ => Response::error("Not found", 404),
    }
}

fn health() -> Result<Response> {
    // Ordering that only a real per-ecosystem comparison gets right: the
    // Debian fix for DLA-3942-1 must outrank the version it replaced.
    let deb_ok = version::compare(Ecosystem::Deb, "1.1.1n-0+deb11u6", "1.1.1n-0+deb11u3").is_gt();
    // el10 must outrank el9, which plain text ordering gets backwards.
    let rpm_ok = version::compare(Ecosystem::Rpm, "1.0-1.el10", "1.0-1.el9").is_gt();

    Response::ok(format!(
        "vulnwatch-worker ok; core linked; deb_ordering={deb_ok} rpm_ordering={rpm_ok}"
    ))
}
