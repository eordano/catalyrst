-- catalyrst-economy: on-chain confirmation + 2-tx forward-to-escrow tracking.
--
-- FIX #2 (confirmation) + FIX #3 (secondary forward) for the Landiler broker.
--
-- Before this change a broker buy was marked terminal as soon as
-- `eth_sendRawTransaction` returned a hash ('sent'), and a `safeTransferFrom`
-- forward into the escrow had nowhere to be recorded. Both are corrected:
--
--   * The buy tx is now CONFIRMED only after `eth_getTransactionReceipt` reports
--     status==1. The lifecycle therefore grows two terminal-truth states.
--   * Acquiring the NFT and forwarding it into custody are TWO transactions
--     (DCL `_mint`/`executeOrder` land the token at the relayer, which must then
--     `safeTransferFrom` it to the escrow WITH the buyer `_data`). The forward
--     tx hash and the auto-assigned minted tokenId are tracked here.
--
-- `status` lifecycle (broker_purchases):
--   'pending'    — claim row inserted BEFORE the buy broadcast (keyed path).
--   'sent'       — buy broadcast, hash recorded, receipt NOT yet observed.
--   'reverted'   — the buy (or forward) tx was mined with status==0.
--   'bought'     — buy confirmed on-chain (status==1); awaiting the forward tx.
--   'forwarding' — forward safeTransferFrom broadcast; receipt not yet observed.
--   'confirmed'  — the NFT is in escrow custody with the buyer lease recorded
--                  (forward tx mined with status==1). ONLY this returns 200+txHash.
--   'error'      — a pre-broadcast failure (validation/estimate) after the claim.
--
-- All additions are pure-additive and safe to re-run. `status` stays free TEXT
-- (no CHECK to alter); the new states are written by the handler/reconciler.

ALTER TABLE broker_purchases ADD COLUMN IF NOT EXISTS forward_tx_hash TEXT;
ALTER TABLE broker_purchases ADD COLUMN IF NOT EXISTS minted_token_id TEXT;

-- The reconciler scans non-terminal rows ('sent','bought','forwarding') to
-- re-poll their receipts; a partial index keeps that scan cheap.
CREATE INDEX IF NOT EXISTS broker_purchases_unsettled_idx
    ON broker_purchases (status)
    WHERE status IN ('sent', 'bought', 'forwarding');
