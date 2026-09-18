-- no-transaction
-- GET /experiments (handlers/experiments.rs LIST_SQL) aggregates every segment
-- event carrying properties.exp_key: 85 k of 933 k rows. No existing index
-- leads with source plus that JSON key, so each call seq-scanned the 260 MB
-- heap (5,992 calls in 50 h, 144 ms and 33 k buffer hits each). The handler
-- now memoises the aggregate for 60 s; this partial expression index serves
-- the refreshes that remain through a bitmap index scan: the WHERE clause is
-- the query's predicate verbatim, so the scan visits only the heap pages that
-- hold qualifying rows (9 k of 32 k on 2026-09-17) and never evaluates the
-- jsonb filter on the other 850 k rows. It cannot be index-only: the planner
-- needs `body` itself to be an index column for that ("Index-Only Scans and
-- Covering Indexes" in the manual), and including the jsonb body would cost
-- more than it saves.
--
-- Built CONCURRENTLY so the build never blocks ingest writes; CONCURRENTLY
-- cannot run inside a transaction, hence the no-transaction directive above.
-- If a concurrent build is interrupted it leaves an INVALID index that
-- IF NOT EXISTS then skips: drop it and rerun.
CREATE INDEX CONCURRENTLY IF NOT EXISTS telemetry_events_experiments_idx
    ON telemetry_events (
        source,
        (body->'properties'->>'exp_key'),
        (body->'properties'->>'variant'),
        (body->>'event')
    )
    WHERE source = 'segment' AND body->'properties'->>'exp_key' IS NOT NULL;
