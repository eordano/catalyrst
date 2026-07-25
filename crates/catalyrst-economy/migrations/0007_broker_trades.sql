-- catalyrst-economy: off-chain trade acceptance (mode 'trade').
--
-- A broker buy of a Marketplace v3 signed trade accepts a SIGNATURE, not a
-- token listing, so the natural double-spend key is the trade's
-- hashed_signature (keccak256 of the signer's EIP-712 signature — the same
-- value the contract's signatureUses accounting is keyed on, and the same
-- value marketplace.trades is UNIQUE on).
--
-- The per-request idempotency_key (checkout:<id>:<line>) still deduplicates
-- RETRIES of one logical purchase; this column + partial unique index
-- additionally refuse a SECOND logical purchase of the same 1-use trade
-- under a different key (two checkouts racing for the same signed listing):
-- the second INSERT hits broker_purchases_trade_sig_uidx and the handler
-- returns a terminal error instead of burning gas on a guaranteed
-- SignatureOveruse/UsedTradeId revert.
--
-- Rows that ended 'reverted' or 'error' leave the index so the trade can be
-- re-attempted (the on-chain contract remains the authority).

ALTER TABLE broker_purchases ADD COLUMN IF NOT EXISTS trade_hashed_signature TEXT;

CREATE UNIQUE INDEX IF NOT EXISTS broker_purchases_trade_sig_uidx
    ON broker_purchases (trade_hashed_signature)
    WHERE trade_hashed_signature IS NOT NULL
      AND status NOT IN ('reverted', 'error');
