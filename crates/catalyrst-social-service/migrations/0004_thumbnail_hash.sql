-- Persist community thumbnail bytes locally so this catalyst can serve
-- GET /social/communities/{id}/raw-thumbnail.png itself, instead of only
-- constructing a URL that points at an external CDN.
--
-- The bytes live in the content-addressed ContentStore (sha256 file store,
-- same one the federation /content endpoints use). Here we record the hash
-- of the current thumbnail per community alongside the existing has_thumbnail
-- flag, so the raw-thumbnail handler can resolve community_id -> hash -> bytes.
ALTER TABLE community_ranking_metrics
    ADD COLUMN IF NOT EXISTS thumbnail_hash TEXT;
