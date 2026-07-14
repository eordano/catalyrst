-- 0013-reading-crowd-range — the deck's own per-cell crowd range, stored on
-- each ask reading so the ask page renders the comparator from a DB column.
-- The string is the per-cell range of the strategy deck's slide "09 |
-- MARKET-CELL PORTFOLIO" (11.10, Scott McCarthy working-strategy deck),
-- verbatim minus the trailing sentence period:
--   creator-led-social-competition    → 6–24 active players + spectators
--   community-operated-game-clubs     → 8–50 recurring participants
--   collaborative-build-and-play-labs → 2–12 contributors + observers
--     (the labs string is unrendered until a labs ask exists — no reading row
--     carries it today, and no surface displays a range without a row)
-- NULL for a reading that fits no cell: no cell, no range. Idempotent, guarded
-- in the 0003+ style; mirrored as end-state into schema.sql (the lockstep
-- rule).

DO $mig$
BEGIN
  IF to_regclass('foundry.request_reading') IS NULL THEN
    RAISE NOTICE '0013-reading-crowd-range: foundry.request_reading absent, nothing to do';
    RETURN;
  END IF;

  ALTER TABLE foundry.request_reading ADD COLUMN IF NOT EXISTS crowd_range text;
END
$mig$;
