-- Fixup for the Stage-1 fixture (migration 0002): 3 of the 4 seeded badge
-- definitions (decentraland_citizen, walkabout, emotionista) shipped with
-- EMPTY 3D asset fields, and every asset URL -- including the fully
-- populated ones (open_for_business) -- points at
-- `https://badges.decentraland.org/assets/...`, a path shape that 404s even
-- on real prod (confirmed live; prod actually serves from a differently
-- shaped `assets-cdn.decentraland.org/{id}/{2d|3d}/{normal|hrm|basecolor}.png`
-- host). Rather than adopt prod's host/shape directly (which would defeat
-- the point of self-hosting), this backfills the missing 3D fields using
-- our OWN existing `badges.decentraland.org/assets/{id}/{dim}/{file}.png`
-- scheme so it stays a single consistent shape -- the crate's config.rs
-- `BADGES_PUBLIC_ASSET_BASE_URL` rewrite (applied at serve time, not here)
-- is what swaps the host to the deployment's self-hosted origin. The actual
-- PNG bytes for these paths ship in crates/catalyrst-badges/assets/ and are
-- served via the new /assets ServeDir route:
--   - open_for_business, decentraland_citizen: real art mirrored one-time
--     from the public, unauthenticated assets-cdn.decentraland.org (static
--     provenance, not a live API dependency).
--   - walkabout, emotionista: assets-cdn has no equivalent under these ids
--     (404), so these are solid-color placeholders pending the authoritative
--     definitions (same caveat migration 0002 already carries).
--
-- 2D hrm/baseColor stay empty for badges that never had them (matches what's
-- actually available upstream); only the 3D set --  what
-- BadgeInfoModule.Badge3DImage renders -- is backfilled here.

UPDATE badge_definitions SET assets =
  '{"2d":{"normal":"https://badges.decentraland.org/assets/decentraland_citizen/2d/normal.png","hrm":"","baseColor":""},
    "3d":{"normal":"https://badges.decentraland.org/assets/decentraland_citizen/3d/normal.png","hrm":"https://badges.decentraland.org/assets/decentraland_citizen/3d/hrm.png","baseColor":"https://badges.decentraland.org/assets/decentraland_citizen/3d/baseColor.png"}}'::jsonb
WHERE id = 'decentraland_citizen';

UPDATE badge_definitions SET assets =
  '{"2d":{"normal":"https://badges.decentraland.org/assets/walkabout/2d/normal.png","hrm":"","baseColor":""},
    "3d":{"normal":"https://badges.decentraland.org/assets/walkabout/3d/normal.png","hrm":"https://badges.decentraland.org/assets/walkabout/3d/hrm.png","baseColor":"https://badges.decentraland.org/assets/walkabout/3d/baseColor.png"}}'::jsonb
WHERE id = 'walkabout';

UPDATE badge_definitions SET assets =
  '{"2d":{"normal":"https://badges.decentraland.org/assets/emotionista/2d/normal.png","hrm":"","baseColor":""},
    "3d":{"normal":"https://badges.decentraland.org/assets/emotionista/3d/normal.png","hrm":"https://badges.decentraland.org/assets/emotionista/3d/hrm.png","baseColor":"https://badges.decentraland.org/assets/emotionista/3d/baseColor.png"}}'::jsonb
WHERE id = 'emotionista';
