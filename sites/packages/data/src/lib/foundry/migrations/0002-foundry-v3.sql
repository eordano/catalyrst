-- 0002-foundry-v3 — take a live v2 foundry database to the v3 shape.
--
-- v2 modelled a trial programme that never happened: teams, cohorts, sessions,
-- gate verdicts, revenue, payouts, relay runs, an LCG-driven bot swarm. None of
-- it described anything that occurred, so none of it is relabelled here — the
-- tables are dropped and the rows are deleted.
--
-- What survives: action_log (append-only history of real mutations, kept in
-- full), and the identity of request/pledge/scene/scene_changelog, reshaped to
-- v3. The one real visitor pledge is lost with the rest: it pledged to a
-- fabricated request, and keeping it would keep a dangling reference to
-- fiction (DESIGN-V3 §0.7).
--
-- trajectory and bot_report are dropped rather than altered: their primary key
-- changes type (bigserial -> text) and every row in them is fiction. schema.sql
-- recreates both in the v3 shape immediately after this migration runs.
--
-- Runs exactly once per database, recorded in foundry.foundry_migration. On an
-- installation that never had v2 (dev, e2e, a fresh prod) this is a no-op: the
-- guard below returns before touching anything, and schema.sql provisions the
-- v3 end state directly.

DO $mig$
BEGIN
  IF to_regclass('foundry.program') IS NULL
     AND to_regclass('foundry.team') IS NULL THEN
    RAISE NOTICE '0002-foundry-v3: no v2 install found, nothing to migrate';
    RETURN;
  END IF;

  -- 1. the fiction-only surface. The view goes first: it reads trajectory.
  DROP VIEW IF EXISTS foundry.demand_trajectories;
  DROP TABLE IF EXISTS
    foundry.payout_run,
    foundry.revenue_entry,
    foundry.team_weekly,
    foundry.gate,
    foundry.clip,
    foundry.relay_run,
    foundry.swipe,
    foundry.variant,
    foundry.session,
    foundry.cohort,
    foundry.template,
    foundry.team,
    foundry.program
    CASCADE;

  -- 2. trajectory + bot_report: re-created by schema.sql in the v3 shape.
  DROP TABLE IF EXISTS foundry.bot_report CASCADE;
  DROP TABLE IF EXISTS foundry.trajectory CASCADE;

  -- 3. purge the seeded rows from the tables whose identity we keep.
  --    Order follows the foreign keys: pledge -> request, changelog -> scene,
  --    scene -> request.
  DELETE FROM foundry.pledge;
  DELETE FROM foundry.scene_changelog;
  DELETE FROM foundry.scene;
  DELETE FROM foundry.request;

  -- 4. request: the display count was seed_pledges + count(pledge). It is now
  --    count(pledge), so the seeded addend has nowhere left to hide.
  ALTER TABLE foundry.request DROP COLUMN IF EXISTS seed_pledges;
  ALTER TABLE foundry.request DROP CONSTRAINT IF EXISTS request_origin_check;
  ALTER TABLE foundry.request
    ADD CONSTRAINT request_origin_check CHECK (origin IN ('visitor','import'));
  ALTER TABLE foundry.request ALTER COLUMN origin SET DEFAULT 'visitor';

  -- 5. scene: a pipeline of invented stage timings becomes a registry of real
  --    Worlds deployments.
  ALTER TABLE foundry.scene
    DROP COLUMN IF EXISTS stage_minutes,
    DROP COLUMN IF EXISTS total_minutes,
    DROP COLUMN IF EXISTS first_session_at,
    DROP COLUMN IF EXISTS first_session_within_24h,
    DROP COLUMN IF EXISTS deploy_week,
    DROP COLUMN IF EXISTS idea_at,
    DROP COLUMN IF EXISTS brief_at,
    DROP COLUMN IF EXISTS template_at,
    DROP COLUMN IF EXISTS edits_at,
    DROP COLUMN IF EXISTS deployed_at,
    DROP COLUMN IF EXISTS team_id,
    DROP COLUMN IF EXISTS cell,
    DROP COLUMN IF EXISTS status,
    DROP COLUMN IF EXISTS memory_ab,
    DROP COLUMN IF EXISTS brief,
    DROP COLUMN IF EXISTS request_id,
    DROP COLUMN IF EXISTS template_id,
    DROP COLUMN IF EXISTS origin,
    DROP COLUMN IF EXISTS sid;

  IF EXISTS (
       SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'foundry' AND table_name = 'scene'
          AND column_name = 'name'
     )
     AND NOT EXISTS (
       SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'foundry' AND table_name = 'scene'
          AND column_name = 'title'
     ) THEN
    ALTER TABLE foundry.scene RENAME COLUMN name TO title;
  END IF;

  ALTER TABLE foundry.scene
    ADD COLUMN IF NOT EXISTS title text,
    ADD COLUMN IF NOT EXISTS world_name text,
    ADD COLUMN IF NOT EXISTS entity_id text,
    ADD COLUMN IF NOT EXISTS deployed_at timestamptz,
    ADD COLUMN IF NOT EXISTS size_bytes bigint,
    ADD COLUMN IF NOT EXISTS parcels int,
    ADD COLUMN IF NOT EXISTS repo_path text,
    ADD COLUMN IF NOT EXISTS bot_manifest text,
    ADD COLUMN IF NOT EXISTS source text NOT NULL DEFAULT 'repo',
    ADD COLUMN IF NOT EXISTS source_note text NOT NULL DEFAULT '',
    ADD COLUMN IF NOT EXISTS gdd_doc_id text;
  ALTER TABLE foundry.scene ALTER COLUMN title SET NOT NULL;

  IF NOT EXISTS (
    SELECT 1 FROM pg_constraint
     WHERE conname = 'scene_source_check' AND conrelid = 'foundry.scene'::regclass
  ) THEN
    ALTER TABLE foundry.scene
      ADD CONSTRAINT scene_source_check CHECK (source IN ('worlds-mirror','repo'));
  END IF;

  -- 6. scene_changelog: an ask/change pair invented per scene becomes one note
  --    per real deployment, carrying the mirror entity it was read from.
  ALTER TABLE foundry.scene_changelog
    DROP COLUMN IF EXISTS ask,
    DROP COLUMN IF EXISTS change,
    ADD COLUMN IF NOT EXISTS note text NOT NULL DEFAULT '',
    ADD COLUMN IF NOT EXISTS source_note text NOT NULL DEFAULT '';
  ALTER TABLE foundry.scene_changelog
    ALTER COLUMN "at" TYPE timestamptz USING "at"::timestamptz;
  ALTER TABLE foundry.scene_changelog DROP CONSTRAINT IF EXISTS scene_changelog_origin_check;
  ALTER TABLE foundry.scene_changelog
    ADD CONSTRAINT scene_changelog_origin_check CHECK (origin IN ('import','visitor'));
  ALTER TABLE foundry.scene_changelog ALTER COLUMN origin SET DEFAULT 'import';
END
$mig$;
