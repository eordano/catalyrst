-- catalyrst-economy: quota reservation columns.
--
-- Ports transactions-server migration 1778100766232_add-reservation-columns
-- (4f78f54, "harden relayer against quota TOCTOU"). Replaces the
-- read-then-insert quota with an atomic advisory-locked reserve: the handler
-- inserts a reservation row (tx_hash NULL, session_id set) under a per-user
-- advisory lock BEFORE broadcasting, then either confirms it (tx_hash set,
-- session_id NULL) on a successful broadcast or releases it (row deleted) on a
-- pre-broadcast failure. That requires:
--   - tx_hash droppable to NULL so a pending reservation carries no hash yet
--     (multiple NULLs stay distinct under the existing UNIQUE(tx_hash));
--   - a UNIQUE session_id so confirm/release can address one reservation;
--   - a (user_address, created_at) index backing the per-day COUNT.
ALTER TABLE transactions ALTER COLUMN tx_hash DROP NOT NULL;

ALTER TABLE transactions ADD COLUMN IF NOT EXISTS session_id TEXT UNIQUE;

CREATE INDEX IF NOT EXISTS idx_transactions_user_created
    ON transactions (user_address, created_at);
