# Worklog

Newest first. One entry per working session. Record what changed and what you
deliberately left undone, so the next agent does not have to guess.

Format: `## YYYY-MM-DD — <agent> — <phase>`

## 2026-09-12 (session end) — Claude — review fixes

Ran security-review + rust-review subagents on the day's work before wrapping.
Security verdict: **PASS** — no secrets/tokens/hostname in tracked git content,
all SQL parameterized, ingest auth sound. Fixed the findings with real impact:

1. **Write-budget under-priced indexed writes (HIGH, fixed).** Each
   `advisory_ranges` and `advisory_aliases` insert also writes an index entry,
   costing 2 budget units, not 1 — so the guard could pass a batch D1 then
   hard-rejects mid-write instead of deferring cleanly. Added
   `core::batch::write_cost` constants (KEV=1, ADVISORY=1, ALIAS=2, RANGE=2)
   keyed to the migration's index list, and used them in ingest/nvd/kev cost
   calcs. Update these if an index is added/dropped.
2. **Unauthenticated /sync/* routes (HIGH, fixed).** `/sync/kev` and `/sync/nvd`
   were anonymously triggerable on the public hostname (could burn the write
   budget / NVD rate limit). Added `require_post_auth` (POST + bearer, reusing
   `ingest::check_auth`) to all three write routes. Verified live: no-token
   POST → 401 on all three; token → 200; /health stays open. Cron (3.6) calls
   the sync fns directly, not over HTTP, so it needs no token.
3. **nvd_sync Vec::with_capacity off-by-one (LOW, fixed).** Now sized to the
   real statement count (advisories + aliases + 2).

Tracked, NOT changed tonight:
- **NVD does a full-corpus crawl, not "modified since" windowing (HIGH per the
  reviewer, but it is correct-if-inefficient, not broken).** The doc comment in
  nvd_sync.rs claims lastModStartDate windows; the code walks startIndex over
  the whole corpus and wraps. It re-upserts everything each cycle. Fine for now
  (budget guard bounds it), but wire real date windows or fix the doc in 3.6.
- record_sync_error double-swallow (MEDIUM): if the error-write itself fails,
  only a console log remains. Consider an unconditional last_attempt stamp.
- ingest body buffered before MAX_BODY_BYTES check (MEDIUM): gated behind auth;
  check Content-Length before buffering for true memory bound.
- constant_time_eq hand-rolled (LOW): correct; could use the `subtle` crate.
- cargo audit / Dependabot not yet run (INFO).

All fixes: 101 tests, fmt + clippy clean, wasm builds, deployed + verified live.
## 2026-09-12 (session cont.) — Claude — Phase 3.2 (OSV) via the collector

OSV is now flowing, end to end, using the architecture the size limit forced:
the **collector on elysium** does the bulk download, the Worker ingests
filtered ranges. Full three-feed pipeline (KEV flags + NVD scores + OSV ranges)
is live.

New pieces:
- `core::ingest` — versioned wire contract (`OsvIngest`/`OsvRange`,
  `OSV_INGEST_VERSION`, `is_supported()`). Pure, wasm-clean, shared by both
  sides. `Ecosystem::as_db_str`/`from_db_str` and `Severity::as_db_str` added.
- `vulnwatch-collector` crate (native, elysium) with the `osv-fetch` binary:
  downloads the 4 OSV ecosystem zips (Ubuntu 682MB, Debian 68MB, Red Hat 26MB,
  Alpine 4MB), unzips + parses with `core::feeds::osv`, resolves each distro
  record to its CVE alias, and POSTs ranges in 2000-chunks with a bearer token.
  Config via env (URL/token/work-dir), never the repo. Native deps (ureq, zip,
  anyhow) stay in this crate; core + worker still build for wasm32.
- `worker/src/ingest.rs` — authenticated `POST /ingest/osv`: constant-time
  bearer check against the `INGEST_TOKEN` secret, 4MB body cap, contract-version
  check, per-field validation, then writes `advisory_ranges` budget-guarded.

Deploy/run facts:
- Built the collector ON elysium (RHEL10, cargo 1.94) against the SAME samba
  share the Mac uses (`/mnt/samba_pool/samba/Coding/Rust/vulnwatch` == the Mac's
  `~/mnt/Coding`), CARGO_TARGET_DIR=/tmp so the build stays off the share.
  Work dir `/mnt/nvme_backup/vulnwatch-osv` (449GB free; needed a one-time
  `sudo mkdir + chown`).
- INGEST_TOKEN set as a Worker secret (openssl rand -hex 32); the same value in
  the collector's env. NVD_API_KEY also set by the user.

**The bug this run caught, and the fix (important):** the first collector run
parsed 3.6M ranges and the Worker "accepted" 94k — but only 5 became rows; the
other ~94k were dropped by the advisory_ranges FK for CVEs we do not track yet,
AND they had already been charged to the daily write budget (counter hit 99.5k
for 3.7k real rows). Fix: the ingest route now checks which CVEs exist in
`advisories` (chunked `IN(...)` reads, cheap) BEFORE spending budget, so only
writable ranges cost the cap. Re-ran: 3.6M parsed -> 51 accepted -> 51 written,
budget spent == rows written. Reset the inflated counter to the true row count.
Reads rose (existence checks) but reads are the abundant resource (5M/day) vs
writes (100k/day) — correct trade.

Only ~10 ranges match so far because NVD has synced just 2000 of 390k CVEs;
matches grow as NVD pages in. The mechanism is proven.

101 tests (96 core / 3 collector / 2 worker); fmt + clippy clean; core + worker
build for wasm32; collector builds native on elysium.

Deliberately left undone:
- The collector re-downloads all zips each run and sends everything; a smarter
  version could fetch our CVE list first, but FK-before-budget already makes the
  waste free (reads only). Fine for now.
- Cron (3.6) still not wired; syncs + collector are manual.
- systemd timer for the collector on elysium not yet created (task 4.5).
- /sync/* routes still unauthenticated; only /ingest/* is authed (7.1 partial).

## 2026-09-12 (session cont.) — Claude — Phase 3.3 done, 3.2 blocked

**3.3 NVD sync: complete and live.** `POST /sync/nvd` pulls one NVD 2.0 API
page (2000 CVEs) from the `startIndex` cursor, writes `advisories` + aliases
with an upsert, advances the cursor (wrapping to 0 past the end for the next
modification cycle), all under the daily budget guard. Verified live: page 1
wrote 2000 advisories (of 390614 total), cursor at 2000, and the shared budget
counter went 3418 → 5418, confirming the guard tracks across feeds.

- API key is the `NVD_API_KEY` Worker secret (user set it via
  `wrangler secret put`). Used if present, keyless fallback otherwise.
- `core::advisory::Severity::as_db_str()` added (tested) so writes match the
  `advisories.severity` CHECK constraint.
- Route handling refactored into a shared `run_sync` helper in lib.rs.
- NVD advisories have empty `affected` BY DESIGN (NVD uses CPEs). Ranges come
  from OSV, joined on the CVE id via advisory_aliases. Do not "fix" this.

**3.2 OSV sync: BLOCKED, and this is an architecture finding, not a bug.**
Every route to "all of our ecosystems" hits OSV's distribution shape:

- Per-ecosystem `all.zip` is far too big for a Worker: **Ubuntu 682 MB**,
  Debian 68 MB, Red Hat 26 MB, Alpine 4 MB. A Worker has ~128 MB memory and a
  CPU-time cap.
- `/v1/query` needs a package name; there is no "everything in Debian:12" call.
- `/v1/vulns/CVE-X` returns the base record, usually WITHOUT distro package
  ranges. The ranges live in separate records (`DEBIAN-CVE-*`, `UBUNTU-CVE-*`,
  etc.) that alias the CVE — so it is ~5 fetches per CVE to cover our distros.

Conclusion: the bulk OSV fetch+filter belongs in the **collector on elysium**
(Phase 4), which has no size limit and already POSTs to the Worker. Options for
whoever picks this up: (a) do OSV in the collector and POST ranges to a new
`/ingest/osv` route, or (b) an interim Worker path that fetches per-CVE ranges
for KEV CVEs only (~1700, bounded). Tracker 3.2 annotated with all of this.

92 tests; fmt + clippy clean; both crates build for wasm32.

Deliberately left undone:
- 3.2 OSV (blocked as above) — so `advisory_ranges` is still empty and the
  matcher has nothing to match against yet. NVD scores + KEV flags are in;
  ranges are the missing third feed.
- Cron (3.6) not wired; syncs are manual triggers.
- All /sync/* routes unauthenticated — gate before public (7.1/7.2).

## 2026-09-12 (later still) — Claude — Phase 3.5

Write-budget guard, live and proven. The daily D1 write cap is per calendar
day and shared across all feeds, so the counter is shared and resets on
rollover.

- `core::batch::DailyBudget` (pure, 9 tests): opens from a persisted
  `(day, spent)` for today, resets to zero on a new day, saturates at the cap
  so a miscount can never wrap to a huge allowance, and yields a per-run
  `Budget` that refuses an over-large batch whole.
- Migration `0002_budget_state.sql`: a one-row `budget_state(day, rows_written)`
  table, applied to the live D1. Kept separate from `sync_state.rows_written`
  (which stays "rows in this feed's last run") so neither meaning is overloaded.
- Worker `budget` module: reads/opens the counter and returns a
  `persist_statement` included in the SAME atomic batch as the data, so the
  count and the writes commit together. `day_key` slices the date from the
  RFC 3339 stamp (2 tests).
- Wired into KEV: it prices the run at one write per catalog row, checks the
  budget, defers the whole run (`deferred: true`, not an error) if it will not
  fit, else records the spend. Verified live: `budget_state` went
  1709 → 3418 across two same-day runs; a new day would reset it.

Context from the dashboard: the 17k "rows written" and 8k "queries" that looked
alarming were almost all my repeated debug re-syncs (each KEV run is ~1709
writes). Reads were only ~2k and storage 238 kB — nowhere near their caps. The
write cap is the one that bites, which is exactly what this guard protects.

**Deploy gotcha (important):** `wrangler deploy` reused a STALE build artifact
once — the version id changed but the running code was the previous build (the
`deferred` field was missing and `budget_state` stayed at 0). Force a clean
worker build after code changes: remove `build/` and the
`vulnwatch_worker*` deps under the target dir before deploy. Confirm the change
is live by a visible signal in the response, not just a new version id.

93 tests; fmt + clippy clean; both crates build for wasm32.

Deliberately left undone:

- KEV never actually defers (it always fits), so the deferral PATH is covered
  by unit tests but not yet exercised end to end. OSV/NVD (3.2/3.3) are where a
  real partial run happens; watch it there.
- `/sync/kev` still unauthenticated — same gating caveat (7.1/7.2).
- Cron (3.6) still not wired.

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
