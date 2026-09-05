-- Dashboard read-path performance.
--
-- The issues ("group") view recomputed a 4-nested regexp_replace fingerprint
-- over every row in the window on every request, the window filter had no
-- leading-received_at index (so it seq-scanned), and the search/level filters
-- were pure jsonb/expression scans. This migration materializes the
-- fingerprint as a STORED generated column and adds the supporting indexes.
--
-- All inputs to the generated column are IMMUTABLE (split_part, COALESCE,
-- NULLIF, regexp_replace, the jsonb ->> / #>> operators), which is what makes
-- a STORED generated column legal here. On the existing populated table the
-- column backfills automatically as part of the ADD COLUMN.

-- The title expression (kept identical to TITLE1 in handlers/dashboard.rs):
--   split_part(COALESCE(NULLIF(body->>'message',''),
--     NULLIF(body#>>'{logentry,message}',''),
--     NULLIF(body#>>'{exception,values,0,type}','')
--       || COALESCE(': ' || (body#>>'{exception,values,0,value}'), ''),
--     '(no message)'), E'\n', 1)
-- normalized by the fingerprint() regexp chain in dashboard.rs.

ALTER TABLE telemetry_events
    ADD COLUMN IF NOT EXISTS fingerprint text
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
                    '(no message)'),
                  E'\n', 1),
                '<[^>]*>', '', 'g'),
              '^[0-9]{2}:[0-9]{2}:[0-9]{2}\.[0-9]+ - ', ''),
            'https?://[^ ]+', 'URL', 'g'),
          '-?[0-9]+', 'N', 'g')
    ) STORED;

-- Issues view: GROUP BY fingerprint within the time window, newest first.
CREATE INDEX IF NOT EXISTS telemetry_events_fingerprint_idx
    ON telemetry_events (fingerprint, received_at DESC);

-- Events view + stats: window filter (received_at > now() - interval) ordered
-- newest-first. No leading-received_at index existed before this.
CREATE INDEX IF NOT EXISTS telemetry_events_received_at_idx
    ON telemetry_events (received_at DESC);

-- Level facet filter (body->>'level' = $3) and the by_level stats rollup.
CREATE INDEX IF NOT EXISTS telemetry_events_level_idx
    ON telemetry_events ((body->>'level'), received_at DESC);

-- Title substring search (TITLE1 ILIKE '%q%'). Trigram GIN over the same
-- stored fingerprint approximates the title closely enough for the facet
-- search and turns the ILIKE into an index scan instead of a per-row regexp.
CREATE EXTENSION IF NOT EXISTS pg_trgm;
CREATE INDEX IF NOT EXISTS telemetry_events_fingerprint_trgm_idx
    ON telemetry_events USING gin (fingerprint gin_trgm_ops);
