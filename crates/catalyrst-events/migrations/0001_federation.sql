-- catalyrst-events federation tables.
--
-- The read path serves the archive-owned `event` table (places_events DB,
-- read-only for this service). Federation-signed moderator actions land in the
-- local overlay tables this migration owns:
--
--   * moderators                — the local moderator allow-list. A wallet in
--     this table may sign profile-settings + schedule actions. Mirrors the
--     upstream `profile_settings.permissions` gate (EditAnyProfile /
--     EditAnySchedule), collapsed to a single moderator capability.
--   * event_profile_settings    — per-user notification preferences + the
--     moderator permission set (upstream ProfileSettings shape).
--   * schedules_local           — federation-owned schedule lifecycle.
--   * signed_actions_events     — append-only dedup log keyed by signature_hash.
--
-- seen_nonces is the shared per-signer replay store (00-primitives.md §2.2),
-- co-owned with the places crate over the same places_events DB — CREATE IF NOT
-- EXISTS keeps both idempotent.

CREATE TABLE IF NOT EXISTS moderators (
    address     text PRIMARY KEY,
    added_by    text,
    added_at    bigint NOT NULL
);

CREATE TABLE IF NOT EXISTS event_profile_settings (
    "user"           text PRIMARY KEY,
    email            text,
    email_verified   boolean NOT NULL DEFAULT false,
    use_local_time   boolean NOT NULL DEFAULT true,
    notify_by_email  boolean NOT NULL DEFAULT false,
    notify_by_browser boolean NOT NULL DEFAULT false,
    permissions      jsonb NOT NULL DEFAULT '[]'::jsonb,
    created_at       timestamptz NOT NULL DEFAULT now(),
    updated_at       timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS schedules_local (
    id            text PRIMARY KEY,
    name          text NOT NULL,
    description   text,
    image         text,
    theme         text,
    background    jsonb NOT NULL DEFAULT '[]'::jsonb,
    active_since  timestamptz,
    active_until  timestamptz,
    active        boolean NOT NULL DEFAULT true,
    created_at    timestamptz NOT NULL DEFAULT now(),
    updated_at    timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS signed_actions_events (
    signature_hash  text PRIMARY KEY,
    signer          text NOT NULL,
    action_type     text NOT NULL,
    message_payload jsonb NOT NULL,
    signed_at       bigint NOT NULL,
    received_at     bigint NOT NULL,
    origin_peer     text,
    seq             bigserial UNIQUE NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_sae_signer ON signed_actions_events (signer, action_type, signed_at DESC);
CREATE INDEX IF NOT EXISTS idx_sae_seq ON signed_actions_events (seq);

CREATE TABLE IF NOT EXISTS seen_nonces (
    signer     text NOT NULL,
    nonce      text NOT NULL,
    expires_at bigint NOT NULL,
    PRIMARY KEY (signer, nonce)
);

CREATE INDEX IF NOT EXISTS idx_seen_nonces_expires ON seen_nonces (expires_at);
