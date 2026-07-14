-- De-admission has to leave a mark, and the mark has to be bounded.
--
-- Removing a peer from federation-peers.toml and restarting is the revocation
-- mechanism. Before this migration the restart revoked nothing: `remote_worlds` rows
-- are written per peer_id and nothing ever compared them to the admitted set, so a
-- peer the DAO had dropped went on being published under our origin indefinitely.
--
-- The sweep that closes that (`RemoteWorldsComponent::revoke_peers_no_longer_admitted`)
-- DELETEs the per-world rows, because those are unbounded: a peer can hold tens of
-- thousands of worlds and there is no ceiling on how many peers pass through the file
-- over a deployment's life, so a tombstone per world is a table that only grows. But a
-- bare DELETE would erase the only record that we ever published that peer at all.
--
-- So the two granularities are split. The unbounded thing (one row per world) is
-- deleted. The bounded thing (one row per peer we have ever contacted, which is
-- already this table's shape) is tombstoned. What survives a revocation is: that it
-- happened, when, how many rows it destroyed, and — from the columns already here —
-- when we last successfully heard from the peer and how much it was serving. The world
-- NAMES go to the log at sweep time, which is where an unbounded list belongs.
--
-- Nullable and defaulted, so this migration cannot fail on an existing table.
ALTER TABLE remote_peer_status
    ADD COLUMN IF NOT EXISTS deadmitted_at TIMESTAMPTZ;

-- Cumulative across sweeps: a peer can be de-admitted, re-admitted, re-polled and
-- de-admitted again, and each sweep adds what it destroyed rather than overwriting the
-- previous count.
ALTER TABLE remote_peer_status
    ADD COLUMN IF NOT EXISTS deadmitted_worlds_deleted BIGINT NOT NULL DEFAULT 0;

-- Finding the orphans is an operator question ("what are we holding for peers that are
-- no longer in the file?"), and it is answered by a partial index rather than a scan.
CREATE INDEX IF NOT EXISTS remote_peer_status_deadmitted_idx
    ON remote_peer_status (deadmitted_at)
    WHERE deadmitted_at IS NOT NULL;
