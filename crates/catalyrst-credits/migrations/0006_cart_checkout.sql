-- catalyrst-credits cart + checkout saga (Phase 3 of the Landiler marketplace)
--
-- The fiat -> Credits -> wearables flow debits Credits ONCE, locally, in this DB
-- (atomic, REJECT-if-short — never a clamp), then fulfils the on-chain NFT buy
-- asynchronously through an outbox/saga. This migration adds the four tables that
-- substrate needs:
--
--   carts            one open cart per wallet (UNIQUE address)
--   cart_items       server-re-priced line items (unit_price_credits NUMERIC)
--   checkouts        one row per checkout attempt, keyed by an Idempotency-Key
--   fulfillment_outbox  one row per cart line, drained by the background worker
--
-- Money discipline: every Credits/price amount is NUMERIC (read/bound as ::text,
-- never f64). Quantities are plain INTs. All additive; CREATE ... IF NOT EXISTS
-- only. No BEGIN/COMMIT here — sqlx wraps each migration in its own transaction.

-- One open cart per wallet. The wallet address is the natural key (lowercase,
-- enforced by the caller) so cart lookups never need a separate id round-trip.
CREATE TABLE IF NOT EXISTS carts (
    id         BIGSERIAL PRIMARY KEY,
    address    TEXT NOT NULL UNIQUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- A cart line. `unit_price_credits` is the server-computed Credit price captured
-- at add time (and re-computed at checkout) — the client price is never trusted.
-- UNIQUE(cart_id,item_id) lets a re-add bump qty / re-price instead of dup'ing.
CREATE TABLE IF NOT EXISTS cart_items (
    id                 BIGSERIAL PRIMARY KEY,
    cart_id            BIGINT NOT NULL REFERENCES carts(id) ON DELETE CASCADE,
    item_id            TEXT NOT NULL,
    urn                TEXT NOT NULL,
    category           TEXT NOT NULL,
    qty                INT NOT NULL DEFAULT 1 CHECK (qty > 0),
    unit_price_credits NUMERIC NOT NULL,
    added_at           TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (cart_id, item_id)
);

-- One row per checkout attempt. `idempotency_key` (the client's Idempotency-Key
-- header) is UNIQUE so a retried POST /checkout re-finds the same row instead of
-- double-debiting. `total_credits` is the authoritative server-priced total;
-- `ledger_id` back-references the `credit_ledger` spend row for reconciliation.
CREATE TABLE IF NOT EXISTS checkouts (
    id              BIGSERIAL PRIMARY KEY,
    idempotency_key TEXT NOT NULL UNIQUE,
    address         TEXT NOT NULL,
    total_credits   NUMERIC NOT NULL DEFAULT 0,
    -- 'reversing' is the FROZEN-for-compensation state: a fulfilment line failed
    -- terminally, so no further lines may be processed (the worker drains only
    -- 'fulfilling' checkouts) while the refund of the undelivered portion is
    -- applied. It resolves to 'reversed' (nothing delivered) or 'failed' (a
    -- sibling line already confirmed on-chain, so it is a partial fulfilment).
    status          TEXT NOT NULL DEFAULT 'reserving'
                        CHECK (status IN ('reserving', 'debited', 'fulfilling',
                                          'fulfilled', 'failed', 'reversing',
                                          'reversed')),
    ledger_id       BIGINT,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_checkouts_address ON checkouts(address);

-- The fulfilment outbox: one row per cart line at checkout time. The background
-- worker drains 'pending' rows by POSTing to the economy broker; a retryable
-- error bumps `attempts` and leaves the row 'pending', a terminal error marks it
-- 'failed' (and the checkout is compensated/refunded). `external_ref` records the
-- broker tx hash on confirmation.
-- A line with qty=N expands to N rows (one broker buy == one minted unit), so
-- the number of broker buys equals the charged quantity. `unit_price_credits`
-- is the per-UNIT Credit price captured at checkout; it lets compensation refund
-- exactly the undelivered (non-'confirmed') portion of a partially-fulfilled
-- checkout without re-pricing, and makes SUM(unit_price_credits) over a
-- checkout's rows equal the debited total by construction.
CREATE TABLE IF NOT EXISTS fulfillment_outbox (
    id                 BIGSERIAL PRIMARY KEY,
    checkout_id        BIGINT NOT NULL REFERENCES checkouts(id),
    item_id            TEXT NOT NULL,
    urn                TEXT NOT NULL,
    token_id           TEXT,
    unit_price_credits NUMERIC NOT NULL,
    mode               TEXT NOT NULL,
    status       TEXT NOT NULL DEFAULT 'pending'
                     CHECK (status IN ('pending', 'sent', 'confirmed', 'failed')),
    attempts     INT NOT NULL DEFAULT 0,
    last_error   TEXT,
    external_ref TEXT,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_fulfillment_outbox_status
    ON fulfillment_outbox(status);

CREATE INDEX IF NOT EXISTS idx_fulfillment_outbox_checkout
    ON fulfillment_outbox(checkout_id);
