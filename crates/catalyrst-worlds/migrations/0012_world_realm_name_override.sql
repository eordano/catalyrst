-- A world whose realmName ends in "dcl.eth" is unreachable on a self-hosted node
-- whenever the same NAME is also published on Decentraland's own worlds server:
-- the client fetches <asset-bundle-registry>/worlds/<realmName>/manifest, and a
-- non-empty manifest makes it load scene definitions from the official registry
-- instead of this node's scenesUrn. The scene's metadata.worldConfiguration.name
-- is restored by every deploy, so the escape has to live outside the entity.
--
-- NULL means "derive it" (see resolve_realm_name); a value here is authoritative
-- and survives republishing.
ALTER TABLE worlds ADD COLUMN IF NOT EXISTS realm_name_override VARCHAR;
