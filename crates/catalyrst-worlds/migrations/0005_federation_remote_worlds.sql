-- Worlds a federated peer says it holds. A read-only mirror of a public listing.
--
-- Deliberately absent, and absent for the same reason: there is no `owner` column.
-- A peer's /worlds response carries an `owner` field (handlers/worlds_list.rs:118),
-- so an ownership assertion arrives in-band on every poll. Giving it nowhere to land
-- is stronger than remembering to drop it. Ownership for any name -- local or
-- peer-reported -- resolves only through permissions.rs::resolve_world_owner against
-- squid_marketplace.ens.
--
-- Also deliberately absent: any FOREIGN KEY to worlds(name). world_permissions and
-- world_permission_parcels attach by FK to worlds(name) only, so no ACL row can ever
-- be created against a remote world.
CREATE TABLE IF NOT EXISTS remote_worlds (
    peer_id          TEXT        NOT NULL,
    world_name       TEXT        NOT NULL,
    title            TEXT,
    description      TEXT,
    content_rating   TEXT,
    categories       TEXT[],
    thumbnail_hash   TEXT,
    deployed_scenes  BIGINT      NOT NULL DEFAULT 0,
    last_deployed_at TIMESTAMPTZ,
    observed_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    hidden_since     TIMESTAMPTZ,
    PRIMARY KEY (peer_id, world_name),
    CONSTRAINT remote_worlds_peer_id_lowercase CHECK (peer_id    = lower(peer_id)),
    CONSTRAINT remote_worlds_name_lowercase    CHECK (world_name = lower(world_name))
);

CREATE INDEX IF NOT EXISTS remote_worlds_name_idx ON remote_worlds (world_name);

-- One row per peer we have ever attempted. `last_success_at` going stale while
-- `last_attempt_at` advances is how an unreachable peer is reported: the mirror
-- keeps serving its last-good rows and says, on the wire, that they are stale.
-- An unreachable peer must never collapse to an empty list, which is
-- indistinguishable from "this peer holds no worlds".
CREATE TABLE IF NOT EXISTS remote_peer_status (
    peer_id          TEXT PRIMARY KEY,
    last_attempt_at  TIMESTAMPTZ,
    last_success_at  TIMESTAMPTZ,
    last_error       TEXT,
    worlds_observed  BIGINT NOT NULL DEFAULT 0,
    entries_skipped  BIGINT NOT NULL DEFAULT 0,
    truncated        BOOLEAN NOT NULL DEFAULT FALSE,
    CONSTRAINT remote_peer_status_id_lowercase CHECK (peer_id = lower(peer_id))
);
