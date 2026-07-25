CREATE TABLE IF NOT EXISTS telemetry_events (
    id          BIGSERIAL PRIMARY KEY,
    source      TEXT NOT NULL,
    project     TEXT NOT NULL DEFAULT '',
    event_kind  TEXT NOT NULL DEFAULT '',
    received_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    body        JSONB NOT NULL
);
CREATE INDEX IF NOT EXISTS telemetry_events_source_idx ON telemetry_events (source, received_at DESC);
CREATE INDEX IF NOT EXISTS telemetry_events_kind_idx ON telemetry_events (event_kind, received_at DESC);
