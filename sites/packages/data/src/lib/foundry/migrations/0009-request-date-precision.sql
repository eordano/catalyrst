-- 0009-request-date-precision — sourced_date_only records whether an imported
-- ask's original public date carried a clock time or only a day. The timeline
-- was sniffing T00:00:00 out of the stored timestamp to decide; precision is
-- now a stored fact set by the importer, not a heuristic. Idempotent; mirrored
-- as end-state into schema.sql (the lockstep rule).

DO $mig$
BEGIN
  IF to_regclass('foundry.request') IS NULL THEN
    RAISE NOTICE '0009-request-date-precision: foundry.request absent, nothing to do';
    RETURN;
  END IF;

  ALTER TABLE foundry.request
    ADD COLUMN IF NOT EXISTS sourced_date_only boolean NOT NULL DEFAULT false;
END
$mig$;
