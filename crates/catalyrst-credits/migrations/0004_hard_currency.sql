-- catalyrst-credits hard-currency (Credits) spend path
--
-- Phase 1 of the Landiler marketplace: turns the rewards-only ledger into a
-- closed-loop spendable currency. Two additive, non-destructive changes:
--
-- 1. Widen the `credit_ledger.kind` CHECK to allow the new money kinds
--    ('spend','purchase','refund') while KEEPING the existing
--    ('grant','claim','expire','consume') — `consume` is still used by the
--    admin revoke path, so dropping it would break admin_revoke_credits.
--
--    The original constraint (0001_initial.sql) is an INLINE UNNAMED CHECK, so
--    its generated name is not portable. We locate it dynamically (the only
--    CHECK on credit_ledger mentioning `kind`) and replace it with a NAMED one.
--
-- 2. `credit_spend_idempotency` — mirrors `credit_grant_idempotency` so the
--    Phase-3 checkout saga can retry a spend safely.
--
-- NOTE: no BEGIN/COMMIT here — sqlx wraps each migration in its own transaction.

DO $$
DECLARE
    con_name text;
BEGIN
    SELECT con.conname
      INTO con_name
      FROM pg_constraint con
      JOIN pg_class rel ON rel.oid = con.conrelid
      JOIN pg_namespace n ON n.oid = rel.relnamespace
     WHERE rel.relname = 'credit_ledger'
       AND con.contype = 'c'
       AND pg_get_constraintdef(con.oid) ILIKE '%kind%'
     LIMIT 1;

    IF con_name IS NOT NULL THEN
        EXECUTE format('ALTER TABLE credit_ledger DROP CONSTRAINT %I', con_name);
    END IF;
END
$$;

ALTER TABLE credit_ledger
    ADD CONSTRAINT credit_ledger_kind_check
    CHECK (kind IN ('grant', 'claim', 'expire', 'consume', 'spend', 'purchase', 'refund'));

-- Makes a Credits spend safe to retry: the first spend for a key records its
-- result, and any replay of the same key returns that stored result WITHOUT
-- applying a second spend. The UNIQUE primary key plus an insert-inside-the-
-- spend-transaction makes the de-dup atomic even under concurrent retries.
CREATE TABLE IF NOT EXISTS credit_spend_idempotency (
    -- client-supplied idempotency key (opaque text, scoped to spends).
    idempotency_key TEXT PRIMARY KEY,
    -- the address the original spend debited.
    address         TEXT NOT NULL,
    -- exact NUMERIC amount debited by the original spend, kept lossless.
    amount          NUMERIC NOT NULL,
    -- the resulting balance returned by the original spend, kept lossless.
    available       NUMERIC NOT NULL,
    -- opaque reference to the spend's cause (e.g. checkout/order id).
    tx_ref          TEXT,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_credit_spend_idem_address
    ON credit_spend_idempotency(address);
