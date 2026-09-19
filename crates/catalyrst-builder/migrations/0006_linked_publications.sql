CREATE TABLE linked_collection_publications (
    collection_id UUID PRIMARY KEY REFERENCES collections(id) ON DELETE CASCADE,
    preparation JSONB NOT NULL,
    snapshot JSONB NOT NULL,
    salt TEXT NOT NULL UNIQUE,
    cheque JSONB,
    status TEXT NOT NULL CHECK (status IN ('prepared', 'signing', 'authorized', 'under_review')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK ((status IN ('prepared', 'signing')) = (cheque IS NULL))
);
