-- catalyrst-market favorites store (user lists / picks).
--
-- A minimal port of the read surface of decentraland/marketplace-favorites-server
-- (the service that backs `GET /v1/lists`). The tables live in the `favorites`
-- schema — owned by the marketplace-writer role this migration runs as — kept
-- separate from both the upstream-owned `squid_marketplace` read schema and the
-- `marketplace` federation/admin schema.
--
-- READ ONLY is wired today: `GET /v1/lists` reads these tables (returning an
-- empty result until the write path exists). Writes are wallet-scoped — a list
-- is created and picked into by the EIP-712-signed list owner — and are
-- intentionally NOT implemented here: they depend on the federation auth path
-- (see the auth gap noted in the handler). The tables are created empty and the
-- endpoint degrades gracefully (empty `results`, `total = 0`) so nothing 500s.
--
-- Additive only; no existing read/write path changes behavior.

CREATE TABLE IF NOT EXISTS favorites.lists (
    id            uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
    name          text        NOT NULL,
    description   text,
    user_address  varchar(42) NOT NULL,
    created_at    timestamptz NOT NULL DEFAULT now(),
    updated_at    timestamptz,
    is_private    boolean     NOT NULL DEFAULT false,
    permission    text
);

CREATE INDEX IF NOT EXISTS lists_user_address_idx ON favorites.lists (user_address);

-- One row per item picked into a list. `item_id` is the squid item id
-- (contract-blockchainId) the favorite points at.
CREATE TABLE IF NOT EXISTS favorites.picks (
    item_id       text        NOT NULL,
    user_address  varchar(42) NOT NULL,
    list_id       uuid        NOT NULL REFERENCES favorites.lists(id) ON DELETE CASCADE,
    created_at    timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (item_id, list_id)
);

CREATE INDEX IF NOT EXISTS picks_list_id_idx ON favorites.picks (list_id);
