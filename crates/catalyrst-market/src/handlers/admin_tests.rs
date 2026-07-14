use super::*;
use serde_json::json;

#[test]
fn timing_safe_eq_matches_and_mismatches() {
    assert!(timing_safe_eq(b"secret", b"secret"));
    assert!(!timing_safe_eq(b"secret", b"secreT"));
    assert!(!timing_safe_eq(b"secret", b"secret-longer"));
    assert!(!timing_safe_eq(b"", b"x"));
}

#[test]
fn bearer_token_parses_prefix() {
    let mut h = HeaderMap::new();
    h.insert("authorization", "Bearer abc123".parse().unwrap());
    assert_eq!(bearer_token(&h), Some("abc123"));

    let mut h2 = HeaderMap::new();
    h2.insert("authorization", "Basic abc123".parse().unwrap());
    assert_eq!(bearer_token(&h2), None);

    assert_eq!(bearer_token(&HeaderMap::new()), None);
}

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
