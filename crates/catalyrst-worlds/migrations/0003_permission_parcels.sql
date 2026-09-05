-- catalyrst-worlds: parcel-scoped permissions.
--
-- Ported from decentraland/worlds-content-server (a3978fa "Add new parcel
-- permissions"). A `world_permissions` row (deployment | streaming allow-list
-- entry for an address) is WORLD-WIDE when it has NO rows here, and
-- PARCEL-SCOPED when one or more parcels are listed. The write surface
-- (POST/DELETE .../address/:address/parcels) adds/removes these rows; granting a
-- world-wide permission clears them (making the address unrestricted again).
--
-- Parcels are stored canonicalized ("<x>,<y>", no leading zeros / whitespace)
-- so they compare by value with a scene's parcels.

CREATE TABLE IF NOT EXISTS world_permission_parcels (
    permission_id INTEGER NOT NULL,
    parcel        VARCHAR NOT NULL,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (permission_id, parcel),
    CONSTRAINT fk_world_permission_parcels_permission
        FOREIGN KEY (permission_id) REFERENCES world_permissions (id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS world_permission_parcels_permission_idx
    ON world_permission_parcels (permission_id);
