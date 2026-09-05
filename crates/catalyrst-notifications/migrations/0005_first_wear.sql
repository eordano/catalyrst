-- friend_first_wear ingestion state (see src/first_wear.rs).
--
-- Numbered 0005/0006: earlier 0003/0004 copies were applied to the live DB
-- by a dev-cycle restart before deploy, which crashed any OLDER binary
-- ("version 3 ... missing in resolved migrations"). Those ledger rows were
-- reconciled away (2026-07-06); every statement here is idempotent, so
-- re-application over the existing tables is a no-op.
--
-- worn_history is the per-address baseline of every marketplace wearable that
-- address has EVER had in a deployed profile. "First wear" means: the address
-- already has a baseline (so brand-new addresses seed silently instead of
-- spamming their whole outfit) and the urn was never in it. The PK is also
-- the claim that stops double-notification across worker restarts.
CREATE TABLE IF NOT EXISTS worn_history (
    address    TEXT NOT NULL,
    urn        TEXT NOT NULL,
    -- entity_timestamp (ms) of the profile deployment that introduced it
    first_seen BIGINT NOT NULL,
    PRIMARY KEY (address, urn)
);

-- Poll cursor over catalyst profile deployments (single row). Initialized to
-- now() on first run so history is never replayed as fake "first wears".
CREATE TABLE IF NOT EXISTS first_wear_cursor (
    id        INT PRIMARY KEY DEFAULT 1 CHECK (id = 1),
    last_seen TIMESTAMP NOT NULL
);
