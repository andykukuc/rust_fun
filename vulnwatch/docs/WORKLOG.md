# Worklog

Newest first. One entry per working session. Record what changed and what you
deliberately left undone, so the next agent does not have to guess.

Format: `## YYYY-MM-DD — <agent> — <phase>`

## 2026-09-12 (later) — Claude — Phase 3.4

KEV sync, live and proven end to end: `POST /sync/kev` fetches the CISA
catalog, parses it, and mirrors it into the `kev` table, then `/health` reports
kev as `fresh`. First real data in the database: **1709 CVEs**, catalog
`2026.09.11`.

- `core::feeds::kev` extended to carry `dateAdded` per entry (`KevEntry`) and a
  `rows()` helper for the full-mirror write. `contains`/`cve_ids` preserved.
- `worker/src/sync.rs`: fetch (fails loud on non-200), parse, and an **atomic
  D1 batch** of `DELETE FROM kev` + one INSERT per entry + the `sync_state`
  update. Refuses an empty catalog so a broken fetch cannot wipe the table.
- `core::health::format_epoch_millis` added (inverse of the parser, round-trip
  tested) so the Worker writes RFC 3339 `last_success`.

Three bugs found and fixed on the way, each worth remembering:

1. **D1 caps a statement at 100 bound params.** The first prune used
   `cve_id NOT IN (?1..?N)` with N>1700 — `variable number must be between ?1
   and ?100`. Replaced with delete-then-insert in one atomic batch, which also
   reads more correctly for a snapshot feed.
2. **Timestamp format mismatch.** `worker::Date::to_string()` yields a human
   string (`Sat Sep 12 2026 ... (Coordinated Universal Time)`), which the
   RFC 3339 freshness parser rightly rejected as `invalid_timestamp` — data was
   correct, health was red. Fixed by formatting epoch millis through the tested
   core helper. `js_sys::Date::to_iso_string` was tried first and did NOT
   produce the ISO form here; dependency removed.
3. The DB `last_error` can be stale after a later success wrote nothing to it on
   the failing path — read the actual table state (row counts), not just
   `last_error`, when diagnosing.

84 tests, fmt + clippy clean, both crates build for wasm32.

Deliberately left undone:

- **Auth.** `/sync/kev` is an unauthenticated write path, same caveat as the
  rest — gate before the hostname is used beyond /health (tasks 7.1/7.2).
- **Budget guard (3.5)** still to come before the large feeds. KEV fits in one
  run so it does not defer, but OSV/NVD must not run unguarded.
- **Cron (3.6)** not wired; sync is manual-trigger only for now.

## 2026-09-12 — Claude — Phase 3.1

D1 database `vulnwatch` created (`1997f371-8fd7-43ce-b8e6-c66c795984d7`, ENAM)
and `migrations/0001_initial.sql` applied via the Cloudflare API: 9 tables, 5
indexes, and the three `sync_state` cursors seeded. Bound to the Worker as
`DB`. Deployed (version `e0865b3d`) and verified end to end against the real
remote D1 with `wrangler dev --remote`, then over the live custom domain.

`/health` now reads `sync_state` and reports each feed's freshness, returning
**503** when any feed is stale or has never synced, so an uptime check alone
catches a silently stalled pipeline (the failure task 7.3 warns about). The
freshness arithmetic is a new pure module, `core::health` (RFC 3339 epoch
parse + `Freshness::evaluate`), with 10 tests. Core is now 78 tests, clippy
clean, fmt clean, and both crates build for wasm32.

**Custom domain:** attached on a zone we control. The hostname is deployment
data and this repo is public, so it is kept out of committed config the same
way `docs/INVENTORY.local.md` is — it lives in the gitignored
`worker/wrangler.local.jsonc`. The domain persists on the account across
`wrangler deploy` regardless.

**0.3 still unconfirmed and now proven unreadable from every API path** I have
(account subscriptions returns an auth error; `default_usage_model` and account
`type` are identical on Free and Paid). The recorded decision stands: assume
Free, guard is load-bearing. Confirm from the dashboard when convenient.

Deliberately left undone / flagged:

- **Auth.** Every route is currently unauthenticated. `/health` is low-risk
  (cursor ages only), but the ingest and findings routes (Phases 5-6) must not
  ship on the public hostname until task 7.1 auth and 7.2 rate limiting exist.
  Gate behind Cloudflare Access in the meantime. Noted in `wrangler.jsonc`.
- **Deploy env quirk (Mac):** `worker-build`'s child `cargo` writes to the
  workspace `target/`, which on the SMB share was built by the Windows box and
  is not writable from the Mac (`Permission denied`). Export
  `CARGO_TARGET_DIR=/tmp/vulnwatch-target` (or any local path) for the deploy
  so the child process inherits it. Do NOT commit a `.cargo/config.toml` for
  this — it would wrongly redirect the Windows box too.

## 2026-09-08 — Claude — Phases 1 and 2

Both phases complete. 68 tests pass, clippy is clean, and `vulnwatch-core`
builds for `wasm32-unknown-unknown`.

**Phase 0 (partial):** installed the `wasm32-unknown-unknown` target (0.1) and
captured real feed fixtures (0.4) — OSV `DLA-3942-1`, NVD `CVE-2022-0778`, CISA
KEV catalog `2026.09.04`. Host and container specifics went to
`docs/INVENTORY.local.md` (gitignored); this repo is public. Every parser test reads these; no test touches the
network.

**Phase 1:** `Severity` banding with clamping for out-of-scale and NaN scores;
purl parsing that validates percent-escapes; version ordering for deb, rpm and
apk implemented against each package manager's documented rules; OSV, NVD and
KEV parsers; and the matcher as `Advisory::affects` / `AffectedRange::covers`.

**Phase 2:** `migrations/0001_initial.sql`, validated by executing it against a
real SQLite in-memory database and asserting the CHECK constraints reject bad
severity, source and ecosystem values. Five indexes, each justified in a
comment. `core::batch` holds the pure arithmetic of batching and the row-write
budget.

Two things worth knowing:

- The antisymmetry test caught a wrong expectation in its own data — an epoch
  makes `1:1.0` greater than `2.0`, not less. That is exactly the class of
  error that reaches the dashboard as a false negative, so the property test
  earned its place on day one.
- NVD expresses what is affected as CPEs, not package versions, so NVD
  advisories parse with an empty `affected` list. Version ranges come from OSV
  and are joined to NVD scores and KEV flags on the CVE id, which is what
  `advisory_aliases` exists for. **Do not "fix" the empty list.**

Deliberately left undone:

- **0.2** — no `workers-rs` hello world yet. Nothing in Phases 1-2 needed it,
  but Phase 3 should not start before it passes.
- **0.3** — the Workers plan is still unconfirmed. This decides how the initial
  seed must be chunked, so settle it before writing sync code.
- **0.5** — elysium's Docker images have not been inventoried, so the OSV
  ecosystem filter is not yet pinned. Phase 3.2 depends on this.
- `core::batch` prices index cost as a caller-supplied multiplier. Nothing
  verifies that multiplier against real D1 accounting yet; do that during 3.5.

## 2026-09-08 (later) — Claude — Phase 0 close-out

`gh` 2.100.0 installed to `C:\Users\andyk\tools\gh\bin` (no winget/scoop on the
box; pulled the official release zip) and added to the user PATH. It was
already authenticated as `andykukuc`.

**0.2 done.** The Worker is deployed: version `f122a474-11b2-4c86-a936-921c9921660d`,
uploaded with `workers_dev: false` so it has no public URL. Rust -> WASM ->
Workers is proven end to end against the real account.

Two traps found on the way:

- `worker-build` must run from `worker/`, not the workspace root, or it fails
  parsing the workspace `Cargo.toml` for a missing `[package]`. `wrangler.jsonc`
  now lives in `worker/`.
- `RUSTFLAGS=-C target-cpu=native` in the environment produced ~13 MB of
  "not a recognized feature for this target" warnings on the wasm build and
  hid the real error completely. Unset it before any Worker build.

**0.3 not confirmed.** Wrangler's OAuth token has no billing scope, so
`/accounts/{id}/subscriptions` returns 403 and `workers/account-settings` only
reports `default_usage_model: standard`, which both plans report. Rather than
block on it, the decision is recorded in AGENTS.md: assume Free and design the
write-budget guard as load-bearing. That is correct under either plan.

D1: no databases exist on the account yet. Phase 2's migration has not been
applied anywhere.
