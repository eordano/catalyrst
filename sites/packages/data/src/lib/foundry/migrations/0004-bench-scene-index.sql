-- 0004-bench-scene-index — index foundry.bot_report on scene_id.
--
-- schema.sql now declares bot_report_scene_idx, but a database provisioned before
-- this migration will not pick it up from a CREATE INDEX that only runs on a
-- fresh install. The per-scene reads (countBenchReports, listBenchReports, the
-- games_passing subquery) all filter/group by scene_id and sequential-scan
-- without it. Idempotent: runs exactly once per database, recorded in
-- foundry.foundry_migration.

DO $mig$
BEGIN
  IF to_regclass('foundry.bot_report') IS NULL THEN
    RAISE NOTICE '0004-bench-scene-index: no bot_report table, nothing to do';
    RETURN;
  END IF;

  CREATE INDEX IF NOT EXISTS bot_report_scene_idx ON foundry.bot_report (scene_id);
END
$mig$;
