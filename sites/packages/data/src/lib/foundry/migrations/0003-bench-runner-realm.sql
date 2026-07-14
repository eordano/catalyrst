-- 0003-bench-runner-realm — carry the run's runner and realm on bot_report.
--
-- The v3 install (0002 + schema.sql) shipped bot_report without a `runner` or a
-- `realm` column, so an arena sandbox simulation was indistinguishable from a run
-- against a real scene, and games_passing counted it. schema.sql now declares
-- both columns, but a database that already has a v3 bot_report will not pick
-- them up from `CREATE TABLE IF NOT EXISTS` — this migration adds them and
-- backfills the runner from each report's linked trajectory.
--
-- `realm` is left NULL for existing rows: the pre-fix ingest dropped scene.realm
-- before it reached storage, so there is nothing to backfill; a re-ingest fills
-- it. Runs exactly once per database, recorded in foundry.foundry_migration.

DO $mig$
BEGIN
  IF to_regclass('foundry.bot_report') IS NULL THEN
    RAISE NOTICE '0003-bench-runner-realm: no bot_report table, nothing to do';
    RETURN;
  END IF;

  ALTER TABLE foundry.bot_report
    ADD COLUMN IF NOT EXISTS runner text,
    ADD COLUMN IF NOT EXISTS realm text;

  IF NOT EXISTS (
    SELECT 1 FROM pg_constraint
     WHERE conname = 'bot_report_runner_check'
       AND conrelid = 'foundry.bot_report'::regclass
  ) THEN
    ALTER TABLE foundry.bot_report
      ADD CONSTRAINT bot_report_runner_check
      CHECK (runner IS NULL OR runner IN ('dclbots','arena'));
  END IF;

  -- Backfill the runner from the episode each report links, so an already-ingested
  -- arena run is recognised as a sandbox simulation and drops out of games_passing.
  UPDATE foundry.bot_report b
     SET runner = t.runner
    FROM foundry.trajectory t
   WHERE b.trajectory_id = t.id
     AND b.runner IS NULL
     AND t.runner IS NOT NULL;
END
$mig$;
