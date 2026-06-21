-- Ported verbatim (columns/indexes/CHECKs) from decentraland/social-service-ea
-- node-pg-migrate migrations, collapsed into one idempotent sqlx migration:
--   1712167263170_friendships-table
--   1712942549875_friendships-history
--   1736277138587_add-indexes
--   1737745803297_add-blocks
--   1741805019806_social-settings
--   1748888407073_add-private-voice-chat
--   1749077198194_only-one-private-voice-chat-per-user
--   1772000000000_user-mutes
--   1772795904215_add-situation-reactions-visibility
--
-- These tables live alongside catalyrst-communities' community_members in the
-- shared `communities` database so voice-chat authorization joins are local.

-- friendships ---------------------------------------------------------------
CREATE TABLE IF NOT EXISTS friendships (
    id                 UUID PRIMARY KEY,
    address_requester  VARCHAR NOT NULL,
    address_requested  VARCHAR NOT NULL,
    is_active          BOOLEAN DEFAULT FALSE,
    created_at         TIMESTAMP DEFAULT now(),
    updated_at         TIMESTAMP DEFAULT now()
);

CREATE INDEX IF NOT EXISTS friendships_address_requester
    ON friendships USING hash (address_requester);

CREATE INDEX IF NOT EXISTS friendships_address_requester_lower
    ON friendships (LOWER(address_requester) text_pattern_ops);

DO $$ BEGIN
    ALTER TABLE friendships
        ADD CONSTRAINT unique_addresses UNIQUE (address_requester, address_requested);
EXCEPTION WHEN duplicate_table OR duplicate_object THEN NULL;
END $$;

-- friendship_actions (append-only history) ----------------------------------
CREATE TABLE IF NOT EXISTS friendship_actions (
    id             UUID PRIMARY KEY,
    friendship_id  UUID NOT NULL,
    action         VARCHAR NOT NULL,
    acting_user    VARCHAR NOT NULL,
    metadata       JSON,
    timestamp      TIMESTAMP DEFAULT now()
);

CREATE INDEX IF NOT EXISTS friendship_actions_friendship_id
    ON friendship_actions (friendship_id);

-- blocks --------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS blocks (
    id               UUID PRIMARY KEY,
    blocker_address  VARCHAR NOT NULL,
    blocked_address  VARCHAR NOT NULL,
    blocked_at       TIMESTAMP DEFAULT now()
);

CREATE INDEX IF NOT EXISTS blocks_blocked_address ON blocks (blocked_address);
CREATE UNIQUE INDEX IF NOT EXISTS blocks_blocker_address_blocked_address
    ON blocks (blocker_address, blocked_address);

-- social_settings -----------------------------------------------------------
CREATE TABLE IF NOT EXISTS social_settings (
    address                            VARCHAR(42) PRIMARY KEY,
    private_messages_privacy           VARCHAR NOT NULL DEFAULT 'only_friends',
    blocked_users_messages_visibility  VARCHAR NOT NULL DEFAULT 'show_messages',
    show_situation_reactions           VARCHAR NOT NULL DEFAULT 'show'
);

DO $$ BEGIN
    ALTER TABLE social_settings ADD CONSTRAINT valid_private_messages_privacy
        CHECK (private_messages_privacy IN ('only_friends', 'all'));
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

DO $$ BEGIN
    ALTER TABLE social_settings ADD CONSTRAINT valid_blocked_users_messages_visibility
        CHECK (blocked_users_messages_visibility IN ('show_messages', 'do_not_show_messages'));
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

DO $$ BEGIN
    ALTER TABLE social_settings ADD CONSTRAINT valid_show_situation_reactions
        CHECK (show_situation_reactions IN ('show', 'hide'));
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

-- user_mutes (backs the REST /v1/mutes surface) -----------------------------
CREATE TABLE IF NOT EXISTS user_mutes (
    muter_address  VARCHAR NOT NULL,
    muted_address  VARCHAR NOT NULL,
    muted_at       TIMESTAMP NOT NULL DEFAULT now(),
    PRIMARY KEY (muter_address, muted_address)
);

-- private_voice_chats -------------------------------------------------------
-- Keeps expires_at + updated_at from the original add-private-voice-chat
-- migration (we apply TTL in-process rather than the later drop-column path,
-- which is simpler for a single-node port).
CREATE TABLE IF NOT EXISTS private_voice_chats (
    id              UUID PRIMARY KEY,
    caller_address  VARCHAR NOT NULL,
    callee_address  VARCHAR NOT NULL,
    created_at      TIMESTAMP NOT NULL DEFAULT now(),
    updated_at      TIMESTAMP NOT NULL DEFAULT now(),
    expires_at      TIMESTAMP NOT NULL
);

DO $$ BEGIN
    ALTER TABLE private_voice_chats
        ADD CONSTRAINT private_voice_chats_no_self_call_check
        CHECK (caller_address != callee_address);
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

DO $$ BEGIN
    ALTER TABLE private_voice_chats
        ADD CONSTRAINT private_voice_chats_unique_caller UNIQUE (caller_address);
EXCEPTION WHEN duplicate_table OR duplicate_object THEN NULL;
END $$;

DO $$ BEGIN
    ALTER TABLE private_voice_chats
        ADD CONSTRAINT private_voice_chats_unique_callee UNIQUE (callee_address);
EXCEPTION WHEN duplicate_table OR duplicate_object THEN NULL;
END $$;

CREATE INDEX IF NOT EXISTS private_voice_chats_caller_address
    ON private_voice_chats (caller_address);
CREATE INDEX IF NOT EXISTS private_voice_chats_callee_address
    ON private_voice_chats (callee_address);
