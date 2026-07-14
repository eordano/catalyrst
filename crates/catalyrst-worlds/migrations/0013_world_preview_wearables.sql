-- Which smart wearables this world is previewing. The client GETs
-- <realm>/preview-wearables on entry, takes data[0], and RUNS its scene.json as
-- a scene -- so the answer has to be an explicit choice, never "every wearable
-- this node happens to host". NULL or empty means nothing is being previewed,
-- which is the honest answer for a normally deployed world.
ALTER TABLE worlds ADD COLUMN IF NOT EXISTS preview_wearable_urns TEXT[];
