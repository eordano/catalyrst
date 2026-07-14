const MV_TRADES: &str = include_str!("../migrations/0011_squid_trades_v3_contract_scope.sql");

fn statements(sql: &str) -> String {
    sql.lines()
        .filter(|line| !line.trim_start().starts_with("--"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn contract_scoped_view(sql: &str) -> &str {
    sql.split_once("\n    ELSIF ")
        .expect("0011 must branch between the contract-scoped view and the legacy-schema one")
        .0
}

fn legacy_schema_view(sql: &str) -> &str {
    sql.split_once("\n    ELSIF ")
        .expect("0011 must branch between the contract-scoped view and the legacy-schema one")
        .1
        .split_once("\n    ELSE")
        .expect("the legacy-schema branch must end at the do-nothing ELSE")
        .0
}

#[test]
fn the_signature_index_stub_carries_the_holding_deployment() {
    let sql = statements(MV_TRADES);
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
    let sql = statements(MV_TRADES);
    assert!(sql.contains("trade_digest"), "{sql}");
    assert!(
        sql.contains("ADD COLUMN IF NOT EXISTS trade_digest text"),
        "{sql}"
    );
    assert!(sql.contains("idx_squid_trades_trade_trade_digest"), "{sql}");
}

#[test]
fn the_two_provisioning_halves_fail_independently() {
    let sql = statements(MV_TRADES);
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
    let sql = statements(MV_TRADES);
    let view = contract_scoped_view(&sql);
    assert!(
        view.contains("LOWER(si_signer.address) = LOWER(t.signer)"),
        "{view}"
    );
    assert!(
        view.contains("LOWER(si_signer.contract) = LOWER(t.contract)"),
        "{view}"
    );
    assert!(
        view.contains("LOWER(si_contract.address) = LOWER(t.contract)"),
        "{view}"
    );
    assert!(
        view.contains("LOWER(si_contract.contract) = LOWER(t.contract)"),
        "{view}"
    );
}

#[test]
fn the_marketplace_address_whitelist_is_gone_from_the_contract_scoped_view() {
    let sql = statements(MV_TRADES);
    let view = contract_scoped_view(&sql);
    for address in [
        "0x540fb08edb56aae562864b390542c97f562825ba",
        "0x2d6b3508f9aca32d2550f92b2addba932e73c1ff",
        "0xa40b1d129b8906888720686f3a01921ddf37716f",
        "0x1b67d0e31eeb6b52d8eeed71d3616c2f5b33b8e7",
    ] {
        assert!(!view.contains(address), "{address}");
    }
    assert!(
        !view.contains("ON t.network = si_contract.network"),
        "{view}"
    );
}

#[test]
fn the_squid_network_enum_is_normalised_on_every_index_join() {
    let sql = statements(MV_TRADES);
    let normalised = "CASE WHEN t.network = 'MATIC' THEN 'POLYGON' ELSE t.network END";
    assert_eq!(
        contract_scoped_view(&sql).matches(normalised).count(),
        2,
        "{sql}"
    );
    assert_eq!(
        legacy_schema_view(&sql).matches(normalised).count(),
        1,
        "{sql}"
    );
}

#[test]
fn the_legacy_schema_view_scopes_the_signer_join_by_network() {
    let sql = statements(MV_TRADES);
    let view = legacy_schema_view(&sql);
    assert!(
        view.contains(
            "ON LOWER(si_signer.address) = LOWER(t.signer)\n            AND si_signer.network ="
        ),
        "{view}"
    );
    assert!(!view.contains("si_signer.contract"), "{view}");
    assert!(
        view.contains("ON t.network = si_contract.network"),
        "{view}"
    );
    assert!(
        view.contains("CREATE UNIQUE INDEX IF NOT EXISTS idx_mv_trades_id"),
        "{view}"
    );
}

#[test]
fn the_view_is_gated_on_a_reachable_squid_trades_schema() {
    let sql = statements(MV_TRADES);
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
        contract_scoped_view(&sql).contains("CREATE UNIQUE INDEX IF NOT EXISTS idx_mv_trades_id"),
        "{sql}"
    );
}

#[test]
fn the_status_branches_are_unchanged_from_the_applied_view() {
    let sql = statements(MV_TRADES);
    for branch in [
        "st.action = 'cancelled'",
        "canc.cancellations > 0",
        "t.expires_at < now()::timestamptz(3)",
        "(t.checks ->> 'signerSignatureIndex')::int",
        "(t.checks ->> 'contractSignatureIndex')::int",
        "exec.executions >= (t.checks ->> 'uses')::int",
    ] {
        assert!(sql.contains(branch), "{branch}");
        assert!(contract_scoped_view(&sql).contains(branch), "{branch}");
        assert!(legacy_schema_view(&sql).contains(branch), "{branch}");
    }
}
