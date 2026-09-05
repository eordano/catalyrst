-- catalyrst-credits Stripe pack purchases (Phase 2 of the Landiler marketplace)
--
-- Adds the fiat -> Credits purchase substrate: buyable Credit packs, a Stripe
-- event de-dup ledger (at-least-once webhook delivery -> exactly-once effect),
-- and the per-purchase record correlating a Stripe PaymentIntent to a grant.
--
-- Money discipline: Credits amounts are NUMERIC (read/bound as ::text, never
-- f64); Stripe fiat amounts are INTEGER minor units (cents) -> BIGINT.
--
-- All additive; CREATE ... IF NOT EXISTS only. No BEGIN/COMMIT here — sqlx wraps
-- each migration in its own transaction.

-- Buyable Credit packs. `credits` is the NUMERIC Credits granted on purchase;
-- `price_cents` is the fiat charge in minor units (cents).
CREATE TABLE IF NOT EXISTS credit_packs (
    sku         TEXT PRIMARY KEY,
    title       TEXT NOT NULL,
    credits     NUMERIC NOT NULL,
    price_cents BIGINT NOT NULL,
    currency    TEXT NOT NULL,
    active      BOOLEAN NOT NULL DEFAULT TRUE,
    sort_order  INT NOT NULL DEFAULT 0,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Stripe event de-dup ledger. The event_id PK records each delivery once;
-- `processed_at` (stamped only after all side effects commit) makes de-dup
-- COMPLETION-based: a delivery whose row exists but is not yet processed is
-- re-driven on Stripe's retry (the side effects are idempotent), so a crash
-- between recording and granting never silently drops a paid customer's Credits.
CREATE TABLE IF NOT EXISTS stripe_events (
    event_id    TEXT PRIMARY KEY,
    type        TEXT NOT NULL,
    payload     JSONB,
    received_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    processed_at TIMESTAMPTZ
);

-- Per-purchase record correlating a Stripe PaymentIntent to a Credits grant.
-- `credits`/`amount_cents` are captured at intent-creation time; the webhook
-- reads `credits` authoritatively from this row (never trusts webhook metadata).
CREATE TABLE IF NOT EXISTS credit_purchases (
    id                     BIGSERIAL PRIMARY KEY,
    address                TEXT NOT NULL,
    sku                    TEXT NOT NULL,
    credits                NUMERIC NOT NULL,
    amount_cents           BIGINT NOT NULL,
    currency               TEXT NOT NULL,
    stripe_payment_intent  TEXT,
    stripe_event_id        TEXT,
    method                 TEXT NOT NULL DEFAULT 'card',
    status                 TEXT NOT NULL DEFAULT 'pending'
                               CHECK (status IN ('pending', 'paid', 'refunded', 'disputed')),
    created_at             TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at             TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- One purchase row per PaymentIntent (partial index ignores NULLs for the
-- MANA/secondary path that has no Stripe intent).
CREATE UNIQUE INDEX IF NOT EXISTS uq_credit_purchases_payment_intent
    ON credit_purchases(stripe_payment_intent)
    WHERE stripe_payment_intent IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_credit_purchases_address
    ON credit_purchases(address);

-- Makes a Credits refund safe to retry, mirroring credit_spend_idempotency.
-- The Stripe webhook keys a refund/dispute clawback by the Stripe event.id, so
-- an at-least-once redelivery that re-reaches refund() applies the clawback
-- AT MOST once (the first refund records its result; any replay of the same key
-- returns that stored result WITHOUT crediting a second time). The UNIQUE
-- primary key plus an insert-inside-the-refund-transaction makes the de-dup
-- atomic even under concurrent redeliveries.
CREATE TABLE IF NOT EXISTS credit_refund_idempotency (
    -- idempotency key (the Stripe event.id for the reversal).
    idempotency_key TEXT PRIMARY KEY,
    -- the address the original refund credited.
    address         TEXT NOT NULL,
    -- exact NUMERIC amount credited back by the original refund, kept lossless.
    amount          NUMERIC NOT NULL,
    -- the resulting balance returned by the original refund, kept lossless.
    available       NUMERIC NOT NULL,
    -- opaque reference to the refund's cause (e.g. Stripe event id).
    tx_ref          TEXT,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_credit_refund_idem_address
    ON credit_refund_idempotency(address);
