-- catalyrst-economy: the escrow-action log (Landiler Phase 6).
--
-- One row per relayer-broadcast LandilerEscrow lifecycle call routed through
-- POST /v1/broker/{reclaim,release}. Like `broker_purchases`, an escrow action
-- is a DIRECT relayer transaction (the relayer is both the on-chain `from`/gas
-- payer and the escrow operator). The credits crate's ReleaseWorker (release)
-- and return-handler (reclaim) drive these over the loopback bearer surface.
--
--   action = 'reclaim' — operator-only, BEFORE unlock (return-before-unlock).
--   action = 'release' — permissionless, AT/AFTER unlock (settle to buyer).
--
-- Lives in the WRITABLE `marketplace` schema (same as `broker_purchases`).
--
-- `status` lifecycle:
--   'pending' — claim row inserted BEFORE the on-chain broadcast (keyed path).
--   'sent'    — tx broadcast, hash recorded (keyless path records this directly).
--   'error'   — the broadcast attempt returned an error after the claim.
--
-- IDEMPOTENCY (funds/gas safety): a reclaim/release moves a real NFT on-chain
-- and the credits callers are at-least-once. `idempotency_key` makes a retry a
-- no-op: the handler claims the key with INSERT ... ON CONFLICT DO NOTHING
-- BEFORE broadcasting, so a second POST with the same key never re-broadcasts —
-- it returns the recorded txHash (or 409 while in-flight).

CREATE TABLE IF NOT EXISTS escrow_actions (
    id              BIGSERIAL    PRIMARY KEY,
    action          TEXT         NOT NULL CHECK (action IN ('reclaim', 'release')),
    collection      TEXT,
    token_id        TEXT,
    buyer           TEXT,
    escrow_address  TEXT,
    tx_hash         TEXT,
    idempotency_key TEXT,
    status          TEXT         NOT NULL DEFAULT 'sent',
    chain_id        BIGINT,
    created_at      TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ  NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS escrow_actions_escrow_address_idx
    ON escrow_actions (escrow_address);

-- One logical escrow action per idempotency key. NULL keys (keyless calls) are
-- exempt via the partial predicate, so the claim-first INSERT must repeat the
-- same `WHERE idempotency_key IS NOT NULL` to infer this index.
CREATE UNIQUE INDEX IF NOT EXISTS escrow_actions_idempotency_key_uidx
    ON escrow_actions (idempotency_key) WHERE idempotency_key IS NOT NULL;
