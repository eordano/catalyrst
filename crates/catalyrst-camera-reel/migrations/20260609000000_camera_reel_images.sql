-- camera-reel reels. Mirrors upstream `images` table (decentraland/camera-reel-service)
-- but named `camera_reel_images` to avoid colliding with anything else in places_events.
-- url / thumbnail_url here hold {api_url}/api/images/{content-hash} instead of the S3 bucket
-- object name, since the catalyrst port stores image + thumbnail content-addressed on disk.
CREATE TABLE IF NOT EXISTS camera_reel_images (
    id            UUID PRIMARY KEY,
    user_address  TEXT NOT NULL,
    url           TEXT NOT NULL,
    thumbnail_url TEXT NOT NULL DEFAULT '',
    metadata      JSONB NOT NULL,
    is_public     BOOLEAN NOT NULL DEFAULT FALSE,
    created_at    TIMESTAMP DEFAULT now()
);

CREATE INDEX IF NOT EXISTS camera_reel_images_user_address_idx
    ON camera_reel_images (user_address);

CREATE INDEX IF NOT EXISTS camera_reel_images_user_address_is_public_idx
    ON camera_reel_images (user_address, is_public);

CREATE INDEX IF NOT EXISTS camera_reel_images_place_id_idx
    ON camera_reel_images ((metadata->>'placeId'));

CREATE INDEX IF NOT EXISTS camera_reel_images_place_id_is_public_created_at_desc_idx
    ON camera_reel_images ((metadata->>'placeId'), is_public, created_at DESC);
