-- catalyrst-lists schema.
--
-- dcl-lists.decentraland.org serves two admin-curated master lists. The
-- curated data is not reconstructable locally (the upstream POI list is
-- managed via an L2 POI contract / admin tooling and the banned-name list is
-- a hand-maintained denylist), so we co-locate the cache in the existing
-- places_events DB and keep it fresh with a periodic puller
-- (scripts/bootstrap-catalyrst-lists.sh) that POSTs the live endpoints and
-- upserts here.
--
-- Both tables are tiny (live counts: 51 POIs, 11 banned names).

CREATE TABLE IF NOT EXISTS lists_poi (
    coord       TEXT PRIMARY KEY,             -- parcel coordinate "x,y"
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS lists_banned_name (
    name        TEXT PRIMARY KEY,             -- denylisted name string (case-sensitive, as upstream serves)
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
