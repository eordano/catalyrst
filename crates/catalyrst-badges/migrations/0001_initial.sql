-- catalyrst-badges schema. Read-only API; per-user progress tables are written
-- out-of-band by a (deferred) event consumer (Stage 2). Definitions/tiers are
-- seeded from a static fixture (Stage 1) since the upstream defs repo is private.

CREATE TABLE IF NOT EXISTS badge_definitions (
    id          text PRIMARY KEY,
    name        text NOT NULL,
    description text,
    category    text,
    is_tier     boolean NOT NULL DEFAULT false,
    -- jsonb { "2d": {normal,hrm,baseColor}, "3d": {...} }
    assets      jsonb NOT NULL DEFAULT '{}'::jsonb
);

CREATE INDEX IF NOT EXISTS badge_definitions_category_idx
    ON badge_definitions (category);

CREATE TABLE IF NOT EXISTS badge_tiers (
    badge_id       text NOT NULL REFERENCES badge_definitions(id) ON DELETE CASCADE,
    tier_id        text NOT NULL,
    tier_name      text NOT NULL,
    description    text,
    assets         jsonb NOT NULL DEFAULT '{}'::jsonb,
    criteria_steps integer NOT NULL DEFAULT 0,
    ordinal        integer NOT NULL DEFAULT 0,
    PRIMARY KEY (badge_id, tier_id)
);

CREATE INDEX IF NOT EXISTS badge_tiers_badge_ordinal_idx
    ON badge_tiers (badge_id, ordinal);

-- Per-user progress, event-driven writes. One row per (address, badge).
CREATE TABLE IF NOT EXISTS user_badge_progress (
    address               text NOT NULL,
    badge_id              text NOT NULL REFERENCES badge_definitions(id) ON DELETE CASCADE,
    steps_done            integer NOT NULL DEFAULT 0,
    completed_at          timestamptz,
    last_completed_tier_id text,
    updated_at            timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (address, badge_id)
);

-- Backs progress.achievedTiers and the /preview endpoint.
CREATE TABLE IF NOT EXISTS user_achieved_tiers (
    address      text NOT NULL,
    badge_id     text NOT NULL REFERENCES badge_definitions(id) ON DELETE CASCADE,
    tier_id      text NOT NULL,
    completed_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (address, badge_id, tier_id)
);

CREATE INDEX IF NOT EXISTS user_achieved_tiers_address_idx
    ON user_achieved_tiers (address, completed_at);
