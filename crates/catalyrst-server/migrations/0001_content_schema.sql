-- Content-server schema for catalyrst-server (catalyrst-live).
--
-- catalyrst-server has no in-binary schema bootstrap: it reads/writes a content
-- DB that, in the original deployment, was created by the upstream TS
-- content-server (decentraland/catalyst) via node-pg-migrate. A fresh sync
-- replica has no such pre-existing schema, so this file reproduces exactly the
-- tables/indexes/sequence catalyrst-server's queries (entity_cache.rs,
-- sync_backends.rs, bin/live.rs) depend on.
--
-- Apply this once against a fresh content DB, before starting the catalyrst
-- content/sync process — NOT via sqlx::migrate!, because catalyrst-media already
-- owns the shared content DB's _sqlx_migrations table (a second sqlx migrator on
-- the same table would collide).
--
-- Fully idempotent: CREATE ... IF NOT EXISTS + inline constraints, so it is a
-- safe no-op on an already-populated content DB and a full create on a fresh box.
--
-- NOT included here (owned elsewhere, by design):
--   * the deployment_notify_trigger + notify_new_deployment() function
--     (bin/live.rs::install_notify_trigger installs them itself);
--   * the vestigial delta_pointer_result enum (no column/function uses it);
--   * translation_cache (catalyrst-media's migration), and the node-pg-migrate
--     `migrations` / sqlx `_sqlx_migrations` bookkeeping tables.

CREATE SEQUENCE IF NOT EXISTS public.deployments_id_seq
    AS integer START WITH 1 INCREMENT BY 1 NO MINVALUE NO MAXVALUE CACHE 1;

CREATE TABLE IF NOT EXISTS public.deployments (
    id integer NOT NULL DEFAULT nextval('public.deployments_id_seq'::regclass),
    deployer_address text NOT NULL,
    version text NOT NULL,
    entity_type text NOT NULL,
    entity_id text NOT NULL,
    entity_metadata json,
    entity_timestamp timestamp without time zone NOT NULL,
    entity_pointers text[] NOT NULL,
    local_timestamp timestamp without time zone NOT NULL,
    auth_chain json NOT NULL,
    deleter_deployment integer,
    CONSTRAINT deployments_pkey PRIMARY KEY (id),
    CONSTRAINT deployments_entity_id_key UNIQUE (entity_id)
);
ALTER SEQUENCE public.deployments_id_seq OWNED BY public.deployments.id;

CREATE TABLE IF NOT EXISTS public.content_files (
    deployment integer NOT NULL,
    content_hash text NOT NULL,
    key text NOT NULL,
    CONSTRAINT content_files_uniq_deployment_key UNIQUE (deployment, key)
);

CREATE TABLE IF NOT EXISTS public.active_pointers (
    pointer character varying NOT NULL,
    entity_id text NOT NULL,
    CONSTRAINT active_pointers_pkey PRIMARY KEY (pointer)
);

CREATE TABLE IF NOT EXISTS public.failed_deployments (
    entity_id text NOT NULL,
    entity_type text NOT NULL,
    failure_time timestamp without time zone NOT NULL,
    reason text NOT NULL,
    auth_chain json NOT NULL,
    error_description text NOT NULL,
    snapshot_hash text NOT NULL,
    CONSTRAINT failed_deployments_pkey PRIMARY KEY (entity_id)
);

CREATE TABLE IF NOT EXISTS public.processed_snapshots (
    hash text NOT NULL,
    process_time timestamp without time zone NOT NULL,
    CONSTRAINT processed_snapshots_pkey PRIMARY KEY (hash)
);

CREATE TABLE IF NOT EXISTS public.snapshots (
    hash text,
    init_timestamp timestamp without time zone NOT NULL,
    end_timestamp timestamp without time zone NOT NULL,
    replaced_hashes text[] NOT NULL,
    number_of_entities integer NOT NULL,
    generation_time timestamp without time zone NOT NULL,
    CONSTRAINT snapshots_pkey PRIMARY KEY (init_timestamp, end_timestamp)
);

CREATE TABLE IF NOT EXISTS public.system_properties (
    key text NOT NULL,
    value text NOT NULL,
    CONSTRAINT system_properties_pkey PRIMARY KEY (key)
);

-- Indexes (match the upstream content-server / node-pg-migrate set exactly).
CREATE INDEX IF NOT EXISTS active_pointers_entity_id_idx ON public.active_pointers USING btree (entity_id);
CREATE INDEX IF NOT EXISTS active_pointers_pointer_ops_idx ON public.active_pointers USING btree (pointer varchar_pattern_ops);
CREATE INDEX IF NOT EXISTS content_files_content_hash_index ON public.content_files USING btree (content_hash);
CREATE INDEX IF NOT EXISTS deployer_address_lower_case ON public.deployments USING btree (lower(deployer_address) text_pattern_ops);
CREATE INDEX IF NOT EXISTS deployments_deployer_addr_ts_idx ON public.deployments USING btree (deployer_address, entity_timestamp DESC);
CREATE INDEX IF NOT EXISTS deployments_entity_pointers_index ON public.deployments USING gin (entity_pointers);
CREATE INDEX IF NOT EXISTS deployments_entity_timestamp_entity_id_idx ON public.deployments USING btree (entity_timestamp DESC, entity_id DESC);
CREATE INDEX IF NOT EXISTS deployments_entity_type_index ON public.deployments USING btree (entity_type);
CREATE INDEX IF NOT EXISTS deployments_full_snapshots_ix ON public.deployments USING btree (deleter_deployment DESC, local_timestamp) INCLUDE (entity_type);
CREATE INDEX IF NOT EXISTS deployments_local_timestamp_lower_entity_id_idx ON public.deployments USING btree (local_timestamp DESC, lower(entity_id) DESC);
CREATE INDEX IF NOT EXISTS deployments_type_entity_ts_idx ON public.deployments USING btree (entity_type, entity_timestamp DESC, entity_id DESC);
CREATE INDEX IF NOT EXISTS deployments_type_local_ts_idx ON public.deployments USING btree (entity_type, local_timestamp DESC, lower(entity_id) DESC);
