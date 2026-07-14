-- 0015-gdd-grounding — a program brief carries its grounding as stored keys:
-- the market cell it reads (grounds_cell) and the ask ids it quotes
-- (grounding_request_ids), so the shelf and each ask can link back to the
-- brief without parsing prose.
DO $mig$
BEGIN
  IF to_regclass('foundry.gdd_doc') IS NULL THEN
    RAISE NOTICE '0015-gdd-grounding: no gdd_doc yet, schema.sql provisions the end state';
    RETURN;
  END IF;
  ALTER TABLE foundry.gdd_doc ADD COLUMN IF NOT EXISTS grounds_cell text;
  ALTER TABLE foundry.gdd_doc
    ADD COLUMN IF NOT EXISTS grounding_request_ids jsonb NOT NULL DEFAULT '[]'::jsonb;
END
$mig$;
