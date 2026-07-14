-- 0011-emotional-jobs — this program's own reading of each game's observable
-- design against the six emotional jobs of the strategy deck's slide
-- "10 | EMOTIONAL WHITE SPACE" (11.10, Scott McCarthy working-strategy deck).
-- One row per job the game's design serves, each a curated judgment this
-- program made on read_at — never a fact the deployment entity carries. A row
-- with a NULL job means "read, and honestly serves none of the six"; a scene
-- with no rows at all has not been read. Idempotent, guarded in the 0003+
-- style; mirrored as end-state into schema.sql (the lockstep rule).

DO $mig$
BEGIN
  IF to_regclass('foundry.scene') IS NULL THEN
    RAISE NOTICE '0011-emotional-jobs: foundry.scene absent, nothing to do';
    RETURN;
  END IF;

  CREATE TABLE IF NOT EXISTS foundry.scene_emotional_job (
    id          bigserial PRIMARY KEY,
    scene_id    text NOT NULL REFERENCES foundry.scene(id),
    job         text CHECK (job IN ('A','B','C','D','E','F')),
    rationale   text NOT NULL,
    confidence  text NOT NULL CHECK (confidence IN ('evidence-backed','inferred')),
    read_at     date NOT NULL,
    basis       text NOT NULL
  );
  CREATE UNIQUE INDEX IF NOT EXISTS scene_emotional_job_uq
    ON foundry.scene_emotional_job (scene_id, job) WHERE job IS NOT NULL;
  CREATE UNIQUE INDEX IF NOT EXISTS scene_emotional_job_none_uq
    ON foundry.scene_emotional_job (scene_id) WHERE job IS NULL;
END
$mig$;
