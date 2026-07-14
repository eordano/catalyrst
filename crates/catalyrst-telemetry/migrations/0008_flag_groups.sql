-- Audience groups: named cohorts a flag or experiment can be targeted at, so an
-- override can reach some users instead of the whole world. Membership is an
-- explicit user-key list OR a deterministic percentage rollout (see
-- handlers/groups.rs: the bucket is an FNV-1a hash of "<group>:<user_key>", kept
-- in Rust so it is stable across Postgres versions and unit-testable).
--
-- Resolution precedence, matching operator intuition ("off for everyone except
-- internal"): a matching group target BEATS the global override, which beats the
-- upstream FLAGS_URL config. `priority` breaks ties when a user matches several
-- groups (highest wins, then name).
--
-- Created unqualified: the connection sets search_path=telemetry, the same
-- convention migrations 0001-0007 rely on.
CREATE TABLE IF NOT EXISTS flag_groups (
    name         text PRIMARY KEY,
    description  text NOT NULL DEFAULT '',
    members      text[] NOT NULL DEFAULT '{}',
    rollout_pct  integer NOT NULL DEFAULT 0 CHECK (rollout_pct BETWEEN 0 AND 100),
    priority     integer NOT NULL DEFAULT 0,
    updated_at   timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS flag_group_targets (
    flag           text NOT NULL,
    group_name     text NOT NULL REFERENCES flag_groups(name) ON DELETE CASCADE,
    state          text NOT NULL DEFAULT 'on'
                   CHECK (state IN ('on','off','forced')),
    forced_variant text,
    updated_at     timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (flag, group_name)
);

CREATE TABLE IF NOT EXISTS experiment_group_targets (
    exp_key        text NOT NULL,
    group_name     text NOT NULL REFERENCES flag_groups(name) ON DELETE CASCADE,
    killed         boolean NOT NULL DEFAULT false,
    forced_variant text,
    flags          jsonb NOT NULL DEFAULT '{}'::jsonb,
    updated_at     timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (exp_key, group_name)
);

-- Product area is a display tag. It lives apart from the override tables so a
-- flag can be filed under an area without being overridden.
CREATE TABLE IF NOT EXISTS product_areas (
    kind       text NOT NULL CHECK (kind IN ('flag','experiment')),
    name       text NOT NULL,
    area       text NOT NULL,
    updated_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (kind, name)
);
