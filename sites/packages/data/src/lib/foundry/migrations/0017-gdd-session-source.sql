-- 0017-gdd-session-source — the edit feature mints new versions with
-- source = 'session' (a visitor session's own edit, supersede-on-edit), but the
-- source CHECK still named only the three import origins, so every edit died on
-- gdd_doc_source_check. Widen the constraint to the four values the code
-- writes. Idempotent, guarded in the 0003+ style; schema.sql carries the same
-- end state (the lockstep rule).

DO $mig$
BEGIN
  IF to_regclass('foundry.gdd_doc') IS NULL THEN
    RAISE NOTICE '0017-gdd-session-source: foundry.gdd_doc absent, nothing to do';
    RETURN;
  END IF;

  IF EXISTS (
    SELECT 1 FROM pg_constraint
     WHERE conname = 'gdd_doc_source_check'
       AND pg_get_constraintdef(oid) LIKE '%session%'
  ) THEN
    RETURN;
  END IF;

  ALTER TABLE foundry.gdd_doc DROP CONSTRAINT IF EXISTS gdd_doc_source_check;
  ALTER TABLE foundry.gdd_doc
    ADD CONSTRAINT gdd_doc_source_check
    CHECK (source IN ('slack-import','copilot','program','session'));
END
$mig$;
