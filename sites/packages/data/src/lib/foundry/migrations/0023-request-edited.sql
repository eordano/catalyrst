-- 0023-request-edited — edited_at records that an ask's own author revised
-- its wording after posting. Only visitor-origin rows ever carry it: an
-- imported ask is verbatim public speech and editRequest refuses to touch
-- one. The prior wording is preserved in foundry.action_log by the edit
-- itself, and every surface that renders the ask shows the stamp, so a
-- reader can never mistake a revised ask for its original wording.
-- Idempotent; mirrored as end-state into schema.sql (the lockstep rule).

DO $mig$
BEGIN
  IF to_regclass('foundry.request') IS NULL THEN
    RAISE NOTICE '0023-request-edited: foundry.request absent, nothing to do';
    RETURN;
  END IF;

  ALTER TABLE foundry.request
    ADD COLUMN IF NOT EXISTS edited_at timestamptz;
END
$mig$;
