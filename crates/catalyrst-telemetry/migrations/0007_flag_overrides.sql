-- Operator overrides for feature flags / gates: the flag counterpart to the
-- per-experiment overrides in 0006. The dashboard's flags view (GET /dash/flags)
-- and the public /flags page MERGE these over the upstream FLAGS_URL config so a
-- forced flag reflects in current values. The dashboard mutates them through
-- POST /dash/flag (loopback, unauthenticated like /dash/experiment and
-- /dash/issue/state); every mutation also records an admin_audit row.
--
-- Created unqualified: the connection sets search_path=telemetry, the same
-- convention migrations 0001-0006 rely on. `state` is the forcing mode:
--   on     -> flag forced true
--   off    -> flag forced false
--   forced -> flag forced true and the A/B variant pinned to forced_variant
CREATE TABLE IF NOT EXISTS flag_overrides (
    flag           text PRIMARY KEY,
    state          text NOT NULL DEFAULT 'on'
                   CHECK (state IN ('on','off','forced')),
    forced_variant text,
    updated_at     timestamptz NOT NULL DEFAULT now()
);
