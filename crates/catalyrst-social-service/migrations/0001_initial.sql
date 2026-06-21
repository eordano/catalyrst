-- catalyrst-communities initial schema.
--
-- This is a fresh DB design (NOT a port of social-service-ea's node-pg-migrate
-- sequence). Tables are collapsed from upstream's 1747997882050..1772000000000
-- migration run into their final shape so initial bootstrap is one
-- transaction. Rationale documented per-table where the shape differs from
-- the literal upstream schema.

CREATE TABLE IF NOT EXISTS communities (
    id                          UUID PRIMARY KEY,
    name                        VARCHAR NOT NULL,
    description                 TEXT NOT NULL,
    owner_address               VARCHAR NOT NULL,
    private                     BOOLEAN NOT NULL DEFAULT FALSE,
    active                      BOOLEAN NOT NULL DEFAULT TRUE,
    unlisted                    BOOLEAN NOT NULL DEFAULT FALSE,
    ranking_score               REAL    NOT NULL DEFAULT 0,
    editors_choice              BOOLEAN NOT NULL DEFAULT FALSE,
    last_score_calculated_at    TIMESTAMP,
    created_at                  TIMESTAMP NOT NULL DEFAULT now(),
    updated_at                  TIMESTAMP NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_communities_ranking_sort
    ON communities (editors_choice DESC, ranking_score DESC, name ASC)
    WHERE active = TRUE AND unlisted = FALSE;

CREATE INDEX IF NOT EXISTS idx_communities_last_score_calculated_at
    ON communities (last_score_calculated_at)
    WHERE active = TRUE;

CREATE INDEX IF NOT EXISTS idx_communities_owner_address
    ON communities (owner_address);

CREATE TABLE IF NOT EXISTS community_members (
    community_id    UUID    NOT NULL REFERENCES communities (id) ON DELETE CASCADE,
    member_address  VARCHAR NOT NULL,
    role            VARCHAR NOT NULL,
    joined_at       TIMESTAMP NOT NULL DEFAULT now(),
    PRIMARY KEY (community_id, member_address)
);

CREATE INDEX IF NOT EXISTS idx_community_members_member_address
    ON community_members (member_address, community_id);

CREATE TABLE IF NOT EXISTS community_bans (
    community_id    UUID    NOT NULL REFERENCES communities (id) ON DELETE CASCADE,
    banned_address  VARCHAR NOT NULL,
    banned_by       VARCHAR NOT NULL,
    banned_at       TIMESTAMP NOT NULL DEFAULT now(),
    reason          TEXT,
    active          BOOLEAN NOT NULL DEFAULT TRUE,
    unbanned_by     VARCHAR,
    unbanned_at     TIMESTAMP,
    PRIMARY KEY (community_id, banned_address)
);

CREATE INDEX IF NOT EXISTS idx_community_bans_active
    ON community_bans (community_id, active);

-- community_places.id is TEXT (not UUID) because place ids may be either a
-- UUID (scene) or a world name like "foo.dcl.eth" — upstream landed on this
-- shape in 1751000000000_change-community-places-id-to-text.
CREATE TABLE IF NOT EXISTS community_places (
    id              TEXT    NOT NULL,
    community_id    UUID    NOT NULL REFERENCES communities (id) ON DELETE CASCADE,
    added_by        VARCHAR NOT NULL,
    added_at        TIMESTAMP NOT NULL DEFAULT now(),
    PRIMARY KEY (id, community_id)
);

CREATE INDEX IF NOT EXISTS idx_community_places_community
    ON community_places (community_id);

CREATE TABLE IF NOT EXISTS community_requests (
    id              UUID PRIMARY KEY,
    community_id    UUID    NOT NULL REFERENCES communities (id) ON DELETE CASCADE,
    member_address  VARCHAR NOT NULL,
    status          VARCHAR NOT NULL DEFAULT 'pending',
    type            VARCHAR NOT NULL,
    created_at      TIMESTAMP NOT NULL DEFAULT now(),
    updated_at      TIMESTAMP NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_community_requests_community_type_status
    ON community_requests (community_id, type, status);

CREATE INDEX IF NOT EXISTS idx_community_requests_community_status
    ON community_requests (community_id, status);

CREATE INDEX IF NOT EXISTS idx_community_requests_member_type_status
    ON community_requests (member_address, type, status);

CREATE INDEX IF NOT EXISTS idx_community_requests_community_member_type_status
    ON community_requests (community_id, member_address, type, status);

CREATE TABLE IF NOT EXISTS community_posts (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    community_id    UUID    NOT NULL REFERENCES communities (id) ON DELETE CASCADE,
    author_address  VARCHAR NOT NULL,
    content         TEXT    NOT NULL,
    created_at      TIMESTAMP NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS community_posts_community_id_idx
    ON community_posts (community_id);

CREATE INDEX IF NOT EXISTS community_posts_community_created_idx
    ON community_posts (community_id, created_at);

CREATE TABLE IF NOT EXISTS community_post_likes (
    post_id      UUID    NOT NULL REFERENCES community_posts (id) ON DELETE CASCADE,
    user_address VARCHAR NOT NULL,
    liked_at     TIMESTAMP NOT NULL DEFAULT now(),
    PRIMARY KEY (post_id, user_address)
);

CREATE INDEX IF NOT EXISTS community_post_likes_post_id_idx
    ON community_post_likes (post_id);

CREATE TABLE IF NOT EXISTS community_ranking_metrics (
    community_id              UUID PRIMARY KEY REFERENCES communities (id) ON DELETE CASCADE,
    events_count              INTEGER NOT NULL DEFAULT 0,
    photos_count              INTEGER NOT NULL DEFAULT 0,
    streams_count             INTEGER NOT NULL DEFAULT 0,
    events_total_attendees    INTEGER NOT NULL DEFAULT 0,
    streams_total_participants INTEGER NOT NULL DEFAULT 0,
    has_thumbnail             BOOLEAN NOT NULL DEFAULT FALSE,
    updated_at                TIMESTAMP NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_community_ranking_metrics_updated_at
    ON community_ranking_metrics (updated_at);

-- Active community voice chats. Upstream tracks these in Redis; we sink the
-- minimum subset needed by the GET /v1/community-voice-chats/active read.
-- An empty table is the correct steady state until the federation write path
-- starts inserting rows.
CREATE TABLE IF NOT EXISTS community_voice_chats (
    community_id  UUID PRIMARY KEY REFERENCES communities (id) ON DELETE CASCADE,
    started_at    TIMESTAMP NOT NULL DEFAULT now(),
    participants  INTEGER   NOT NULL DEFAULT 0
);
