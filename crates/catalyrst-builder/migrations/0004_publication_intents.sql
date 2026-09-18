ALTER TABLE collections ADD COLUMN publication_pending BOOLEAN NOT NULL DEFAULT FALSE;

CREATE TABLE collection_publications (
    collection_id UUID PRIMARY KEY REFERENCES collections(id) ON DELETE CASCADE,
    preparation JSONB NOT NULL,
    tx_hash TEXT UNIQUE,
    contract_address TEXT UNIQUE,
    status TEXT NOT NULL CHECK (status IN ('prepared', 'submitted', 'published', 'reverted')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
