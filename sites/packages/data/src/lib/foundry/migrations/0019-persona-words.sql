-- 0019-persona-words — an optional self-description on the persona, in the
-- visitor's own words (the steward claim's `basis` pattern applied to people:
-- identity-as-brand rather than identity-as-record, per the 08-17 directive
-- review). Idempotent, guarded in the 0003+ style; schema.sql carries the same
-- end state (the lockstep rule).

DO $mig$
BEGIN
  IF to_regclass('foundry.persona') IS NULL THEN
    RAISE NOTICE '0019-persona-words: foundry.persona absent, nothing to do';
    RETURN;
  END IF;

  IF EXISTS (
    SELECT 1 FROM information_schema.columns
     WHERE table_schema = 'foundry' AND table_name = 'persona'
       AND column_name = 'words'
  ) THEN
    RETURN;
  END IF;

  ALTER TABLE foundry.persona ADD COLUMN words text;
  ALTER TABLE foundry.persona
    ADD CONSTRAINT persona_words_len
    CHECK (words IS NULL OR char_length(words) BETWEEN 1 AND 280);
END
$mig$;
