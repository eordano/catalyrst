CREATE TABLE IF NOT EXISTS friend_messages (
    id                 BIGSERIAL PRIMARY KEY,
    sender_address     TEXT NOT NULL,
    recipient_address  TEXT NOT NULL,
    body               TEXT NOT NULL,
    created_at         TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS friend_messages_sender_recipient_created
    ON friend_messages (sender_address, recipient_address, created_at);

CREATE INDEX IF NOT EXISTS friend_messages_recipient_sender_created
    ON friend_messages (recipient_address, sender_address, created_at);
