-- Ported from decentraland/social-service-ea migration
--   1749835946066_expire-private-voice-chats
--
-- Upstream's FINAL private_voice_chats schema DROPPED expires_at and updated_at
-- and relies purely on the unique caller/callee constraints plus a created_at-
-- based sweep (voiceDb.expirePrivateVoiceChat) to keep the table holding only
-- "live" rows. The earlier catalyrst port kept expires_at and applied the TTL
-- in-process; this migration ALIGNS the schema to upstream exactly.
--
-- The in-process busy filter and the admin "active calls" read now derive the
-- liveness window from created_at + the configured PRIVATE_VOICE_CHAT_EXPIRATION
-- _TIME (the same value the sweep uses), so a stale past-TTL row that has not yet
-- been reclaimed by the background sweep still does NOT falsely block a fresh
-- call with a ConflictingError.

ALTER TABLE private_voice_chats DROP COLUMN IF EXISTS expires_at;
ALTER TABLE private_voice_chats DROP COLUMN IF EXISTS updated_at;

-- The expires_at CHECK constraint went away with the column on most engines, but
-- drop it explicitly in case the column drop left it behind (older PG, or a
-- standalone constraint name).
DO $$ BEGIN
    ALTER TABLE private_voice_chats DROP CONSTRAINT private_voice_chats_expires_at_check;
EXCEPTION WHEN undefined_object THEN NULL;
END $$;

CREATE INDEX IF NOT EXISTS private_voice_chats_created_at
    ON private_voice_chats (created_at);
