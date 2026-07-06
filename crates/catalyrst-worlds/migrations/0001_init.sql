-- catalyrst-worlds schema — ported from decentraland/worlds-content-server
-- migrations 0002 / 0011 / 0013 / 0015 / 0016 / 0017 / 0020, collapsed to the
-- final shape (post 0016 drops entity_id/entity/deployer/size from `worlds`;
-- scene data lives in `world_scenes`). Access settings live in worlds.access
-- (JSONB) per upstream worldsManager.storeAccess.

CREATE TABLE IF NOT EXISTS worlds (
    name              VARCHAR NOT NULL PRIMARY KEY,
    owner             VARCHAR,
    -- AccessSetting JSON: {type: unrestricted|shared-secret|allow-list|nft-ownership, ...}
    -- shared-secret stores a bcrypt hash under "secret"; allow-list stores wallets[]/communities[]
    access            JSONB,
    -- over-storage block marker (worlds.isWorldBlocked checks this is non-null)
    blocked_since     TIMESTAMPTZ,
    size              BIGINT,
    spawn_coordinates VARCHAR,
    -- world settings (migration 0017)
    title             VARCHAR,
    description       TEXT,
    content_rating    VARCHAR,
    skybox_time       INTEGER,
    categories        TEXT[],
    single_player     BOOLEAN,
    show_in_places    BOOLEAN,
    thumbnail_hash    VARCHAR,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at        TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS worlds_owner_index ON worlds (owner);

CREATE TABLE IF NOT EXISTS world_scenes (
    world_name            VARCHAR NOT NULL,
    entity_id             VARCHAR NOT NULL,
    deployment_auth_chain JSON NOT NULL,
    entity                JSONB NOT NULL,
    deployer              VARCHAR NOT NULL,
    parcels               TEXT[] NOT NULL,
    size                  BIGINT NOT NULL,
    created_at            TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (world_name, entity_id),
    CONSTRAINT fk_world_name
        FOREIGN KEY (world_name) REFERENCES worlds (name) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS world_scenes_world_name_idx ON world_scenes (world_name);
CREATE INDEX IF NOT EXISTS world_scenes_parcels_idx ON world_scenes USING GIN (parcels);
CREATE INDEX IF NOT EXISTS world_scenes_deployer_idx ON world_scenes (deployer);

-- deployment / streaming allow-lists (migration 0020)
CREATE TABLE IF NOT EXISTS world_permissions (
    id              SERIAL PRIMARY KEY,
    world_name      VARCHAR NOT NULL,
    permission_type VARCHAR NOT NULL,
    address         VARCHAR NOT NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (world_name, permission_type, address),
    CONSTRAINT fk_world_permissions_world_name
        FOREIGN KEY (world_name) REFERENCES worlds (name) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS world_permissions_address_idx ON world_permissions (address);
CREATE INDEX IF NOT EXISTS world_permissions_world_permission_idx
    ON world_permissions (world_name, permission_type);

-- over-storage / platform block list (migration 0013) — keyed by wallet
CREATE TABLE IF NOT EXISTS blocked (
    wallet     VARCHAR NOT NULL PRIMARY KEY,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS blocked_wallet_index ON blocked (wallet);
