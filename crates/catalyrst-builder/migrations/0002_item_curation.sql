-- Item curation status for the admin console (admin-console.md §4).
--
-- The initial schema only carried an `is_approved` boolean on items, which
-- cannot distinguish a never-reviewed item ("pending") from one a curator
-- explicitly rejected. The console's curation queue needs that tristate, so we
-- add an explicit `curation_status` column ('pending' | 'approved' | 'rejected')
-- and backfill it from the existing boolean. The boolean is kept in sync by the
-- status handler so existing readers (to_full_item -> is_approved) keep working.

ALTER TABLE items
    ADD COLUMN IF NOT EXISTS curation_status TEXT NOT NULL DEFAULT 'pending';

UPDATE items
    SET curation_status = CASE WHEN is_approved THEN 'approved' ELSE 'pending' END
    WHERE curation_status = 'pending';

CREATE INDEX IF NOT EXISTS items_curation_status_idx ON items (curation_status);
