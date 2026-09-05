-- catalyrst-builder initial schema.
--
-- A fresh, minimal slice of decentraland/builder-server (the pre-publish draft
-- store behind builder-api.decentraland.org). NOT a literal replay of the
-- builder-server node-pg-migrate sequence (migrations/1597864779562_create-items.ts
-- and friends) — the columns the explorer's BuilderApiDtos actually need are
-- collapsed into final shape so bootstrap is one transaction. This DB is distinct
-- from marketplace_squid (on-chain published view), content (catalyst entities),
-- and communities (social data).

CREATE TABLE IF NOT EXISTS collections (
    id              UUID PRIMARY KEY,
    name            TEXT NOT NULL,
    eth_address     TEXT NOT NULL,
    salt            TEXT,
    contract_address TEXT,
    urn_suffix      TEXT,
    third_party_id  TEXT,
    is_published    BOOLEAN NOT NULL DEFAULT FALSE,
    is_approved     BOOLEAN NOT NULL DEFAULT FALSE,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS collections_eth_address_idx ON collections (lower(eth_address));

CREATE TABLE IF NOT EXISTS items (
    id                      UUID PRIMARY KEY,
    urn_suffix              TEXT,
    name                    TEXT NOT NULL,
    description             TEXT,
    thumbnail               TEXT,
    video                   TEXT,
    eth_address             TEXT NOT NULL,
    collection_id           UUID REFERENCES collections (id) ON DELETE CASCADE,
    blockchain_item_id      TEXT,
    price                   TEXT,
    beneficiary             TEXT,
    rarity                  TEXT,
    type                    TEXT NOT NULL,             -- 'wearable' | 'emote'
    data                    JSONB NOT NULL DEFAULT '{}'::jsonb,
    metrics                 JSONB,
    utility                 TEXT,
    mappings                JSONB,
    is_published            BOOLEAN NOT NULL DEFAULT FALSE,
    is_approved             BOOLEAN NOT NULL DEFAULT FALSE,
    in_catalyst             BOOLEAN NOT NULL DEFAULT FALSE,
    total_supply            BIGINT  NOT NULL DEFAULT 0,
    local_content_hash      TEXT,
    content_hash            TEXT,
    catalyst_content_hash   TEXT,
    created_at              TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at              TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS items_collection_id_idx ON items (collection_id);
CREATE INDEX IF NOT EXISTS items_eth_address_idx ON items (lower(eth_address));

-- Content map: filename -> content hash. The explorer joins each hash to
-- BuilderApiContent (/v1/storage/contents/{hash}) to build download URLs.
CREATE TABLE IF NOT EXISTS item_contents (
    item_id     UUID NOT NULL REFERENCES items (id) ON DELETE CASCADE,
    file        TEXT NOT NULL,
    hash        TEXT NOT NULL,
    PRIMARY KEY (item_id, file)
);

CREATE TABLE IF NOT EXISTS newsletter_subscriptions (
    email       TEXT PRIMARY KEY,
    source      TEXT NOT NULL DEFAULT 'auth',
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
