-- catalyrst-market federation tables.
--
-- The existing READ tables live in the `squid_marketplace` schema, which is
-- owned by the upstream indexer and exposes SELECT-only access. This migration
-- owns the wallet-anchored signed-action log in the `marketplace` schema. The
-- existing /v1/bids, /v1/orders, /v1/trades handlers continue to read from
-- squid_marketplace only; the new /v1/federation/* read endpoints serve ONLY
-- this local log.
--
-- Authority model is wallet-anchored, not role-anchored: the signer of a
-- BidPlace / OrderCreate / BidCancel / OrderCancel is the actor with no
-- further check. BidAccept additionally requires the signer to currently own
-- at least one nft for the bid's item_id (verified at apply time against
-- squid_marketplace.nft). TradeRecord may be signed by any wallet — the
-- on-chain tx_hash is the canonical proof.

CREATE TABLE IF NOT EXISTS market_bids_local (
    signature_hash    TEXT       PRIMARY KEY,
    item_id           TEXT       NOT NULL,
    signer            TEXT       NOT NULL,
    price             NUMERIC    NOT NULL,
    expires_at        BIGINT     NOT NULL,
    fingerprint       TEXT       NOT NULL DEFAULT '',
    signed_at         BIGINT     NOT NULL,
    message_payload   JSONB      NOT NULL,
    received_at       BIGINT     NOT NULL,
    seq               BIGSERIAL  UNIQUE NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_mbl_item   ON market_bids_local (item_id, signed_at DESC);
CREATE INDEX IF NOT EXISTS idx_mbl_signer ON market_bids_local (signer, signed_at DESC);
CREATE INDEX IF NOT EXISTS idx_mbl_seq    ON market_bids_local (seq);

CREATE TABLE IF NOT EXISTS market_orders_local (
    signature_hash    TEXT       PRIMARY KEY,
    item_id           TEXT       NOT NULL,
    signer            TEXT       NOT NULL,
    price             NUMERIC    NOT NULL,
    expires_at        BIGINT     NOT NULL,
    signed_at         BIGINT     NOT NULL,
    message_payload   JSONB      NOT NULL,
    received_at       BIGINT     NOT NULL,
    seq               BIGSERIAL  UNIQUE NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_mol_item   ON market_orders_local (item_id, signed_at DESC);
CREATE INDEX IF NOT EXISTS idx_mol_signer ON market_orders_local (signer, signed_at DESC);
CREATE INDEX IF NOT EXISTS idx_mol_seq    ON market_orders_local (seq);

CREATE TABLE IF NOT EXISTS market_trades_local (
    signature_hash         TEXT       PRIMARY KEY,
    order_signature_hash   TEXT       NOT NULL,
    buyer                  TEXT       NOT NULL,
    tx_hash                TEXT       NOT NULL,
    taken_at               BIGINT     NOT NULL,
    signed_at              BIGINT     NOT NULL,
    message_payload        JSONB      NOT NULL,
    received_at            BIGINT     NOT NULL,
    seq                    BIGSERIAL  UNIQUE NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_mtl_order ON market_trades_local (order_signature_hash);
CREATE INDEX IF NOT EXISTS idx_mtl_buyer ON market_trades_local (buyer, signed_at DESC);
CREATE INDEX IF NOT EXISTS idx_mtl_tx    ON market_trades_local (tx_hash);
CREATE INDEX IF NOT EXISTS idx_mtl_seq   ON market_trades_local (seq);

CREATE TABLE IF NOT EXISTS market_cancellations (
    signature_hash         TEXT       PRIMARY KEY,
    target_signature_hash  TEXT       NOT NULL,
    kind                   TEXT       NOT NULL CHECK (kind IN ('bid','order')),
    signer                 TEXT       NOT NULL,
    signed_at              BIGINT     NOT NULL,
    message_payload        JSONB      NOT NULL,
    received_at            BIGINT     NOT NULL,
    seq                    BIGSERIAL  UNIQUE NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_mc_target ON market_cancellations (target_signature_hash);
CREATE INDEX IF NOT EXISTS idx_mc_signer ON market_cancellations (signer, signed_at DESC);
CREATE INDEX IF NOT EXISTS idx_mc_seq    ON market_cancellations (seq);

CREATE TABLE IF NOT EXISTS market_bid_acceptances (
    signature_hash      TEXT       PRIMARY KEY,
    bid_signature_hash  TEXT       NOT NULL,
    signer              TEXT       NOT NULL,
    signed_at           BIGINT     NOT NULL,
    message_payload     JSONB      NOT NULL,
    received_at         BIGINT     NOT NULL,
    seq                 BIGSERIAL  UNIQUE NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_mba_bid    ON market_bid_acceptances (bid_signature_hash);
CREATE INDEX IF NOT EXISTS idx_mba_signer ON market_bid_acceptances (signer, signed_at DESC);
CREATE INDEX IF NOT EXISTS idx_mba_seq    ON market_bid_acceptances (seq);

CREATE TABLE IF NOT EXISTS market_seen_nonces (
    signer      TEXT   NOT NULL,
    nonce       TEXT   NOT NULL,
    expires_at  BIGINT NOT NULL,
    PRIMARY KEY (signer, nonce)
);

CREATE INDEX IF NOT EXISTS idx_market_seen_nonces_expires ON market_seen_nonces (expires_at);
