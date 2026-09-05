-- catalyrst-credits admin audit
-- High-risk financial admin controls (seasons/goals CRUD, grant/revoke
-- credits, block/unblock a user) are bearer-gated and must be auditable.
-- Every successful admin mutation appends one immutable row here.

CREATE TABLE IF NOT EXISTS admin_audit (
    id           BIGSERIAL PRIMARY KEY,
    -- what was mutated, e.g. 'season.create', 'goal.delete',
    -- 'credits.grant', 'credits.revoke', 'user.block', 'user.unblock'.
    action       TEXT NOT NULL,
    -- target wallet address (lowercase 0x...) when the action targets a user;
    -- NULL for config-only actions (seasons/goals).
    address      TEXT,
    -- target entity id (season/week/goal id) when relevant.
    entity_id    BIGINT,
    -- exact NUMERIC amount for grant/revoke, kept lossless as text.
    amount       NUMERIC,
    -- free-form reason supplied by the operator (audit trail).
    reason       TEXT,
    -- full request detail captured as JSON for forensics.
    detail       JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_admin_audit_action  ON admin_audit(action);
CREATE INDEX IF NOT EXISTS idx_admin_audit_address ON admin_audit(address);
CREATE INDEX IF NOT EXISTS idx_admin_audit_created ON admin_audit(created_at DESC);
