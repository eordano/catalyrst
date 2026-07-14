-- 0010-market-cells — this program's own reading of each game against the
-- three gaming cells of the strategy deck's slide "09 | MARKET-CELL PORTFOLIO"
-- (11.10, Scott McCarthy working-strategy deck). A row is a curated judgment
-- this program made on classified_at — never a fact the deployment entity
-- carries. cell NULL with a rationale means "examined and honestly
-- unclassifiable" (the deck's slide 12 makes cell fit a gate a concept can
-- fail); a scene with no row has not been read at all. Idempotent, guarded in
-- the 0003+ style; mirrored as end-state into schema.sql (the lockstep rule).

DO $mig$
BEGIN
  IF to_regclass('foundry.scene') IS NULL THEN
    RAISE NOTICE '0010-market-cells: foundry.scene absent, nothing to do';
    RETURN;
  END IF;

  CREATE TABLE IF NOT EXISTS foundry.scene_market_cell (
    scene_id      text PRIMARY KEY REFERENCES foundry.scene(id),
    cell          text CHECK (cell IN (
                    'creator-led-social-competition',
                    'community-operated-game-clubs',
                    'collaborative-build-and-play-labs')),
    rationale     text NOT NULL,
    confidence    text NOT NULL CHECK (confidence IN ('evidence-backed','inferred')),
    classified_at date NOT NULL,
    basis         text NOT NULL
  );
END
$mig$;
