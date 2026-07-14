-- Foundry v3 end-state schema. Idempotent: safe to re-apply.
--
-- Fresh installs (dev, e2e) provision from this file directly. The live
-- database walks migrations/*.sql to get here from the v2 shape; every table
-- below is created IF NOT EXISTS, so applying this file after the migrations is
-- a no-op on an already-migrated database and a full install on an empty one.
--
-- There is no seed, sim or projection anywhere in this schema. Rows arrive from
-- five places only: the worlds mirror import (scene, scene_changelog), harness
-- executions (trajectory, trajectory_event, bot_report), the copilot gateway's
-- own usage accounting (llm_usage), visitors (request, pledge, action_log), and
-- the ask importer (request rows with origin='import', verbatim from a public
-- permalink).

CREATE SCHEMA IF NOT EXISTS foundry;

CREATE TABLE IF NOT EXISTS foundry.foundry_migration (
  name       text PRIMARY KEY,
  applied_at timestamptz NOT NULL DEFAULT now()
);

-- Visitor asks. `source` is free text the asker supplies about themselves.
-- For origin='import' rows, `source` is the original author's public handle,
-- `source_url` the public permalink, `sourced_at` the original post date, and
-- `sid` NULL — an import has no session. source_url/sourced_at stay NULL on
-- every visitor row: columns only an import can honestly fill.
CREATE TABLE IF NOT EXISTS foundry.request (
  id text PRIMARY KEY DEFAULT gen_random_uuid()::text,
  title text NOT NULL,
  body text NOT NULL,
  source text NOT NULL,
  status text NOT NULL DEFAULT 'open' CHECK (status IN ('open','approved','closed')),
  origin text NOT NULL DEFAULT 'visitor' CHECK (origin IN ('visitor','import')),
  sid text,
  created_at timestamptz NOT NULL DEFAULT now(),
  source_url text,
  sourced_at timestamptz,
  -- Whether the original public post recorded only a DAY: set by the importer,
  -- read by the timeline so a date-only ask never grows an invented midnight.
  sourced_date_only boolean NOT NULL DEFAULT false,
  -- When the ask's own author last revised its wording (visitor rows only —
  -- an imported ask stays verbatim). Prior wording lives in action_log.
  edited_at timestamptz
);

-- The author handles of imported asks, reserved as persona names at import time
-- so a visitor cannot claim a quoted author's handle and stand next to their
-- words. released_at is the operator's unblock for a genuine returning author;
-- a released row stays released across re-imports.
CREATE TABLE IF NOT EXISTS foundry.reserved_handle (
  handle            text PRIMARY KEY,
  source_request_id text REFERENCES foundry.request(id),
  reserved_at       timestamptz NOT NULL DEFAULT now(),
  released_at       timestamptz,
  release_note      text
);
CREATE UNIQUE INDEX IF NOT EXISTS reserved_handle_ci
  ON foundry.reserved_handle (lower(handle));

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
  created_at timestamptz NOT NULL DEFAULT now(),
  -- The deployment entity's own scene.json display facts, read from the worlds
  -- content server by foundry:import-real; NULL until an import has read them.
  description text,
  thumbnail_url text
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

-- This program's own reading of each game against the three gaming cells of
-- the strategy deck's slide "09 | MARKET-CELL PORTFOLIO" (11.10, Scott McCarthy
-- working-strategy deck). A row is a curated judgment this program made on
-- classified_at — never a fact the deployment entity carries. cell NULL with a
-- rationale means "examined and honestly unclassifiable" (the deck's slide 12
-- makes cell fit a gate a concept can fail); a scene with no row has not been
-- read at all.
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

-- This program's own reading of each game's observable design against the six
-- emotional jobs of the strategy deck's slide "10 | EMOTIONAL WHITE SPACE"
-- (11.10, Scott McCarthy working-strategy deck). One row per job the game's
-- design serves, each a curated judgment this program made on read_at — never
-- a fact the deployment entity carries. A row with a NULL job means "read, and
-- honestly serves none of the six"; a scene with no rows at all has not been
-- read.
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

-- This program's own reading of each imported ask against the deck's three
-- gaming cells and six emotional jobs (11.10, Scott McCarthy working-strategy
-- deck, slides 09-10). A row is a curated judgment made on read_at, never a
-- fact the ask carries. cell NULL = read, fits no single cell; shelf_answer
-- NULL = no game on the shelf answers it; no row = not read. crowd_range is
-- the deck's own per-cell range string from slide "09 | MARKET-CELL
-- PORTFOLIO", verbatim minus the trailing sentence period; NULL when the
-- reading fits no cell.
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
  basis        text NOT NULL,
  crowd_range  text
);

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
  source text NOT NULL CHECK (source IN ('slack-import','copilot','program','session')),
  source_ref text,
  body_md text NOT NULL,
  honesty jsonb NOT NULL DEFAULT '{}'::jsonb,
  hypotheses jsonb NOT NULL DEFAULT '[]'::jsonb,
  grounds_cell text,
  grounding_request_ids jsonb NOT NULL DEFAULT '[]'::jsonb,
  created_at timestamptz NOT NULL,
  updated_at timestamptz NOT NULL DEFAULT now()
);

-- This program's own reading that a design doc and a deployed game share a
-- concept. A row is a dated adjacency judgment — never a claim the game
-- implements the doc, which only scene.gdd_doc_id / gdd_doc.scene_id may
-- carry. relation is fixed to 'same-concept' so the table cannot quietly grow
-- stronger claims.
-- A person's signature on a specific doc version. Versions are immutable (an
-- edit mints v(n+1)), so a signature can never be hollowed out by later edits.
-- The signer is a claimed persona (a signature needs a name); append-only,
-- one per (doc, signer). No row = "no person has approved this version".
CREATE TABLE IF NOT EXISTS foundry.gdd_approval (
  id     bigserial PRIMARY KEY,
  doc_id text NOT NULL REFERENCES foundry.gdd_doc(id),
  sid    text NOT NULL,
  at     timestamptz NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX IF NOT EXISTS gdd_approval_once
  ON foundry.gdd_approval (doc_id, sid);

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

-- 0005-society end-state (kept in lockstep with migrations/0005-society.sql).
-- The persistent-society layer: personas, one permission ledger, ritual
-- (sessions), stewardship (consent + appeals), and scene continuity (transfer +
-- attribution). Nothing here backfills an existing row; every table is new.

-- One persona per durable sid, editable only by the cookie-holder. The avatar is
-- a spec over the real DCL base-avatar catalog, re-rendered live.
CREATE TABLE IF NOT EXISTS foundry.persona (
  sid             text PRIMARY KEY,
  display_name    text NOT NULL,
  avatar_body_urn text,
  avatar          jsonb NOT NULL DEFAULT '{}'::jsonb,
  -- Optional self-description, in the visitor's own words.
  words           text,
  claimed_at      timestamptz NOT NULL DEFAULT now(),
  updated_at      timestamptz NOT NULL DEFAULT now(),
  CONSTRAINT persona_words_len
    CHECK (words IS NULL OR char_length(words) BETWEEN 1 AND 280),
  CONSTRAINT persona_name_len     CHECK (char_length(display_name) BETWEEN 2 AND 32),
  CONSTRAINT persona_name_trim    CHECK (display_name = btrim(display_name)),
  CONSTRAINT persona_name_charset CHECK (display_name ~ '^[[:alnum:] ._-]+$'),
  CONSTRAINT persona_name_not_reserved
    CHECK (lower(display_name) NOT IN
           ('admin','anonymous','visitor','operator','foundry','system','host'))
);
CREATE UNIQUE INDEX IF NOT EXISTS persona_display_name_ci
  ON foundry.persona (lower(display_name));

-- A carry code moves a persona to another browser: a one-time bearer secret
-- whose redemption re-issues the persona's own sid cookie there, so grants,
-- pledges and acts follow automatically. Plaintext is shown once at mint and
-- never stored — only its sha256. Append-only: a newer mint supersedes the
-- last (revoked_at), redemption records the abandoned fresh sid.
CREATE TABLE IF NOT EXISTS foundry.persona_carry_code (
  id            bigserial PRIMARY KEY,
  sid           text NOT NULL REFERENCES foundry.persona(sid),
  code_hash     text NOT NULL UNIQUE,
  created_at    timestamptz NOT NULL DEFAULT now(),
  revoked_at    timestamptz,
  redeemed_at   timestamptz,
  redeemed_from text,
  CONSTRAINT carry_code_redeem_pair
    CHECK ((redeemed_at IS NULL) = (redeemed_from IS NULL))
);
CREATE UNIQUE INDEX IF NOT EXISTS persona_carry_code_active_one
  ON foundry.persona_carry_code (sid)
  WHERE revoked_at IS NULL AND redeemed_at IS NULL;

-- 0022-sid-alias end-state (lockstep with migrations/0022-sid-alias.sql).
-- A persona outlives any one browser session. sid_alias maps an abandoned
-- session sid onto the persona's canonical sid so roles, consent, and act
-- attribution survive cookie loss. Append-only rows are never rewritten;
-- reads resolve through this table instead.
CREATE TABLE IF NOT EXISTS foundry.sid_alias (
  alias_sid   text PRIMARY KEY,
  persona_sid text NOT NULL REFERENCES foundry.persona(sid),
  via         text NOT NULL CHECK (via IN ('return-code', 'operator-rebind')),
  linked_at   timestamptz NOT NULL DEFAULT now(),
  CHECK (alias_sid <> persona_sid)
);

CREATE INDEX IF NOT EXISTS sid_alias_persona ON foundry.sid_alias (persona_sid);

-- One-time codes redeemed into a role_grant. `code` is minted in Node.
CREATE TABLE IF NOT EXISTS foundry.role_invite (
  code            text PRIMARY KEY,
  role            text NOT NULL CHECK (role IN ('admin','host','create','start')),
  note            text NOT NULL DEFAULT '',
  created_via     text NOT NULL CHECK (created_via IN ('operator','host')),
  created_by_sid  text,
  created_at      timestamptz NOT NULL DEFAULT now(),
  expires_at      timestamptz,
  redeemed_by_sid text,
  redeemed_at     timestamptz,
  CONSTRAINT role_invite_redeem_pair
    CHECK ((redeemed_by_sid IS NULL) = (redeemed_at IS NULL)),
  CONSTRAINT role_invite_operator_null
    CHECK ((created_via = 'operator') = (created_by_sid IS NULL))
);

-- The single permission ledger. Append-only: revocation is a recorded second
-- fact, never a delete.
CREATE TABLE IF NOT EXISTS foundry.role_grant (
  id             bigserial PRIMARY KEY,
  sid            text NOT NULL,
  role           text NOT NULL CHECK (role IN ('admin','host','create','start')),
  granted_by_sid text,
  invite_code    text REFERENCES foundry.role_invite(code),
  note           text NOT NULL DEFAULT '',
  created_at     timestamptz NOT NULL DEFAULT now(),
  revoked_at     timestamptz,
  revoked_by_sid text,
  revoke_note    text,
  CONSTRAINT role_grant_privileged_provenance
    CHECK (role NOT IN ('admin','host') OR granted_by_sid IS DISTINCT FROM sid),
  CONSTRAINT role_grant_revoke_pair
    CHECK ((revoked_at IS NULL) = (revoked_by_sid IS NULL))
);
CREATE UNIQUE INDEX IF NOT EXISTS role_grant_active_one
  ON foundry.role_grant (sid, role) WHERE revoked_at IS NULL;
CREATE INDEX IF NOT EXISTS role_grant_active_sid
  ON foundry.role_grant (sid) WHERE revoked_at IS NULL;

-- Append-only consent ledger. Current state = latest row per (sid, topic).
CREATE TABLE IF NOT EXISTS foundry.consent_event (
  id    bigserial PRIMARY KEY,
  sid   text NOT NULL,
  topic text NOT NULL CHECK (topic IN ('steward-code','roster-listing')),
  state text NOT NULL CHECK (state IN ('granted','withdrawn')),
  at    timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS consent_event_sid_topic_idx
  ON foundry.consent_event (sid, topic, at DESC);

-- A scheduled gathering. Weekly occurrences are derived arithmetically at read
-- time (capped horizon) and never materialized.
CREATE TABLE IF NOT EXISTS foundry.session_series (
  id               text PRIMARY KEY DEFAULT gen_random_uuid()::text,
  title            text NOT NULL,
  body             text NOT NULL DEFAULT '',
  scene_id         text REFERENCES foundry.scene(id),
  cadence          text NOT NULL CHECK (cadence IN ('once','weekly')),
  first_at         timestamptz NOT NULL,
  duration_minutes int NOT NULL DEFAULT 60 CHECK (duration_minutes BETWEEN 15 AND 480),
  created_by_sid   text NOT NULL,
  created_at       timestamptz NOT NULL DEFAULT now(),
  retired_at       timestamptz,
  retired_by_sid   text,
  CONSTRAINT session_series_retire_pair
    CHECK ((retired_at IS NULL) = (retired_by_sid IS NULL))
);
CREATE INDEX IF NOT EXISTS session_series_first_idx
  ON foundry.session_series (first_at) WHERE retired_at IS NULL;

-- The pledge pattern, keyed to a specific derived occurrence.
CREATE TABLE IF NOT EXISTS foundry.session_rsvp (
  series_id     text NOT NULL REFERENCES foundry.session_series(id),
  occurrence_at timestamptz NOT NULL,
  sid           text NOT NULL,
  created_at    timestamptz NOT NULL DEFAULT now(),
  PRIMARY KEY (series_id, occurrence_at, sid)
);

-- A recorded handoff of a steward seat. Only sha256(code) is stored. Created
-- BEFORE scene_steward because scene_steward.via_transfer_id FKs it.
CREATE TABLE IF NOT EXISTS foundry.scene_transfer (
  id           text PRIMARY KEY DEFAULT gen_random_uuid()::text,
  scene_id     text NOT NULL REFERENCES foundry.scene(id),
  from_sid     text NOT NULL,
  token_hash   text NOT NULL UNIQUE,
  note         text NOT NULL DEFAULT '',
  status       text NOT NULL DEFAULT 'offered' CHECK (status IN ('offered','accepted','revoked')),
  created_at   timestamptz NOT NULL DEFAULT now(),
  expires_at   timestamptz NOT NULL,
  accepted_at  timestamptz,
  accepted_sid text,
  CONSTRAINT scene_transfer_accept_pair
    CHECK ((status = 'accepted') = (accepted_sid IS NOT NULL AND accepted_at IS NOT NULL))
);
CREATE INDEX IF NOT EXISTS scene_transfer_scene_idx
  ON foundry.scene_transfer (scene_id, created_at DESC);

-- A recorded, unverified stewardship CLAIM. Rows are never deleted; release
-- closes them (append-only history). Multiple active stewards = a team.
CREATE TABLE IF NOT EXISTS foundry.scene_steward (
  id              bigserial PRIMARY KEY,
  scene_id        text NOT NULL REFERENCES foundry.scene(id),
  sid             text NOT NULL,
  basis           text NOT NULL DEFAULT '',
  via_transfer_id text REFERENCES foundry.scene_transfer(id),
  since           timestamptz NOT NULL DEFAULT now(),
  released_at     timestamptz,
  release_reason  text CHECK (release_reason IS NULL OR release_reason IN ('self','transfer')),
  CONSTRAINT scene_steward_release_pair
    CHECK ((released_at IS NULL) = (release_reason IS NULL))
);
CREATE UNIQUE INDEX IF NOT EXISTS scene_steward_active_uq
  ON foundry.scene_steward (scene_id, sid) WHERE released_at IS NULL;
CREATE INDEX IF NOT EXISTS scene_steward_scene_idx
  ON foundry.scene_steward (scene_id, since DESC);

-- A visitor contests a decision that exists as a row. subject is polymorphic
-- (no FK): the filing UI derives its subject list from real decisions.
CREATE TABLE IF NOT EXISTS foundry.appeal (
  id              text PRIMARY KEY DEFAULT gen_random_uuid()::text,
  sid             text NOT NULL,
  subject_kind    text NOT NULL CHECK (subject_kind IN ('request','role_grant','session_series')),
  subject_id      text NOT NULL,
  body            text NOT NULL,
  status          text NOT NULL DEFAULT 'open' CHECK (status IN ('open','withdrawn','upheld','declined')),
  created_at      timestamptz NOT NULL DEFAULT now(),
  resolved_by_sid text,
  resolved_at     timestamptz,
  resolution_note text,
  CONSTRAINT appeal_resolved_pair
    CHECK ((status IN ('upheld','declined')) = (resolved_at IS NOT NULL)),
  CONSTRAINT appeal_resolution_note
    CHECK (status NOT IN ('upheld','declined') OR resolution_note IS NOT NULL)
);
CREATE UNIQUE INDEX IF NOT EXISTS appeal_open_uniq
  ON foundry.appeal (sid, subject_kind, subject_id) WHERE status = 'open';
CREATE INDEX IF NOT EXISTS appeal_status_idx
  ON foundry.appeal (status, created_at DESC);
CREATE INDEX IF NOT EXISTS appeal_sid_idx ON foundry.appeal (sid);

-- action_log's first reader (the community timeline + per-scene memory) sorts on
-- `at`; without this index it sequential-scans.
CREATE INDEX IF NOT EXISTS action_log_at_idx ON foundry.action_log (at DESC);

-- One row per sid: when they last opened the timeline. The "since your last
-- visit" line needs a real visit marker — action_log records acts, not reads.
CREATE TABLE IF NOT EXISTS foundry.timeline_visit (
  sid text PRIMARY KEY,
  at  timestamptz NOT NULL
);
