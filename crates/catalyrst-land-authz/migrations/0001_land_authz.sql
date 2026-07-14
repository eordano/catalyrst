-- The schema itself is created by the deployment's land-authz bootstrap.
-- Do not add CREATE SCHEMA here: this role holds CREATE on the schema but not
-- on the database, and even the IF NOT EXISTS form demands the latter.
CREATE TABLE IF NOT EXISTS land_authz.authz_event (
    block_number  BIGINT   NOT NULL,
    log_index     INTEGER  NOT NULL,
    block_time    BIGINT   NOT NULL,
    token_address TEXT     NOT NULL,
    kind          TEXT     NOT NULL,
    token_id      NUMERIC,
    account       TEXT,
    operator      TEXT,
    approved      BOOLEAN,
    PRIMARY KEY (block_number, log_index)
);

CREATE INDEX IF NOT EXISTS authz_event_kind_idx
    ON land_authz.authz_event (token_address, kind, block_number, log_index);

-- Per-token rights. `operator` comes from Approval, `update_operator` from
-- UpdateOperator; a Transfer of the token clears both, which is why Transfer
-- logs are indexed even though they grant nothing themselves.
CREATE TABLE IF NOT EXISTS land_authz.token_right (
    token_address   TEXT    NOT NULL,
    token_id        NUMERIC NOT NULL,
    x               INTEGER,
    y               INTEGER,
    operator        TEXT,
    update_operator TEXT,
    updated_block   BIGINT  NOT NULL,
    updated_log     INTEGER NOT NULL,
    PRIMARY KEY (token_address, token_id)
);

-- The reverse direction ("which parcels may this address update") is a first
-- class index, not a scan: the lands-permissions route is a lookup by operator.
CREATE INDEX IF NOT EXISTS token_right_update_operator_idx
    ON land_authz.token_right (update_operator)
    WHERE update_operator IS NOT NULL;

CREATE INDEX IF NOT EXISTS token_right_operator_idx
    ON land_authz.token_right (operator)
    WHERE operator IS NOT NULL;

CREATE INDEX IF NOT EXISTS token_right_xy_idx
    ON land_authz.token_right (x, y)
    WHERE x IS NOT NULL;

-- Account-wide rights, scoped per registry: UpdateManager and ApprovalForAll
-- authorise an operator over everything an account holds in one contract, so
-- they key on (registry, account, operator) and never on a token.
CREATE TABLE IF NOT EXISTS land_authz.account_right (
    token_address TEXT    NOT NULL,
    account       TEXT    NOT NULL,
    operator      TEXT    NOT NULL,
    kind          TEXT    NOT NULL,
    is_approved   BOOLEAN NOT NULL,
    updated_block BIGINT  NOT NULL,
    updated_log   INTEGER NOT NULL,
    PRIMARY KEY (token_address, account, operator, kind)
);

CREATE INDEX IF NOT EXISTS account_right_account_idx
    ON land_authz.account_right (account, token_address, kind)
    WHERE is_approved;

CREATE INDEX IF NOT EXISTS account_right_operator_idx
    ON land_authz.account_right (operator, kind)
    WHERE is_approved;

-- LAND packs its signed coordinates into the two 128-bit halves of the token
-- id. Keeping the unpacking here means the fold, the reverse lookup and any
-- ad-hoc query all read coordinates the same way the contract wrote them.
CREATE OR REPLACE FUNCTION land_authz.decode_coord(half NUMERIC) RETURNS INTEGER
LANGUAGE sql IMMUTABLE AS $$
  SELECT (CASE WHEN half >= 170141183460469231731687303715884105728::numeric
               THEN half - 340282366920938463463374607431768211456::numeric
               ELSE half END)::INTEGER
$$;

CREATE OR REPLACE FUNCTION land_authz.token_x(token_id NUMERIC) RETURNS INTEGER
LANGUAGE sql IMMUTABLE AS $$
  SELECT land_authz.decode_coord(div(token_id, 340282366920938463463374607431768211456::numeric))
$$;

CREATE OR REPLACE FUNCTION land_authz.token_y(token_id NUMERIC) RETURNS INTEGER
LANGUAGE sql IMMUTABLE AS $$
  SELECT land_authz.decode_coord(mod(token_id, 340282366920938463463374607431768211456::numeric))
$$;

CREATE TABLE IF NOT EXISTS land_authz.index_cursor (
    id         TEXT PRIMARY KEY,
    last_block BIGINT      NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
