-- 0014-gdd-program-source — gdd_doc.source gains 'program': a document
-- drafted by this site's own program, neither imported nor a copilot recording.
DO $mig$
BEGIN
  IF to_regclass('foundry.gdd_doc') IS NULL THEN
    RAISE NOTICE '0014-gdd-program-source: no gdd_doc yet, schema.sql provisions the end state';
    RETURN;
  END IF;
  ALTER TABLE foundry.gdd_doc DROP CONSTRAINT IF EXISTS gdd_doc_source_check;
  ALTER TABLE foundry.gdd_doc ADD CONSTRAINT gdd_doc_source_check
    CHECK (source IN ('slack-import','copilot','program'));
END
$mig$;
