-- Published-event outbox.
--
-- Upstream comms-gatekeeper publishes platform events (currently the
-- CommunityStreamingEnded streaming event) to an SNS topic that the
-- notifications service consumes. catalyrst has no SNS; this durable outbox is
-- the equivalent sink. Each row holds the EXACT event JSON the upstream
-- publisher would have put on the wire (the serialized `@dcl/schemas`
-- CommunityStreamingEndedEvent), keyed by the event's `key` so downstream
-- consumers can de-duplicate. A consumer marks a row consumed by setting
-- `consumed_at`.
CREATE TABLE IF NOT EXISTS published_events (
    id            BIGINT GENERATED ALWAYS AS IDENTITY,
    event_key     TEXT NOT NULL,                  -- event.key (idempotency key)
    event_type    TEXT NOT NULL,                  -- event.type   (e.g. 'streaming')
    event_subtype TEXT NOT NULL,                  -- event.subType (e.g. 'community-streaming-ended')
    payload       JSONB NOT NULL,                 -- the full event JSON, verbatim
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    consumed_at   TIMESTAMPTZ,
    PRIMARY KEY (id),
    UNIQUE (event_key)
);

CREATE INDEX IF NOT EXISTS idx_published_events_unconsumed
    ON published_events (created_at)
    WHERE consumed_at IS NULL;
