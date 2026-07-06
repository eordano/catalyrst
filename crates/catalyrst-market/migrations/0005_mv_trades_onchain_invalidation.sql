-- catalyrst-market: on-chain trade invalidation for mv_trades (Marketplace v3).
--
-- Audit gap #11. Migration 0004 derived `status` only from catalyrst's local
-- federation log (market_cancellations + expiry → cancelled, market_trades_local
-- executions → sold) and explicitly COLLAPSED the on-chain
-- signature-index-invalidation path. That diverges from upstream
-- marketplace-server's `mv_trades` (src/logic/trades/materialized-view.ts), whose
-- `status` CASE flips a trade out of `open` the instant it is invalidated
-- on-chain via the `squid_trades.{trade,signature_index}` indexer.
--
-- This migration restores the three missing on-chain invalidation branches,
-- byte-faithful to the upstream view:
--   (a) squid_trades.trade.action = 'cancelled'  — an explicit on-chain cancel
--       of the trade's signature.
--   (b) signer signature_index nonce bump — the signer rotated their per-account
--       signature index, invalidating every trade signed with the old index.
--   (c) marketplace-contract signature_index nonce bump — the off-chain
--       marketplace contract rotated its global signature index, invalidating
--       every trade on that network.
-- All three resolve to `status = 'cancelled'`, exactly as upstream. The
-- `squid_trades.trade.action = 'executed'` count also feeds the `sold` branch.
--
-- `squid_trades` is the table set produced by decentraland's trades-squid-core
-- indexer (schema.graphql Trade/SignatureIndex; migration 1728499832070-Data.js).
-- It is created here with the identical column set/types if the migration role is
-- allowed to; in a real deployment the indexer owns it. The MV joins it with
-- LEFT JOINs, so against an unpopulated index the branches behave exactly as
-- upstream's do:
--   * st NULL          → branch (a) never fires.
--   * si_signer NULL   → branch (b) fires iff signerSignatureIndex != 0.
--   * si_contract NULL → branch (c) fires iff contractSignatureIndex != 0.
-- i.e. a freshly-signed trade (both indices == its current on-chain index) stays
-- `open`, and the instant a real cancellation / nonce-bump row lands the trade
-- flips to `cancelled` on the next refresh — the exact upstream semantics, no
-- approximation.
--
-- Robustness: the catalyrst migration role may lack CREATE-schema privilege (the
-- indexer provisions `squid_trades` elsewhere). The whole thing runs inside a
-- single PL/pgSQL block that (1) best-effort-creates `squid_trades`, catching
-- insufficient_privilege exactly as upstream's recreateTradesMaterializedView
-- does, and (2) recreates `mv_trades` with the on-chain branches ONLY when both
-- squid_trades tables are reachable; otherwise it leaves migration 0004's
-- local-source view in place. The view therefore always exists with the correct
-- column set, and gains the on-chain branches as soon as the indexer schema is
-- available.

DO $mig$
BEGIN
    -- Best-effort provision of the trades-squid-core schema + tables. Mirrors the
    -- exception handling upstream uses around privileged DDL.
    BEGIN
        CREATE SCHEMA IF NOT EXISTS squid_trades;

        -- decentraland/trades-squid-core 1728499832070-Data.js, verbatim columns.
        CREATE TABLE IF NOT EXISTS squid_trades.trade (
            id                   character varying NOT NULL PRIMARY KEY,
            signature            text NOT NULL,
            network              character varying(8) NOT NULL,
            action               character varying(9) NOT NULL,
            "timestamp"          numeric,
            caller               text NOT NULL,
            tx_hash              text NOT NULL,
            sent_beneficiary     text,
            received_beneficiary text
        );
        CREATE INDEX IF NOT EXISTS idx_squid_trades_trade_signature
            ON squid_trades.trade (signature);

        CREATE TABLE IF NOT EXISTS squid_trades.signature_index (
            id       character varying NOT NULL PRIMARY KEY,
            address  text NOT NULL,
            network  character varying(8) NOT NULL,
            "index"  integer NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_squid_trades_signature_index_address
            ON squid_trades.signature_index (address);
    EXCEPTION WHEN insufficient_privilege THEN
        RAISE NOTICE 'Insufficient privilege to provision squid_trades; relying on indexer-provisioned schema';
    END;

    -- Only recreate the view with the on-chain branches if both source tables are
    -- reachable; otherwise keep migration 0004's local-source view untouched.
    IF to_regclass('squid_trades.trade') IS NOT NULL
       AND to_regclass('squid_trades.signature_index') IS NOT NULL THEN

        DROP MATERIALIZED VIEW IF EXISTS marketplace.mv_trades;

        CREATE MATERIALIZED VIEW marketplace.mv_trades AS
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
                -- (a) explicit on-chain cancellation of this trade's signature, OR a
                -- catalyrst-local signed cancellation targeting its hashed_signature.
                WHEN COUNT(CASE WHEN st.action = 'cancelled' THEN 1 END) > 0             THEN 'cancelled'
                WHEN canc.cancellations > 0                                              THEN 'cancelled'
                -- expiry.
                WHEN t.expires_at < now()::timestamptz(3)                                THEN 'cancelled'
                -- (b) signer signature_index nonce bump.
                WHEN (
                    (si_signer.index IS NOT NULL
                        AND si_signer.index != (t.checks ->> 'signerSignatureIndex')::int)
                    OR (si_signer.index IS NULL
                        AND (t.checks ->> 'signerSignatureIndex')::int != 0)
                    )                                                                    THEN 'cancelled'
                -- (c) marketplace-contract signature_index nonce bump.
                WHEN (
                    (si_contract.index IS NOT NULL
                        AND si_contract.index != (t.checks ->> 'contractSignatureIndex')::int)
                    OR (si_contract.index IS NULL
                        AND (t.checks ->> 'contractSignatureIndex')::int != 0)
                    )                                                                    THEN 'cancelled'
                -- sold: on-chain executions reached the trade's `uses` allowance, OR
                -- the catalyrst-local execution log did.
                WHEN COUNT(DISTINCT st.id) FILTER (WHERE st.action = 'executed') >= (t.checks ->> 'uses')::int
                                                                                        THEN 'sold'
                WHEN exec.executions >= (t.checks ->> 'uses')::int                      THEN 'sold'
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
        -- (a) on-chain trade actions for this signature (cancelled / executed).
        LEFT JOIN squid_trades.trade AS st
            ON st.signature = t.hashed_signature
        -- (b) the signer's current on-chain signature index.
        LEFT JOIN squid_trades.signature_index AS si_signer
            ON LOWER(si_signer.address) = LOWER(t.signer)
        -- (c) the off-chain marketplace contract's current signature index for the
        -- trade's network. Addresses are the OffChainMarketplace{,V2}
        -- {Polygon,Ethereum} mainnet contracts from decentraland-transactions.
        -- Upstream embeds them via getContract(...).address; both squid-indexed
        -- addresses and these literals are compared lowercased — identical to the
        -- live getTrades query (ports/trades/queries.ts, which `.toLowerCase()`s
        -- all four). marketplace-server's materialized-view.ts happens to leave the
        -- mixed-case Polygon-V1 literal un-lowercased (so that one branch never
        -- fires against lowercase index rows); we match the semantically-correct
        -- queries.ts path so every contract-nonce bump genuinely invalidates,
        -- which is the whole point of this branch.
        LEFT JOIN (
            SELECT *
            FROM squid_trades.signature_index idx
            WHERE LOWER(idx.address) IN (
                '0x540fb08edb56aae562864b390542c97f562825ba',
                '0x2d6b3508f9aca32d2550f92b2addba932e73c1ff',
                '0xa40b1d129b8906888720686f3a01921ddf37716f',
                '0x1b67d0e31eeb6b52d8eeed71d3616c2f5b33b8e7'
            )
        ) AS si_contract
            ON t.network = si_contract.network
        -- catalyrst-local execution count (federated TradeRecord log), keyed by the
        -- order's signature hash; feeds the `sold` branch alongside on-chain.
        LEFT JOIN (
            SELECT order_signature_hash AS hashed_signature, COUNT(*) AS executions
            FROM marketplace.market_trades_local
            GROUP BY order_signature_hash
        ) AS exec ON exec.hashed_signature = t.hashed_signature
        -- catalyrst-local signed-cancellation count, keyed by the target's signature
        -- hash; feeds the `cancelled` branch alongside on-chain cancellations.
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
            si_contract.index,
            si_signer.index,
            exec.executions,
            canc.cancellations;

        CREATE UNIQUE INDEX IF NOT EXISTS idx_mv_trades_id ON marketplace.mv_trades (id);
        CREATE INDEX IF NOT EXISTS idx_mv_trades_status_type ON marketplace.mv_trades (status, type);
        CREATE INDEX IF NOT EXISTS idx_mv_trades_created_at ON marketplace.mv_trades (created_at DESC);
        CREATE INDEX IF NOT EXISTS idx_mv_trades_category ON marketplace.mv_trades (sent_nft_category);
        CREATE INDEX IF NOT EXISTS idx_mv_trades_contract_token ON marketplace.mv_trades (contract_address_sent, sent_token_id);
    ELSE
        RAISE NOTICE 'squid_trades.{trade,signature_index} unavailable; keeping 0004 local-source mv_trades';
    END IF;
END
$mig$;
