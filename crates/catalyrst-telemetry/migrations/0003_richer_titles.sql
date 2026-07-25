-- Richer event titles so non-error payloads aren't all "(no message)".
--
-- ~1000 events (sentry sessions/transactions/envelopes, segment track/identify)
-- carry no message/logentry/exception, so the 0002 title chain fell through to
-- the literal "(no message)" and they all collapsed into one noisy issue. Those
-- payloads DO have meaningful fields — extract them, and fall back to "(kind)"
-- (e.g. "(session)") instead of "(no message)" for anything still unnamed:
--   transaction -> body.transaction (e.g. "loading_process")
--   segment track -> body.event
--   session     -> "session (<status|started|update>)"
--   identify    -> "identify <userId>"
--
-- The fingerprint is a STORED generated column, whose expression can't be
-- ALTERed in place, so we drop + re-add it (recomputes for all rows) and
-- recreate its indexes. Title logic is kept identical to TITLE1 in
-- handlers/dashboard.rs.

DROP INDEX IF EXISTS telemetry_events_fingerprint_idx;
DROP INDEX IF EXISTS telemetry_events_fingerprint_trgm_idx;
ALTER TABLE telemetry_events DROP COLUMN IF EXISTS fingerprint;

ALTER TABLE telemetry_events
    ADD COLUMN fingerprint text
    GENERATED ALWAYS AS (
        regexp_replace(
          regexp_replace(
            regexp_replace(
              regexp_replace(
                split_part(
                  COALESCE(
                    NULLIF(body->>'message',''),
                    NULLIF(body#>>'{logentry,message}',''),
                    NULLIF(body#>>'{exception,values,0,type}','')
                      || COALESCE(': ' || (body#>>'{exception,values,0,value}'), ''),
                    NULLIF(body->>'transaction',''),
                    NULLIF(body->>'event',''),
                    CASE WHEN event_kind = 'session'
                         THEN 'session (' || COALESCE(NULLIF(body->>'status',''),
                                CASE WHEN (body->>'init')::boolean THEN 'started' ELSE 'update' END) || ')'
                    END,
                    CASE WHEN body->>'userId' IS NOT NULL
                         THEN 'identify ' || (body->>'userId')
                    END,
                    '(' || event_kind || ')'),
                  E'\n', 1),
                '<[^>]*>', '', 'g'),
              '^[0-9]{2}:[0-9]{2}:[0-9]{2}\.[0-9]+ - ', ''),
            'https?://[^ ]+', 'URL', 'g'),
          '-?[0-9]+', 'N', 'g')
    ) STORED;

CREATE INDEX IF NOT EXISTS telemetry_events_fingerprint_idx
    ON telemetry_events (fingerprint, received_at DESC);
CREATE INDEX IF NOT EXISTS telemetry_events_fingerprint_trgm_idx
    ON telemetry_events USING gin (fingerprint gin_trgm_ops);
