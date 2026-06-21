CREATE TABLE IF NOT EXISTS scene_admin (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    place_id    TEXT NOT NULL,
    admin       VARCHAR NOT NULL,
    added_by    VARCHAR NOT NULL,
    created_at  TIMESTAMP NOT NULL DEFAULT now(),
    active      BOOLEAN NOT NULL DEFAULT TRUE
);

CREATE INDEX IF NOT EXISTS idx_scene_admin_place_active
    ON scene_admin (place_id, active);

CREATE INDEX IF NOT EXISTS idx_scene_admin_admin
    ON scene_admin (admin);

CREATE TABLE IF NOT EXISTS scene_bans (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    place_id        TEXT NOT NULL,
    banned_address  VARCHAR NOT NULL,
    banned_by       VARCHAR NOT NULL,
    banned_at       TIMESTAMP NOT NULL DEFAULT now(),
    UNIQUE (place_id, banned_address)
);

CREATE INDEX IF NOT EXISTS idx_scene_bans_place
    ON scene_bans (place_id);

CREATE INDEX IF NOT EXISTS idx_scene_bans_address
    ON scene_bans (banned_address);

CREATE TABLE IF NOT EXISTS scene_stream_access (
    id                    UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    place_id              TEXT NOT NULL,
    streaming_url         TEXT NOT NULL,
    streaming_key         TEXT NOT NULL,
    ingress_id            TEXT,
    room_id               TEXT,
    generated_by          VARCHAR NOT NULL,
    created_at            TIMESTAMP NOT NULL DEFAULT now(),
    expiration_time       TIMESTAMP,
    streaming             BOOLEAN NOT NULL DEFAULT FALSE,
    streaming_start_time  TIMESTAMP,
    active                BOOLEAN NOT NULL DEFAULT TRUE
);

CREATE INDEX IF NOT EXISTS idx_scene_stream_access_place_active
    ON scene_stream_access (place_id, active);

CREATE TABLE IF NOT EXISTS voice_chat_users (
    address            VARCHAR NOT NULL,
    room_name          TEXT NOT NULL,
    status             VARCHAR NOT NULL,
    joined_at          TIMESTAMP NOT NULL DEFAULT now(),
    status_updated_at  TIMESTAMP NOT NULL DEFAULT now(),
    PRIMARY KEY (address, room_name)
);

CREATE INDEX IF NOT EXISTS idx_voice_chat_users_status_updated
    ON voice_chat_users (status, status_updated_at);

CREATE TABLE IF NOT EXISTS community_voice_chat_users (
    address            VARCHAR NOT NULL,
    room_name          TEXT NOT NULL,
    is_moderator       BOOLEAN NOT NULL DEFAULT FALSE,
    status             VARCHAR NOT NULL,
    joined_at          TIMESTAMP NOT NULL DEFAULT now(),
    status_updated_at  TIMESTAMP NOT NULL DEFAULT now(),
    created_at         TIMESTAMP NOT NULL DEFAULT now(),
    PRIMARY KEY (address, room_name)
);

CREATE INDEX IF NOT EXISTS idx_community_voice_chat_users_room
    ON community_voice_chat_users (room_name);

CREATE TABLE IF NOT EXISTS user_bans (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    banned_address  VARCHAR NOT NULL,
    banned_by       VARCHAR NOT NULL,
    reason          TEXT NOT NULL,
    custom_message  TEXT,
    banned_at       TIMESTAMP NOT NULL DEFAULT now(),
    expires_at      TIMESTAMP,
    active          BOOLEAN NOT NULL DEFAULT TRUE
);

CREATE INDEX IF NOT EXISTS idx_user_bans_banned_address
    ON user_bans (banned_address);

CREATE INDEX IF NOT EXISTS idx_user_bans_active
    ON user_bans (active, expires_at);

CREATE TABLE IF NOT EXISTS user_warnings (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    warned_address  VARCHAR NOT NULL,
    warned_by       VARCHAR NOT NULL,
    reason          TEXT NOT NULL,
    warned_at       TIMESTAMP NOT NULL DEFAULT now(),
    created_at      TIMESTAMP NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_user_warnings_warned_address
    ON user_warnings (warned_address);
