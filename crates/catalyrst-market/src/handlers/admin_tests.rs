use super::*;
use serde_json::json;

#[test]
fn target_kind_validation() {
    assert!(valid_target_kind("bid"));
    assert!(valid_target_kind("order"));
    assert!(valid_target_kind("trade"));
    assert!(!valid_target_kind("listing"));
    assert!(!valid_target_kind(""));
}

#[test]
fn wire_identity_error_envelope() {
    let dto = AdminError {
        ok: false,
        message: "admin bearer token required".to_string(),
    };
    assert_eq!(
        serde_json::to_value(&dto).unwrap(),
        json!({ "ok": false, "message": "admin bearer token required" })
    );

    let dto = AdminError {
        ok: false,
        message: "admin controls disabled (CATALYRST_MARKET_ADMIN_TOKEN unset)".to_string(),
    };
    assert_eq!(
        serde_json::to_value(&dto).unwrap(),
        json!({
            "ok": false,
            "message": "admin controls disabled (CATALYRST_MARKET_ADMIN_TOKEN unset)"
        })
    );
}

#[test]
fn wire_identity_set_flag_ok() {
    let dto = SetFlagResponse {
        ok: true,
        target_kind: "bid".to_string(),
        target_hash: "0xabc".to_string(),
        severity: "hide".to_string(),
    };
    assert_eq!(
        serde_json::to_value(&dto).unwrap(),
        json!({ "ok": true, "target_kind": "bid", "target_hash": "0xabc", "severity": "hide" })
    );
}

#[test]
fn wire_identity_clear_flag_ok() {
    let removed = ClearFlagResponse {
        ok: true,
        target_hash: "0xabc".to_string(),
        removed: true,
    };
    assert_eq!(
        serde_json::to_value(&removed).unwrap(),
        json!({ "ok": true, "target_hash": "0xabc", "removed": true })
    );

    let noop = ClearFlagResponse {
        ok: true,
        target_hash: "0xdef".to_string(),
        removed: false,
    };
    assert_eq!(
        serde_json::to_value(&noop).unwrap(),
        json!({ "ok": true, "target_hash": "0xdef", "removed": false })
    );
}

#[test]
fn wire_identity_list_flags() {
    let entry = FlagEntry {
        target_hash: "0xabc".to_string(),
        target_kind: "order".to_string(),
        severity: "review".to_string(),
        reason: "spam".to_string(),
        flagged_by: "admin-token".to_string(),
        flagged_at: 1_700_000_000,
    };
    let dto = ListEnvelope::of(vec![entry]);
    assert_eq!(
        serde_json::to_value(&dto).unwrap(),
        json!({
            "data": [{
                "target_hash": "0xabc",
                "target_kind": "order",
                "severity": "review",
                "reason": "spam",
                "flagged_by": "admin-token",
                "flagged_at": 1_700_000_000_i64,
            }],
            "total": 1
        })
    );

    let empty: ListEnvelope<FlagEntry> = ListEnvelope::of(vec![]);
    assert_eq!(
        serde_json::to_value(&empty).unwrap(),
        json!({ "data": [], "total": 0 })
    );
}

#[test]
fn wire_identity_dispute_action() {
    let opened = DisputeActionResponse {
        ok: true,
        trade_hash: "0xtrade".to_string(),
        status: "open".to_string(),
    };
    assert_eq!(
        serde_json::to_value(&opened).unwrap(),
        json!({ "ok": true, "trade_hash": "0xtrade", "status": "open" })
    );

    for status in ["resolved", "rejected"] {
        let dto = DisputeActionResponse {
            ok: true,
            trade_hash: "0xtrade".to_string(),
            status: status.to_string(),
        };
        assert_eq!(
            serde_json::to_value(&dto).unwrap(),
            json!({ "ok": true, "trade_hash": "0xtrade", "status": status })
        );
    }
}

#[test]
fn wire_identity_list_disputes() {
    let open = DisputeEntry {
        trade_hash: "0xtrade".to_string(),
        status: "open".to_string(),
        reason: "fraud".to_string(),
        resolution: String::new(),
        opened_by: "admin-token".to_string(),
        opened_at: 1_700_000_000,
        resolved_by: None,
        resolved_at: None,
    };
    let v = serde_json::to_value(&open).unwrap();
    assert_eq!(
        v,
        json!({
            "trade_hash": "0xtrade",
            "status": "open",
            "reason": "fraud",
            "resolution": "",
            "opened_by": "admin-token",
            "opened_at": 1_700_000_000_i64,
            "resolved_by": null,
            "resolved_at": null,
        })
    );
    let obj = v.as_object().unwrap();
    assert!(obj.contains_key("resolved_by"));
    assert!(obj.contains_key("resolved_at"));

    let resolved = DisputeEntry {
        trade_hash: "0xtrade".to_string(),
        status: "resolved".to_string(),
        reason: "fraud".to_string(),
        resolution: "refunded".to_string(),
        opened_by: "admin-token".to_string(),
        opened_at: 1_700_000_000,
        resolved_by: Some("admin-token".to_string()),
        resolved_at: Some(1_700_000_100),
    };
    let dto = ListEnvelope::of(vec![resolved]);
    assert_eq!(
        serde_json::to_value(&dto).unwrap(),
        json!({
            "data": [{
                "trade_hash": "0xtrade",
                "status": "resolved",
                "reason": "fraud",
                "resolution": "refunded",
                "opened_by": "admin-token",
                "opened_at": 1_700_000_000_i64,
                "resolved_by": "admin-token",
                "resolved_at": 1_700_000_100_i64,
            }],
            "total": 1
        })
    );

    let empty: ListEnvelope<DisputeEntry> = ListEnvelope::of(vec![]);
    assert_eq!(
        serde_json::to_value(&empty).unwrap(),
        json!({ "data": [], "total": 0 })
    );
}

#[test]
fn wire_identity_force_cancel() {
    let fresh = ForceCancelResponse {
        ok: true,
        target_hash: "0xh".to_string(),
        cancellation_hash: "operator:deadbeef".to_string(),
        already_cancelled: None,
    };
    let v = serde_json::to_value(&fresh).unwrap();
    assert_eq!(
        v,
        json!({ "ok": true, "target_hash": "0xh", "cancellation_hash": "operator:deadbeef" })
    );
    assert!(!v.as_object().unwrap().contains_key("already_cancelled"));

    let replay = ForceCancelResponse {
        ok: true,
        target_hash: "0xh".to_string(),
        cancellation_hash: "operator:prior".to_string(),
        already_cancelled: Some(true),
    };
    assert_eq!(
        serde_json::to_value(&replay).unwrap(),
        json!({
            "ok": true,
            "target_hash": "0xh",
            "cancellation_hash": "operator:prior",
            "already_cancelled": true,
        })
    );
}

#[test]
fn wire_identity_list_audit() {
    let entry = AuditEntry {
        id: 42,
        actor: "admin-token".to_string(),
        action: "flag.set".to_string(),
        target_kind: "bid".to_string(),
        target_hash: "0xabc".to_string(),
        detail: json!({ "severity": "hide", "reason": "spam", "legacy_extra": [1, 2] }),
        created_at: 1_700_000_000,
    };
    let dto = ListEnvelope::of(vec![entry]);
    assert_eq!(
        serde_json::to_value(&dto).unwrap(),
        json!({
            "data": [{
                "id": 42,
                "actor": "admin-token",
                "action": "flag.set",
                "target_kind": "bid",
                "target_hash": "0xabc",
                "detail": { "severity": "hide", "reason": "spam", "legacy_extra": [1, 2] },
                "created_at": 1_700_000_000_i64,
            }],
            "total": 1
        })
    );

    let empty: ListEnvelope<AuditEntry> = ListEnvelope::of(vec![]);
    assert_eq!(
        serde_json::to_value(&empty).unwrap(),
        json!({ "data": [], "total": 0 })
    );
}

#[test]
fn wire_identity_audit_details() {
    assert_eq!(
        to_detail_value(
            FlagSetDetail {
                severity: "hide",
                reason: "spam"
            },
            "test"
        ),
        json!({ "severity": "hide", "reason": "spam" })
    );
    assert_eq!(to_detail_value(EmptyDetail {}, "test"), json!({}));
    assert_eq!(
        to_detail_value(ReasonDetail { reason: "fraud" }, "test"),
        json!({ "reason": "fraud" })
    );
    assert_eq!(
        to_detail_value(
            DisputeResolveDetail {
                status: "resolved",
                resolution: "refunded"
            },
            "test"
        ),
        json!({ "status": "resolved", "resolution": "refunded" })
    );
    assert_eq!(
        to_detail_value(
            ForceCancelDetail {
                reason: "rug",
                cancellation_hash: "operator:deadbeef"
            },
            "test"
        ),
        json!({ "reason": "rug", "cancellation_hash": "operator:deadbeef" })
    );
    assert_eq!(
        to_detail_value(
            OperatorCancelPayload {
                operator_force_cancel: true,
                actor: "admin-token",
                reason: "rug",
                target_kind: "order",
            },
            "test"
        ),
        json!({
            "operator_force_cancel": true,
            "actor": "admin-token",
            "reason": "rug",
            "target_kind": "order",
        })
    );
}

fn parts_with_auth(authorization: Option<&str>) -> Parts {
    let mut builder = axum::http::Request::builder();
    if let Some(value) = authorization {
        builder = builder.header("authorization", value);
    }
    let request = builder.body(()).expect("request builds");
    request.into_parts().0
}

async fn reject_status_and_body(
    configured: Option<&str>,
    authorization: Option<&str>,
) -> (StatusCode, Value) {
    let mut parts = parts_with_auth(authorization);
    let response = match establish_admin(configured.map(str::to_string), &mut parts).await {
        Ok(_) => panic!("expected a rejection, not an established admin identity"),
        Err(response) => response,
    };
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 4096)
        .await
        .expect("body collects");
    let value: Value = serde_json::from_slice(&bytes).expect("json body");
    (status, value)
}

#[tokio::test]
async fn a_matching_bearer_establishes_admin() {
    let mut parts = parts_with_auth(Some("Bearer topsecret"));
    let admin = establish_admin(Some("topsecret".to_string()), &mut parts)
        .await
        .unwrap_or_else(|_| panic!("a matching secret establishes admin"));
    assert_eq!(admin.actor(), "admin-token");
}

#[tokio::test]
async fn an_unset_token_fails_closed_as_403_disabled() {
    for presented in [Some("Bearer anything"), None] {
        let (status, body) = reject_status_and_body(None, presented).await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        assert_eq!(body["ok"], json!(false));
        assert_eq!(
            body["message"],
            "admin controls disabled (CATALYRST_MARKET_ADMIN_TOKEN unset)"
        );
    }
}

#[tokio::test]
async fn a_missing_or_wrong_bearer_is_403_unauthorized() {
    for presented in [None, Some("Bearer wrong"), Some("Basic topsecret")] {
        let (status, body) = reject_status_and_body(Some("topsecret"), presented).await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        assert_eq!(body["ok"], json!(false));
        assert_eq!(body["message"], "admin bearer token required");
    }
}

mod admin_gate_precedence {
    use std::sync::Arc;
    use std::time::Duration;

    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use axum::routing::post;
    use axum::Router;
    use tower::ServiceExt;

    use crate::fed::market_domain;
    use crate::fed::replay::Replay;
    use crate::handlers::admin::set_flag;
    use crate::ports::accounts::AccountsComponent;
    use crate::ports::activity::ActivityComponent;
    use crate::ports::analytics_day_data::AnalyticsDayDataComponent;
    use crate::ports::bids::BidsComponent;
    use crate::ports::catalog::CatalogComponent;
    use crate::ports::collections::CollectionsComponent;
    use crate::ports::contracts::ContractsComponent;
    use crate::ports::items::ItemsComponent;
    use crate::ports::lists::ListsComponent;
    use crate::ports::mana_rate::ManaUsdRateComponent;
    use crate::ports::nfts::NftsComponent;
    use crate::ports::orders::OrdersComponent;
    use crate::ports::owners::OwnersComponent;
    use crate::ports::prices::PricesComponent;
    use crate::ports::rankings::RankingsComponent;
    use crate::ports::sales::SalesComponent;
    use crate::ports::shop_catalog::ShopCatalogComponent;
    use crate::ports::stats::StatsComponent;
    use crate::ports::trades::TradesComponent;
    use crate::ports::trendings::TrendingsComponent;
    use crate::ports::usage_grants::UsageGrantsComponent;
    use crate::ports::user_assets::UserAssetsComponent;
    use crate::ports::volume::VolumeComponent;
    use crate::{AppState, AppStateInner};
    use catalyrst_fed::RateLimiter;

    fn state_with_admin_token(admin_token: Option<&str>) -> AppState {
        let pool = sqlx::postgres::PgPoolOptions::new()
            .connect_lazy("postgres://unused@127.0.0.1:1/unused")
            .expect("lazy pool builds without connecting");
        let sales = Arc::new(SalesComponent::new(pool.clone()));
        let bids = Arc::new(BidsComponent::new(pool.clone()));
        let orders = Arc::new(OrdersComponent::new(pool.clone()));
        let trades = Arc::new(TradesComponent::new(pool.clone(), false));
        Arc::new(AppStateInner {
            accounts: AccountsComponent::new(pool.clone()),
            activity: ActivityComponent::new(sales, bids, orders, trades),
            analytics_day_data: AnalyticsDayDataComponent::new(pool.clone()),
            bids: BidsComponent::new(pool.clone()),
            catalog: CatalogComponent::new(pool.clone()),
            collections: CollectionsComponent::new(pool.clone()),
            contracts: ContractsComponent::new(pool.clone()),
            items: ItemsComponent::new(pool.clone()),
            lists: ListsComponent::new(pool.clone()).with_write(pool.clone()),
            mana_usd_rate: ManaUsdRateComponent::new("http://127.0.0.1:9".into(), 0.02, 86400),
            nfts: NftsComponent::new(pool.clone()),
            orders: OrdersComponent::new(pool.clone()),
            owners: OwnersComponent::new(pool.clone()),
            prices: PricesComponent::new(pool.clone()),
            rankings: RankingsComponent::new(pool.clone()),
            sales: SalesComponent::new(pool.clone()),
            shop_catalog: ShopCatalogComponent::new(pool.clone()),
            stats: StatsComponent::new(pool.clone()),
            trades: TradesComponent::new(pool.clone(), false),
            trendings: TrendingsComponent::new(pool.clone()),
            user_assets: UserAssetsComponent::new(pool.clone(), false),
            usage_grants: UsageGrantsComponent::new(Some(pool.clone())),
            volume: VolumeComponent::new(AnalyticsDayDataComponent::new(pool.clone())),
            mv_trades_refresh_lock: Arc::new(tokio::sync::Mutex::new(())),
            replay: Replay::empty(pool.clone()),
            pool,
            limiter: Arc::new(RateLimiter::new(120, Duration::from_secs(60))),
            domain: market_domain(),
            admin_token: admin_token.map(str::to_string),
            trade_rpc: Default::default(),
            http: reqwest::Client::new(),
        })
    }

    async fn set_flag_status(authorization: Option<&str>) -> StatusCode {
        let app = Router::new()
            .route("/v1/admin/moderation/{kind}/{hash}/flag", post(set_flag))
            .with_state(state_with_admin_token(Some("topsecret")));
        let mut builder = Request::builder()
            .method("POST")
            .uri("/v1/admin/moderation/listing/0xabc/flag");
        if let Some(value) = authorization {
            builder = builder.header("authorization", value);
        }
        let request = builder.body(Body::empty()).expect("request builds");
        app.oneshot(request)
            .await
            .expect("router responds")
            .status()
    }

    #[tokio::test]
    async fn unauthenticated_invalid_kind_is_403_not_400() {
        for presented in [None, Some("Bearer wrong")] {
            assert_eq!(set_flag_status(presented).await, StatusCode::FORBIDDEN);
        }
    }

    #[tokio::test]
    async fn established_admin_still_gets_400_for_an_invalid_kind() {
        assert_eq!(
            set_flag_status(Some("Bearer topsecret")).await,
            StatusCode::BAD_REQUEST
        );
    }
}
