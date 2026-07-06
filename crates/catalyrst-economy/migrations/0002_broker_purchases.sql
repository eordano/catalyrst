-- catalyrst-economy: the broker-buy log (Landiler Phase 4).
--
-- One row per relayer-paid NFT purchase routed to the Landiler escrow. Unlike
-- `transactions` (user meta-tx relay), a broker buy is a DIRECT relayer
-- transaction: the relayer is both the on-chain `from` (gas + MANA payer) and
-- the caller. The credits crate's fulfillment worker drives these over the
-- loopback bearer surface (POST /v1/broker/buy).
--
-- Lives in the WRITABLE `marketplace` schema (same as `transactions`). Money is
-- NUMERIC: `price_wei` is the MANA-wei integer price as NUMERIC/text — NEVER
-- f64.
--
-- `status` lifecycle:
--   'pending' — claim row inserted BEFORE the on-chain broadcast (keyed path).
--   'sent'    — tx broadcast, hash recorded (keyless path records this directly).
--   'error'   — the broadcast attempt returned an error after the claim.
-- A reconciler later confirms/fails 'sent' rows on-chain.
--
-- IDEMPOTENCY (funds safety): a broker buy moves real MANA + gas on-chain, and
-- the credits fulfillment worker calls it at-least-once (a lost HTTP response
-- after a successful broadcast triggers a retry). `idempotency_key` makes the
-- retry a no-op: the handler claims the key with INSERT ... ON CONFLICT DO
-- NOTHING BEFORE broadcasting, so a second POST with the same key never
-- re-broadcasts — it returns the recorded txHash (or 409 while in-flight).

CREATE TABLE IF NOT EXISTS broker_purchases (
    id              BIGSERIAL    PRIMARY KEY,
    idempotency_key TEXT,
    tx_hash         TEXT,
    collection      TEXT         NOT NULL,
    item_id         TEXT,
    token_id        TEXT,
    buyer_address   TEXT,
    escrow_address  TEXT         NOT NULL,
    price_wei       NUMERIC      NOT NULL,
    chain_id        BIGINT       NOT NULL,
    mode            TEXT         NOT NULL,
    status          TEXT         NOT NULL DEFAULT 'sent',
    created_at      TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ  NOT NULL DEFAULT NOW()
);

-- Forward-compat if an earlier (keyless) version of the table already exists:
-- both additions are pure-additive and safe to re-run.
ALTER TABLE broker_purchases ADD COLUMN IF NOT EXISTS idempotency_key TEXT;
ALTER TABLE broker_purchases ADD COLUMN IF NOT EXISTS updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW();

CREATE INDEX IF NOT EXISTS broker_purchases_escrow_address_idx
    ON broker_purchases (escrow_address);

-- One logical broker buy per idempotency key. NULL keys (legacy/keyless calls)
-- are exempt via the partial predicate, so the claim-first INSERT must repeat
-- the same `WHERE idempotency_key IS NOT NULL` to infer this index.
CREATE UNIQUE INDEX IF NOT EXISTS broker_purchases_idempotency_key_uidx
    ON broker_purchases (idempotency_key) WHERE idempotency_key IS NOT NULL;
