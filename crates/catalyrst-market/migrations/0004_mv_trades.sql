-- catalyrst-market off-chain trades materialized view (Marketplace v3).
--
-- This is the catalyrst port of marketplace-server's
-- src/logic/trades/materialized-view.ts `mv_trades`. It powers the price /
-- isOnSale / trade* columns and the isOnSale/minPrice/maxPrice filters for
-- every Marketplace-v3 off-chain listing surfaced by /v1/items and the catalog.
-- Without it those columns are dead (price 0, isOnSale false, trade* NULL).
--
-- WIRE/SEMANTIC PARITY with the upstream view:
--   * Same column set and types (id, created_at, type, signer,
--     contract_address_sent, amount_received, available, assets jsonb,
--     sent_contract_address, sent_token_id, sent_nft_category, sent_item_id,
--     sent_nft_id, network, expires_at, trade_contract, status).
--   * Same `assets` json object: one key per direction ('sent' / 'received'),
--     each carrying contract_address/direction/beneficiary/extra/token_id/
--     item_id/amount/creator/owner/category/nft_id/issued_id/nft_name.
--   * `amount_received` = the erc20 amount of the received side.
--   * `available`       = the squid item's available supply (NULL for nft orders
--     that do not join an item — handled downstream by `available IS NULL OR
--     available > 0`).
--   * trades_owner_ok: only surface a trade whose every `sent` ERC721 asset is
--     still owned by the signer (stale-listing guard, identical to upstream).
--
-- STATUS, adapted to catalyrst's data model:
--   Upstream derives status from the on-chain squid_trades.{trade,signature_index}
--   schema (cancelled / sold / signature-index invalidation). That schema is not
--   indexed in the catalyrst squid mirror. catalyrst instead tracks the same two
--   reachable lifecycle transitions in its own wallet-signed federation log:
--     * cancelled — the trade expired (expires_at < now()), OR a signed
--       cancellation targeting this trade's hashed_signature was recorded in
--       marketplace.market_cancellations.
--     * sold      — the number of recorded executions in
--       marketplace.market_trades_local (keyed by order_signature_hash) reached
--       the trade's `uses` allowance.
--     * open      — otherwise.
--   The signature-index-invalidation branch (signer/contract nonce bumps) has no
--   catalyrst data source, so it collapses into the cancellation/expiry path,
--   which covers every state reachable through the catalyrst write path.
--
-- Refreshed out-of-band by the catalyrst-market refresh task (see lib.rs). The
-- unique index on (id) lets that refresh run CONCURRENTLY once primed.

CREATE MATERIALIZED VIEW IF NOT EXISTS marketplace.mv_trades AS
WITH trades_owner_ok AS (
    SELECT t.id
    FROM marketplace.trades t
    JOIN marketplace.trade_assets ta ON t.id = ta.trade_id
    LEFT JOIN marketplace.trade_assets_erc721 erc721_asset ON ta.id = erc721_asset.asset_id
    LEFT JOIN squid_marketplace.nft nft
        ON ta.contract_address = nft.contract_address
        AND ta.direction = 'sent'
        AND nft.token_id = erc721_asset.token_id::numeric
    WHERE t.type IN ('public_item_order', 'public_nft_order')
    GROUP BY t.id
    HAVING bool_and(ta.direction != 'sent' OR nft.owner_address = t.signer)
)
SELECT
    t.id,
    t.created_at,
    t.type::text AS type,
    t.signer,
    MAX(CASE WHEN av.direction = 'sent'     THEN av.contract_address END) AS contract_address_sent,
    MAX(CASE WHEN av.direction = 'received' THEN av.amount END)          AS amount_received,
    MAX(CASE WHEN av.direction = 'sent'     THEN av.available END)       AS available,
    json_object_agg(
        av.direction,
        json_build_object(
            'contract_address', av.contract_address,
            'direction',        av.direction,
            'beneficiary',      av.beneficiary,
            'extra',            av.extra,
            'token_id',         av.token_id,
            'item_id',          av.item_id,
            'amount',           av.amount,
            'creator',          av.creator,
            'owner',            av.nft_owner,
            'category',         av.category,
            'nft_id',           av.nft_id,
            'issued_id',        av.issued_id,
            'nft_name',         av.nft_name
        )
    ) AS assets,
    MAX(av.contract_address) FILTER (WHERE av.direction = 'sent') AS sent_contract_address,
    MAX(av.token_id)         FILTER (WHERE av.direction = 'sent') AS sent_token_id,
    MAX(av.category)         FILTER (WHERE av.direction = 'sent') AS sent_nft_category,
    MAX(av.item_id)          FILTER (WHERE av.direction = 'sent') AS sent_item_id,
    MAX(av.nft_id)           FILTER (WHERE av.direction = 'sent') AS sent_nft_id,
    t.network,
    t.expires_at,
    MAX(t.contract) AS trade_contract,
    CASE
        WHEN exec.executions >= (t.checks ->> 'uses')::int                       THEN 'sold'
        WHEN canc.cancellations > 0                                              THEN 'cancelled'
        WHEN t.expires_at < now()::timestamptz(3)                                THEN 'cancelled'
        ELSE 'open'
    END AS status
FROM marketplace.trades AS t
JOIN trades_owner_ok    AS ok ON t.id = ok.id
JOIN (
    SELECT
        ta.id,
        ta.trade_id,
        ta.contract_address,
        ta.direction::text AS direction,
        ta.beneficiary,
        ta.extra,
        erc721_asset.token_id,
        erc20_asset.amount,
        item.creator,
        item.available,
        nft.owner_address      AS nft_owner,
        nft.category,
        nft.id                 AS nft_id,
        nft.issued_id          AS issued_id,
        nft.name               AS nft_name,
        coalesce(nft.item_blockchain_id::text, item_asset.item_id) AS item_id
    FROM marketplace.trade_assets AS ta
    LEFT JOIN marketplace.trade_assets_erc721 AS erc721_asset
        ON ta.id = erc721_asset.asset_id
    LEFT JOIN marketplace.trade_assets_erc20 AS erc20_asset
        ON ta.id = erc20_asset.asset_id
    LEFT JOIN marketplace.trade_assets_item AS item_asset
        ON ta.id = item_asset.asset_id
    LEFT JOIN squid_marketplace.item AS item
        ON ta.contract_address = item.collection_id
        AND item_asset.item_id::numeric = item.blockchain_id
    LEFT JOIN squid_marketplace.nft AS nft
        ON ta.contract_address = nft.contract_address
        AND erc721_asset.token_id::numeric = nft.token_id
) AS av ON t.id = av.trade_id
LEFT JOIN (
    SELECT order_signature_hash AS hashed_signature, COUNT(*) AS executions
    FROM marketplace.market_trades_local
    GROUP BY order_signature_hash
) AS exec ON exec.hashed_signature = t.hashed_signature
LEFT JOIN (
    SELECT target_signature_hash AS hashed_signature, COUNT(*) AS cancellations
    FROM marketplace.market_cancellations
    GROUP BY target_signature_hash
) AS canc ON canc.hashed_signature = t.hashed_signature
WHERE t.type IN ('public_item_order', 'public_nft_order')
GROUP BY
    t.id,
    t.type,
    t.created_at,
    t.network,
    t.chain_id,
    t.signer,
    t.checks,
    t.expires_at,
    exec.executions,
    canc.cancellations;

CREATE UNIQUE INDEX IF NOT EXISTS idx_mv_trades_id ON marketplace.mv_trades (id);
CREATE INDEX IF NOT EXISTS idx_mv_trades_status_type ON marketplace.mv_trades (status, type);
CREATE INDEX IF NOT EXISTS idx_mv_trades_created_at ON marketplace.mv_trades (created_at DESC);
CREATE INDEX IF NOT EXISTS idx_mv_trades_category ON marketplace.mv_trades (sent_nft_category);
CREATE INDEX IF NOT EXISTS idx_mv_trades_contract_token ON marketplace.mv_trades (contract_address_sent, sent_token_id);
