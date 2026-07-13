-- Schema for the catalyrst-governance archive.
-- Faithfully mirrors scripts/governance-archive-schema.sql from the umbrella
-- reference node. Modeled after the Decentraland governance API response shapes
-- (https://governance.decentraland.org/api). Lives in its own `governance`
-- database; read consumers connect via a dedicated RO role.

CREATE TABLE IF NOT EXISTS proposals (
    id                   TEXT PRIMARY KEY,        -- UUID
    title                TEXT,
    description          TEXT,
    type                 TEXT,
    status               TEXT,
    "user"               TEXT,                    -- ETH address (author)
    snapshot_id          TEXT,
    snapshot_space       TEXT,
    snapshot_network     TEXT,
    snapshot_proposal    JSONB,
    discourse_id         INTEGER,
    discourse_topic_id   INTEGER,
    discourse_topic_slug TEXT,
    start_at             TIMESTAMPTZ,
    finish_at            TIMESTAMPTZ,
    created_at           TIMESTAMPTZ,
    updated_at           TIMESTAMPTZ,
    enacted              BOOLEAN,
    enacted_by           TEXT,
    enacted_description  TEXT,
    enacting_tx          TEXT,
    passed_by            TEXT,
    passed_description   TEXT,
    rejected_by          TEXT,
    rejected_description TEXT,
    deleted              BOOLEAN,
    deleted_by           TEXT,
    required_to_pass     INTEGER,
    vesting_addresses    JSONB,
    configuration        JSONB,
    raw                  JSONB NOT NULL,
    fetched_at           TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_proposals_status     ON proposals (status);
CREATE INDEX IF NOT EXISTS idx_proposals_type       ON proposals (type);
CREATE INDEX IF NOT EXISTS idx_proposals_user       ON proposals (lower("user"));
CREATE INDEX IF NOT EXISTS idx_proposals_created_at ON proposals (created_at DESC);
CREATE INDEX IF NOT EXISTS idx_proposals_finish_at  ON proposals (finish_at DESC);

CREATE TABLE IF NOT EXISTS projects (
    id                   TEXT PRIMARY KEY,        -- UUID
    proposal_id          TEXT,
    title                TEXT,
    status               TEXT,
    type                 TEXT,
    "user"               TEXT,                    -- author ETH address
    enacting_tx          TEXT,
    enacted_description  TEXT,
    configuration        JSONB,
    vesting_addresses    JSONB,
    funding              JSONB,
    latest_update        JSONB,
    created_at           TIMESTAMPTZ,
    updated_at           TIMESTAMPTZ,
    raw                  JSONB NOT NULL,
    fetched_at           TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_projects_status      ON projects (status);
CREATE INDEX IF NOT EXISTS idx_projects_proposal    ON projects (proposal_id);

CREATE TABLE IF NOT EXISTS project_updates (
    id                   TEXT PRIMARY KEY,        -- UUID
    project_id           TEXT,
    proposal_id          TEXT,
    status               TEXT,
    health               TEXT,
    introduction         TEXT,
    highlights           TEXT,
    blockers             TEXT,
    next_steps           TEXT,
    additional_notes     TEXT,
    author               TEXT,                    -- ETH address
    due_date             TIMESTAMPTZ,
    completion_date      TIMESTAMPTZ,
    discourse_topic_id   INTEGER,
    discourse_topic_slug TEXT,
    created_at           TIMESTAMPTZ,
    updated_at           TIMESTAMPTZ,
    raw                  JSONB NOT NULL,
    fetched_at           TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_project_updates_project  ON project_updates (project_id);
CREATE INDEX IF NOT EXISTS idx_project_updates_proposal ON project_updates (proposal_id);

CREATE TABLE IF NOT EXISTS budgets (
    id                   TEXT PRIMARY KEY,        -- UUID
    start_at             TIMESTAMPTZ,
    finish_at            TIMESTAMPTZ,
    total                BIGINT,
    allocated            BIGINT,
    categories           JSONB,
    raw                  JSONB NOT NULL,
    fetched_at           TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS vestings (
    address              TEXT PRIMARY KEY,        -- ETH address
    token                TEXT,
    status               TEXT,
    total                BIGINT,
    vested               BIGINT,
    released             BIGINT,
    releasable           BIGINT,
    start_at             TEXT,                    -- kept as TEXT; some dates are far-future
    finish_at            TEXT,
    raw                  JSONB NOT NULL,
    fetched_at           TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS members (
    address              TEXT NOT NULL,           -- ETH address
    role                 TEXT NOT NULL,           -- 'committee' or 'council'
    fetched_at           TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (address, role)
);

CREATE TABLE IF NOT EXISTS sync_state (
    key                  TEXT PRIMARY KEY,
    value                TEXT,
    updated_at           TIMESTAMPTZ NOT NULL DEFAULT now()
);
