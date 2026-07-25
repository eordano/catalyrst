-- catalyrst-economy: relayer-custodied NAME transfers (the manage/release leg
-- of the NAMEs lease model). Name BUYS reuse `broker_purchases` (mode
-- 'name-mint' / 'name-secondary', chain_id = the NAMEs chain, escrow_address =
-- the custody address); transfers get their own ledger because they move an
-- already-custodied NFT rather than spend MANA.
--
-- Same funds-safety shape as broker_purchases: claim the idempotency key with
-- INSERT ... ON CONFLICT DO NOTHING BEFORE broadcasting, so an at-least-once
-- caller can never double-send a token.
--
-- `status`: 'pending' -> 'sent' -> 'confirmed' | 'reverted'; 'error' when the
-- broadcast attempt itself failed (safe to re-arm).

CREATE TABLE IF NOT EXISTS name_transfers (
    id              BIGSERIAL    PRIMARY KEY,
    idempotency_key TEXT         NOT NULL UNIQUE,
    registrar       TEXT         NOT NULL,
    token_id        TEXT         NOT NULL,
    from_address    TEXT         NOT NULL,
    to_address      TEXT         NOT NULL,
    chain_id        BIGINT       NOT NULL,
    tx_hash         TEXT,
    status          TEXT         NOT NULL DEFAULT 'pending',
    created_at      TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ  NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS name_transfers_token_id_idx
    ON name_transfers (token_id);
