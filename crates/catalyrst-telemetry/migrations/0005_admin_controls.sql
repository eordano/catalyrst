-- Admin controls owned by catalyrst-telemetry (admin-console §4 "LATER" tranche):
-- data retention/purge, ingest enable/disable toggle, per-project quota, bulk
-- delete/export, issue history/audit, regroup, and release state. Every mutation
-- behind these routes is bearer-gated (loopback-trusted, like the rest of /dash/*)
-- and records an audit row.
--
-- Tables are created unqualified: the connection sets search_path=telemetry, the
-- same convention the existing migrations (0001-0004) rely on.

-- Singleton key/value settings store. Currently holds the ingest enable/disable
-- toggle (key='ingest_enabled', value 'true'/'false'). One row per key; the
-- ingest path mirrors this into an in-memory atomic at boot and on each toggle,
-- so the hot insert path never reads this table.
CREATE TABLE IF NOT EXISTS admin_settings (
    key        text PRIMARY KEY,
    value      text NOT NULL,
    updated_at timestamptz NOT NULL DEFAULT now()
);

-- Per-project daily ingest quota: at most `daily_limit` events accepted per
-- project per UTC day. NULL/absent row means unlimited (the default, so existing
-- behavior is unchanged until an operator sets a quota). Enforced O(1) per event
-- against an in-memory per-day counter seeded from this table.
CREATE TABLE IF NOT EXISTS project_quota (
    project     text PRIMARY KEY,
    daily_limit bigint NOT NULL CHECK (daily_limit >= 0),
    updated_at  timestamptz NOT NULL DEFAULT now()
);

-- Audit log: who did what, when. `actor` is a free-form operator label from the
-- request (?actor=), `action` is the admin verb, `detail` is a JSON snapshot of
-- the request/effect. Doubles as the issue history/audit feed (filter on
-- action LIKE 'issue.%' or detail->>'fingerprint').
CREATE TABLE IF NOT EXISTS admin_audit (
    id         BIGSERIAL PRIMARY KEY,
    at         timestamptz NOT NULL DEFAULT now(),
    actor      text NOT NULL DEFAULT 'admin',
    action     text NOT NULL,
    detail     jsonb NOT NULL DEFAULT '{}'::jsonb
);
CREATE INDEX IF NOT EXISTS admin_audit_at_idx ON admin_audit (at DESC);
CREATE INDEX IF NOT EXISTS admin_audit_action_idx ON admin_audit (action, at DESC);
-- Fast lookup of the history for a single fingerprint (issue audit drill-down).
CREATE INDEX IF NOT EXISTS admin_audit_fingerprint_idx
    ON admin_audit ((detail->>'fingerprint'), at DESC);

-- Issue regroup / merge: map a source fingerprint onto a canonical one so several
-- machine-distinct fingerprints collapse into one logical issue in the dashboard.
-- The read path can LEFT JOIN this to resolve the effective fingerprint; the
-- merge itself is recorded here and in admin_audit.
CREATE TABLE IF NOT EXISTS issue_merge (
    source_fingerprint    text PRIMARY KEY,
    canonical_fingerprint text NOT NULL,
    merged_at             timestamptz NOT NULL DEFAULT now(),
    CHECK (source_fingerprint <> canonical_fingerprint)
);
CREATE INDEX IF NOT EXISTS issue_merge_canonical_idx
    ON issue_merge (canonical_fingerprint);

-- Release state: operator-set lifecycle state for a release string (active,
-- archived, broken). Lets the console flag a known-bad release or archive a
-- retired one. Read-only views can LEFT JOIN on the release string.
CREATE TABLE IF NOT EXISTS release_state (
    release    text PRIMARY KEY,
    state      text NOT NULL DEFAULT 'active'
               CHECK (state IN ('active','archived','broken')),
    note       text,
    updated_at timestamptz NOT NULL DEFAULT now()
);
