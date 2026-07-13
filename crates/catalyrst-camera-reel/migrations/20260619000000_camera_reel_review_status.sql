-- Moderation review state for camera-reel images.
-- review_status is one of: 'ok' (default), 'flagged' (pending review), 'rejected'.
-- Set by moderators via the admin PATCH /admin/images/{id}/review route.
ALTER TABLE camera_reel_images
    ADD COLUMN IF NOT EXISTS review_status TEXT NOT NULL DEFAULT 'ok';

CREATE INDEX IF NOT EXISTS camera_reel_images_review_status_idx
    ON camera_reel_images (review_status)
    WHERE review_status <> 'ok';
