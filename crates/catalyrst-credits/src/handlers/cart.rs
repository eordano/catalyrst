use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::handlers::prices::QuoteCache;
use crate::handlers::signer_from;
use crate::http::ApiError;
use crate::ports::checkout::{CartView, CheckoutIdemRow, RepricedLine};
use crate::ports::pricing::BasisKind;
use crate::purchase_intent::{
    verify_intent_matches_order, verify_purchase_intent, PurchaseIntentIn,
};
use crate::AppState;

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "credits/"))]
pub struct CartLineOut {
    #[serde(rename = "itemId")]
    item_id: String,
    collection: String,
    urn: String,
    category: String,
    qty: i32,
    #[serde(rename = "unitPriceCredits")]
    unit_price_credits: String,
}

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "credits/"))]
pub struct CartOut {
    address: String,
    items: Vec<CartLineOut>,
    #[serde(rename = "totalCredits")]
    total_credits: String,
}

fn cart_out(address: &str, cart: &CartView) -> CartOut {
    CartOut {
        address: address.to_string(),
        items: cart
            .items
            .iter()
            .map(|i| CartLineOut {
                item_id: i.item_id.clone(),
                collection: i.collection.clone(),
                urn: i.urn.clone(),
                category: i.category.clone(),
                qty: i.qty,
                unit_price_credits: i.unit_price_credits.clone(),
            })
            .collect(),
        total_credits: cart.total_credits.clone(),
    }
}

pub async fn get_cart(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<CartOut>, ApiError> {
    let signer = signer_from(&headers, "get", "/cart")?;
    let cart = state.credits.get_cart(&signer).await?;
    Ok(Json(cart_out(&signer, &cart)))
}

#[derive(Debug, Deserialize)]
pub struct AddItemBody {
    #[serde(rename = "itemId")]
    item_id: String,

    collection: String,

    #[serde(default)]
    qty: Option<i32>,
}

#[derive(Debug, Deserialize)]
pub struct CheckoutScopeItem {
    #[serde(rename = "itemId")]
    item_id: String,
    collection: String,
}

#[derive(Debug, Default, Deserialize)]
pub struct CheckoutBody {
    #[serde(default)]
    items: Option<Vec<CheckoutScopeItem>>,

    #[serde(default)]
    intent: Option<PurchaseIntentIn>,

    #[serde(rename = "intentSignature", default)]
    intent_signature: Option<String>,
}

pub(crate) fn ensure_intent_present(
    has_signed_intent: bool,
    require_purchase_intent: bool,
) -> Result<(), ApiError> {
    if !has_signed_intent && require_purchase_intent {
        return Err(ApiError::unauthorized(
            "purchase requires a signed intent: sign the EIP-712 purchase sheet and send it as \
             `intent` + `intentSignature`",
        ));
    }
    Ok(())
}

pub(crate) fn ensure_qty_fillable(kind: &BasisKind, qty: i32) -> Result<(), ApiError> {
    if kind.is_single_listing() && qty > 1 {
        return Err(ApiError::conflict(
            "this item is stocked from individual marketplace listings, so it's one per checkout \
             while supplies are listed individually — please keep the quantity at 1",
        ));
    }
    Ok(())
}

pub(crate) fn validate_item_id(raw: &str) -> Result<String, ApiError> {
    let s = raw.trim();
    if s.is_empty() || s.len() > 200 {
        return Err(ApiError::bad_request("invalid itemId"));
    }
    if !s.chars().all(|c| c.is_ascii_graphic() && c != '\\') {
        return Err(ApiError::bad_request("invalid itemId"));
    }
    Ok(s.to_string())
}

pub(crate) fn validate_collection(raw: &str) -> Result<String, ApiError> {
    let s = raw.trim().to_ascii_lowercase();
    let is_addr = s
        .strip_prefix("0x")
        .is_some_and(|hex| hex.len() == 40 && hex.bytes().all(|b| b.is_ascii_hexdigit()));
    if !is_addr {
        return Err(ApiError::bad_request(
            "invalid collection (expected 0x + 40 hex)",
        ));
    }
    Ok(s)
}

pub async fn add_item(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<AddItemBody>,
) -> Result<Json<CartOut>, ApiError> {
    let signer = signer_from(&headers, "post", "/cart/items")?;
    let item_id = validate_item_id(&body.item_id)?;
    let collection = validate_collection(&body.collection)?;
    let qty = body.qty.unwrap_or(1);
    if qty <= 0 {
        return Err(ApiError::bad_request("qty must be > 0"));
    }

    let priced = state
        .pricing
        .price_item_for_mode(
            &state.credits.pool,
            &collection,
            &item_id,
            &state.checkout_fulfillment_mode,
        )
        .await?;

    ensure_qty_fillable(&priced.basis.kind, qty)?;

    state
        .credits
        .add_item(
            &signer,
            &item_id,
            &collection,
            &priced.basis.info.urn,
            &priced.basis.info.category,
            qty,
            &priced.credit_price,
        )
        .await?;

    let cart = state.credits.get_cart(&signer).await?;
    Ok(Json(cart_out(&signer, &cart)))
}

pub async fn remove_item(
    State(state): State<AppState>,
    Path((collection, item_id)): Path<(String, String)>,
    headers: HeaderMap,
) -> Result<Json<CartOut>, ApiError> {
    let path = format!("/cart/items/{}/{}", collection, item_id);
    let signer = signer_from(&headers, "delete", &path)?;
    let collection = validate_collection(&collection)?;
    let item_id = validate_item_id(&item_id)?;
    state
        .credits
        .remove_item(&signer, &collection, &item_id)
        .await?;
    let cart = state.credits.get_cart(&signer).await?;
    Ok(Json(cart_out(&signer, &cart)))
}

fn idempotency_key(headers: &HeaderMap) -> Result<String, ApiError> {
    let raw = headers
        .get("Idempotency-Key")
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ApiError::bad_request("missing Idempotency-Key header"))?;
    if raw.len() > 200 {
        return Err(ApiError::bad_request("Idempotency-Key too long"));
    }
    Ok(raw.to_string())
}

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "credits/"))]
pub struct CheckoutStartOut {
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    id: i64,
    status: String,
    replayed: bool,
}

pub(crate) fn replay_for_signer(
    prior: Option<CheckoutIdemRow>,
    signer: &str,
) -> Result<Option<CheckoutStartOut>, ApiError> {
    match prior {
        Some(row) if row.address.eq_ignore_ascii_case(signer) => Ok(Some(CheckoutStartOut {
            id: row.id,
            status: row.status,
            replayed: true,
        })),
        Some(_) => Err(ApiError::conflict(
            "Idempotency-Key already used by a different wallet",
        )),
        None => Ok(None),
    }
}

type CheckoutScopeAndIntent = (
    Option<Vec<(String, String)>>,
    Option<(PurchaseIntentIn, String)>,
);

pub(crate) fn seed_quote_cache_after_drift(cache: &QuoteCache, repriced: &[RepricedLine]) {
    for line in repriced {
        cache.put(
            &line.collection.to_ascii_lowercase(),
            &line.item_id,
            Some(line.unit_price_credits.clone()),
        );
    }
}

pub async fn checkout(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Result<Json<CheckoutBody>, axum::extract::rejection::JsonRejection>,
) -> Result<Json<CheckoutStartOut>, ApiError> {
    let signer = signer_from(&headers, "post", "/checkout")?;
    let idem = idempotency_key(&headers)?;

    if let Some(out) = replay_for_signer(
        state
            .credits
            .find_checkout_by_idempotency_key(&idem)
            .await?,
        &signer,
    )? {
        return Ok(Json(out));
    }

    let (scope, signed_intent): CheckoutScopeAndIntent = match body {
        Ok(Json(b)) => {
            let scope = match b.items {
                Some(items) if !items.is_empty() => {
                    let mut v = Vec::with_capacity(items.len());
                    for it in &items {
                        v.push((
                            validate_collection(&it.collection)?,
                            validate_item_id(&it.item_id)?,
                        ));
                    }
                    Some(v)
                }
                _ => None,
            };
            let signed = match (b.intent, b.intent_signature) {
                (Some(i), Some(s)) => Some((i, s)),
                (None, None) => None,
                _ => {
                    return Err(ApiError::bad_request(
                        "intent and intentSignature must be provided together",
                    ))
                }
            };
            (scope, signed)
        }
        Err(_) => (None, None),
    };

    match &signed_intent {
        Some((intent, sig)) => {
            let now = chrono::Utc::now().timestamp().max(0) as u64;
            verify_purchase_intent(intent, sig, &signer, &idem, now)?;
        }
        None => ensure_intent_present(false, state.require_purchase_intent)?,
    }

    if state.usage_grants_pool.is_none() {
        return Err(ApiError::not_implemented(
            "checkout is disabled: the escrow/lease overlay is not configured \
             (USAGE_GRANTS_PG_CONNECTION_STRING unset). Refusing to debit Credits for an item \
             that would be invisible in the backpack.",
        ));
    }

    if state.economy_admin_token.is_none() || state.escrow_address.is_none() {
        return Err(ApiError::not_implemented(
            "checkout is disabled: the broker/escrow fulfilment path is not configured \
             (CATALYRST_ECONOMY_ADMIN_TOKEN / LANDILER_ESCROW_ADDRESS unset). Refusing to debit \
             Credits for an order that cannot be fulfilled.",
        ));
    }

    let cart = state.credits.get_cart(&signer).await?;
    if cart.items.is_empty() {
        return Err(ApiError::bad_request("cart is empty"));
    }

    if let Some(scope) = &scope {
        for (c, i) in scope {
            let found = cart
                .items
                .iter()
                .any(|l| l.collection.eq_ignore_ascii_case(c) && &l.item_id == i);
            if !found {
                return Err(ApiError::bad_request(format!(
                    "scoped item {c}:{i} is not in the cart"
                )));
            }
        }
    }

    let mut repriced: Vec<RepricedLine> = Vec::with_capacity(cart.items.len());
    for line in &cart.items {
        if let Some(scope) = &scope {
            let in_scope = scope
                .iter()
                .any(|(c, i)| line.collection.eq_ignore_ascii_case(c) && &line.item_id == i);
            if !in_scope {
                continue;
            }
        }

        let priced = state
            .pricing
            .price_item_for_mode(
                &state.credits.pool,
                &line.collection,
                &line.item_id,
                &state.checkout_fulfillment_mode,
            )
            .await?;
        ensure_qty_fillable(&priced.basis.kind, line.qty)?;
        let (mode, token_id, trade_id) = match &priced.basis.kind {
            BasisKind::Primary => ("primary", None, None),
            BasisKind::Secondary { token_id } => ("secondary", Some(token_id.clone()), None),
            BasisKind::Trade { trade_id } => ("trade", None, Some(trade_id.clone())),
        };
        repriced.push(RepricedLine {
            item_id: line.item_id.clone(),
            collection: line.collection.clone(),
            urn: priced.basis.info.urn,
            category: priced.basis.info.category,
            qty: line.qty,
            unit_price_credits: priced.credit_price,

            token_id,
            trade_id,
            basis_wei: Some(priced.basis.basis_wei),
            mode: mode.to_string(),
        });
    }

    if repriced.is_empty() {
        return Err(ApiError::bad_request("no items to checkout"));
    }

    if let Some((intent, _)) = &signed_intent {
        if let Err(err) = verify_intent_matches_order(intent, &repriced) {
            if matches!(err, ApiError::Conflict(_)) {
                seed_quote_cache_after_drift(&state.quote_cache, &repriced);
            }
            return Err(err);
        }
    }

    let outcome = state
        .credits
        .run_checkout(&signer, &idem, &repriced)
        .await?;

    Ok(Json(CheckoutStartOut {
        id: outcome.id,
        status: outcome.status,
        replayed: outcome.replayed,
    }))
}

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "credits/"))]
pub struct CheckoutOut {
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    id: i64,
    address: String,
    #[serde(rename = "totalCredits")]
    total_credits: String,
    status: String,
}

pub async fn get_checkout(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    headers: HeaderMap,
) -> Result<Json<CheckoutOut>, ApiError> {
    let path = format!("/checkout/{}", id);
    let signer = signer_from(&headers, "get", &path)?;

    let row = state
        .credits
        .get_checkout(id)
        .await?
        .ok_or_else(|| ApiError::not_found("checkout not found"))?;
    if row.address.to_lowercase() != signer {
        return Err(ApiError::forbidden("checkout does not belong to signer"));
    }

    Ok(Json(CheckoutOut {
        id: row.id,
        address: row.address,
        total_credits: row.total_credits,
        status: row.status,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn wire_identity_cart_populated() {
        let cart = CartView {
            items: vec![
                crate::ports::checkout::CartItemRow {
                    item_id: "12".into(),
                    collection: "0x59a90bad9570ecd08895f132daf7b79696337f61".into(),
                    urn: "urn:decentraland:matic:collections-v2:0x59a9:12".into(),
                    category: "wearable".into(),
                    qty: 2,
                    unit_price_credits: "1.5".into(),
                },
                crate::ports::checkout::CartItemRow {
                    item_id: "3".into(),
                    collection: "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
                    urn: "urn:decentraland:matic:collections-v2:0xaaaa:3".into(),
                    category: "emote".into(),
                    qty: 1,
                    unit_price_credits: "0".into(),
                },
            ],
            total_credits: "3".into(),
        };
        assert_eq!(
            serde_json::to_value(cart_out("0xabc", &cart)).unwrap(),
            json!({
                "address": "0xabc",
                "items": [
                    {
                        "itemId": "12",
                        "collection": "0x59a90bad9570ecd08895f132daf7b79696337f61",
                        "urn": "urn:decentraland:matic:collections-v2:0x59a9:12",
                        "category": "wearable",
                        "qty": 2,
                        "unitPriceCredits": "1.5",
                    },
                    {
                        "itemId": "3",
                        "collection": "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                        "urn": "urn:decentraland:matic:collections-v2:0xaaaa:3",
                        "category": "emote",
                        "qty": 1,
                        "unitPriceCredits": "0",
                    },
                ],
                "totalCredits": "3",
            })
        );
    }

    #[test]
    fn wire_identity_cart_empty() {
        let cart = CartView {
            items: vec![],
            total_credits: "0".into(),
        };
        assert_eq!(
            serde_json::to_value(cart_out("0xabc", &cart)).unwrap(),
            json!({
                "address": "0xabc",
                "items": [],
                "totalCredits": "0",
            })
        );
    }

    #[test]
    fn wire_identity_checkout_start() {
        let fresh = CheckoutStartOut {
            id: 41,
            status: "fulfilling".into(),
            replayed: false,
        };
        assert_eq!(
            serde_json::to_value(&fresh).unwrap(),
            json!({ "id": 41, "status": "fulfilling", "replayed": false })
        );
        let replayed = CheckoutStartOut {
            id: 41,
            status: "fulfilled".into(),
            replayed: true,
        };
        assert_eq!(
            serde_json::to_value(&replayed).unwrap(),
            json!({ "id": 41, "status": "fulfilled", "replayed": true })
        );
    }

    #[test]
    fn wire_identity_checkout_get() {
        let out = CheckoutOut {
            id: 41,
            address: "0xabc".into(),
            total_credits: "1.5".into(),
            status: "fulfilled".into(),
        };
        assert_eq!(
            serde_json::to_value(&out).unwrap(),
            json!({
                "id": 41,
                "address": "0xabc",
                "totalCredits": "1.5",
                "status": "fulfilled",
            })
        );
    }

    #[test]
    fn no_intent_checkout_is_rejected_under_default_config() {
        let require = crate::config::DEFAULT_REQUIRE_PURCHASE_INTENT;
        assert!(require, "intent enforcement must be the default");
        let err = ensure_intent_present(false, require).unwrap_err();
        match &err {
            crate::http::ApiError::Unauthorized(m) => {
                assert!(
                    m.contains("purchase requires a signed intent"),
                    "message must say why: {m}"
                );
            }
            other => panic!("expected 401 Unauthorized, got {other:?}"),
        }
    }

    #[test]
    fn signed_intent_or_explicit_optout_passes_the_gate() {
        ensure_intent_present(true, true).unwrap();
        ensure_intent_present(true, false).unwrap();
        ensure_intent_present(false, false).unwrap();
    }

    fn secondary_kind() -> BasisKind {
        BasisKind::Secondary {
            token_id: "2901".into(),
        }
    }

    fn trade_kind() -> BasisKind {
        BasisKind::Trade {
            trade_id: "1bbe7d78-dd71-4cbe-9085-70d679d3ad11".into(),
        }
    }

    #[test]
    fn secondary_basis_qty_above_one_is_rejected() {
        let err = ensure_qty_fillable(&secondary_kind(), 2).unwrap_err();
        match &err {
            crate::http::ApiError::Conflict(m) => {
                assert!(
                    m.contains("one per checkout"),
                    "message must explain the limit: {m}"
                );
            }
            other => panic!("expected 409 Conflict, got {other:?}"),
        }
        assert!(ensure_qty_fillable(&secondary_kind(), 100).is_err());
    }

    #[test]
    fn trade_basis_is_one_per_checkout_too() {
        assert!(ensure_qty_fillable(&trade_kind(), 2).is_err());
        assert!(ensure_qty_fillable(&trade_kind(), 100).is_err());
        ensure_qty_fillable(&trade_kind(), 1).unwrap();
    }

    #[test]
    fn primary_qty_keeps_real_qty_semantics() {
        ensure_qty_fillable(&BasisKind::Primary, 2).unwrap();
        ensure_qty_fillable(&BasisKind::Primary, 100).unwrap();
        ensure_qty_fillable(&secondary_kind(), 1).unwrap();
    }

    #[test]
    fn replay_decision_same_signer_returns_the_original_checkout() {
        let out = replay_for_signer(
            Some(CheckoutIdemRow {
                id: 41,
                address: "0xAbC0000000000000000000000000000000000001".into(),
                status: "fulfilling".into(),
            }),
            "0xabc0000000000000000000000000000000000001",
        )
        .unwrap()
        .expect("must replay");
        assert_eq!(
            serde_json::to_value(&out).unwrap(),
            json!({ "id": 41, "status": "fulfilling", "replayed": true })
        );
    }

    #[test]
    fn replay_decision_different_signer_is_a_409() {
        let err = replay_for_signer(
            Some(CheckoutIdemRow {
                id: 41,
                address: "0xabc0000000000000000000000000000000000001".into(),
                status: "fulfilled".into(),
            }),
            "0xdef0000000000000000000000000000000000002",
        )
        .unwrap_err();
        assert!(
            matches!(err, crate::http::ApiError::Conflict(_)),
            "got {err:?}"
        );
    }

    #[test]
    fn replay_decision_fresh_key_proceeds_to_the_validation_gates() {
        assert!(replay_for_signer(None, "0xabc").unwrap().is_none());
    }

    #[test]
    fn post_drift_reseed_makes_the_next_quote_serve_checkout_prices() {
        use std::time::Duration;

        let collection = "0x59a90bad9570ecd08895f132daf7b79696337f61";
        let item_id = "12";

        let cache = QuoteCache::new(Duration::from_secs(60), 100);
        cache.put(collection, item_id, Some("1.5".into()));

        let repriced = vec![RepricedLine {
            item_id: item_id.into(),
            collection: collection.to_ascii_uppercase().replace("0X", "0x"),
            urn: "urn:decentraland:matic:collections-v2:0x59a9:12".into(),
            category: "wearable".into(),
            qty: 1,
            unit_price_credits: "2".into(),
            token_id: Some("2901".into()),
            trade_id: None,
            basis_wei: Some("20000000000000000".into()),
            mode: "secondary".into(),
        }];
        let intent = PurchaseIntentIn {
            buyer: "0xf39fd6e51aad88f6f4ce6ab8827279cfffb92266".into(),
            items: crate::purchase_intent::canonical_items(&repriced),
            total_credits: "1.5".into(),
            currency: "CREDITS".into(),
            nonce: "idem-1".into(),
            expires_at: 4_000_000_000,
        };
        let err = verify_intent_matches_order(&intent, &repriced).unwrap_err();
        assert!(
            matches!(err, crate::http::ApiError::Conflict(_)),
            "got {err:?}"
        );

        seed_quote_cache_after_drift(&cache, &repriced);
        assert_eq!(cache.get(collection, item_id), Some(Some("2".into())));
    }

    #[test]
    fn validates_item_id() {
        assert_eq!(validate_item_id(" 0xabc-1 ").unwrap(), "0xabc-1");
        assert!(validate_item_id("").is_err());
        assert!(validate_item_id("   ").is_err());
        assert!(validate_item_id("a\\b").is_err());
        assert!(validate_item_id(&"x".repeat(201)).is_err());
    }

    #[test]
    fn validates_collection() {
        assert_eq!(
            validate_collection(" 0x59A90BAD9570ECD08895F132DAF7B79696337F61 ").unwrap(),
            "0x59a90bad9570ecd08895f132daf7b79696337f61"
        );
        assert_eq!(
            validate_collection("0x59a90bad9570ecd08895f132daf7b79696337f61").unwrap(),
            "0x59a90bad9570ecd08895f132daf7b79696337f61"
        );
        assert!(validate_collection("").is_err());
        assert!(validate_collection("59a90bad9570ecd08895f132daf7b79696337f61").is_err());
        assert!(validate_collection("0x123").is_err());
        assert!(validate_collection(&format!("0x{}", "z".repeat(40))).is_err());
        assert!(validate_collection(&format!("0x{}", "a".repeat(41))).is_err());
    }
}
