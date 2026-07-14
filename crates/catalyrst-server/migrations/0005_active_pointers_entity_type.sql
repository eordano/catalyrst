-- 0005: entity_type on active_pointers.
-- The sync apply path and external pointer tooling filter active pointers by
-- entity type, and the server write paths have inserted the third column since
-- the feature landed - but the schema change never shipped, so every sync
-- batch failed with 'column "entity_type" of relation "active_pointers" does
-- not exist'. Nullable: pointers whose deployment predates the column (or was
-- pruned) have no type to backfill from.
--
-- Apply like 0001/0002: psql -f against the content DB, never sqlx::migrate!
-- (catalyrst-media owns the shared _sqlx_migrations table). Idempotent.

ALTER TABLE public.active_pointers
    ADD COLUMN IF NOT EXISTS entity_type text;

UPDATE public.active_pointers ap
SET entity_type = d.entity_type
FROM public.deployments d
WHERE d.entity_id = ap.entity_id
  AND ap.entity_type IS NULL;

CREATE INDEX IF NOT EXISTS active_pointers_entity_type_idx
    ON public.active_pointers USING btree (entity_type);
