-- catalyrst-credits grant idempotency + operator identity
--
-- Two additive, non-destructive changes:
--
-- 1. `admin_audit.actor` — the operator label resolved from the trusted
--    `X-Catalyrst-Admin` request header (set server-side by the admin console).
--    Nullable so existing rows and header-less calls stay valid.
--
-- 2. `credit_grant_idempotency` — makes operator credit grants safe to retry.
--    A grant may carry an optional client-supplied idempotency key; the first
--    grant for a key records its result, and any replay of the same key returns
--    that stored result WITHOUT applying a second grant. The UNIQUE primary key
--    plus an insert-inside-the-grant-transaction makes the de-dup atomic even
--    under concurrent retries.

ALTER TABLE admin_audit
    ADD COLUMN IF NOT EXISTS actor TEXT;

CREATE TABLE IF NOT EXISTS credit_grant_idempotency (
    -- client-supplied idempotency key (opaque text, scoped to grants).
    idempotency_key TEXT PRIMARY KEY,
    -- the parameters of the original grant, for audit/debugging and so a replay
    -- can be detected as a true duplicate of the same request.
    address         TEXT NOT NULL,
    -- exact NUMERIC amount applied by the original grant, kept lossless.
    amount          NUMERIC NOT NULL,
    -- the resulting balance returned by the original grant, kept lossless.
    available       NUMERIC NOT NULL,
    -- operator identity (X-Catalyrst-Admin) that performed the original grant.
    actor           TEXT,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_credit_grant_idem_address
    ON credit_grant_idempotency(address);
