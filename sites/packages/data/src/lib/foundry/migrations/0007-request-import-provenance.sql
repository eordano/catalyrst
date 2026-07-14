-- 0007-request-import-provenance — import provenance on foundry.request: the
-- public permalink (source_url) and the ask's original public date (sourced_at).
-- Only the ask importer can honestly fill them, so they stay NULL on every
-- visitor row. Idempotent, guarded in the 0003+ style; mirrored as end-state
-- into schema.sql (the lockstep rule).

DO $mig$
BEGIN
  IF to_regclass('foundry.request') IS NULL THEN
    RAISE NOTICE '0007-request-import-provenance: foundry.request absent, nothing to do';
    RETURN;
  END IF;

  ALTER TABLE foundry.request ADD COLUMN IF NOT EXISTS source_url text;
  ALTER TABLE foundry.request ADD COLUMN IF NOT EXISTS sourced_at timestamptz;
END
$mig$;
