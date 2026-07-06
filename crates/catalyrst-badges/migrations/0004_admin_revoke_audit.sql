-- Admin action audit log. Grant provenance lives inline on the per-user
-- progress/tier rows (migration 0003), but revoke DELETEs those rows, so the
-- "who revoked, and when" information had nowhere to land — leaving an
-- asymmetric audit trail. This independent, append-only log records every
-- bearer-gated admin grant/revoke regardless of whether the user rows survive
-- the operation. Non-destructive: read API never touches this table.

CREATE TABLE IF NOT EXISTS badge_admin_audit (
    id        bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    action    text NOT NULL,            -- 'grant' | 'revoke'
    address   text NOT NULL,
    badge_id  text NOT NULL,
    tier_id   text,                     -- grant: requested tier (nullable)
    actor     text NOT NULL,            -- X-Catalyrst-Admin label, else 'admin-token'
    acted_at  timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS badge_admin_audit_address_idx
    ON badge_admin_audit (address, acted_at);
