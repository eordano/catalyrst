-- Community suspension state (admin-console moderation control, see
-- docs/admin-console.md §4 "catalyrst-communities: community suspend/unsuspend").
--
-- Suspension is a SEPARATE axis from `active`. `active = FALSE` is the
-- deletion/tombstone state driven by the EIP-712 federation write path
-- (apply_delete); a suspension is an operator/moderator hold on a still-existing
-- community, set locally on this catalyst and never minted as a signed federation
-- action. Keeping it on its own column means unsuspend restores the community to
-- exactly its prior (active) visibility without having to reconstruct the deleted
-- flag, and a suspended community can still be deleted/restored through the
-- normal federation path independently.
ALTER TABLE communities
    ADD COLUMN IF NOT EXISTS suspended          BOOLEAN   NOT NULL DEFAULT FALSE,
    ADD COLUMN IF NOT EXISTS suspended_at       TIMESTAMP,
    ADD COLUMN IF NOT EXISTS suspended_by       VARCHAR,
    ADD COLUMN IF NOT EXISTS suspension_reason  TEXT;

CREATE INDEX IF NOT EXISTS idx_communities_suspended
    ON communities (suspended)
    WHERE suspended = TRUE;
