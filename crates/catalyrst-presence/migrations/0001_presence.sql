-- catalyrst-presence: unified user-count history archive.
--
-- This ONE migration consolidates the three earlier standalone archivers that
-- used to each own a separate database:
--
--   * archipelago-archive.py  (peer / island / hot-scene snapshots)
--   * comms-archive.py        (per-genesis-scene occupancy: who is in each scene)
--   * worlds-membership-archive.py (per-world membership: who is in each world)
--
-- Unlike the originals, the catalyrst-presence collector reads from the LOCAL
-- catalyrst services (archipelago / comms) rather than the public DCL endpoints,
-- so every snapshot pass here writes ALL of the rows below in one coherent round.
--
-- A "snapshot" is one collection pass. The `snapshots` table is the parent of
-- every per-entity row (peers, islands, hot scenes, scene occupancy, world
-- membership) so a single `snapshot_id` ties together everything captured in the
-- same instant. Per-day rollups live in `daily_stats`.

-- ---------------------------------------------------------------------------
-- One row per collection pass.
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS snapshots (
    id                  BIGSERIAL   PRIMARY KEY,
    taken_at            TIMESTAMPTZ NOT NULL DEFAULT now(),
    -- archipelago section
    peers_count         INTEGER     NOT NULL DEFAULT 0,
    islands_count       INTEGER     NOT NULL DEFAULT 0,
    hot_scenes_count    INTEGER     NOT NULL DEFAULT 0,
    -- genesis comms section
    scenes_polled       INTEGER     NOT NULL DEFAULT 0,
    scene_users_total   INTEGER     NOT NULL DEFAULT 0,
    -- worlds membership section
    worlds_polled       INTEGER     NOT NULL DEFAULT 0,
    active_worlds       INTEGER     NOT NULL DEFAULT 0,
    world_users_total   INTEGER     NOT NULL DEFAULT 0,
    -- /live-data totalUsers cross-check, if available
    worlds_live_total   INTEGER
);
CREATE INDEX IF NOT EXISTS idx_snapshots_taken_at ON snapshots (taken_at DESC);

-- ---------------------------------------------------------------------------
-- Archipelago: individual peers seen this snapshot.
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS peer_snapshots (
    snapshot_id     BIGINT NOT NULL REFERENCES snapshots(id) ON DELETE CASCADE,
    address         TEXT   NOT NULL,
    parcel_x        INTEGER,
    parcel_y        INTEGER,
    position_x      DOUBLE PRECISION,
    position_y      DOUBLE PRECISION,
    position_z      DOUBLE PRECISION,
    last_ping       BIGINT,
    PRIMARY KEY (snapshot_id, address)
);
CREATE INDEX IF NOT EXISTS idx_peer_snapshots_address ON peer_snapshots (address);

-- ---------------------------------------------------------------------------
-- Archipelago: island occupancy this snapshot.
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS island_snapshots (
    snapshot_id     BIGINT NOT NULL REFERENCES snapshots(id) ON DELETE CASCADE,
    island_id       TEXT   NOT NULL,
    peer_count      INTEGER,
    max_peers       INTEGER,
    center_x        DOUBLE PRECISION,
    center_y        DOUBLE PRECISION,
    center_z        DOUBLE PRECISION,
    radius          DOUBLE PRECISION,
    PRIMARY KEY (snapshot_id, island_id)
);

-- ---------------------------------------------------------------------------
-- Archipelago: hot-scene headcounts this snapshot.
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS hot_scene_snapshots (
    snapshot_id     BIGINT NOT NULL REFERENCES snapshots(id) ON DELETE CASCADE,
    scene_id        TEXT   NOT NULL,
    name            TEXT,
    base_x          INTEGER,
    base_y          INTEGER,
    users_count     INTEGER,
    parcel_count    INTEGER,
    creator         TEXT,
    description     TEXT,
    PRIMARY KEY (snapshot_id, scene_id)
);
CREATE INDEX IF NOT EXISTS idx_hot_scene_snapshots_name ON hot_scene_snapshots (name);

-- ---------------------------------------------------------------------------
-- Genesis comms: per-scene occupancy (the actual addresses in each hot scene).
-- `pointer` is the base parcel "x,y"; `realm` is the genesis realm (e.g. main).
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS scene_occupancy (
    snapshot_id     BIGINT      NOT NULL REFERENCES snapshots(id) ON DELETE CASCADE,
    taken_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    pointer         TEXT        NOT NULL,      -- e.g. "96,-132" (base parcel)
    scene_name      TEXT,
    realm           TEXT        NOT NULL DEFAULT 'main',
    addresses       JSONB       NOT NULL,      -- array of lowercase 0x addresses
    count           INTEGER     NOT NULL DEFAULT 0,
    PRIMARY KEY (snapshot_id, pointer, realm)
);
CREATE INDEX IF NOT EXISTS idx_occupancy_time    ON scene_occupancy (taken_at DESC);
CREATE INDEX IF NOT EXISTS idx_occupancy_pointer ON scene_occupancy (pointer, taken_at DESC);
CREATE INDEX IF NOT EXISTS idx_occupancy_count   ON scene_occupancy (count DESC);

-- ---------------------------------------------------------------------------
-- Worlds membership: who is in each `.eth` world room (realm=<world>).
-- These worlds are invisible to archipelago and to genesis comms.
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS world_membership (
    snapshot_id  BIGINT      NOT NULL REFERENCES snapshots(id) ON DELETE CASCADE,
    taken_at     TIMESTAMPTZ NOT NULL,
    world_name   TEXT        NOT NULL,
    addresses    JSONB       NOT NULL,         -- lowercase 0x wallet list
    count        INTEGER     NOT NULL,         -- length(addresses)
    live_users   INTEGER,                      -- /live-data per-world count, cross-check
    PRIMARY KEY (snapshot_id, world_name)
);
CREATE INDEX IF NOT EXISTS idx_world_membership_world ON world_membership (world_name, taken_at DESC);
CREATE INDEX IF NOT EXISTS idx_world_membership_taken ON world_membership (taken_at DESC);

-- ---------------------------------------------------------------------------
-- Per-UTC-day rollup across all three sources.
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS daily_stats (
    date                    DATE PRIMARY KEY,
    snapshots_taken         INTEGER          NOT NULL DEFAULT 0,
    peak_peers              INTEGER          NOT NULL DEFAULT 0,
    avg_peers               DOUBLE PRECISION NOT NULL DEFAULT 0,
    peak_hot_scene_users    INTEGER          NOT NULL DEFAULT 0,
    peak_scene_users        INTEGER          NOT NULL DEFAULT 0,
    peak_world_users        INTEGER          NOT NULL DEFAULT 0
);
