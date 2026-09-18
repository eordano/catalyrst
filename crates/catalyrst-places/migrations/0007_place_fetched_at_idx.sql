-- MAX(fetched_at) freshness probes run twice a minute; CONCURRENTLY is safe because both runners feed this file to psql outside a transaction.
CREATE INDEX CONCURRENTLY IF NOT EXISTS place_fetched_at_idx ON place (fetched_at);
