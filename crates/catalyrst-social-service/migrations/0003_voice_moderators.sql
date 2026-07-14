-- Track the moderator participant count for active community voice chats so the
-- GET /v1/community-voice-chats/active read can report `moderatorCount` to match
-- the upstream comms-gatekeeper shape. Upstream sources this live from the
-- gatekeeper RPC; we mirror the minimum subset locally (1 indexed SQL read).
ALTER TABLE community_voice_chats
    ADD COLUMN IF NOT EXISTS moderators INTEGER NOT NULL DEFAULT 0;
