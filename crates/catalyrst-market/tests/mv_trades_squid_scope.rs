const APPLIED_0011: &str = include_str!("../migrations/0011_squid_trades_v3_contract_scope.sql");
const FINAL_0012: &str = include_str!("../migrations/0012_mv_trades_v3_cancellation_semantics.sql");

fn statements(sql: &str) -> String {
    sql.lines()
        .filter(|line| !line.trim_start().starts_with("--"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn contract_scoped_view(sql: &str) -> &str {
    sql.split_once("\n    ELSIF ")
        .expect(
            "the migration must branch between the contract-scoped view and the legacy-schema one",
        )
        .0
}

fn legacy_schema_view(sql: &str) -> &str {
    sql.split_once("\n    ELSIF ")
        .expect(
            "the migration must branch between the contract-scoped view and the legacy-schema one",
        )
        .1
        .split_once("\n    ELSE")
        .expect("the legacy-schema branch must end at the do-nothing ELSE")
        .0
}

const SIGNER_ONLY_CANCELLATION: &str =
    "COUNT(CASE WHEN st.action = 'cancelled' AND LOWER(st.caller) = LOWER(t.signer) THEN 1 END) > 0";
const EITHER_IDENTIFIER: &str = "ON st.signature = ANY(ARRAY[t.hashed_signature, t.trade_digest])";
const NETWORK_NORMALISED: &str = "CASE WHEN t.network = 'MATIC' THEN 'POLYGON' ELSE t.network END";

mod applied_0011 {
    use super::*;

    #[test]
    fn the_signature_index_stub_carries_the_holding_deployment() {
        let sql = statements(APPLIED_0011);
        assert!(sql.contains("contract text NOT NULL"), "{sql}");
        assert!(
            sql.contains("ADD COLUMN IF NOT EXISTS contract text NOT NULL"),
            "{sql}"
        );
        assert!(
            sql.contains("idx_squid_trades_signature_index_contract"),
            "{sql}"
        );
    }

    #[test]
    fn the_trade_stub_carries_the_v3_digest() {
        let sql = statements(APPLIED_0011);
        assert!(
            sql.contains("ADD COLUMN IF NOT EXISTS trade_digest text"),
            "{sql}"
        );
        assert!(sql.contains("idx_squid_trades_trade_trade_digest"), "{sql}");
    }

    #[test]
    fn the_two_provisioning_halves_fail_independently() {
        let sql = statements(APPLIED_0011);
        let between = sql
            .split_once("CREATE TABLE IF NOT EXISTS squid_trades.trade")
            .expect("the trade stub must be provisioned")
            .1
            .split_once("CREATE TABLE IF NOT EXISTS squid_trades.signature_index")
            .expect("the signature_index stub must be provisioned")
            .0;
        assert!(
            between.contains("EXCEPTION WHEN insufficient_privilege THEN"),
            "{between}"
        );
        assert!(
            sql.contains("EXCEPTION WHEN insufficient_privilege OR not_null_violation THEN"),
            "{sql}"
        );
    }

    #[test]
    fn both_signature_index_joins_are_scoped_to_the_trades_own_contract() {
        let sql = statements(APPLIED_0011);
        let view = contract_scoped_view(&sql);
        for predicate in [
            "LOWER(si_signer.address) = LOWER(t.signer)",
            "LOWER(si_signer.contract) = LOWER(t.contract)",
            "LOWER(si_contract.address) = LOWER(t.contract)",
            "LOWER(si_contract.contract) = LOWER(t.contract)",
        ] {
            assert!(view.contains(predicate), "{predicate}\n{view}");
        }
    }

    #[test]
    fn the_view_is_gated_on_a_reachable_squid_trades_schema() {
        let sql = statements(APPLIED_0011);
        assert!(
            sql.contains("to_regclass('squid_trades.trade') IS NOT NULL"),
            "{sql}"
        );
        assert!(sql.contains("column_name = 'contract'"), "{sql}");
    }
}

mod final_0012 {
    use super::*;

    #[test]
    fn trades_gain_the_digest_column_indexed_only_where_set() {
        let sql = statements(FINAL_0012);
        assert!(
            sql.contains(
                "ALTER TABLE marketplace.trades ADD COLUMN IF NOT EXISTS trade_digest text;"
            ),
            "{sql}"
        );
        assert!(
            sql.contains("ON marketplace.trades (trade_digest) WHERE trade_digest IS NOT NULL;"),
            "{sql}"
        );
    }

    #[test]
    fn only_the_signers_own_cancellation_counts_in_both_views() {
        let sql = statements(FINAL_0012);
        for view in [contract_scoped_view(&sql), legacy_schema_view(&sql)] {
            assert!(view.contains(SIGNER_ONLY_CANCELLATION), "{view}");
            assert!(
                !view.contains("WHEN COUNT(CASE WHEN st.action = 'cancelled' THEN 1 END)"),
                "an unscoped cancellation count is the griefing hole upstream 61a0515 closed:\n{view}"
            );
        }
    }

    #[test]
    fn on_chain_actions_match_on_either_identifier_in_both_views() {
        let sql = statements(FINAL_0012);
        for view in [contract_scoped_view(&sql), legacy_schema_view(&sql)] {
            assert!(view.contains(EITHER_IDENTIFIER), "{view}");
            assert!(
                !view.contains("ON st.signature = t.hashed_signature\n"),
                "{view}"
            );
            assert!(!view.contains("st.trade_digest = t.trade_digest"), "{view}");
        }
    }

    #[test]
    fn indexer_columns_are_compared_raw_and_trade_columns_lowercased() {
        let sql = statements(FINAL_0012);
        let view = contract_scoped_view(&sql);
        for predicate in [
            "ON si_signer.address = LOWER(t.signer)",
            "AND si_signer.contract = LOWER(t.contract)",
            "ON si_contract.address = LOWER(t.contract)",
            "AND si_contract.contract = LOWER(t.contract)",
        ] {
            assert!(view.contains(predicate), "{predicate}\n{view}");
        }
        for view in [contract_scoped_view(&sql), legacy_schema_view(&sql)] {
            assert!(!view.contains("LOWER(si_signer."), "{view}");
            assert!(!view.contains("LOWER(si_contract."), "{view}");
            assert!(!view.contains("LOWER(idx."), "{view}");
        }
    }

    #[test]
    fn the_squid_network_enum_is_normalised_on_every_index_join() {
        let sql = statements(FINAL_0012);
        assert_eq!(
            contract_scoped_view(&sql)
                .matches(NETWORK_NORMALISED)
                .count(),
            2,
            "{sql}"
        );
        assert_eq!(
            legacy_schema_view(&sql).matches(NETWORK_NORMALISED).count(),
            2,
            "{sql}"
        );
    }

    #[test]
    fn the_legacy_schema_view_joins_the_contract_counter_on_the_trades_own_contract() {
        let sql = statements(FINAL_0012);
        let view = legacy_schema_view(&sql);
        assert!(
            view.contains(
                "ON si_signer.address = LOWER(t.signer)\n            AND si_signer.network ="
            ),
            "{view}"
        );
        assert!(!view.contains("si_signer.contract"), "{view}");
        assert!(!view.contains("si_contract.contract"), "{view}");
        assert!(
            view.contains(
                "ON si_contract.address = LOWER(t.contract)\n            AND si_contract.network ="
            ),
            "{view}"
        );
        assert!(
            !view.contains("ON t.network = si_contract.network"),
            "{view}"
        );
    }

    #[test]
    fn the_marketplace_address_whitelist_is_gone_from_both_views() {
        let sql = statements(FINAL_0012);
        for address in [
            "0x540fb08edb56aae562864b390542c97f562825ba",
            "0x2d6b3508f9aca32d2550f92b2addba932e73c1ff",
            "0xa40b1d129b8906888720686f3a01921ddf37716f",
            "0x1b67d0e31eeb6b52d8eeed71d3616c2f5b33b8e7",
        ] {
            assert!(!sql.contains(address), "{address}");
        }
        assert!(!sql.contains("LATERAL"), "{sql}");
    }

    #[test]
    fn each_view_rebuild_tolerates_only_insufficient_privilege() {
        let sql = statements(FINAL_0012);
        for view in [contract_scoped_view(&sql), legacy_schema_view(&sql)] {
            let (before_drop, from_drop) = view
                .split_once("DROP MATERIALIZED VIEW IF EXISTS marketplace.mv_trades;")
                .expect("each branch drops the view before rebuilding it");
            assert!(
                before_drop.trim_end().ends_with("BEGIN"),
                "the DROP must open its own sub-transaction:\n{before_drop}"
            );
            let (rebuild, handler) = from_drop
                .split_once("EXCEPTION WHEN insufficient_privilege THEN")
                .expect("each rebuild must end in an insufficient_privilege handler");
            assert!(rebuild.contains("CREATE MATERIALIZED VIEW marketplace.mv_trades AS"));
            assert!(rebuild.contains("CREATE UNIQUE INDEX IF NOT EXISTS idx_mv_trades_id"));
            assert!(handler.contains("RAISE NOTICE"), "{handler}");
            assert!(handler.contains("keeping the existing view"), "{handler}");
            assert!(!handler.contains("WHEN OTHERS"), "{handler}");
            assert!(!handler.contains("OR not_null_violation"), "{handler}");
        }
        assert_eq!(sql.matches("EXCEPTION WHEN").count(), 2, "{sql}");
    }

    #[test]
    fn the_view_is_gated_on_a_reachable_squid_trades_schema() {
        let sql = statements(FINAL_0012);
        assert!(
            sql.contains("to_regclass('squid_trades.trade') IS NOT NULL"),
            "{sql}"
        );
        assert!(
            sql.contains("to_regclass('squid_trades.signature_index') IS NOT NULL"),
            "{sql}"
        );
        assert!(sql.contains("column_name = 'contract'"), "{sql}");
        assert!(
            sql.contains("keeping the existing mv_trades"),
            "a database without squid_trades keeps whatever view it has:\n{sql}"
        );
    }

    #[test]
    fn the_catalyrst_local_status_branches_survive_the_rebuild() {
        let sql = statements(FINAL_0012);
        for branch in [
            "canc.cancellations > 0",
            "t.expires_at < now()::timestamptz(3)",
            "(t.checks ->> 'signerSignatureIndex')::int",
            "(t.checks ->> 'contractSignatureIndex')::int",
            "COUNT(DISTINCT st.id) FILTER (WHERE st.action = 'executed') >= (t.checks ->> 'uses')::int",
            "exec.executions >= (t.checks ->> 'uses')::int",
            "FROM marketplace.market_trades_local",
            "FROM marketplace.market_cancellations",
        ] {
            assert!(contract_scoped_view(&sql).contains(branch), "{branch}");
            assert!(legacy_schema_view(&sql).contains(branch), "{branch}");
        }
    }
}
