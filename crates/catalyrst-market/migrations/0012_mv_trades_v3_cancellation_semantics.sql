-- catalyrst-market: land mv_trades on marketplace-server's settled 2026-08-21
-- definition (0446723 and the fixes folded into TRADES_MV_CREATE_SQL).
--
-- 0011 is applied and immutable, so the view is dropped and re-created here.
-- Replaying this file after a later redefinition must yield the newer view: it
-- is a derived object with no state of its own, and the migration role may not
-- own squid_trades, so every DROP/CREATE/INDEX sequence below is its own
-- sub-transaction that tolerates insufficient_privilege and nothing else. A
-- role that sees the squid tables through to_regclass but cannot SELECT them
-- keeps whichever view is in place instead of aborting boot. 0005 and 0011
-- are applied, immutable and carry no such guard, so a greenfield apply under
-- that same role still aborts at them before reaching this file; closing that
-- needs the migration runner (a per-file allowance for SQLSTATE 42501 on those
-- two), not another migration.
--
-- Invariants a future redefinition has to keep:
--
--   * Only the SIGNER's cancellation counts. cancelSignature takes no signer
--     check and the contract keys the flag on keccak256(caller, digest) while
--     settlement reads keccak256(signer, digest), so a stranger's cancellation
--     is a no-op on chain; counting it let anyone grief a listing into reading
--     cancelled while it stayed settleable.
--   * On-chain actions match on EITHER identifier. V1/V2 key a cancellation on
--     keccak256(signature bytes), V3 on the trade's EIP-712 digest, and the
--     indexer writes whichever applies into squid_trades.trade.signature.
--     ANY(ARRAY[...]) rather than an OR so the planner still drives the join off
--     the indexer's signature index; trade_digest stays NULL for trades signed
--     against a version that keys on the signature hash.
--   * Indexer-side columns are compared raw. trades-squid-core writes
--     signature_index.address/contract from the emitting log.address and the
--     decoded event argument, both lowercase, and LOWER() on them defeated the
--     btree indexes 0011 provisions. LOWER() stays on marketplace.trades.*:
--     0002's contract default and older rows may carry checksummed addresses.
--   * Both signature-index joins are exact on the row's whole identity where the
--     schema carries it (address, contract, network), with 'MATIC' translated to
--     the indexer's 'POLYGON'.
--
-- The legacy-schema branch (signature_index without `contract`) now joins
-- si_contract on the trade's own contract address instead of 0005's
-- four-address whitelist matched by raw network. Two consequences on a database
-- that still runs it: Polygon trades finally compare against a contract counter
-- (a trade whose signed contractSignatureIndex no longer equals the current
-- counter flips open -> cancelled, which is the on-chain truth), and a Polygon
-- trade signed at a non-zero index that used to match no row (reading
-- cancelled) flips cancelled -> open once its row matches with an equal
-- counter. Trades with an empty `contract` match no counter row and are judged
-- by their signed indexes alone.

ALTER TABLE marketplace.trades ADD COLUMN IF NOT EXISTS trade_digest text;
CREATE INDEX IF NOT EXISTS idx_trades_trade_digest
    ON marketplace.trades (trade_digest) WHERE trade_digest IS NOT NULL;

DO $mig$
BEGIN
    IF to_regclass('squid_trades.trade') IS NOT NULL
       AND to_regclass('squid_trades.signature_index') IS NOT NULL
       AND EXISTS (
           SELECT 1
           FROM information_schema.columns
           WHERE table_schema = 'squid_trades'
             AND table_name = 'signature_index'
             AND column_name = 'contract'
       ) THEN

        BEGIN
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
                WHEN COUNT(CASE WHEN st.action = 'cancelled' AND LOWER(st.caller) = LOWER(t.signer) THEN 1 END) > 0
                                                                                        THEN 'cancelled'
                WHEN canc.cancellations > 0                                              THEN 'cancelled'
                WHEN t.expires_at < now()::timestamptz(3)                                THEN 'cancelled'
                WHEN (
                    (si_signer.index IS NOT NULL
                        AND si_signer.index != (t.checks ->> 'signerSignatureIndex')::int)
                    OR (si_signer.index IS NULL
                        AND (t.checks ->> 'signerSignatureIndex')::int != 0)
                    )                                                                    THEN 'cancelled'
                WHEN (
                    (si_contract.index IS NOT NULL
                        AND si_contract.index != (t.checks ->> 'contractSignatureIndex')::int)
                    OR (si_contract.index IS NULL
                        AND (t.checks ->> 'contractSignatureIndex')::int != 0)
                    )                                                                    THEN 'cancelled'
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
        LEFT JOIN squid_trades.trade AS st
            ON st.signature = ANY(ARRAY[t.hashed_signature, t.trade_digest])
        LEFT JOIN squid_trades.signature_index AS si_signer
            ON si_signer.address = LOWER(t.signer)
            AND si_signer.contract = LOWER(t.contract)
            AND si_signer.network = CASE WHEN t.network = 'MATIC' THEN 'POLYGON' ELSE t.network END
        LEFT JOIN squid_trades.signature_index AS si_contract
            ON si_contract.address = LOWER(t.contract)
            AND si_contract.contract = LOWER(t.contract)
            AND si_contract.network = CASE WHEN t.network = 'MATIC' THEN 'POLYGON' ELSE t.network END
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
            si_contract.index,
            si_signer.index,
            exec.executions,
            canc.cancellations;

        CREATE UNIQUE INDEX IF NOT EXISTS idx_mv_trades_id ON marketplace.mv_trades (id);
        CREATE INDEX IF NOT EXISTS idx_mv_trades_status_type ON marketplace.mv_trades (status, type);
        CREATE INDEX IF NOT EXISTS idx_mv_trades_created_at ON marketplace.mv_trades (created_at DESC);
        CREATE INDEX IF NOT EXISTS idx_mv_trades_category ON marketplace.mv_trades (sent_nft_category);
        CREATE INDEX IF NOT EXISTS idx_mv_trades_contract_token ON marketplace.mv_trades (contract_address_sent, sent_token_id);
        EXCEPTION WHEN insufficient_privilege THEN
            RAISE NOTICE 'Insufficient privilege on squid_trades to rebuild the contract-scoped mv_trades; keeping the existing view';
        END;

    ELSIF to_regclass('squid_trades.trade') IS NOT NULL
          AND to_regclass('squid_trades.signature_index') IS NOT NULL THEN

        BEGIN
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
                WHEN COUNT(CASE WHEN st.action = 'cancelled' AND LOWER(st.caller) = LOWER(t.signer) THEN 1 END) > 0
                                                                                        THEN 'cancelled'
                WHEN canc.cancellations > 0                                              THEN 'cancelled'
                WHEN t.expires_at < now()::timestamptz(3)                                THEN 'cancelled'
                WHEN (
                    (si_signer.index IS NOT NULL
                        AND si_signer.index != (t.checks ->> 'signerSignatureIndex')::int)
                    OR (si_signer.index IS NULL
                        AND (t.checks ->> 'signerSignatureIndex')::int != 0)
                    )                                                                    THEN 'cancelled'
                WHEN (
                    (si_contract.index IS NOT NULL
                        AND si_contract.index != (t.checks ->> 'contractSignatureIndex')::int)
                    OR (si_contract.index IS NULL
                        AND (t.checks ->> 'contractSignatureIndex')::int != 0)
                    )                                                                    THEN 'cancelled'
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
        LEFT JOIN squid_trades.trade AS st
            ON st.signature = ANY(ARRAY[t.hashed_signature, t.trade_digest])
        LEFT JOIN squid_trades.signature_index AS si_signer
            ON si_signer.address = LOWER(t.signer)
            AND si_signer.network = CASE WHEN t.network = 'MATIC' THEN 'POLYGON' ELSE t.network END
        LEFT JOIN squid_trades.signature_index AS si_contract
            ON si_contract.address = LOWER(t.contract)
            AND si_contract.network = CASE WHEN t.network = 'MATIC' THEN 'POLYGON' ELSE t.network END
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
            si_contract.index,
            si_signer.index,
            exec.executions,
            canc.cancellations;

        CREATE UNIQUE INDEX IF NOT EXISTS idx_mv_trades_id ON marketplace.mv_trades (id);
        CREATE INDEX IF NOT EXISTS idx_mv_trades_status_type ON marketplace.mv_trades (status, type);
        CREATE INDEX IF NOT EXISTS idx_mv_trades_created_at ON marketplace.mv_trades (created_at DESC);
        CREATE INDEX IF NOT EXISTS idx_mv_trades_category ON marketplace.mv_trades (sent_nft_category);
        CREATE INDEX IF NOT EXISTS idx_mv_trades_contract_token ON marketplace.mv_trades (contract_address_sent, sent_token_id);
        EXCEPTION WHEN insufficient_privilege THEN
            RAISE NOTICE 'Insufficient privilege on squid_trades to rebuild the legacy-schema mv_trades; keeping the existing view';
        END;

    ELSE
        RAISE NOTICE 'squid_trades.{trade,signature_index} unavailable; keeping the existing mv_trades';
    END IF;
END
$mig$;
