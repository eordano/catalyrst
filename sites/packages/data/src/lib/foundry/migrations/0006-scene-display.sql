-- 0006-scene-display — the deployment entity's own scene.json display facts:
-- display.description and the navmapThumbnail resolved to its content URL.
-- Written by foundry:import-real from the worlds content server; NULL until an
-- import has read them. Idempotent, guarded in the 0003+ style; mirrored as
-- end-state into schema.sql (the lockstep rule).

DO $mig$
BEGIN
  IF to_regclass('foundry.scene') IS NULL THEN
    RAISE NOTICE '0006-scene-display: foundry.scene absent, nothing to do';
    RETURN;
  END IF;

  ALTER TABLE foundry.scene ADD COLUMN IF NOT EXISTS description text;
  ALTER TABLE foundry.scene ADD COLUMN IF NOT EXISTS thumbnail_url text;
END
$mig$;
