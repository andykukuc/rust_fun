# AGENTS.md — Elysium Vuln Watch

Shared working agreement for **Claude Code** and **Codex** on this repo.
Both agents read this file first. Humans: see `README.md`.

Tracker: **`docs/TRACKER.md`** — canonical, in-repo, readable by both agents.
Tick tasks there in the same commit as the work.

A rendered human view of the same plan is published at
https://claude.ai/code/artifact/e8424962-9e23-4235-9b7a-82f4227268e2 — it is
behind a claude.ai session, so no terminal agent can read it. `docs/TRACKER.md`
is the source of truth; the artifact is regenerated from it.

---

## What this is

Public vulnerability feeds matched against the Docker images actually running
on `elysium`. Answers one question: *what do I run, right now, that is being
actively exploited in the wild.*

It is **not** a CVE mirror and not a scanner. The value is the join between
public advisories and this specific homelab's real inventory.

## Where this lives

This project is a subtree inside the public `andykukuc/rust_fun` repository, at
`vulnwatch/`. Work here, commit and push from the repository root as normal.

The subtree was imported squashed, so the granular phase-1/2 history lives only
in the original standalone repo. Do not try to reconstruct it here.

`vulnwatch/docs/INVENTORY.local.md` holds the real host inventory and is
gitignored. If it is missing on your machine, regenerate it rather than
committing anything host-specific.

## Architecture

Three units. Keep the boundaries — they are what make the thing testable.

| Crate | Target | Responsibility |
|---|---|---|
| `core` | native + `wasm32-unknown-unknown` | Domain types, feed parsers, version comparison, the matcher. **Zero I/O.** |
| `collector` | native, runs on elysium | Enumerate Docker images and their packages, POST inventory to the Worker. |
| `worker` | `wasm32-unknown-unknown` | Cron feed sync, ingest, matching, API, dashboard. Binds D1. |

**`core` must never gain an I/O dependency.** No `reqwest`, no `tokio`, no
filesystem, no `std::time::SystemTime`. It compiles for wasm32 or the Worker
cannot use it, and that build is a CI gate, not a suggestion.

Workers have no LAN access and no Docker socket. Anything touching elysium's
local state belongs in `collector`, full stop.

## Hard constraints (verified 2026-09-08, re-verify before relying on these)

D1 free tier, **enforced since 2026-09-01** — past the cap queries *fail*,
they do not throttle:

| | Free | Paid |
|---|---|---|
| Storage | 5 GB | 5 GB, then $0.75/GB-mo |
| Rows read | 5M / day | 25B / mo |
| Rows written | **100k / day** | 50M / mo |

Consequences that shape the code:

- NVD is ~300k CVEs. A naive full seed exceeds several days of free-tier writes.
- Every index costs **one extra written row** per indexed write.
- Feeds are filtered to our own ecosystems **at ingest**. This is the single
  decision keeping the project inside budget — do not "temporarily" widen it.
- Every sync run counts rows written and stops cleanly before the cap,
  resuming next run from its cursor. A sync that cannot finish in one run is
  normal and must not be treated as an error.

## Ground rules

**Test-first.** Write the failing test, watch it fail, implement, watch it
pass. Parser and matcher tests read committed fixtures in `fixtures/` — never
the live network. No test may make an outbound request.

**Errors are explicit.** No `.unwrap()` outside tests. Malformed feed records
are rejected loudly with context, never skipped silently. A silent false
negative here means a clean dashboard that means nothing — that is the worst
failure this tool can have.

**Immutable by default.** Return new values; do not mutate in place.

**Small files.** 200–400 lines typical, 800 hard ceiling. Split by
responsibility, not by layer.

**No secrets in the repo, ever.** NVD API key and the collector's bearer token
are Worker secrets / environment variables. Never a literal, never logged,
never in a fixture. If one is ever committed, stop and rotate it before
anything else.

**Treat all feed and inventory data as untrusted input.** It is parsed, not
trusted. Bound every payload size. Validate at the boundary.

## Before you call anything done

```sh
cargo test
cargo fmt -- --check
cargo clippy --all-targets -- -D warnings
cargo build -p core --target wasm32-unknown-unknown   # the gate that matters
```

On this Windows box `cargo` is not on PATH in Git Bash — it lives at
`~/.cargo/bin/cargo.exe`. Rust 1.89.0 is installed; `wasm32-unknown-unknown`
is **not** yet (task 0.1).

## Working together

- **One agent per phase at a time.** Phases are the unit of ownership. Announce
  the phase you are taking in `docs/WORKLOG.md` before starting.
- **Log what you did** in `docs/WORKLOG.md`: date, agent, phase, what changed,
  what you deliberately left undone. The next agent reads this before touching
  anything.
- **Do not rewrite the other agent's in-flight work.** If something looks wrong,
  write it down in the worklog and say so — do not silently "fix" it.
- **Tick `docs/TRACKER.md`** when a task is genuinely finished, not when it compiles.
- Commits: `<type>: <description>` — feat, fix, refactor, docs, test, chore,
  perf, ci.
- If you disagree with a decision recorded here, argue it in the worklog and
  get a human answer. Do not fork the architecture quietly.

## Ecosystem filter (settled by task 0.5)

Sync only these OSV streams, filtered at ingest:

```
Ubuntu:26.04   Ubuntu:24.04   Debian:12   Alpine:v3.24   Red Hat:10
```

That is what the deployment actually runs. Widening this list is a
storage-budget decision, not a convenience one.

**Distroless images cannot be inventoried.** Some images have no shell and no
package manager. The collector must mark them `uninventoriable`; if it reports
zero packages they appear clean, which is the exact false negative this project
exists to prevent.

Host and container specifics live in `docs/INVENTORY.local.md`, which is
gitignored — this repository is public, and an inventory is deployment data,
not source. See `docs/INVENTORY.md` for why.

## Environment notes

`RUSTFLAGS=-C target-cpu=native` is set in the shell environment on the
Windows box. wasm32 ignores it (with noisy warnings), but it means native
builds are tuned to that specific CPU — do not ship a native collector binary
built there to another machine.

## Current state

Phases 1 and 2 are complete: 68 tests, clippy clean, and both crates build for
wasm32-unknown-unknown. Phase 0 is closed except for the two tasks that need
Cloudflare credentials (0.2 deploy, 0.3 plan confirmation).

The design descends from a Rust + SQLite scraper (`N:\Rust\rustedsims4db`)
whose discovery loop guessed random IDs. Two bugs found there directly shaped
this design: an ID range that mostly missed its target, and HTTP 429 responses
silently discarded so the loop hammered harder while finding nothing. **Both
disappear when you enumerate from a cursor instead of guessing.** Do not
reintroduce random probing anywhere in this project.
