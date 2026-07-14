-- catalyrst-communities federation tables.
--
-- Design choice: we KEEP the 0001 read-projection tables (communities,
-- community_members, community_posts, community_places, community_bans) and
-- PROJECT into them from the federation log on apply. The federation tables
-- below are the canonical signed-action log; the 0001 tables are the
-- materialised read view that the existing GET handlers consume unchanged.
--
-- The bridge between the two namespaces is `community_id_to_uuid(hex)`: the
-- federation `community_id` is a hex SHA-256(creator||name||nonce); we map
-- it to a deterministic UUID (first 16 bytes of the hash, RFC-4122 v4
-- variant bits set) and use that as the PK in the 0001 tables. This keeps
-- the existing GET handlers working with UUID-shaped community ids.

CREATE TABLE IF NOT EXISTS communities_local (
    community_id    TEXT      PRIMARY KEY,
    creator         TEXT      NOT NULL,
    signature       TEXT      NOT NULL,
    name            TEXT      NOT NULL,
    description     TEXT      NOT NULL DEFAULT '',
    signed_at       BIGINT    NOT NULL,
    nonce           TEXT      NOT NULL,
    received_at     BIGINT    NOT NULL,
    seq             BIGSERIAL UNIQUE NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_communities_local_seq ON communities_local (seq);

CREATE TABLE IF NOT EXISTS community_role_log (
    signature_hash  TEXT      PRIMARY KEY,
    community_id    TEXT      NOT NULL,
    signer          TEXT      NOT NULL,
    target          TEXT      NOT NULL,
    role            TEXT      NOT NULL CHECK (role IN ('owner','admin','mod','member','banned')),
    signed_at       BIGINT    NOT NULL,
    message_payload JSONB     NOT NULL,
    received_at     BIGINT    NOT NULL,
    seq             BIGSERIAL UNIQUE NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_crl_community  ON community_role_log (community_id, target, signed_at DESC);
CREATE INDEX IF NOT EXISTS idx_crl_seq        ON community_role_log (seq);

CREATE TABLE IF NOT EXISTS community_role_current (
    community_id    TEXT      NOT NULL,
    member          TEXT      NOT NULL,
    role            TEXT      NOT NULL CHECK (role IN ('owner','admin','mod','member','banned')),
    effective_since BIGINT    NOT NULL,
    last_sig_hash   TEXT      NOT NULL,
    PRIMARY KEY (community_id, member)
);

CREATE INDEX IF NOT EXISTS idx_crc_member ON community_role_current (member);

CREATE TABLE IF NOT EXISTS community_posts_log (
    signature_hash  TEXT      PRIMARY KEY,
    community_id    TEXT      NOT NULL,
    author          TEXT      NOT NULL,
    content_hash    TEXT      NOT NULL,
    signed_at       BIGINT    NOT NULL,
    received_at     BIGINT    NOT NULL,
    deleted_by_sig  TEXT,
    seq             BIGSERIAL UNIQUE NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_cpl_community ON community_posts_log (community_id, signed_at DESC);
CREATE INDEX IF NOT EXISTS idx_cpl_seq       ON community_posts_log (seq);

CREATE TABLE IF NOT EXISTS community_post_likes_log (
    signature_hash    TEXT   PRIMARY KEY,
    post_signature_hash TEXT NOT NULL,
    signer            TEXT   NOT NULL,
    signed_at         BIGINT NOT NULL,
    received_at       BIGINT NOT NULL,
    unliked_by_sig    TEXT,
    seq               BIGSERIAL UNIQUE NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_cpll_post ON community_post_likes_log (post_signature_hash);
CREATE INDEX IF NOT EXISTS idx_cpll_seq  ON community_post_likes_log (seq);

CREATE TABLE IF NOT EXISTS community_places_log (
    signature_hash  TEXT      PRIMARY KEY,
    community_id    TEXT      NOT NULL,
    place_id        TEXT      NOT NULL,
    action          TEXT      NOT NULL CHECK (action IN ('add','remove')),
    signer          TEXT      NOT NULL,
    signed_at       BIGINT    NOT NULL,
    received_at     BIGINT    NOT NULL,
    seq             BIGSERIAL UNIQUE NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_cpls_community ON community_places_log (community_id, signed_at DESC);
CREATE INDEX IF NOT EXISTS idx_cpls_seq       ON community_places_log (seq);

CREATE TABLE IF NOT EXISTS community_requests_log (
    signature_hash  TEXT      PRIMARY KEY,
    community_id    TEXT      NOT NULL,
    request_id      TEXT      NOT NULL,
    status          TEXT      NOT NULL,
    signer          TEXT      NOT NULL,
    signed_at       BIGINT    NOT NULL,
    received_at     BIGINT    NOT NULL,
    seq             BIGSERIAL UNIQUE NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_crql_community ON community_requests_log (community_id, signed_at DESC);
CREATE INDEX IF NOT EXISTS idx_crql_seq       ON community_requests_log (seq);

CREATE TABLE IF NOT EXISTS seen_nonces (
    signer      TEXT   NOT NULL,
    nonce       TEXT   NOT NULL,
    expires_at  BIGINT NOT NULL,
    PRIMARY KEY (signer, nonce)
);

CREATE INDEX IF NOT EXISTS idx_seen_nonces_expires ON seen_nonces (expires_at);

CREATE OR REPLACE FUNCTION crl_apply_to_current() RETURNS TRIGGER AS $$
DECLARE
    prev_sig    TEXT;
    prev_signed BIGINT;
    keep_prev   BOOLEAN := FALSE;
BEGIN
    SELECT last_sig_hash, effective_since INTO prev_sig, prev_signed
        FROM community_role_current
        WHERE community_id = NEW.community_id AND member = NEW.target;

    IF FOUND THEN
        IF prev_signed > NEW.signed_at THEN
            keep_prev := TRUE;
        ELSIF prev_signed = NEW.signed_at AND prev_sig < NEW.signature_hash THEN
            keep_prev := TRUE;
        END IF;
    END IF;

    IF NOT keep_prev THEN
        INSERT INTO community_role_current (community_id, member, role, effective_since, last_sig_hash)
        VALUES (NEW.community_id, NEW.target, NEW.role, NEW.signed_at, NEW.signature_hash)
        ON CONFLICT (community_id, member) DO UPDATE
            SET role = EXCLUDED.role,
                effective_since = EXCLUDED.effective_since,
                last_sig_hash = EXCLUDED.last_sig_hash;
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS crl_apply_trg ON community_role_log;
CREATE TRIGGER crl_apply_trg
    AFTER INSERT ON community_role_log
    FOR EACH ROW EXECUTE FUNCTION crl_apply_to_current();
