# Rust Vuln Watch

Matches public vulnerability feeds (OSV, NVD, CISA KEV) against the Docker
images actually running on a target host, and surfaces the ones that are being
actively exploited.

Rust throughout: a pure `core` crate shared between a native collector that
runs at home and a Cloudflare Worker (Rust → WASM) that syncs feeds into D1,
does the matching, and serves the dashboard.

- Working agreement for AI agents: [`AGENTS.md`](AGENTS.md)
- Session log: [`docs/WORKLOG.md`](docs/WORKLOG.md)
- Build tracker: https://claude.ai/code/artifact/e8424962-9e23-4235-9b7a-82f4227268e2

## Status

Phases 1 (core crate) and 2 (D1 schema) are complete: 68 tests, clippy clean,
and `vulnwatch-core` builds for `wasm32-unknown-unknown`.

```sh
cargo test
cargo fmt -- --check
cargo clippy --all-targets -- -D warnings
cargo build -p vulnwatch-core --target wasm32-unknown-unknown
```

Next: Phase 0 leftovers (a workers-rs hello world, confirming the Workers
plan, and inventorying the host's images to pin the ecosystem filter), then
Phase 3 feed sync.
