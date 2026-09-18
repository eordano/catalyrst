-- Watermark of the wearable_last_seen refresh: the content-DB clock (UTC, the local_timestamp
-- domain) read before the last scan, and when the last full 30-day pass ran. One row; none
-- means never scanned, so the next pass is a full one.
CREATE TABLE IF NOT EXISTS marketplace.wearable_last_seen_refresh (
    id              boolean PRIMARY KEY DEFAULT true CHECK (id),
    scanned_through timestamp NOT NULL,
    last_full_at    timestamptz NOT NULL
);
