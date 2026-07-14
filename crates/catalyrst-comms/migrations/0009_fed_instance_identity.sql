-- Persisted per-instance federation identity, the fallback for FED_PEER_ID.
--
-- mls_groups.epoch_author is a federation identity: it decides which catalyst
-- may advance a group's epoch. The code used to fall back to the literal
-- "local" when FED_PEER_ID was unset, so every unconfigured instance claimed
-- the same identity — an epoch-author collision the moment two of them ever
-- share a group. Mint one stable random id per instance instead, and lift the
-- legacy "local" rows onto it so groups created before this migration keep
-- their epoch author (under the old default those rows meant "this instance"
-- anyway).
CREATE TABLE IF NOT EXISTS fed_instance_identity (
    only_row   BOOLEAN PRIMARY KEY DEFAULT TRUE CHECK (only_row),
    peer_id    TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

INSERT INTO fed_instance_identity (only_row, peer_id)
VALUES (TRUE, 'instance-' || gen_random_uuid())
ON CONFLICT (only_row) DO NOTHING;

UPDATE mls_groups
SET epoch_author = (SELECT peer_id FROM fed_instance_identity)
WHERE epoch_author = 'local';
