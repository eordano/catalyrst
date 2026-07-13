-- catalyrst-world-storage initial schema.
--
-- Consolidates the five upstream world-storage-service node-pg-migrate
-- migrations into the final shape they converge to:
--   1767637355865_create-world-storage-table
--   1767637359664_create-player-storage-table
--   1767637363602_create-env-variables-table
--   1771358829982_add-value-size-column   (value_size integer NOT NULL)
--   1775047654810_add-place-id-column     (place_id uuid in the composite PK)
--
-- Three storage namespaces, each scoped by (world_name, place_id):
--   * world_storage  — per-scene JSON key/value store
--   * player_storage — per-scene, per-player JSON key/value store
--   * env_variables  — per-scene encrypted (AES-256-GCM) env vars; values are
--                      stored as raw ciphertext bytes (value_enc bytea) so plaintext
--                      never touches the database.

CREATE TABLE IF NOT EXISTS world_storage (
    world_name varchar(255) NOT NULL,
    place_id   uuid         NOT NULL,
    key        varchar(255) NOT NULL,
    value      jsonb        NOT NULL,
    value_size integer      NOT NULL DEFAULT 0,
    created_at timestamp    NOT NULL DEFAULT current_timestamp,
    updated_at timestamp    NOT NULL DEFAULT current_timestamp,
    CONSTRAINT world_storage_pkey PRIMARY KEY (world_name, place_id, key)
);

CREATE TABLE IF NOT EXISTS player_storage (
    world_name     varchar(255) NOT NULL,
    place_id       uuid         NOT NULL,
    player_address varchar(255) NOT NULL,
    key            varchar(255) NOT NULL,
    value          jsonb        NOT NULL,
    value_size     integer      NOT NULL DEFAULT 0,
    created_at     timestamp    NOT NULL DEFAULT current_timestamp,
    updated_at     timestamp    NOT NULL DEFAULT current_timestamp,
    CONSTRAINT player_storage_pkey PRIMARY KEY (world_name, place_id, player_address, key)
);

CREATE TABLE IF NOT EXISTS env_variables (
    world_name varchar(255) NOT NULL,
    place_id   uuid         NOT NULL,
    key        varchar(255) NOT NULL,
    value_enc  bytea        NOT NULL,
    value_size integer      NOT NULL DEFAULT 0,
    created_at timestamp    NOT NULL DEFAULT current_timestamp,
    updated_at timestamp    NOT NULL DEFAULT current_timestamp,
    CONSTRAINT env_variables_pkey PRIMARY KEY (world_name, place_id, key)
);
