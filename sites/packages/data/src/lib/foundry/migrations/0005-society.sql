-- 0005-society — the persistent-society layer: personas, one permission ledger,
-- ritual (sessions), stewardship (consent + appeals), and scene continuity
-- (transfer + attribution). Every table is NEW; nothing backfills an existing
-- row. Idempotent and guarded in the 0003/0004 style (to_regclass gate +
-- CREATE ... IF NOT EXISTS everywhere), so re-applying is a no-op. Mirrored
-- verbatim as end-state into schema.sql (the lockstep rule). Applied out of band
-- only: `cd catalyrst/sites && FOUNDRY_DATABASE_URL=... npm run foundry:import-real -- --migrate-only`.
--
-- Consolidation notes:
--  * ONE permission ledger (role_grant: admin/host/create/start) reached by ONE
--    invite ledger (role_invite). The originally-separate title_grant/title_invite
--    are dropped — "titles" are roles; the roster is a view over role_grant.
--  * Invite codes are minted in Node (crypto.randomBytes) and passed in, so this
--    migration needs neither pgcrypto nor gen_random_bytes. gen_random_uuid() is a
--    pg18 builtin already relied on by schema.sql.
--  * scene has NO owner column (0002 dropped sid, never added deployer) — ownership
--    lives entirely in the new scene_steward/scene_transfer tables.

DO $mig$
BEGIN
  IF to_regclass('foundry.scene') IS NULL OR to_regclass('foundry.request') IS NULL THEN
    RAISE NOTICE '0005-society: base foundry tables absent, nothing to do';
    RETURN;
  END IF;

  -- Persona: one per durable sid, editable only by the cookie-holder. The avatar
  -- is a spec over the real DCL base-avatar catalog (BODY_SHAPE_URNS + palette
  -- indices), re-rendered live — no stored image can rot into fiction.
  CREATE TABLE IF NOT EXISTS foundry.persona (
    sid             text PRIMARY KEY,
    display_name    text NOT NULL,
    avatar_body_urn text,
    avatar          jsonb NOT NULL DEFAULT '{}'::jsonb,
    claimed_at      timestamptz NOT NULL DEFAULT now(),
    updated_at      timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT persona_name_len     CHECK (char_length(display_name) BETWEEN 2 AND 32),
    CONSTRAINT persona_name_trim    CHECK (display_name = btrim(display_name)),
    CONSTRAINT persona_name_charset CHECK (display_name ~ '^[[:alnum:] ._-]+$'),
    CONSTRAINT persona_name_not_reserved
      CHECK (lower(display_name) NOT IN
             ('admin','anonymous','visitor','operator','foundry','system','host'))
  );
  CREATE UNIQUE INDEX IF NOT EXISTS persona_display_name_ci
    ON foundry.persona (lower(display_name));

  -- Role invites: one-time codes redeemed into a role_grant. `code` is minted in
  -- Node and supplied by the app (no SQL default → no pgcrypto dependency).
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
  -- fact, never a delete. Privileged roles can never be self-granted (operator
  -- NULL grants allowed); start/create self-grants at /select are honest records
  -- of a door choice and gate nothing.
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
  -- 'steward-code' is load-bearing: withdrawing it pauses host powers immediately.
  -- 'roster-listing' gates whether a holder's badge appears on the public roster.
  CREATE TABLE IF NOT EXISTS foundry.consent_event (
    id    bigserial PRIMARY KEY,
    sid   text NOT NULL,
    topic text NOT NULL CHECK (topic IN ('steward-code','roster-listing')),
    state text NOT NULL CHECK (state IN ('granted','withdrawn')),
    at    timestamptz NOT NULL DEFAULT now()
  );
  CREATE INDEX IF NOT EXISTS consent_event_sid_topic_idx
    ON foundry.consent_event (sid, topic, at DESC);

  -- A scheduled gathering. weekly occurrences are derived arithmetically from
  -- first_at at read time (capped horizon) and never materialized — a stored
  -- occurrence that never happened would be fiction. Retirement is recorded.
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

  -- The pledge pattern, keyed to a specific derived occurrence. occurrence_at is
  -- validated in the app against the schedule derivation before an RSVP is
  -- accepted; the shown count is count(*) over this table and nothing else.
  CREATE TABLE IF NOT EXISTS foundry.session_rsvp (
    series_id     text NOT NULL REFERENCES foundry.session_series(id),
    occurrence_at timestamptz NOT NULL,
    sid           text NOT NULL,
    created_at    timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (series_id, occurrence_at, sid)
  );

  -- A recorded handoff of a steward seat. Only sha256(code) is stored: the raw
  -- code is shown once at mint and is then genuinely unrecoverable. 'expired' is
  -- a derived reading (status='offered' AND expires_at < now()), not a rewrite.
  -- Created BEFORE scene_steward because scene_steward.via_transfer_id FKs it.
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

  -- A recorded, unverified stewardship CLAIM by a session — the worlds mirror
  -- carries no deployer, so verification is impossible and never implied. Rows
  -- are never deleted; release closes them (append-only history). basis is the
  -- claimant's own words. Multiple active stewards = a team.
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

  -- A visitor contests a decision that exists as a row; a host/admin answers with
  -- a required note. subject is polymorphic (no FK): the filing UI derives its
  -- subject list from real decisions touching the appellant, so no appeal can
  -- reference an invented decision. One open appeal per (sid, subject).
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

  -- action_log gets its first-ever reader (the community timeline + per-scene
  -- memory). Without this the flagship page sequential-scans on its sort key.
  CREATE INDEX IF NOT EXISTS action_log_at_idx ON foundry.action_log (at DESC);
END
$mig$;
