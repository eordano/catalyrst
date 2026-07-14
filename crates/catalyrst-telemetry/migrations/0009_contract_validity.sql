-- Additive: quarantine-by-flag column for contract-invalid telemetry events.
-- The ingest handler (handlers/segment.rs::store_event) writes a human-readable
-- reason here when a DCL event's shape defeats the telemetry contract; the event
-- is STILL stored (never rejected). NULL means: valid, or validation disabled
-- (no TELEMETRY_CONTRACT_PATH), or a non-contract-governed event (identify/page/…).
ALTER TABLE telemetry_events ADD COLUMN IF NOT EXISTS invalid_reason TEXT;

-- Partial index over just the flagged rows (cheap: only invalid events are
-- indexed) so a future dashboard can list/count contract-invalid events without
-- a full scan. Nothing consumes this yet — see below.
CREATE INDEX IF NOT EXISTS telemetry_events_invalid_idx
    ON telemetry_events (received_at DESC)
    WHERE invalid_reason IS NOT NULL;

-- Where an invalid-events surface could hang off this, when wanted (dashboards
-- deliberately left unchanged for now):
--   valid-only feed:   ... WHERE invalid_reason IS NULL ...
--   invalid count:     SELECT count(*) FROM telemetry_events WHERE invalid_reason IS NOT NULL;
--   invalid breakdown: SELECT body->>'event' AS event, invalid_reason, count(*)
--                        FROM telemetry_events WHERE invalid_reason IS NOT NULL
--                        GROUP BY 1, 2 ORDER BY 3 DESC;
