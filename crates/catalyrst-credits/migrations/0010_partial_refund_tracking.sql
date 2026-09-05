-- catalyrst-credits partial-refund tracking (Stripe webhook hardening)
--
-- A Stripe `charge.refunded` event carries the CUMULATIVE `amount_refunded`
-- for the charge, and fires once per refund. To refund Credits proportionally
-- (and exactly once in aggregate) across multiple partial refunds, the purchase
-- must remember how many fiat cents have already been clawed back, so each event
-- refunds only the INCREMENTAL delta (new cumulative - already-recorded).
--
-- `refunded_cents` is the running total of fiat minor units refunded against the
-- purchase. The proportional Credits refunded per event is
--     credits * (delta_cents / amount_cents)
-- computed in NUMERIC (lossless); summed over a full refund this equals exactly
-- `credits`, preserving the ledger invariant.
--
-- Additive only; no BEGIN/COMMIT (sqlx wraps each migration in its own tx).

ALTER TABLE credit_purchases
    ADD COLUMN IF NOT EXISTS refunded_cents BIGINT NOT NULL DEFAULT 0;
