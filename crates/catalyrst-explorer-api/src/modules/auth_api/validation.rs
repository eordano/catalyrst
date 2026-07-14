use catalyrst_crypto::auth_chain::parse_ephemeral_payload;
use catalyrst_crypto::recover::recover_address;
use catalyrst_crypto::verify::verify_auth_chain;
use catalyrst_types::AuthChain;
use serde_json::Value;

use super::CreateRequestBody;

const DISALLOWED_METHODS: [&str; 1] = ["dcl_personal_sign"];
const MAX_METHOD_LENGTH: usize = 256;
const MAX_PARAMS_ITEMS: usize = 10;
const EPHEMERAL_ADDRESS_PREFIX: &str = "Ephemeral address: ";
const EXPIRATION_PREFIX: &str = "Expiration: ";

fn is_disallowed_method(method: &str) -> bool {
    let normalized = method.trim().to_lowercase();
    DISALLOWED_METHODS.contains(&normalized.as_str())
}

fn is_ephemeral_message(value: &str) -> bool {
    if is_ephemeral_text(value) {
        return true;
    }
    decode_hex_message(value).is_some_and(|decoded| is_ephemeral_text(&decoded))
}

fn is_ephemeral_text(value: &str) -> bool {
    let normalized = value.replace('\r', "");
    let mut lines = normalized.split('\n').skip(1);
    let (Some(address_line), Some(expiration_line)) = (lines.next(), lines.next()) else {
        return false;
    };
    address_line.starts_with(EPHEMERAL_ADDRESS_PREFIX)
        && expiration_line
            .strip_prefix(EXPIRATION_PREFIX)
            .is_some_and(|expiration| !expiration.trim().is_empty())
}

fn decode_hex_message(value: &str) -> Option<String> {
    let body = value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))?;
    if body.len() % 2 != 0 {
        return None;
    }
    let mut bytes = Vec::with_capacity(body.len() / 2);
    for pair in body.as_bytes().chunks(2) {
        let high = char::from(pair[0]).to_digit(16)?;
        let low = char::from(pair[1]).to_digit(16)?;
        bytes.push(((high << 4) | low) as u8);
    }
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

#[derive(Debug)]
pub(super) struct ValidatedRequest<'a> {
    pub params: &'a [Value],
    pub auth_chain: &'a AuthChain,
}

pub(super) fn validate_request_message(
    body: &CreateRequestBody,
) -> Result<ValidatedRequest<'_>, String> {
    if let Some(property) = body.unknown_fields.keys().next() {
        return Err(format!("Unexpected property: {}", property));
    }

    let params = body
        .params
        .as_deref()
        .ok_or_else(|| "params is required".to_string())?;

    if body.method.chars().count() > MAX_METHOD_LENGTH {
        return Err(format!(
            "method must be at most {} characters",
            MAX_METHOD_LENGTH
        ));
    }

    if params.len() > MAX_PARAMS_ITEMS {
        return Err(format!(
            "params must have at most {} items",
            MAX_PARAMS_ITEMS
        ));
    }

    if is_disallowed_method(&body.method) {
        return Err(format!(
            "The {} method is not allowed",
            body.method.trim().to_lowercase()
        ));
    }

    if params
        .iter()
        .filter_map(|param| param.as_str())
        .any(is_ephemeral_message)
    {
        return Err("Signing a Decentraland ephemeral message is not allowed".into());
    }

    let auth_chain = body
        .auth_chain
        .as_ref()
        .ok_or_else(|| "Auth chain is required".to_string())?;

    Ok(ValidatedRequest { params, auth_chain })
}

pub(super) fn validate_auth_chain(chain: &AuthChain) -> Result<String, String> {
    if chain.is_empty() {
        return Err("Auth chain is required".into());
    }
    let owner = chain
        .first()
        .map(|l| l.payload.clone())
        .ok_or_else(|| "Auth chain is required".to_string())?;
    let final_authority = derive_final_authority(chain)?;
    verify_auth_chain(chain, &final_authority, None).map_err(|e| e.to_string())?;
    Ok(owner)
}

fn derive_final_authority(chain: &AuthChain) -> Result<String, String> {
    use catalyrst_crypto::AuthLinkType;

    let last = chain
        .last()
        .ok_or_else(|| "Auth chain is required".to_string())?;
    match last.link_type {
        AuthLinkType::SIGNER => Ok(last.payload.clone()),
        AuthLinkType::EcdsaEphemeral | AuthLinkType::EcdsaEip1654Ephemeral => {
            let (_, ephemeral, _) = parse_ephemeral_payload(&last.payload)
                .map_err(|e| format!("Could not get final authority from auth chain: {}", e))?;
            Ok(ephemeral)
        }
        AuthLinkType::EcdsaSignedEntity | AuthLinkType::EcdsaEip1654SignedEntity => {
            let sig = last
                .signature
                .as_ref()
                .ok_or_else(|| "Missing signature on final link".to_string())?;
            let recovered = recover_address(last.payload.as_bytes(), sig)
                .map_err(|e| format!("Could not recover signer from final link: {}", e))?;
            Ok(recovered)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Map, Value};

    const TEST_EPHEMERAL_ADDRESS: &str = "0x1234567890123456789012345678901234567890";

    fn ephemeral_message(expiration: &str) -> String {
        format!("Decentraland Login\nEphemeral address: {TEST_EPHEMERAL_ADDRESS}\nExpiration: {expiration}")
    }

    fn hex_encode(value: &str) -> String {
        let mut out = String::from("0x");
        for byte in value.as_bytes() {
            out.push_str(&format!("{byte:02x}"));
        }
        out
    }

    fn stub_auth_chain() -> AuthChain {
        vec![catalyrst_types::AuthLink {
            link_type: catalyrst_crypto::AuthLinkType::SIGNER,
            payload: TEST_EPHEMERAL_ADDRESS.into(),
            signature: None,
        }]
    }

    fn request(
        method: &str,
        params: Vec<Value>,
        auth_chain: Option<AuthChain>,
    ) -> CreateRequestBody {
        CreateRequestBody {
            method: method.into(),
            params: Some(params),
            auth_chain,
            unknown_fields: Map::new(),
        }
    }

    fn parse(body: &str) -> Result<CreateRequestBody, serde_json::Error> {
        serde_json::from_str(body)
    }

    #[test]
    fn dcl_personal_sign_is_disallowed_in_any_casing() {
        assert!(is_disallowed_method("dcl_personal_sign"));
        assert!(is_disallowed_method("DCL_Personal_Sign"));
        assert!(is_disallowed_method("  dcl_personal_sign  "));
        assert!(!is_disallowed_method("personal_sign"));
    }

    #[test]
    fn ephemeral_messages_are_detected_plain_and_hex_encoded() {
        let message = ephemeral_message("2100-01-01T00:00:00.000Z");
        assert!(is_ephemeral_message(&message));
        assert!(is_ephemeral_message(&hex_encode(&message)));
        assert!(is_ephemeral_message(&ephemeral_message(
            "2020-01-01T00:00:00.000Z"
        )));
        assert!(is_ephemeral_message(&format!(
            "Totally harmless greeting\nEphemeral address: {TEST_EPHEMERAL_ADDRESS}\nExpiration: 2100-01-01T00:00:00.000Z"
        )));
        assert!(!is_ephemeral_message("Please sign to confirm your order"));
        assert!(!is_ephemeral_message(
            "0xa9059cbb0000000000000000000000001234567890123456789012345678901234567890"
        ));
    }

    #[test]
    fn ephemeral_detection_outlives_the_strict_payload_parser() {
        let bypass_vectors = [
            ephemeral_message(" 2100-01-01T00:00:00.000Z"),
            ephemeral_message("2100-01-01"),
            ephemeral_message("2100-01-01T00:00Z"),
            format!(
                "Decentraland Login\nEphemeral address: {TEST_EPHEMERAL_ADDRESS}beef\nExpiration: 2100-01-01T00:00:00.000Z"
            ),
        ];
        for vector in bypass_vectors {
            assert!(
                parse_ephemeral_payload(&vector).is_err(),
                "vector no longer exercises the parser gap: {vector:?}"
            );
            assert!(is_ephemeral_message(&vector));
            assert!(is_ephemeral_message(&hex_encode(&vector)));
            for param in [vector.clone(), hex_encode(&vector)] {
                let err = validate_request_message(&request(
                    "personal_sign",
                    vec![Value::from(param)],
                    Some(stub_auth_chain()),
                ))
                .unwrap_err();
                assert_eq!(
                    err,
                    "Signing a Decentraland ephemeral message is not allowed"
                );
            }
        }
    }

    #[test]
    fn ephemeral_detection_needs_an_expiration_value() {
        assert!(!is_ephemeral_message(&ephemeral_message("")));
        assert!(!is_ephemeral_message(&ephemeral_message("   ")));
        assert!(!is_ephemeral_message(
            "Decentraland Login\nEphemeral address: 0x1234567890123456789012345678901234567890"
        ));
        assert!(!is_ephemeral_message(
            "Decentraland Login\nEphemeral address: 0x1234\nExpires: 2100-01-01T00:00:00.000Z"
        ));
    }

    #[test]
    fn carriage_returns_do_not_hide_an_ephemeral_message() {
        let message = ephemeral_message("2100-01-01T00:00:00.000Z").replace('\n', "\r\n");
        assert!(is_ephemeral_message(&message));
        assert!(is_ephemeral_message(&hex_encode(&message)));
    }

    #[test]
    fn create_request_refuses_the_retired_sign_in_method() {
        let err =
            validate_request_message(&request("DCL_Personal_Sign", vec![], None)).unwrap_err();
        assert_eq!(err, "The dcl_personal_sign method is not allowed");
    }

    #[test]
    fn create_request_refuses_ephemeral_messages_under_any_method() {
        let message = ephemeral_message("2100-01-01T00:00:00.000Z");
        for (method, params) in [
            ("personal_sign", vec![Value::from(message.clone())]),
            (
                "personal_sign",
                vec![Value::from(hex_encode(&message).as_str())],
            ),
            (
                "eth_sign",
                vec![
                    Value::from(TEST_EPHEMERAL_ADDRESS),
                    Value::from(message.clone()),
                ],
            ),
        ] {
            let err = validate_request_message(&request(method, params, Some(stub_auth_chain())))
                .unwrap_err();
            assert_eq!(
                err,
                "Signing a Decentraland ephemeral message is not allowed"
            );
        }
    }

    #[test]
    fn create_request_bounds_method_length_and_param_count() {
        let long_method = "a".repeat(MAX_METHOD_LENGTH + 1);
        let err = validate_request_message(&request(&long_method, vec![], Some(stub_auth_chain())))
            .unwrap_err();
        assert_eq!(err, "method must be at most 256 characters");

        let max_method = "a".repeat(MAX_METHOD_LENGTH);
        assert!(
            validate_request_message(&request(&max_method, vec![], Some(stub_auth_chain())))
                .is_ok()
        );

        let too_many = vec![json!({ "data": "test" }); MAX_PARAMS_ITEMS + 1];
        let err = validate_request_message(&request("eth_call", too_many, Some(stub_auth_chain())))
            .unwrap_err();
        assert_eq!(err, "params must have at most 10 items");

        let at_max = vec![json!({ "data": "test" }); MAX_PARAMS_ITEMS];
        assert!(
            validate_request_message(&request("eth_call", at_max, Some(stub_auth_chain()))).is_ok()
        );
    }

    #[test]
    fn create_request_refuses_properties_outside_the_wire_shape() {
        let body = parse(
            r#"{"method":"personal_sign","params":[],"authChain":[],"expiration":"2100-01-01T00:00:00.000Z"}"#,
        )
        .unwrap();
        assert_eq!(
            validate_request_message(&body).unwrap_err(),
            "Unexpected property: expiration"
        );
    }

    #[test]
    fn create_request_requires_the_params_array() {
        let body = parse(r#"{"method":"personal_sign","authChain":[]}"#).unwrap();
        assert_eq!(
            validate_request_message(&body).unwrap_err(),
            "params is required"
        );
    }

    #[test]
    fn create_request_keeps_the_wire_fields_out_of_the_unknown_bucket() {
        let body = parse(
            r#"{"method":"personal_sign","params":["hello"],"authChain":[{"type":"SIGNER","payload":"0x1234567890123456789012345678901234567890"}]}"#,
        )
        .unwrap();
        assert!(body.unknown_fields.is_empty());
        let validated = validate_request_message(&body).unwrap();
        assert_eq!(validated.params, [Value::from("hello")]);
        assert_eq!(validated.auth_chain.len(), 1);
    }

    #[test]
    fn create_request_bounds_run_before_the_method_and_payload_checks() {
        let ephemeral = ephemeral_message("2100-01-01T00:00:00.000Z");
        let body = parse(&format!(
            r#"{{"method":"dcl_personal_sign","params":[{}],"stowaway":1}}"#,
            Value::from(ephemeral.as_str())
        ))
        .unwrap();
        assert_eq!(
            validate_request_message(&body).unwrap_err(),
            "Unexpected property: stowaway"
        );

        let body = parse(r#"{"method":"dcl_personal_sign"}"#).unwrap();
        assert_eq!(
            validate_request_message(&body).unwrap_err(),
            "params is required"
        );

        let over_cap = vec![Value::from("filler"); MAX_PARAMS_ITEMS + 1];
        let err =
            validate_request_message(&request("dcl_personal_sign", over_cap, None)).unwrap_err();
        assert_eq!(err, "params must have at most 10 items");

        let long_method = "a".repeat(MAX_METHOD_LENGTH + 1);
        let err =
            validate_request_message(&request(&long_method, vec![Value::from(ephemeral)], None))
                .unwrap_err();
        assert_eq!(err, "method must be at most 256 characters");
    }

    #[test]
    fn create_request_requires_an_auth_chain_after_the_payload_checks() {
        let err = validate_request_message(&request(
            "eth_sendTransaction",
            vec![json!({ "from": "0x123", "to": "0x456" })],
            None,
        ))
        .unwrap_err();
        assert_eq!(err, "Auth chain is required");

        let err =
            validate_request_message(&request("dcl_personal_sign", vec![], None)).unwrap_err();
        assert_eq!(err, "The dcl_personal_sign method is not allowed");

        let err = validate_request_message(&request(
            "personal_sign",
            vec![Value::from(ephemeral_message("2100-01-01T00:00:00.000Z"))],
            None,
        ))
        .unwrap_err();
        assert_eq!(
            err,
            "Signing a Decentraland ephemeral message is not allowed"
        );
    }

    #[test]
    fn create_request_accepts_ordinary_signing_and_non_signing_methods() {
        assert!(validate_request_message(&request(
            "personal_sign",
            vec![
                Value::from("Please sign to confirm your order"),
                Value::from(TEST_EPHEMERAL_ADDRESS),
            ],
            Some(stub_auth_chain()),
        ))
        .is_ok());
        assert!(validate_request_message(&request(
            "wallet_switchEthereumChain",
            vec![json!({ "chainId": "0x1" })],
            Some(stub_auth_chain()),
        ))
        .is_ok());
    }
}
