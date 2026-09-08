# Worklog

Newest first. One entry per working session. Record what changed and what you
deliberately left undone, so the next agent does not have to guess.

Format: `## YYYY-MM-DD — <agent> — <phase>`

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
