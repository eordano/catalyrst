-- Audit fix (concurrency): the upstream-ported `unique_addresses` constraint is
-- DIRECTIONAL — UNIQUE (address_requester, address_requested). It does NOT stop
-- two concurrent requests on the same logical pair (A->B and B->A) from each
-- inserting a row, because the ordered tuples differ. The existence check in
-- `apply_friendship_action` reads the pair UNORDERED but outside the insert's
-- transaction, so both requests see `existing = None` and both INSERT,
-- producing two friendship rows for one pair (lost-update / duplicate).
--
-- This index makes the unordered pair unique at the storage layer, so the
-- second concurrent INSERT fails with a unique violation; the code now catches
-- that and resolves the existing row instead (see apply_friendship_action).
--
-- NOTE: if the table already contains duplicate unordered pairs (e.g. created
-- by the race before this fix), this index creation will FAIL. De-duplicate
-- first, keeping the row with the most recent friendship_action, then re-run.

CREATE UNIQUE INDEX IF NOT EXISTS friendships_unordered_pair
    ON friendships (LEAST(address_requester, address_requested),
                    GREATEST(address_requester, address_requested));
