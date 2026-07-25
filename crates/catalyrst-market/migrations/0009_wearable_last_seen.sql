-- Ranking signal for the shop's "Suggested" sort (experiment): for each item
-- URN worn by a recently-active profile, when a player most recently could
-- have SEEN it in-world (max entity_timestamp across profiles wearing it).
--
-- Refreshed periodically by catalyrst-market from the catalyst content DB
-- (CONTENT_PG_COMPONENT_PSQL_CONNECTION_STRING). Rows are only ever upserted:
-- an item that falls out of the refresh window keeps its old last_seen, which
-- stays truthful — it just ranks lower over time.
--
-- last_seen mirrors deployments.entity_timestamp (timestamp WITHOUT time
-- zone, UTC by catalyst convention), so no tz conversion happens on copy.
CREATE TABLE IF NOT EXISTS marketplace.wearable_last_seen (
    urn text PRIMARY KEY,
    last_seen timestamp NOT NULL,
    refreshed_at timestamptz NOT NULL DEFAULT now()
);
