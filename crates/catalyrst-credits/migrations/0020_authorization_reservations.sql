-- Additive only; no BEGIN/COMMIT (sqlx wraps each migration in its own tx).

ALTER TABLE credit_authorizations
    ADD COLUMN IF NOT EXISTS idempotency_key TEXT;

CREATE UNIQUE INDEX IF NOT EXISTS uq_credit_authorizations_idempotency_key
    ON credit_authorizations (idempotency_key) WHERE idempotency_key IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_credit_authorizations_addr_status_expiry
    ON credit_authorizations (address, status, expires_at);
