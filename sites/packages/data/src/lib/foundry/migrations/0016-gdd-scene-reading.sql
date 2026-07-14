-- 0016-gdd-scene-reading — this program's own reading that a design doc and a
-- deployed game share a concept. A row is a dated adjacency judgment — never a
-- claim the game implements the doc, which only scene.gdd_doc_id /
-- gdd_doc.scene_id may carry. relation is fixed to 'same-concept' so the table
-- cannot quietly grow stronger claims. Idempotent, guarded in the 0003+ style;
-- mirrored as end-state into schema.sql (the lockstep rule).

DO $mig$
BEGIN
  IF to_regclass('foundry.gdd_doc') IS NULL OR to_regclass('foundry.scene') IS NULL THEN
    RAISE NOTICE '0016-gdd-scene-reading: foundry.gdd_doc or foundry.scene absent, nothing to do';
    RETURN;
  END IF;

  CREATE TABLE IF NOT EXISTS foundry.gdd_scene_reading (
    id bigserial PRIMARY KEY,
    gdd_doc_id text NOT NULL REFERENCES foundry.gdd_doc(id),
    scene_id   text NOT NULL REFERENCES foundry.scene(id),
    relation   text NOT NULL CHECK (relation = 'same-concept'),
    rationale  text NOT NULL,
    confidence text NOT NULL CHECK (confidence IN ('evidence-backed','inferred')),
    basis      text NOT NULL,
    read_at    timestamptz NOT NULL,
    UNIQUE (gdd_doc_id, scene_id)
  );
END
$mig$;
