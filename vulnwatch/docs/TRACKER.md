# Build tracker

**Canonical for agents.** Claude and Codex both read and update this file.

A rendered human view of the same plan lives at
<https://claude.ai/code/artifact/e8424962-9e23-4235-9b7a-82f4227268e2>, but that
page sits behind a claude.ai session and no terminal agent can read or tick it.
If the two ever disagree, **this file wins** and the artifact is regenerated
from it.

Progress: **20/38** complete.

Tick a task here in the same commit as the work it describes, and add a line to
`docs/WORKLOG.md` saying what you did and what you deliberately left undone.

## Phase 0 — Foundations (4/5)

- [x] **0.1** Install the WASM target and Wrangler
- [x] **0.2** Deploy a workers-rs hello world — version `f122a474`, `workers_dev: false`, no public URL
- [ ] **0.3** Confirm which Workers plan the account is on — wrangler's token has no billing scope, so this cannot be read from the API. Decided around: assume Free and design the write-budget guard as load-bearing, which is correct under either plan. Confirm from the dashboard when convenient.
- [x] **0.4** Capture real feed fixtures — OSV `DLA-3942-1`, NVD `CVE-2022-0778`, KEV `2026.09.04`
- [x] **0.5** Inventory the Docker images on the target host — ecosystem filter pinned; see `docs/INVENTORY.md`

## Phase 1 — Core crate (8/8)

- [x] **1.1** Advisory and Severity types
- [x] **1.2** OSV JSON parser
- [x] **1.3** NVD CVE JSON parser
- [x] **1.4** KEV feed parser
- [x] **1.5** PackageRef and purl parsing
- [x] **1.6** Per-ecosystem version comparison (deb, rpm, apk)
- [x] **1.7** The matcher: `Advisory::affects` / `AffectedRange::covers`
- [x] **1.8** Verify the crate builds for wasm32

## Phase 2 — D1 schema (4/4)

- [x] **2.1** Migration 0001 — validated against real SQLite
- [x] **2.2** Indexes for the matching join
- [x] **2.3** Chunked upsert helper — `core::batch`
- [x] **2.4** sync_state cursors

## Phase 3 — Feed sync (4/6)

Recommended order is 3.1, 3.4, 3.5, then 3.2 and 3.3. Do the budget guard
**before** the two large feeds, so neither can ever run unguarded.

- [x] **3.1** Worker skeleton and D1 binding — D1 `vulnwatch` created (`1997f371-8fd7-43ce-b8e6-c66c795984d7`), `0001_initial.sql` applied (9 tables, 5 indexes, 3 cursors), bound as `DB`, and `/health` reports each feed's freshness from `sync_state` (503 when any feed is stale or never synced). Deployed version `e0865b3d`; verified via `wrangler dev --remote` against real D1. Freshness logic is `core::health` (10 tests).
- [ ] **3.2** OSV sync — **BLOCKED on ingestion point.** OSV bulk data ships as per-ecosystem zips too large for a Worker (Ubuntu all.zip is 682MB; Debian 68MB), and `/v1/query` needs a package name (no "all of Debian:12" call). Per-CVE `/v1/vulns` gives the base record without distro ranges; distro ranges live in separate `DEBIAN-CVE-*`/`UBUNTU-CVE-*` records. Correct home for the bulk fetch+filter is the **collector on elysium** (Phase 4), which then POSTs ranges to the Worker. Decide: fetch OSV from elysium, or KEV-only per-CVE subset in the Worker as an interim.
- [x] **3.3** NVD sync — paginated by `startIndex` (2000/page over 390614 CVEs), writes advisories + aliases, budget-guarded, cursor resumes. API key is the `NVD_API_KEY` Worker secret (set), used if present else keyless. `POST /sync/nvd` (unauthenticated — gate before public). Verified live: page 1 = 2000 advisories, budget counter 3418→5418 across feeds.
- [x] **3.4** KEV sync — mirrors the CISA catalog into `kev` via an atomic delete+insert D1 batch (avoids the 100-bound-param `NOT IN` ceiling); refuses an empty catalog; writes cursor=catalogVersion and an RFC 3339 `last_success`. Verified live: 1709 CVEs (v2026.09.11), `/health` reports kev `fresh`. Manual trigger `POST /sync/kev` (unauthenticated — must be gated before public use); cron in 3.6.
- [x] **3.5** Write-budget guard — `core::batch::DailyBudget` (shared, day-scoped, resets on rollover, saturates at cap) + `budget_state` table (migration 0002) + worker `budget` module. A run opens the budget, defers whole if a batch will not fit, and persists the counter in the same atomic batch as the data. Wired into KEV and verified live: counter accumulated 1709→3418 across two same-day runs. 93 tests.
- [ ] **3.6** Cron triggers — KEV daily, OSV daily, NVD every six hours, staggered so two large syncs never share a budget.

## Phase 4 — Collector (0/5)

- [ ] **4.1** Enumerate images and digests via the local Docker socket, pinned by digest not tag.
- [ ] **4.2** Extract package lists from images — **risk:** some images are distroless, with no shell and no package manager. They must be marked `uninventoriable`; reporting zero packages renders them clean on the dashboard. See `docs/INVENTORY.md`.
- [ ] **4.3** Serialize the inventory contract — version the payload from day one.
- [ ] **4.4** POST over HTTPS with a bearer token from the environment. Never logged. Fail loudly on rejection rather than exiting zero.
- [ ] **4.5** systemd timer — nightly, output somewhere you would actually notice.

## Phase 5 — Ingest and match (0/3)

- [ ] **5.1** `POST /inventory` — auth, body size limit, schema validation, reject unknown hosts. Every field untrusted.
- [ ] **5.2** Replace host inventory atomically, then run the matcher.
- [ ] **5.3** Write findings with the KEV flag joined in at write time.

## Phase 6 — Surface (0/3)

- [ ] **6.1** `GET /api/findings` — filterable by host, severity, KEV status.
- [ ] **6.2** Dashboard — actively exploited first, then by severity.
- [ ] **6.3** Notify on new KEV findings only. Anything noisier gets muted, and then the tool is decorative.

## Phase 7 — Hardening (0/4)

- [ ] **7.1** Auth on every route. No unauthenticated write path, no unauthenticated read of the inventory.
- [ ] **7.2** Rate limiting on ingest.
- [ ] **7.3** Structured logs and a **staleness alert** — the realistic failure is a sync that quietly stops while the dashboard keeps showing yesterday's clean result. Alert on cursor age, not only on errors.
- [ ] **7.4** Scheduled D1 export — findings history is the part you cannot rebuild from the feeds.
