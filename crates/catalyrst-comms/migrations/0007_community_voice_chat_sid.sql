-- Track the LiveKit participant session id (sid) for each community voice chat user.
-- This lets us tell apart a stale "participant_left" webhook coming from a previous
-- session and the user's current session after an immediate leave + rejoin, so we
-- don't disconnect them (or tear down the room) while they are reconnecting.
ALTER TABLE community_voice_chat_users
    ADD COLUMN IF NOT EXISTS sid TEXT;
