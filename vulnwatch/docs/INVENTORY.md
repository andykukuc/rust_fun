# Inventory

The real inventory lives in `docs/INVENTORY.local.md`, which is gitignored.

This is deliberate. An inventory is a list of exactly which services and
versions run on a specific machine — deployment data, not source. In a public
repository it is a targeting aid, and it gets worse over time: once phase 6
produces findings, the same file's descendants would amount to a public list
of unpatched software on a named person's network. Keep it local.

## What the local file records

For each host: OS and version, container runtime version, and for every running
container its base distribution, version, and package manager.

## The ecosystem filter it produces

This is the output that the code actually depends on — the OSV streams to sync,
filtered at ingest. For the current deployment:

```
Ubuntu:26.04   Ubuntu:24.04   Debian:12   Alpine:v3.24   Red Hat:10
```

All five map onto schemes `vulnwatch-core` already compares — see
`ecosystem_from_label` in `core/src/feeds/osv.rs`. Widening this list is a
storage-budget decision, not a convenience one.

## Distroless images cannot be inventoried

Some images ship with no shell and no package manager. `docker exec` cannot
start `sh` in them, and there is no package database to read, so the method
used for every other container produces nothing.

This matters more than it looks. If the collector reports "0 packages" for such
an image, it renders as **clean** — indistinguishable from one that was scanned
and found healthy. That is exactly the silent false negative this project
exists to avoid.

**The collector must mark these images `uninventoriable`, and the dashboard
must show them as unknown, never as clean.** Whatever else changes, that stays.

Scanning them properly means reading image layers rather than asking the
running container. Out of scope for now; what is in scope is being honest that
they are not covered.
