-- Refund idempotency: record the post-clamp amount actually credited.
--
-- refund_in_tx clamps a refund to the tx_ref's remaining spend window
-- (SUM(spend) - SUM(refund)), so the amount CREDITED can be smaller than the
-- amount REQUESTED. `amount` must keep holding the requested value: the replay
-- guard matches a retried key against it, and overwriting it with the clamped
-- result would 409 legitimate retries (e.g. a re-run compensate after an admin
-- refund, wedging the checkout in 'reversing'). `applied` stores what actually
-- moved; replays report COALESCE(applied, amount), so rows written before this
-- migration (NULL applied) keep their old behavior.
--
-- Additive only; no BEGIN/COMMIT (sqlx wraps each migration in its own tx).

ALTER TABLE credit_refund_idempotency
    ADD COLUMN IF NOT EXISTS applied NUMERIC;
