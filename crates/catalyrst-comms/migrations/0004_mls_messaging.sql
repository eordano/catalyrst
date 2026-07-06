-- MLS (RFC 9420) end-to-end-encrypted messaging — DELIVERY SERVICE state.
--
-- ADR: docs/federation/messaging.md (§5 storage model, §6 signed schemas, §7
-- propagation). This catalyst is the MLS *delivery service*, NOT a group
-- member: it stores and routes OPAQUE MLS bytes (KeyPackages, Welcome, Commit,
-- application ciphertext) and never holds group secrets, so it can never
-- decrypt. Every BYTEA column below is ciphertext or MLS handshake material
-- the server cannot read.
--
-- NEVER store or log plaintext: there is no plaintext column in this schema.

-- ---------------------------------------------------------------------------
-- Key-package directory. One identity (wallet) publishes one-or-more single-use
-- KeyPackages so others can add it to MLS groups while it is offline. The
-- server hands out (and consumes) these on `add_member`. Bytes are the verbatim
-- TLS-serialised `MLSMessage(KeyPackage)` produced by the client.
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS mls_key_packages (
    id            BIGINT GENERATED ALWAYS AS IDENTITY,
    owner         VARCHAR  NOT NULL,             -- lowercase wallet (the credential identity)
    ref_hash      VARCHAR  NOT NULL,             -- sha256(key_package_bytes) hex; client-resolvable handle
    ciphersuite   INTEGER  NOT NULL,             -- RFC 9420 ciphersuite id (u16); must be 0x0001
    key_package   BYTEA    NOT NULL,             -- opaque MLSMessage(KeyPackage) TLS bytes
    consumed_at   TIMESTAMPTZ,                   -- set when claimed for an add; single-use
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (id)
);
CREATE UNIQUE INDEX IF NOT EXISTS uq_kp_ref ON mls_key_packages (ref_hash);
CREATE INDEX IF NOT EXISTS idx_kp_owner_unconsumed
    ON mls_key_packages (owner) WHERE consumed_at IS NULL;

-- ---------------------------------------------------------------------------
-- Groups. A DM is an MLS group of size 2; a community/world channel is size N.
-- The server tracks routing metadata only (creator, kind, epoch-author,
-- current epoch) — never the group's ratchet/secret state.
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS mls_groups (
    group_id          VARCHAR PRIMARY KEY,        -- hex of the 32-byte MLS GroupId
    creator           VARCHAR NOT NULL,           -- lowercase wallet of the creator
    group_kind        VARCHAR NOT NULL,           -- 'dm' | 'channel'
    community_id      VARCHAR,                     -- set when group_kind='channel'
    epoch_author      VARCHAR NOT NULL,           -- catalyst/peer id holding epoch-author role
    current_epoch     BIGINT  NOT NULL DEFAULT 0,
    ciphersuite       INTEGER NOT NULL,           -- pinned at create; must match members' KPs
    last_commit_hash  VARCHAR,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (group_kind IN ('dm', 'channel'))
);

-- Membership roster — used ONLY for authorization (who may fetch history /
-- who receives fan-out). It is NOT the MLS tree; the cryptographic membership
-- lives client-side inside the encrypted group state.
CREATE TABLE IF NOT EXISTS mls_group_members (
    group_id   VARCHAR NOT NULL REFERENCES mls_groups(group_id) ON DELETE CASCADE,
    member     VARCHAR NOT NULL,                  -- lowercase wallet
    added_epoch BIGINT NOT NULL,
    removed_epoch BIGINT,                          -- NULL while active
    PRIMARY KEY (group_id, member)
);
CREATE INDEX IF NOT EXISTS idx_member_active
    ON mls_group_members (member) WHERE removed_epoch IS NULL;

-- ---------------------------------------------------------------------------
-- Handshake history: one row per epoch advance. commit_bytes is the opaque MLS
-- Commit; welcome_bytes the opaque Welcome for members added in this epoch.
-- Peers that fall behind fetch these by `from=<epoch>` (ADR §4, §6 GroupCommit).
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS mls_commits (
    group_id      VARCHAR NOT NULL REFERENCES mls_groups(group_id) ON DELETE CASCADE,
    epoch         BIGINT  NOT NULL,
    commit_bytes  BYTEA   NOT NULL,               -- opaque MLSMessage(Commit)
    welcome_bytes BYTEA,                           -- opaque MLSMessage(Welcome), nullable
    committer     VARCHAR NOT NULL,               -- wallet that authored the commit
    commit_hash   VARCHAR NOT NULL,               -- sha256(commit_bytes) hex
    signed_at     BIGINT  NOT NULL,
    received_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (group_id, epoch)
);

-- ---------------------------------------------------------------------------
-- Application messages. Split content-addressed (ADR §5): message_refs is the
-- ordered, signed index; message_blobs holds the dedup'd ciphertext. The
-- ciphertext is an opaque MLSMessage(PrivateMessage) — undecryptable here.
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS mls_message_blobs (
    ciphertext_hash VARCHAR PRIMARY KEY,          -- sha256(ciphertext) hex
    ciphertext      BYTEA   NOT NULL,             -- opaque MLSMessage(PrivateMessage)
    received_at     TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS mls_message_refs (
    signature_hash  VARCHAR PRIMARY KEY,          -- Signed<MessageRef>::hash() hex
    group_id        VARCHAR NOT NULL REFERENCES mls_groups(group_id) ON DELETE CASCADE,
    author          VARCHAR NOT NULL,             -- lowercase wallet
    epoch           BIGINT  NOT NULL,
    ciphertext_hash VARCHAR NOT NULL REFERENCES mls_message_blobs(ciphertext_hash),
    signed_at       BIGINT  NOT NULL,
    received_at     TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_mr_group_time ON mls_message_refs (group_id, received_at DESC);
