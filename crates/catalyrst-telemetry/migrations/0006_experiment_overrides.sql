-- Per-experiment operator overrides for the sites A/B experiment layer. The
-- sites runtime (app/lib/experiments/flags.ts -> getRuntimeFlags) reads these
-- via GET /dash/experiments?key=<expKey> and lets resolveAssignment honor a
-- kill switch, a forced variant, or arbitrary per-experiment flags. The
-- dashboard mutates them through POST /dash/experiment (loopback, unauthenticated
-- like /dash/issue/state); every mutation also records an admin_audit row.
--
-- Created unqualified: the connection sets search_path=telemetry, the same
-- convention migrations 0001-0005 rely on. The JSON API field "variant" maps to
-- the forced_variant column.
CREATE TABLE IF NOT EXISTS experiment_overrides (
    exp_key        text PRIMARY KEY,
    killed         boolean NOT NULL DEFAULT false,
    forced_variant text,
    flags          jsonb NOT NULL DEFAULT '{}'::jsonb,
    updated_at     timestamptz NOT NULL DEFAULT now()
);
