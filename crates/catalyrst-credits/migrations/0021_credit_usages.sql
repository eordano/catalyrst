-- Additive only; no BEGIN/COMMIT (sqlx wraps each migration in its own tx).
-- One row per on-chain CreditUsed event, keyed by the CreditsManager creditId.
-- The squid indexer ingests CreditUsed logs into this table; the reconciler in
-- ports/reconcile.rs joins it against credit_authorizations to debit balances.

CREATE TABLE IF NOT EXISTS credit_usages (
    credit_id   TEXT PRIMARY KEY,
    address     TEXT,
    value_wei   TEXT,
    tx_hash     TEXT,
    used_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_credit_usages_address ON credit_usages (address);
