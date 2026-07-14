-- Foundry v3 end-state schema. Idempotent: safe to re-apply.
--
-- Fresh installs (dev, e2e) provision from this file directly. The live
-- database walks migrations/*.sql to get here from the v2 shape; every table
-- below is created IF NOT EXISTS, so applying this file after the migrations is
-- a no-op on an already-migrated database and a full install on an empty one.
--
-- There is no seed, sim or projection anywhere in this schema. Rows arrive from
-- four places only: the worlds mirror import (scene, scene_changelog), harness
-- executions (trajectory, trajectory_event, bot_report), the copilot gateway's
-- own usage accounting (llm_usage), and visitors (request, pledge, action_log).

CREATE SCHEMA IF NOT EXISTS foundry;

CREATE TABLE IF NOT EXISTS foundry.foundry_migration (
  name       text PRIMARY KEY,
  applied_at timestamptz NOT NULL DEFAULT now()
);

-- Visitor asks. `source` is free text the asker supplies about themselves.
CREATE TABLE IF NOT EXISTS foundry.request (
  id text PRIMARY KEY DEFAULT gen_random_uuid()::text,
  title text NOT NULL,
  body text NOT NULL,
  source text NOT NULL,
  status text NOT NULL DEFAULT 'open' CHECK (status IN ('open','approved','closed')),
  origin text NOT NULL DEFAULT 'visitor' CHECK (origin IN ('visitor','import')),
  sid text,
  created_at timestamptz NOT NULL DEFAULT now()
);

-- One row per (request, session). The displayed pledge count is count(*) over
-- this table and nothing else.
CREATE TABLE IF NOT EXISTS foundry.pledge (
  request_id text NOT NULL REFERENCES foundry.request(id),
  sid        text NOT NULL,
  created_at timestamptz NOT NULL DEFAULT now(),
  PRIMARY KEY (request_id, sid)
);

-- The registry. `id` is the slug; `title`, `deployed_at`, `size_bytes` and
-- `parcels` are read from the deployment entity in our worlds mirror, never
-- authored here. `deployer` is empty in the mirror rows, so no deployer column
-- exists: a column nothing can honestly fill is a column that invites fiction.
CREATE TABLE IF NOT EXISTS foundry.scene (
  id text PRIMARY KEY DEFAULT gen_random_uuid()::text,
  title text NOT NULL,
  world_name text,
  entity_id text,
  deployed_at timestamptz,
  size_bytes bigint,
  parcels int,
  repo_path text,
  bot_manifest text,
  source text NOT NULL DEFAULT 'repo' CHECK (source IN ('worlds-mirror','repo')),
  source_note text NOT NULL DEFAULT '',
  gdd_doc_id text,
  created_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS foundry.scene_changelog (
  id bigserial PRIMARY KEY,
  scene_id text NOT NULL REFERENCES foundry.scene(id),
  at timestamptz NOT NULL,
  note text NOT NULL DEFAULT '',
  source_note text NOT NULL DEFAULT '',
  origin text NOT NULL DEFAULT 'import' CHECK (origin IN ('import','visitor')),
  sid text
);
CREATE INDEX IF NOT EXISTS scene_changelog_scene_idx
  ON foundry.scene_changelog (scene_id, at DESC);

-- Design documents. The honesty markers and the hypothesis filename state
-- machine are parsed out of the document body at import time and stored
-- structurally, so the surface reports coverage without re-reading prose.
CREATE TABLE IF NOT EXISTS foundry.gdd_doc (
  id text PRIMARY KEY,
  title text NOT NULL,
  kind text NOT NULL CHECK (kind IN ('shortgdd','proposal','brief','feature-design')),
  scene_id text REFERENCES foundry.scene(id),
  version int NOT NULL DEFAULT 1,
  supersedes text REFERENCES foundry.gdd_doc(id),
  source text NOT NULL CHECK (source IN ('slack-import','copilot')),
  source_ref text,
  body_md text NOT NULL,
  honesty jsonb NOT NULL DEFAULT '{}'::jsonb,
  hypotheses jsonb NOT NULL DEFAULT '[]'::jsonb,
  created_at timestamptz NOT NULL,
  updated_at timestamptz NOT NULL DEFAULT now()
);

-- scene.gdd_doc_id closes the cycle with gdd_doc.scene_id, so it is added after
-- both tables exist. NOT VALID: nothing is being back-checked, the constraint
-- only governs writes from here on.
DO $$
BEGIN
  IF NOT EXISTS (
    SELECT 1 FROM pg_constraint
     WHERE conname = 'scene_gdd_fk' AND conrelid = 'foundry.scene'::regclass
  ) THEN
    ALTER TABLE foundry.scene
      ADD CONSTRAINT scene_gdd_fk
      FOREIGN KEY (gdd_doc_id) REFERENCES foundry.gdd_doc(id) NOT VALID;
  END IF;
END
$$;

-- One row per assistant message, keyed by the gateway's own message id so
-- re-ingesting the same session is an upsert rather than a duplicate. Token
-- counts are verbatim; the two price columns record which constant was in force
-- when the row landed, so a later price change cannot silently rewrite history.
CREATE TABLE IF NOT EXISTS foundry.llm_usage (
  message_id text PRIMARY KEY,
  session_id text NOT NULL,
  session_title text,
  model text NOT NULL,
  input_tokens int NOT NULL,
  output_tokens int NOT NULL,
  reasoning_tokens int NOT NULL DEFAULT 0,
  cache_read_tokens int NOT NULL DEFAULT 0,
  cache_write_tokens int NOT NULL DEFAULT 0,
  cost_usd numeric(12,7),
  price_input_per_m numeric(8,4) NOT NULL,
  price_output_per_m numeric(8,4) NOT NULL,
  at timestamptz NOT NULL,
  ingested_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS llm_usage_at ON foundry.llm_usage (at);

-- An episode header. Everything a replay shows is re-derived from
-- trajectory_event; nothing about the run is summarised here that the event log
-- does not already contain. finish_reason is a cache of the last turn/end.
CREATE TABLE IF NOT EXISTS foundry.trajectory (
  id text PRIMARY KEY DEFAULT gen_random_uuid()::text,
  scene_id text REFERENCES foundry.scene(id),
  provenance text NOT NULL CHECK (provenance IN ('bot','visitor')),
  runner text CHECK (runner IS NULL OR runner IN ('dclbots','arena')),
  finish_reason jsonb,
  parent_trajectory_id text REFERENCES foundry.trajectory(id),
  seed_length int,
  evidence_path text,
  created_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS trajectory_created_idx
  ON foundry.trajectory (created_at DESC);

-- Row shape adapted from deepseek-ai/deepseek-harness (MIT) session
-- persistence: append-only events with a contiguous per-episode `seq`, turn and
-- step brackets, verbatim tool calls, and fork-at-seq lineage on the header
-- above. Contiguity is the contract that makes replay a re-derivation rather
-- than a reconstruction, so it is enforced on append, not assumed.
CREATE TABLE IF NOT EXISTS foundry.trajectory_event (
  trajectory_id text NOT NULL REFERENCES foundry.trajectory(id) ON DELETE CASCADE,
  seq int NOT NULL,
  type text NOT NULL,
  time timestamptz NOT NULL,
  data jsonb NOT NULL,
  ignorable boolean,
  PRIMARY KEY (trajectory_id, seq)
);

-- A run of the dcl-scene-bots harness. verdict is NULL when the runner's stdout
-- was not captured: a snapshot without a recorded verdict says so rather than
-- guessing one.
CREATE TABLE IF NOT EXISTS foundry.bot_report (
  id text PRIMARY KEY DEFAULT gen_random_uuid()::text,
  scene_id text REFERENCES foundry.scene(id),
  slug text NOT NULL,
  -- 'arena' = a sandbox simulation, not a run against the deployed World.
  runner text CHECK (runner IS NULL OR runner IN ('dclbots','arena')),
  -- The realm the run targeted; loopback for a local scene copy.
  realm text,
  ran_at timestamptz NOT NULL DEFAULT now(),
  verdict text CHECK (verdict IS NULL OR verdict IN ('pass','fail')),
  checks_total int,
  checks_failed int,
  missing_tools jsonb NOT NULL DEFAULT '[]'::jsonb,
  stubbed_tools jsonb NOT NULL DEFAULT '[]'::jsonb,
  network_writes int,
  shots jsonb NOT NULL DEFAULT '[]'::jsonb,
  evidence_path text,
  trajectory_id text REFERENCES foundry.trajectory(id)
);
CREATE INDEX IF NOT EXISTS bot_report_ran_idx ON foundry.bot_report (ran_at DESC);
-- The per-scene reads (countBenchReports, listBenchReports, the games_passing
-- subquery) all filter/group by scene_id; without this they sequential-scan.
CREATE INDEX IF NOT EXISTS bot_report_scene_idx ON foundry.bot_report (scene_id);

-- Append-only record of every server mutation.
CREATE TABLE IF NOT EXISTS foundry.action_log (
  id bigserial PRIMARY KEY,
  at timestamptz NOT NULL DEFAULT now(),
  sid text NOT NULL,
  action text NOT NULL,
  subject text NOT NULL,
  detail jsonb NOT NULL DEFAULT '{}'::jsonb
);
