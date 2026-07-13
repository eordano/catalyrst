-- Admin grant/revoke provenance. The read-only API never wrote these tables;
-- the bearer-gated admin endpoints (POST/DELETE /users/{address}/badges/{badge_id})
-- now do. These nullable columns record who issued a manual grant and when, so a
-- console-driven grant is distinguishable from event-consumer (Stage-2) writes.

ALTER TABLE user_badge_progress
    ADD COLUMN IF NOT EXISTS granted_by text;

ALTER TABLE user_achieved_tiers
    ADD COLUMN IF NOT EXISTS granted_by text;

ALTER TABLE user_achieved_tiers
    ADD COLUMN IF NOT EXISTS granted_at timestamptz;
