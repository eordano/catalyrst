use alloy_primitives::{keccak256, Address, U256};
use serde::{Deserialize, Serialize};

use crate::http::ApiError;
use crate::ports::checkout::RepricedLine;

pub const INTENT_DOMAIN_NAME: &str = "Catalyst Checkout";
pub const INTENT_DOMAIN_VERSION: &str = "1";
pub const INTENT_CHAIN_ID: u64 = 137;
pub const INTENT_CURRENCY: &str = "CREDITS";

pub const INTENT_MAX_TTL_SECS: u64 = 24 * 60 * 60;

const DOMAIN_TYPE: &str = "EIP712Domain(string name,string version,uint256 chainId)";
const INTENT_TYPE: &str = "PurchaseIntent(address buyer,string items,string totalCredits,\
string currency,string nonce,uint256 expiresAt)";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "credits/"))]
pub struct PurchaseIntentIn {
    pub buyer: String,

    pub items: String,

    #[serde(rename = "totalCredits")]
    pub total_credits: String,

    pub currency: String,

    pub nonce: String,

    #[serde(rename = "expiresAt")]
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub expires_at: u64,
}

pub fn canonical_items(lines: &[RepricedLine]) -> String {
    let mut tuples: Vec<(String, String, i32)> = lines
        .iter()
        .map(|l| (l.collection.to_ascii_lowercase(), l.item_id.clone(), l.qty))
        .collect();
    tuples.sort();
    serde_json::to_string(&tuples).unwrap_or_default()
}

fn domain_separator() -> [u8; 32] {
    let mut enc = Vec::with_capacity(32 * 4);
    enc.extend_from_slice(keccak256(DOMAIN_TYPE.as_bytes()).as_slice());
    enc.extend_from_slice(keccak256(INTENT_DOMAIN_NAME.as_bytes()).as_slice());
    enc.extend_from_slice(keccak256(INTENT_DOMAIN_VERSION.as_bytes()).as_slice());
    enc.extend_from_slice(&U256::from(INTENT_CHAIN_ID).to_be_bytes::<32>());
    keccak256(&enc).0
}

pub fn intent_digest(intent: &PurchaseIntentIn) -> Result<[u8; 32], ApiError> {
    let buyer: Address = intent
        .buyer
        .parse()
        .map_err(|_| ApiError::bad_request("purchase intent buyer is not a valid address"))?;

    let mut enc = Vec::with_capacity(32 * 7);
    enc.extend_from_slice(keccak256(INTENT_TYPE.as_bytes()).as_slice());
    let mut buyer_word = [0u8; 32];
    buyer_word[12..].copy_from_slice(buyer.as_slice());
    enc.extend_from_slice(&buyer_word);
    enc.extend_from_slice(keccak256(intent.items.as_bytes()).as_slice());
    enc.extend_from_slice(keccak256(intent.total_credits.as_bytes()).as_slice());
    enc.extend_from_slice(keccak256(intent.currency.as_bytes()).as_slice());
    enc.extend_from_slice(keccak256(intent.nonce.as_bytes()).as_slice());
    enc.extend_from_slice(&U256::from(intent.expires_at).to_be_bytes::<32>());
    let struct_hash = keccak256(&enc);

    let mut msg = Vec::with_capacity(2 + 32 + 32);
    msg.extend_from_slice(&[0x19, 0x01]);
    msg.extend_from_slice(&domain_separator());
    msg.extend_from_slice(struct_hash.as_slice());
    Ok(keccak256(&msg).0)
}

pub fn verify_purchase_intent(
    intent: &PurchaseIntentIn,
    signature: &str,
    authenticated_signer: &str,
    idempotency_key: &str,
    now_secs: u64,
) -> Result<(), ApiError> {
    if intent.currency != INTENT_CURRENCY {
        return Err(ApiError::bad_request(format!(
            "purchase intent currency must be {INTENT_CURRENCY}"
        )));
    }
    if !intent.buyer.eq_ignore_ascii_case(authenticated_signer) {
        return Err(ApiError::forbidden(
            "purchase intent buyer does not match the authenticated wallet",
        ));
    }
    if intent.nonce != idempotency_key {
        return Err(ApiError::bad_request(
            "purchase intent nonce must equal the Idempotency-Key header",
        ));
    }
    if intent.expires_at <= now_secs {
        return Err(ApiError::bad_request(
            "purchase intent has expired — please review and sign the purchase again",
        ));
    }
    if intent.expires_at - now_secs > INTENT_MAX_TTL_SECS {
        return Err(ApiError::bad_request(
            "purchase intent expiry is too far in the future",
        ));
    }

    let digest = intent_digest(intent)?;
    let recovered = catalyrst_crypto::recover::recover_address_from_digest(&digest, signature)
        .map_err(|e| ApiError::unauthorized(format!("invalid purchase intent signature: {e}")))?;
    if !recovered.eq_ignore_ascii_case(&intent.buyer) {
        return Err(ApiError::unauthorized(
            "purchase intent signature does not match the buyer wallet",
        ));
    }
    Ok(())
}

pub fn verify_intent_matches_order(
    intent: &PurchaseIntentIn,
    repriced: &[RepricedLine],
) -> Result<(), ApiError> {
    let canonical = canonical_items(repriced);
    if intent.items != canonical {
        return Err(ApiError::conflict(
            "the items in this order changed after the purchase was signed — please review and \
             sign again",
        ));
    }
    let total = lines_total(repriced).ok_or_else(|| {
        ApiError::Internal("could not compute the order total for intent verification".into())
    })?;
    if !decimal_eq(&intent.total_credits, &total) {
        return Err(ApiError::conflict(format!(
            "the order total changed after the purchase was signed (signed {}, current {}) — \
             please review and sign again",
            intent.total_credits, total
        )));
    }
    Ok(())
}

fn parse_decimal(s: &str) -> Option<(u128, u32)> {
    let s = s.trim();
    let (int_part, frac_part) = match s.split_once('.') {
        Some((i, f)) => (i, f),
        None => (s, ""),
    };
    if int_part.is_empty() && frac_part.is_empty() {
        return None;
    }
    if int_part.len() > 30 || frac_part.len() > 18 {
        return None;
    }
    if !int_part.bytes().all(|b| b.is_ascii_digit())
        || !frac_part.bytes().all(|b| b.is_ascii_digit())
    {
        return None;
    }
    let mut mantissa: u128 = 0;
    for b in int_part.bytes().chain(frac_part.bytes()) {
        mantissa = mantissa.checked_mul(10)?.checked_add((b - b'0') as u128)?;
    }
    Some((mantissa, frac_part.len() as u32))
}

fn rescale(mantissa: u128, from: u32, to: u32) -> Option<u128> {
    let mut m = mantissa;
    for _ in from..to {
        m = m.checked_mul(10)?;
    }
    Some(m)
}

fn decimal_eq(a: &str, b: &str) -> bool {
    match (parse_decimal(a), parse_decimal(b)) {
        (Some((ma, sa)), Some((mb, sb))) => {
            let s = sa.max(sb);
            matches!(
                (rescale(ma, sa, s), rescale(mb, sb, s)),
                (Some(x), Some(y)) if x == y
            )
        }
        _ => false,
    }
}

fn lines_total(lines: &[RepricedLine]) -> Option<String> {
    let mut acc: u128 = 0;
    let mut scale: u32 = 0;
    for l in lines {
        if l.qty < 0 {
            return None;
        }
        let (m, s) = parse_decimal(&l.unit_price_credits)?;
        let line = m.checked_mul(l.qty as u128)?;
        if s > scale {
            acc = rescale(acc, scale, s)?;
            scale = s;
        }
        acc = acc.checked_add(rescale(line, s, scale)?)?;
    }
    Some(format_scaled(acc, scale))
}

fn format_scaled(mantissa: u128, scale: u32) -> String {
    if scale == 0 {
        return mantissa.to_string();
    }
    let digits = mantissa.to_string();
    let scale = scale as usize;
    let padded = format!("{:0>width$}", digits, width = scale + 1);
    let (int_part, frac_part) = padded.split_at(padded.len() - scale);
    let frac_part = frac_part.trim_end_matches('0');
    if frac_part.is_empty() {
        int_part.to_string()
    } else {
        format!("{int_part}.{frac_part}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const VECTOR_DOMAIN_NAME_KECCAK: &str =
        "94b9b84cc9e8d44af6ca4323f5b15defda634248bfaebca980d77354327751be";
    const VECTOR_SIGNER: &str = "0xf39fd6e51aad88f6f4ce6ab8827279cfffb92266";
    const VECTOR_ITEMS: &str = r#"[["0x59a90bad9570ecd08895f132daf7b79696337f61","12",2],["0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","3",1]]"#;
    const VECTOR_NONCE: &str = "idem-vector-0001";
    const VECTOR_EXPIRES_AT: u64 = 1_767_225_600;
    const VECTOR_DIGEST: &str = "cc577fb0844b7f8a0163e4daf32481bed2beca29d87c1634aa43b42ed34bca1c";
    const VECTOR_SIG: &str = "0x29ababcea69bb9464958c8ccd3b34dce8c82c44c52e80ec0c02a49891344d8da6951071404ce24a3975a4111511adf8bd313d3e9ab5072e89299e2f351800ef71b";

    fn vector_intent() -> PurchaseIntentIn {
        PurchaseIntentIn {
            buyer: VECTOR_SIGNER.into(),
            items: VECTOR_ITEMS.into(),
            total_credits: "3".into(),
            currency: "CREDITS".into(),
            nonce: VECTOR_NONCE.into(),
            expires_at: VECTOR_EXPIRES_AT,
        }
    }

    fn vector_lines() -> Vec<RepricedLine> {
        vec![
            RepricedLine {
                item_id: "3".into(),
                collection: "0xAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".into(),
                urn: "urn:decentraland:matic:collections-v2:0xaaaa:3".into(),
                category: "emote".into(),
                qty: 1,
                unit_price_credits: "0".into(),
                token_id: None,
                trade_id: None,
                basis_wei: None,
                mode: "secondary".into(),
            },
            RepricedLine {
                item_id: "12".into(),
                collection: "0x59a90bad9570ecd08895f132daf7b79696337f61".into(),
                urn: "urn:decentraland:matic:collections-v2:0x59a9:12".into(),
                category: "wearable".into(),
                qty: 2,
                unit_price_credits: "1.5".into(),
                token_id: None,
                trade_id: None,
                basis_wei: None,
                mode: "secondary".into(),
            },
        ]
    }

    const NOW: u64 = VECTOR_EXPIRES_AT - 600;

    fn vector_domain_compiled_in() -> bool {
        hex::encode(keccak256(INTENT_DOMAIN_NAME.as_bytes())) == VECTOR_DOMAIN_NAME_KECCAK
    }

    #[test]
    fn ts_signed_vector_digest_matches() {
        if !vector_domain_compiled_in() {
            return;
        }
        let digest = intent_digest(&vector_intent()).unwrap();
        assert_eq!(hex::encode(digest), VECTOR_DIGEST);
    }

    #[test]
    fn ts_signed_vector_recovers_buyer() {
        if !vector_domain_compiled_in() {
            return;
        }
        let digest = intent_digest(&vector_intent()).unwrap();
        let recovered =
            catalyrst_crypto::recover::recover_address_from_digest(&digest, VECTOR_SIG).unwrap();
        assert_eq!(recovered, VECTOR_SIGNER);
    }

    #[test]
    fn ts_signed_vector_full_verification_passes() {
        if !vector_domain_compiled_in() {
            return;
        }
        verify_purchase_intent(
            &vector_intent(),
            VECTOR_SIG,
            VECTOR_SIGNER,
            VECTOR_NONCE,
            NOW,
        )
        .expect("vector must verify");
        verify_intent_matches_order(&vector_intent(), &vector_lines())
            .expect("vector order must match");
    }

    #[test]
    fn tampered_total_is_rejected() {
        let mut intent = vector_intent();
        intent.total_credits = "4".into();
        let err = verify_purchase_intent(&intent, VECTOR_SIG, VECTOR_SIGNER, VECTOR_NONCE, NOW)
            .unwrap_err();
        assert!(matches!(err, ApiError::Unauthorized(_)), "got {err:?}");
    }

    #[test]
    fn buyer_mismatch_is_rejected() {
        let other = "0x1111111111111111111111111111111111111111";
        let err = verify_purchase_intent(&vector_intent(), VECTOR_SIG, other, VECTOR_NONCE, NOW)
            .unwrap_err();
        assert!(matches!(err, ApiError::Forbidden(_)), "got {err:?}");
    }

    #[test]
    fn nonce_mismatch_is_rejected() {
        let err = verify_purchase_intent(
            &vector_intent(),
            VECTOR_SIG,
            VECTOR_SIGNER,
            "other-key",
            NOW,
        )
        .unwrap_err();
        assert!(matches!(err, ApiError::BadRequest(_)), "got {err:?}");
    }

    #[test]
    fn expired_intent_is_rejected() {
        let err = verify_purchase_intent(
            &vector_intent(),
            VECTOR_SIG,
            VECTOR_SIGNER,
            VECTOR_NONCE,
            VECTOR_EXPIRES_AT,
        )
        .unwrap_err();
        assert!(matches!(err, ApiError::BadRequest(_)), "got {err:?}");
    }

    #[test]
    fn far_future_expiry_is_rejected() {
        let err = verify_purchase_intent(
            &vector_intent(),
            VECTOR_SIG,
            VECTOR_SIGNER,
            VECTOR_NONCE,
            VECTOR_EXPIRES_AT - INTENT_MAX_TTL_SECS - 1,
        )
        .unwrap_err();
        assert!(matches!(err, ApiError::BadRequest(_)), "got {err:?}");
    }

    #[test]
    fn wrong_currency_is_rejected() {
        let mut intent = vector_intent();
        intent.currency = "MANA".into();
        let err = verify_purchase_intent(&intent, VECTOR_SIG, VECTOR_SIGNER, VECTOR_NONCE, NOW)
            .unwrap_err();
        assert!(matches!(err, ApiError::BadRequest(_)), "got {err:?}");
    }

    #[test]
    fn malformed_signature_is_rejected() {
        let err = verify_purchase_intent(
            &vector_intent(),
            "0xdeadbeef",
            VECTOR_SIGNER,
            VECTOR_NONCE,
            NOW,
        )
        .unwrap_err();
        assert!(matches!(err, ApiError::Unauthorized(_)), "got {err:?}");
    }

    #[test]
    fn canonical_items_sorts_and_lowercases() {
        assert_eq!(canonical_items(&vector_lines()), VECTOR_ITEMS);
    }

    #[test]
    fn order_binding_rejects_changed_items() {
        let mut lines = vector_lines();
        lines[0].item_id = "99".into();
        let err = verify_intent_matches_order(&vector_intent(), &lines).unwrap_err();
        assert!(matches!(err, ApiError::Conflict(_)), "got {err:?}");
    }

    #[test]
    fn order_binding_rejects_changed_total() {
        let mut lines = vector_lines();
        lines[1].unit_price_credits = "1.6".into();
        let err = verify_intent_matches_order(&vector_intent(), &lines).unwrap_err();
        assert!(matches!(err, ApiError::Conflict(_)), "got {err:?}");
    }

    #[test]
    fn listing_move_between_quote_and_checkout_is_a_409() {
        let mut lines = vector_lines();
        lines[1].unit_price_credits = "2".into();
        lines[1].token_id = Some("2902".into());
        lines[1].basis_wei = Some("20000000000000000".into());
        let err = verify_intent_matches_order(&vector_intent(), &lines).unwrap_err();
        assert!(matches!(err, ApiError::Conflict(_)), "got {err:?}");
    }

    #[test]
    fn pinned_basis_does_not_perturb_the_signed_binding() {
        let mut lines = vector_lines();
        lines[1].token_id = Some("2901".into());
        lines[1].basis_wei = Some("10000000000000000".into());
        verify_intent_matches_order(&vector_intent(), &lines)
            .expect("pinning is invisible to the signature");
    }

    #[test]
    fn order_binding_tolerates_formatting_drift() {
        let mut intent = vector_intent();
        intent.total_credits = "3.00".into();
        verify_intent_matches_order(&intent, &vector_lines()).expect("3.00 == 3");
    }

    #[test]
    fn decimal_math() {
        assert!(decimal_eq("1.5", "1.50"));
        assert!(decimal_eq("3", "3.000"));
        assert!(decimal_eq("0", "0.0"));
        assert!(!decimal_eq("0.1", "0.11"));
        assert!(!decimal_eq("1", "2"));
        assert!(!decimal_eq("-1", "-1"));
        assert!(!decimal_eq("abc", "abc"));
        assert!(!decimal_eq("", ""));

        assert_eq!(parse_decimal("1.5"), Some((15, 1)));
        assert_eq!(parse_decimal("150"), Some((150, 0)));
        assert_eq!(parse_decimal("0.01"), Some((1, 2)));
        assert_eq!(parse_decimal("1.5.0"), None);

        assert_eq!(format_scaled(15, 1), "1.5");
        assert_eq!(format_scaled(300, 2), "3");
        assert_eq!(format_scaled(1, 2), "0.01");
        assert_eq!(format_scaled(0, 3), "0");
    }

    #[test]
    fn lines_total_matches_sql_semantics() {
        assert_eq!(lines_total(&vector_lines()).unwrap(), "3");
        assert_eq!(lines_total(&[]).unwrap(), "0");
        let mut lines = vector_lines();
        lines[1].unit_price_credits = "1.25".into();
        assert_eq!(lines_total(&lines).unwrap(), "2.5");
    }

    #[test]
    fn wire_identity_purchase_intent_in() {
        let parsed: PurchaseIntentIn = serde_json::from_value(json!({
            "buyer": VECTOR_SIGNER,
            "items": VECTOR_ITEMS,
            "totalCredits": "3",
            "currency": "CREDITS",
            "nonce": VECTOR_NONCE,
            "expiresAt": 1767225600u64,
        }))
        .unwrap();
        assert_eq!(parsed.total_credits, "3");
        assert_eq!(parsed.expires_at, VECTOR_EXPIRES_AT);
        assert_eq!(
            serde_json::to_value(&parsed).unwrap(),
            json!({
                "buyer": VECTOR_SIGNER,
                "items": VECTOR_ITEMS,
                "totalCredits": "3",
                "currency": "CREDITS",
                "nonce": VECTOR_NONCE,
                "expiresAt": 1767225600u64,
            })
        );
    }
}
