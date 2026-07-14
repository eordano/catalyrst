-- 0012-request-readings — this program's own reading of each imported ask
-- against the three gaming cells of the strategy deck's slide "09 | MARKET-CELL
-- PORTFOLIO" and the six emotional jobs of slide "10 | EMOTIONAL WHITE SPACE"
-- (11.10, Scott McCarthy working-strategy deck). A row is a curated judgment
-- made on read_at, never a fact the ask carries. cell NULL = read, fits no
-- single cell; shelf_answer NULL = no game on the shelf answers it; no row =
-- not read. Idempotent, guarded in the 0003+ style; mirrored as end-state into
-- schema.sql (the lockstep rule).

DO $mig$
BEGIN
  IF to_regclass('foundry.request') IS NULL THEN
    RAISE NOTICE '0012-request-readings: foundry.request absent, nothing to do';
    RETURN;
  END IF;

  CREATE TABLE IF NOT EXISTS foundry.request_reading (
    request_id   text PRIMARY KEY REFERENCES foundry.request(id),
    cell         text CHECK (cell IN (
                   'creator-led-social-competition',
                   'community-operated-game-clubs',
                   'collaborative-build-and-play-labs')),
    jobs         text NOT NULL DEFAULT '' CHECK (jobs ~ '^([A-F](,[A-F])*)?$'),
    shelf_answer text REFERENCES foundry.scene(id),
    rationale    text NOT NULL,
    confidence   text NOT NULL CHECK (confidence IN ('evidence-backed','inferred')),
    read_at      date NOT NULL,
    basis        text NOT NULL
  );
END
$mig$;
