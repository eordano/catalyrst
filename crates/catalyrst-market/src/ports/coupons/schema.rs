use serde_json::Value;

use super::errors::CouponError;
use super::types::MAX_COUPON_COLLECTIONS;
use crate::http::params::is_address;

const NETWORKS: [&str; 2] = ["ETHEREUM", "MATIC"];
const SIGNATURE_LEN: usize = 132;
const BYTES32_MAX_LEN: usize = 66;

const COUPON_KEYS: [&str; 9] = [
    "signer",
    "chainId",
    "network",
    "checks",
    "couponAddress",
    "discountType",
    "discount",
    "collections",
    "signature",
];

const CHECKS_KEYS: [&str; 9] = [
    "uses",
    "expiration",
    "effective",
    "salt",
    "contractSignatureIndex",
    "signerSignatureIndex",
    "allowedRoot",
    "allowedProof",
    "externalChecks",
];

fn invalid(why: impl Into<String>) -> CouponError {
    CouponError::InvalidBody(why.into())
}

fn is_hex_pairs(value: &str) -> bool {
    let Some(body) = value.strip_prefix("0x") else {
        return false;
    };
    body.len() % 2 == 0 && body.bytes().all(|b| b.is_ascii_hexdigit())
}

fn field<'a>(object: &'a Value, key: &str) -> Result<&'a Value, CouponError> {
    object
        .get(key)
        .filter(|v| !v.is_null())
        .ok_or_else(|| invalid(format!("{key} is required")))
}

fn string_field<'a>(object: &'a Value, key: &str) -> Result<&'a str, CouponError> {
    field(object, key)?
        .as_str()
        .ok_or_else(|| invalid(format!("{key} must be a string")))
}

fn address_field(object: &Value, key: &str) -> Result<(), CouponError> {
    let value = string_field(object, key)?;
    if !is_address(value) {
        return Err(invalid(format!("{key} must be an address")));
    }
    Ok(())
}

fn positive_integer_field(object: &Value, key: &str) -> Result<(), CouponError> {
    let value = field(object, key)?;
    let Some(number) = value.as_f64().filter(|number| number.fract() == 0.0) else {
        return Err(invalid(format!("{key} must be an integer")));
    };
    if number < 1.0 {
        return Err(invalid(format!("{key} must be at least 1")));
    }
    Ok(())
}

fn bytes32_field(checks: &Value, key: &str) -> Result<(), CouponError> {
    let value = string_field(checks, key)?;
    if value.len() > BYTES32_MAX_LEN || !is_hex_pairs(value) {
        return Err(invalid(format!("checks.{key} must be a hex word")));
    }
    Ok(())
}

fn validate_collections(object: &Value) -> Result<(), CouponError> {
    let value = field(object, "collections")?;
    let Some(collections) = value.as_array() else {
        return Err(invalid("collections must be an array"));
    };
    if collections.is_empty() {
        return Err(invalid("collections must carry at least one address"));
    }
    if collections.len() > MAX_COUPON_COLLECTIONS {
        return Err(invalid(format!(
            "collections may carry at most {MAX_COUPON_COLLECTIONS} addresses"
        )));
    }
    let mut seen = std::collections::HashSet::with_capacity(collections.len());
    for entry in collections {
        let Some(collection) = entry.as_str() else {
            return Err(invalid("every collection must be a string"));
        };
        if !is_address(collection) {
            return Err(invalid("every collection must be an address"));
        }
        if !seen.insert(collection) {
            return Err(invalid("collections must not repeat an entry"));
        }
    }
    Ok(())
}

fn validate_signature(object: &Value) -> Result<(), CouponError> {
    let signature = string_field(object, "signature")?;
    if signature.len() != SIGNATURE_LEN || !is_hex_pairs(signature) {
        return Err(invalid("signature must be 65 hex-encoded bytes"));
    }
    Ok(())
}

/// Upstream gates the body with `CouponCreationSchema` before the handler runs, so a malformed
/// post never reaches the component; this is that gate, spelled out. It runs on the raw body
/// because the rejections it owns -- an unknown key, a repeated collection, a lower-case network
/// -- are all invisible once serde has coerced the body into `CouponCreation`.
pub fn validate_creation_schema(raw: &Value) -> Result<(), CouponError> {
    let Some(object) = raw.as_object() else {
        return Err(invalid("the coupon body must be an object"));
    };
    if let Some(unknown) = object
        .keys()
        .find(|key| !COUPON_KEYS.contains(&key.as_str()))
    {
        return Err(invalid(format!("unknown coupon field {unknown}")));
    }

    address_field(raw, "signer")?;
    address_field(raw, "couponAddress")?;

    let chain_id = field(raw, "chainId")?;
    if !chain_id.is_number() {
        return Err(invalid("chainId must be a number"));
    }

    let network = string_field(raw, "network")?;
    if !NETWORKS.contains(&network) {
        return Err(invalid("network must be ETHEREUM or MATIC"));
    }

    positive_integer_field(raw, "discountType")?;
    positive_integer_field(raw, "discount")?;
    validate_collections(raw)?;
    validate_signature(raw)?;

    let checks = field(raw, "checks")?;
    let Some(checks_object) = checks.as_object() else {
        return Err(invalid("checks must be an object"));
    };
    if let Some(unknown) = checks_object
        .keys()
        .find(|key| !CHECKS_KEYS.contains(&key.as_str()))
    {
        return Err(invalid(format!("unknown checks field {unknown}")));
    }
    bytes32_field(checks, "salt")?;
    bytes32_field(checks, "allowedRoot")?;
    field(checks, "externalChecks")?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const COLLECTION: &str = "0x4c09495cd2d4e3d3fa2808eb655d013de426157b";
    const OTHER_COLLECTION: &str = "0xb0d0d31910da4a14d4e05a9d51b6e9a99a85d676";

    fn body() -> Value {
        json!({
            "signer": "0x1111111111111111111111111111111111111111",
            "chainId": 137,
            "network": "MATIC",
            "checks": {
                "uses": 10,
                "expiration": 1_800_000_000_000i64,
                "effective": 0,
                "salt": format!("0x{}", "11".repeat(32)),
                "contractSignatureIndex": 0,
                "signerSignatureIndex": 0,
                "allowedRoot": format!("0x{}", "00".repeat(32)),
                "externalChecks": []
            },
            "couponAddress": "0x2222222222222222222222222222222222222222",
            "discountType": 1,
            "discount": 300_000,
            "collections": [COLLECTION],
            "signature": format!("0x{}1b", "00".repeat(64))
        })
    }

    fn refusal(value: Value) -> String {
        validate_creation_schema(&value)
            .expect_err("the schema refuses this body")
            .to_string()
    }

    #[test]
    fn the_body_the_shop_posts_passes() {
        validate_creation_schema(&body()).expect("a well-formed body");
    }

    #[test]
    fn the_network_enum_is_case_sensitive() {
        let mut value = body();
        value["network"] = json!("matic");
        assert_eq!(refusal(value), "network must be ETHEREUM or MATIC");
    }

    #[test]
    fn an_exactly_repeated_collection_is_refused_before_the_dedupe() {
        let mut value = body();
        value["collections"] = json!([COLLECTION, COLLECTION]);
        assert_eq!(refusal(value), "collections must not repeat an entry");
    }

    #[test]
    fn a_differently_cased_duplicate_is_left_to_the_component() {
        let mut value = body();
        value["collections"] = json!([COLLECTION, COLLECTION.to_uppercase().replace("0X", "0x")]);
        validate_creation_schema(&value).expect("two spellings are two raw strings");
    }

    #[test]
    fn the_posted_array_is_bounded_before_it_is_deduped() {
        let mut value = body();
        let mut many: Vec<String> = Vec::new();
        for i in 0..=MAX_COUPON_COLLECTIONS {
            many.push(format!("0x{:040x}", i % 2));
        }
        value["collections"] = json!(many);
        assert_eq!(
            refusal(value),
            format!("collections may carry at most {MAX_COUPON_COLLECTIONS} addresses")
        );
    }

    #[test]
    fn an_empty_collection_list_is_refused() {
        let mut value = body();
        value["collections"] = json!([]);
        assert_eq!(
            refusal(value),
            "collections must carry at least one address"
        );
    }

    #[test]
    fn a_collection_that_is_not_an_address_is_refused() {
        let mut value = body();
        value["collections"] = json!([COLLECTION, "not-an-address"]);
        assert_eq!(refusal(value), "every collection must be an address");
    }

    #[test]
    fn external_checks_must_be_sent_even_when_empty() {
        let mut value = body();
        value["checks"]
            .as_object_mut()
            .expect("checks is an object")
            .remove("externalChecks");
        assert_eq!(refusal(value), "externalChecks is required");
    }

    #[test]
    fn an_allowed_root_that_is_not_hex_is_refused() {
        for spelling in ["0xzz", "0x111", "not hex"] {
            let mut value = body();
            value["checks"]["allowedRoot"] = json!(spelling);
            assert_eq!(
                refusal(value),
                "checks.allowedRoot must be a hex word",
                "{spelling}"
            );
        }
    }

    #[test]
    fn an_over_long_salt_is_refused() {
        let mut value = body();
        value["checks"]["salt"] = json!(format!("0x{}", "11".repeat(33)));
        assert_eq!(refusal(value), "checks.salt must be a hex word");
    }

    #[test]
    fn an_unknown_top_level_field_is_refused() {
        let mut value = body();
        value["beneficiary"] = json!(OTHER_COLLECTION);
        assert_eq!(refusal(value), "unknown coupon field beneficiary");
    }

    #[test]
    fn an_unknown_checks_field_is_refused() {
        let mut value = body();
        value["checks"]["beneficiary"] = json!(OTHER_COLLECTION);
        assert_eq!(refusal(value), "unknown checks field beneficiary");
    }

    #[test]
    fn allowed_proof_is_a_checks_field_the_creator_may_send() {
        let mut value = body();
        value["checks"]["allowedProof"] = json!([format!("0x{}", "22".repeat(32))]);
        validate_creation_schema(&value).expect("allowedProof rides the trade checks");
    }

    #[test]
    fn the_discount_fields_must_be_integers_of_at_least_one() {
        let mut value = body();
        value["discount"] = json!(0);
        assert_eq!(refusal(value), "discount must be at least 1");
        let mut value = body();
        value["discountType"] = json!("rate");
        assert_eq!(refusal(value), "discountType must be an integer");
    }

    #[test]
    fn a_discount_spelled_with_a_zero_fraction_is_an_integer() {
        let mut value = body();
        value["discount"] = json!(300_000.0);
        value["discountType"] = json!(1.0);
        validate_creation_schema(&value).expect("json schema integers accept 300000.0");
        let mut value = body();
        value["discount"] = json!(300_000.5);
        assert_eq!(refusal(value), "discount must be an integer");
    }

    #[test]
    fn a_signature_of_the_wrong_width_is_refused() {
        for spelling in [
            format!("0x{}", "00".repeat(64)),
            format!("0x{}1b1b", "00".repeat(64)),
            format!("0x{}zz", "00".repeat(64)),
        ] {
            let mut value = body();
            value["signature"] = json!(spelling);
            assert_eq!(
                refusal(value),
                "signature must be 65 hex-encoded bytes",
                "{spelling}"
            );
        }
    }

    #[test]
    fn the_signer_and_the_coupon_address_must_be_addresses() {
        let mut value = body();
        value["signer"] = json!("0x1234");
        assert_eq!(refusal(value), "signer must be an address");
        let mut value = body();
        value["couponAddress"] = json!(null);
        assert_eq!(refusal(value), "couponAddress is required");
    }
}
