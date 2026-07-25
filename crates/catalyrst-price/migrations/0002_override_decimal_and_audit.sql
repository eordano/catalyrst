-- catalyrst-price: make the override value exact + add an admin audit trail.
--
-- 1) The override `value` was DOUBLE PRECISION (f64). Mirroring the credits
--    crate's never-f64 stance, store it as exact NUMERIC instead so an operator
--    override (e.g. a hand-set MANA/USD spot) keeps full decimal precision and
--    never round-trips through binary float. The handler now accepts/serves it
--    as a decimal string. The cast is value-preserving for existing rows.
--
-- 2) High-risk price overrides (set/clear) are bearer-gated and must be
--    auditable: every successful set/clear appends one immutable row here,
--    attributed to the console-set `X-Catalyrst-Admin` identity.
--
-- Both changes are additive / non-destructive: no rows are dropped, the public
-- price route is untouched, and existing override rows survive the type change.

ALTER TABLE price_overrides
    ALTER COLUMN value TYPE NUMERIC USING value::numeric;

CREATE TABLE IF NOT EXISTS price_override_audit (
    id           BIGSERIAL PRIMARY KEY,
    -- 'override.set' or 'override.clear'.
    action       TEXT        NOT NULL,
    -- the override key this action targeted (lowercased, as stored).
    token_id     TEXT        NOT NULL,
    vs_currency  TEXT        NOT NULL,
    -- exact NUMERIC override value for a set; NULL for a clear.
    value        NUMERIC,
    -- free-form operator note carried on the set (audit trail).
    note         TEXT,
    -- console-attributed admin identity (X-Catalyrst-Admin header; "console"
    -- when the header is absent).
    admin        TEXT        NOT NULL,
    -- full request detail captured as JSON for forensics.
    detail       JSONB       NOT NULL DEFAULT '{}'::jsonb,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_price_override_audit_action
    ON price_override_audit(action);
CREATE INDEX IF NOT EXISTS idx_price_override_audit_key
    ON price_override_audit(token_id, vs_currency);
CREATE INDEX IF NOT EXISTS idx_price_override_audit_created
    ON price_override_audit(created_at DESC);
