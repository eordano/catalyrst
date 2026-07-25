CREATE TABLE IF NOT EXISTS private_messages_privacy (
    address                   VARCHAR PRIMARY KEY,
    private_messages_privacy  VARCHAR NOT NULL DEFAULT 'all',
    updated_at                TIMESTAMP NOT NULL DEFAULT now()
);
