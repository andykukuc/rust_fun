-- Elysium Vuln Watch - initial schema (task 2.1)
--
-- Shaped around D1's row-write budget rather than around what would be
-- natural in local SQLite. Two rules drive the design:
--
--   1. Every index costs one extra written row per indexed write, so each
--      index below has to earn its place in the matching query.
--   2. Feeds are filtered to our own ecosystems at ingest, so these tables
--      hold a homelab-sized slice of OSV/NVD, not the whole corpus.

-- One row per advisory, from any feed.
CREATE TABLE advisories (
    id          TEXT PRIMARY KEY,
    source      TEXT NOT NULL CHECK (source IN ('osv', 'nvd')),
    summary     TEXT NOT NULL DEFAULT '',
    severity    TEXT NOT NULL DEFAULT 'None'
                CHECK (severity IN ('None', 'Low', 'Medium', 'High', 'Critical')),
    modified    TEXT,
    updated_at  TEXT NOT NULL
);

-- The CVE aliases that join an OSV range to an NVD score and a KEV flag.
-- OSV distribution records list these under `upstream`; NVD is itself keyed
-- by CVE. Without this table the three feeds never meet.
CREATE TABLE advisory_aliases (
    advisory_id TEXT NOT NULL REFERENCES advisories (id) ON DELETE CASCADE,
    alias       TEXT NOT NULL,
    PRIMARY KEY (advisory_id, alias)
);

-- Half-open version windows [introduced, fixed).
CREATE TABLE advisory_ranges (
    advisory_id TEXT NOT NULL REFERENCES advisories (id) ON DELETE CASCADE,
    ecosystem   TEXT NOT NULL CHECK (ecosystem IN ('deb', 'rpm', 'apk')),
    package     TEXT NOT NULL,
    introduced  TEXT NOT NULL DEFAULT '0',
    fixed       TEXT,
    PRIMARY KEY (advisory_id, ecosystem, package, introduced)
);

-- CISA Known Exploited Vulnerabilities. Small, and the highest-signal table
-- in the database.
CREATE TABLE kev (
    cve_id          TEXT PRIMARY KEY,
    catalog_version TEXT NOT NULL,
    date_added      TEXT
);

CREATE TABLE hosts (
    id        TEXT PRIMARY KEY,
    label     TEXT NOT NULL,
    last_seen TEXT
);

CREATE TABLE images (
    id        INTEGER PRIMARY KEY AUTOINCREMENT,
    host_id   TEXT NOT NULL REFERENCES hosts (id) ON DELETE CASCADE,
    reference TEXT NOT NULL,
    digest    TEXT NOT NULL,
    UNIQUE (host_id, digest)
);

CREATE TABLE packages (
    image_id  INTEGER NOT NULL REFERENCES images (id) ON DELETE CASCADE,
    ecosystem TEXT NOT NULL CHECK (ecosystem IN ('deb', 'rpm', 'apk')),
    name      TEXT NOT NULL,
    version   TEXT NOT NULL,
    PRIMARY KEY (image_id, ecosystem, name)
);

-- The output: one row per (advisory, image, package) match.
-- `in_kev` is joined in at write time so the dashboard query stays a cheap
-- indexed read instead of a scan across three tables.
CREATE TABLE findings (
    id                INTEGER PRIMARY KEY AUTOINCREMENT,
    advisory_id       TEXT NOT NULL REFERENCES advisories (id) ON DELETE CASCADE,
    image_id          INTEGER NOT NULL REFERENCES images (id) ON DELETE CASCADE,
    ecosystem         TEXT NOT NULL,
    package           TEXT NOT NULL,
    installed_version TEXT NOT NULL,
    fixed_version     TEXT,
    severity          TEXT NOT NULL,
    in_kev            INTEGER NOT NULL DEFAULT 0 CHECK (in_kev IN (0, 1)),
    first_seen        TEXT NOT NULL,
    last_seen         TEXT NOT NULL,
    UNIQUE (advisory_id, image_id, package)
);

-- One cursor row per feed (task 2.4). Every pull resumes from here, so a
-- full re-import is never something the code can do by accident. A sync that
-- stops early on the write budget is normal: it leaves the cursor behind and
-- the next run continues.
CREATE TABLE sync_state (
    feed          TEXT PRIMARY KEY CHECK (feed IN ('osv', 'nvd', 'kev')),
    cursor        TEXT,
    last_success  TEXT,
    last_error    TEXT,
    rows_written  INTEGER NOT NULL DEFAULT 0
);

INSERT INTO sync_state (feed) VALUES ('osv'), ('nvd'), ('kev');

-- Indexes (task 2.2). Each one is paid for on every write to its table, so
-- the list is deliberately short.

-- The matcher's filter: find candidate advisories for an installed package.
CREATE INDEX idx_ranges_lookup ON advisory_ranges (ecosystem, package);

-- The other side of that join.
CREATE INDEX idx_packages_lookup ON packages (ecosystem, name);

-- Resolving a CVE id back to the advisories that mention it.
CREATE INDEX idx_aliases_alias ON advisory_aliases (alias);

-- The dashboard's default view: actively exploited first, then by severity.
CREATE INDEX idx_findings_triage ON findings (in_kev DESC, severity);

-- Replacing one host's findings when fresh inventory arrives.
CREATE INDEX idx_findings_image ON findings (image_id);
