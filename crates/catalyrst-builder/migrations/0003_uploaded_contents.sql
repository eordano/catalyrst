CREATE TABLE uploaded_contents (
    hash TEXT PRIMARY KEY,
    bytes BYTEA NOT NULL CHECK (octet_length(bytes) <= 20971520),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
