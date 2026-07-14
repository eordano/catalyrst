-- catalyrst-market: track trades-squid-core's per-deployment signature indexes.
--
-- decentraland/trades-squid-core c701a95 ("index off-chain marketplace v3")
-- changes two things in the schema migration 0005 stubs for schema compatibility:
--
--   * signature_index gains `contract text NOT NULL` and its row identity moves
--     from (address, network) to (address, contract, network). Both counters the
--     marketplace keeps live in that table -- its own contractSignatureIndex and
--     signerSignatureIndex(signer) -- and both are storage on one deployment, so
--     V1, V2 and V3 each keep their own, all starting at zero.
--   * trade gains a nullable `trade_digest`, the EIP-712 digest V3 keys
--     cancellations on instead of keccak256 of the raw signature bytes.
--
-- Upstream states the pairing: the producer writing per-deployment rows and the
-- consumer matching them on the trade's own contract are only safe together.
-- 0005 shipped the consumer half that is now stale -- si_signer joined on the
-- signer address alone and si_contract on network alone via a hardcoded
-- four-address whitelist. One row per deployment turns both into a fan-out: with
-- si_signer.index and si_contract.index in the GROUP BY, a trade emits one row
-- per matching index row and CREATE UNIQUE INDEX ... (id) then fails, taking the
-- whole view build down. V3 need not be adopted for that to happen; V1 and V2
-- coexisting on a network is enough.
--
-- 0005 is applied and immutable, so this re-provisions the two stub tables with
-- the missing columns and recreates the view with both joins rescoped to
-- t.contract. Same column set, same status semantics, one row per trade again.
--
-- trade_digest is provisioned for column parity only. Correlating a V3
-- cancellation back to its trade through the digest is not wired into the status
-- CASE: catalyrst signs V2 trades exclusively (ports/trades/contracts.rs), so no
-- trade this view covers is cancellable by digest yet.
--
-- Provisioning keeps 0005's best-effort posture (the migration role may not own
-- squid_trades) but is split in two, so one half failing does not undo the other:
-- ADD COLUMN ... contract NOT NULL raises not_null_violation against a populated
-- indexer-owned signature_index, and inside a single block that rollback took
-- squid_trades.trade.trade_digest down with it.
--
-- Which view gets built follows what is actually reachable. With the `contract`
-- column, the contract-scoped view below. Without it, 0005's view rebuilt with one
-- correction: 0005 matched si_signer on the signer address alone, so a signer
-- holding different counters on ETHEREUM and POLYGON fans one trade into two rows
-- and the unique index on (id) rejects the build -- latent only while the two
-- counters happen to be equal, and lib.rs then falls back from a failed CONCURRENT
-- refresh to a plain one, which succeeds and serves the duplicates. The rebuild
-- scopes that join by network. si_contract keeps 0005's four-address whitelist
-- joined on network alone, which retains the same hazard on a network whose V1 and
-- V2 deployments both hold a bumped contract index; rescoping it would change which
-- counter rows with an empty trades.contract compare against, so it stays as 0005
-- shipped it. With neither squid_trades table reachable nothing is rebuilt and
-- whichever view is in place stays.

DO $mig$
BEGIN
    BEGIN
        CREATE SCHEMA IF NOT EXISTS squid_trades;

        BEGIN
            CREATE TABLE IF NOT EXISTS squid_trades.trade (
                id                   character varying NOT NULL PRIMARY KEY,
                signature            text NOT NULL,
                trade_digest         text,
                network              character varying(8) NOT NULL,
                action               character varying(9) NOT NULL,
                "timestamp"          numeric,
                caller               text NOT NULL,
                tx_hash              text NOT NULL,
                sent_beneficiary     text,
                received_beneficiary text
            );
            ALTER TABLE squid_trades.trade
                ADD COLUMN IF NOT EXISTS trade_digest text;
            CREATE INDEX IF NOT EXISTS idx_squid_trades_trade_signature
                ON squid_trades.trade (signature);
            CREATE INDEX IF NOT EXISTS idx_squid_trades_trade_trade_digest
                ON squid_trades.trade (trade_digest);
        EXCEPTION WHEN insufficient_privilege THEN
            RAISE NOTICE 'Could not provision squid_trades.trade; relying on the indexer-provisioned schema';
        END;

        -- Its own block: `contract` is NOT NULL with no default, exactly as upstream
        -- 1787322770919-Data.js, which expects an empty table. Against a populated
        -- indexer-owned one it raises not_null_violation, and sharing a block with the
        -- statements above would roll those back too.
        BEGIN
            CREATE TABLE IF NOT EXISTS squid_trades.signature_index (
                id       character varying NOT NULL PRIMARY KEY,
                address  text NOT NULL,
                contract text NOT NULL,
                network  character varying(8) NOT NULL,
                "index"  integer NOT NULL
            );
            ALTER TABLE squid_trades.signature_index
                ADD COLUMN IF NOT EXISTS contract text NOT NULL;
            CREATE INDEX IF NOT EXISTS idx_squid_trades_signature_index_address
                ON squid_trades.signature_index (address);
            CREATE INDEX IF NOT EXISTS idx_squid_trades_signature_index_contract
                ON squid_trades.signature_index (contract);
        EXCEPTION WHEN insufficient_privilege OR not_null_violation THEN
            RAISE NOTICE 'Could not add squid_trades.signature_index.contract; rebuilding the 0005-shaped view instead';
        END;
    EXCEPTION WHEN insufficient_privilege THEN
        RAISE NOTICE 'Insufficient privilege to provision the squid_trades schema; relying on the indexer-provisioned schema';
    END;

    IF to_regclass('squid_trades.trade') IS NOT NULL
       AND to_regclass('squid_trades.signature_index') IS NOT NULL
       AND EXISTS (
           SELECT 1
           FROM information_schema.columns
           WHERE table_schema = 'squid_trades'
             AND table_name = 'signature_index'
             AND column_name = 'contract'
       ) THEN

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
        -- (b) the signer's current on-chain signature index, scoped to the
        -- marketplace deployment that holds it. trades-squid-core c701a95 made
        -- `contract` part of a signature_index row's identity: signerSignatureIndex
        -- is storage on each deployment, so V1/V2/V3 keep independent counters for
        -- the same signer and each starts at zero. Matching on the signer alone now
        -- pairs a trade with one row per deployment; si_signer.index is in the GROUP
        -- BY, so that becomes one mv_trades row per deployment for a single trade id
        -- and the unique index on (id) rejects the build outright.
        --
        -- The network predicate maps marketplace.trades.network ('MATIC') onto the
        -- squid Network enum ('POLYGON'). It cannot be dropped: the same marketplace
        -- address is deployed on both Ethereum and Amoy, so the contract alone does
        -- not pin down one row.
        LEFT JOIN squid_trades.signature_index AS si_signer
            ON LOWER(si_signer.address) = LOWER(t.signer)
            AND LOWER(si_signer.contract) = LOWER(t.contract)
            AND si_signer.network = CASE WHEN t.network = 'MATIC' THEN 'POLYGON' ELSE t.network END
        -- (c) the signature index of the exact marketplace deployment this trade was
        -- signed against, the counter whose subject and holder are the same contract.
        -- This replaces the four-address whitelist joined on network alone: upstream
        -- now writes one contract row per deployment, so a network-only join fans out
        -- exactly the way (b) does, and the whitelist would need editing for every
        -- new marketplace version. marketplace.trades.contract carries the verifying
        -- contract from the trade's EIP-712 domain, which is by construction the
        -- deployment whose contractSignatureIndex the signer read.
        LEFT JOIN squid_trades.signature_index AS si_contract
            ON LOWER(si_contract.address) = LOWER(t.contract)
            AND LOWER(si_contract.contract) = LOWER(t.contract)
            AND si_contract.network = CASE WHEN t.network = 'MATIC' THEN 'POLYGON' ELSE t.network END
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

    ELSIF to_regclass('squid_trades.trade') IS NOT NULL
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
        -- (b) the signer's current on-chain signature index, scoped to the network
        -- the trade was signed on. 0005 matched the signer address alone, so a signer
        -- holding a counter on both networks joined twice and si_signer.index in the
        -- GROUP BY split one trade into two mv_trades rows, which the unique index on
        -- (id) then rejects. Equal counters collapse in the GROUP BY, which is why it
        -- stays latent until they diverge. marketplace.trades.network says 'MATIC'
        -- where the squid Network enum says 'POLYGON'; under this schema's
        -- (address, network) row identity the predicate leaves at most one row.
        LEFT JOIN squid_trades.signature_index AS si_signer
            ON LOWER(si_signer.address) = LOWER(t.signer)
            AND si_signer.network = CASE WHEN t.network = 'MATIC' THEN 'POLYGON' ELSE t.network END
        -- (c) the off-chain marketplace contract's current signature index for the
        -- trade's network. Addresses are the OffChainMarketplace{,V2}
        -- {Polygon,Ethereum} mainnet contracts from decentraland-transactions.
        -- Upstream embeds them via getContract(...).address; both squid-indexed
        -- addresses and these literals are compared lowercased -- identical to the
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
        RAISE NOTICE 'squid_trades.{trade,signature_index} unavailable; keeping the existing mv_trades';
    END IF;
END
$mig$;
