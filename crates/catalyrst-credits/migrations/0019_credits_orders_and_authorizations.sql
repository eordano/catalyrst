-- Additive only; no BEGIN/COMMIT (sqlx wraps each migration in its own tx).

ALTER TABLE credit_purchases
    ADD COLUMN IF NOT EXISTS order_id TEXT;
ALTER TABLE credit_purchases
    ADD COLUMN IF NOT EXISTS stripe_checkout_session TEXT;
ALTER TABLE credit_purchases
    ADD COLUMN IF NOT EXISTS failure_reason TEXT;

CREATE UNIQUE INDEX IF NOT EXISTS uq_credit_purchases_order_id
    ON credit_purchases (order_id) WHERE order_id IS NOT NULL;
CREATE UNIQUE INDEX IF NOT EXISTS uq_credit_purchases_checkout_session
    ON credit_purchases (stripe_checkout_session) WHERE stripe_checkout_session IS NOT NULL;

ALTER TABLE credit_purchases
    DROP CONSTRAINT IF EXISTS credit_purchases_status_check;
ALTER TABLE credit_purchases
    ADD CONSTRAINT credit_purchases_status_check
    CHECK (status IN ('pending', 'paid', 'refunded', 'disputed', 'failed', 'abandoned'));

CREATE TABLE IF NOT EXISTS credit_authorizations (
    id                TEXT PRIMARY KEY,
    address           TEXT NOT NULL,
    usd_cents         BIGINT NOT NULL,
    amount_wei        TEXT NOT NULL,
    trade_id          TEXT,
    contract_address  TEXT,
    item_id           TEXT,
    source            TEXT,
    status            TEXT NOT NULL DEFAULT 'authorized'
                          CHECK (status IN ('authorized', 'released', 'consumed', 'expired')),
    expires_at        TIMESTAMPTZ NOT NULL,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_credit_authorizations_address
    ON credit_authorizations (address);
