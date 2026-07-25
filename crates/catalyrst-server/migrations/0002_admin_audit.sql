-- Shared admin audit log for the catalyrst admin console.
--
-- Every admin-console mutation (content-local AND the cross-service proxy
-- controls) is recorded here, keyed by the authenticated admin wallet address
-- recovered from the `AdminSession` cookie. This is the single "who did what,
-- when" table the admin-console design (docs/admin-console.md §4) calls for.
--
-- Lives in catalyrst-server's content DB (same DB as 0001_content_schema.sql).
-- Apply this once against the content DB alongside 0001 — NOT via
-- sqlx::migrate! (catalyrst-media owns the shared content DB's
-- _sqlx_migrations table; a second sqlx migrator there would collide). Like
-- 0001 this file is fully idempotent (CREATE ... IF NOT EXISTS).
--
-- Append-only by convention: the application only ever INSERTs.

CREATE TABLE IF NOT EXISTS public.admin_audit (
    id            bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    ts            timestamptz NOT NULL DEFAULT now(),
    admin_address text NOT NULL,
    action        text NOT NULL,
    target        text,
    detail        jsonb NOT NULL DEFAULT '{}'::jsonb,
    result        text NOT NULL
);

CREATE INDEX IF NOT EXISTS admin_audit_ts_idx ON public.admin_audit USING btree (ts DESC);
CREATE INDEX IF NOT EXISTS admin_audit_admin_address_idx ON public.admin_audit USING btree (lower(admin_address));
CREATE INDEX IF NOT EXISTS admin_audit_action_idx ON public.admin_audit USING btree (action);
