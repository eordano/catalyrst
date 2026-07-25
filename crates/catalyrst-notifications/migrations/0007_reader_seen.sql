-- Recipient online-ness signal for friend_first_wear rate limiting: bumped on
-- every authenticated GET /notifications (the in-world bell polls it while
-- the player is online, so "fetched since the last notification" is an
-- honest, dependency-free proxy for "has been online since".
CREATE TABLE IF NOT EXISTS notification_reader_seen (
    address       TEXT PRIMARY KEY,
    -- unix epoch ms of the most recent authenticated list fetch
    last_fetch_at BIGINT NOT NULL
);
