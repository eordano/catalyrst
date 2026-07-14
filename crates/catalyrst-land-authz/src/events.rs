pub const LAND_REGISTRY_MAINNET: &str = "0xf87e31492faf9a91b02ee0deaad50d51d56d5d4d";
pub const ESTATE_REGISTRY_MAINNET: &str = "0x959e104e1a4db6317fa58f8295f586e1a978c297";
pub const LAND_REGISTRY_SEPOLIA: &str = "0x42f4ba48791e2de32f5fbf553441c2672864bb33";
pub const ESTATE_REGISTRY_SEPOLIA: &str = "0x369a7fbe718c870c79f99fb423882e8dd8b20486";

pub const LAND_START_BLOCK_MAINNET: u64 = 4_944_642;

pub const TOPIC_UPDATE_OPERATOR: &str =
    "0x9d9dd80a56a16f715df6eb40b771e24ff8cbea6eed9de28473ce0f28fe5602a9";
pub const TOPIC_UPDATE_MANAGER: &str =
    "0xd79fbfe1644c022b9150727d871532bfcc3e27ffee86fc596a062770ac97b042";
pub const TOPIC_APPROVAL_FOR_ALL: &str =
    "0x17307eab39ab6107e8899845ad3d59bd9653f200f220920489ca2b5937696c31";
pub const TOPIC_APPROVAL: &str =
    "0x8c5be1e5ebec7d5bd14f71427d1e84f3dd0314c0f7b2291e5b200ac8c7c3b925";

// LANDRegistry predates the final ERC721 event set and emits three Transfer
// shapes; all three move a token and so all three clear its per-token rights.
pub const TOPIC_TRANSFER_3: &str =
    "0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef";
pub const TOPIC_TRANSFER_5: &str =
    "0xd5c97f2e041b2046be3b4337472f05720760a198f4d7d84980b7155eec7cca6f";
pub const TOPIC_TRANSFER_6: &str =
    "0x8988d59efc2c4547ef86c88f6543963bab0cea94f8e486e619c7c3a790db93be";

pub const ALL_TOPICS: [&str; 7] = [
    TOPIC_UPDATE_OPERATOR,
    TOPIC_UPDATE_MANAGER,
    TOPIC_APPROVAL_FOR_ALL,
    TOPIC_APPROVAL,
    TOPIC_TRANSFER_3,
    TOPIC_TRANSFER_5,
    TOPIC_TRANSFER_6,
];

pub const KIND_UPDATE_OPERATOR: &str = "update_operator";
pub const KIND_APPROVAL: &str = "approval";
pub const KIND_TRANSFER: &str = "transfer";
pub const KIND_UPDATE_MANAGER: &str = "update_manager";
pub const KIND_APPROVED_FOR_ALL: &str = "approved_for_all";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthzEvent {
    pub block_number: i64,
    pub log_index: i32,
    pub block_time: i64,
    pub token_address: String,
    pub kind: &'static str,
    pub token_id: Option<String>,
    pub account: Option<String>,
    pub operator: Option<String>,
    pub approved: Option<bool>,
}

#[derive(Debug, thiserror::Error)]
pub enum DecodeError {
    #[error("log has no topic0")]
    NoTopic,
    #[error("unhandled topic {0}")]
    UnhandledTopic(String),
    #[error("topic {topic} needs {want} topics, log has {got}")]
    TopicArity {
        topic: &'static str,
        want: usize,
        got: usize,
    },
    #[error("could not read a {field} from the log")]
    Field { field: &'static str },
}

fn addr_from_topic(topic: &str) -> Option<String> {
    let hex = topic.strip_prefix("0x")?;
    if hex.len() != 64 {
        return None;
    }
    Some(format!("0x{}", &hex[24..]).to_lowercase())
}

fn u256_topic_to_decimal(topic: &str) -> Option<String> {
    let hex = topic.strip_prefix("0x")?;
    if hex.len() != 64 {
        return None;
    }
    let mut digits: Vec<u8> = vec![0];
    for c in hex.chars() {
        let nib = c.to_digit(16)?;
        let mut carry = nib;
        for d in digits.iter_mut() {
            let v = (*d as u32) * 16 + carry;
            *d = (v % 10) as u8;
            carry = v / 10;
        }
        while carry > 0 {
            digits.push((carry % 10) as u8);
            carry /= 10;
        }
    }
    while digits.len() > 1 && *digits.last().unwrap() == 0 {
        digits.pop();
    }
    Some(digits.iter().rev().map(|d| (b'0' + d) as char).collect())
}

fn first_word_bool(data: &str) -> Option<bool> {
    let hex = data.strip_prefix("0x")?;
    if hex.len() < 64 {
        return None;
    }
    Some(hex[..64].chars().any(|c| c != '0'))
}

/// LAND token ids pack the signed coordinates as two 128-bit halves; the
/// contract's own `decodeTokenId` is this shift-and-sign-extend, and the
/// reverse lookup needs coordinates without a round trip to the chain.
pub fn decode_land_token_id(token_id_hex: &str) -> Option<(i32, i32)> {
    let hex = token_id_hex.strip_prefix("0x")?;
    if hex.len() != 64 {
        return None;
    }
    let hi = u128::from_str_radix(&hex[..32], 16).ok()?;
    let lo = u128::from_str_radix(&hex[32..], 16).ok()?;
    let sign_extend = |v: u128| -> i32 { (v & 0xffff_ffff) as u32 as i32 };
    Some((sign_extend(hi), sign_extend(lo)))
}

pub fn decode_log(
    address: &str,
    topics: &[String],
    data: &str,
    block_number: i64,
    log_index: i32,
    block_time: i64,
) -> Result<AuthzEvent, DecodeError> {
    let topic0 = topics.first().ok_or(DecodeError::NoTopic)?.to_lowercase();
    let token_address = address.to_lowercase();
    let need = |want: usize, topic: &'static str| -> Result<(), DecodeError> {
        if topics.len() < want {
            return Err(DecodeError::TopicArity {
                topic,
                want,
                got: topics.len(),
            });
        }
        Ok(())
    };
    let base = |kind: &'static str| AuthzEvent {
        block_number,
        log_index,
        block_time,
        token_address: token_address.clone(),
        kind,
        token_id: None,
        account: None,
        operator: None,
        approved: None,
    };

    match topic0.as_str() {
        TOPIC_UPDATE_OPERATOR => {
            need(3, "UpdateOperator")?;
            Ok(AuthzEvent {
                token_id: Some(
                    u256_topic_to_decimal(&topics[1])
                        .ok_or(DecodeError::Field { field: "assetId" })?,
                ),
                operator: Some(
                    addr_from_topic(&topics[2]).ok_or(DecodeError::Field { field: "operator" })?,
                ),
                ..base(KIND_UPDATE_OPERATOR)
            })
        }
        TOPIC_APPROVAL => {
            need(4, "Approval")?;
            Ok(AuthzEvent {
                token_id: Some(
                    u256_topic_to_decimal(&topics[3])
                        .ok_or(DecodeError::Field { field: "assetId" })?,
                ),
                account: addr_from_topic(&topics[1]),
                operator: Some(
                    addr_from_topic(&topics[2]).ok_or(DecodeError::Field { field: "operator" })?,
                ),
                ..base(KIND_APPROVAL)
            })
        }
        TOPIC_TRANSFER_3 | TOPIC_TRANSFER_5 | TOPIC_TRANSFER_6 => {
            need(4, "Transfer")?;
            Ok(AuthzEvent {
                token_id: Some(
                    u256_topic_to_decimal(&topics[3])
                        .ok_or(DecodeError::Field { field: "assetId" })?,
                ),
                account: addr_from_topic(&topics[2]),
                ..base(KIND_TRANSFER)
            })
        }
        TOPIC_UPDATE_MANAGER => {
            need(4, "UpdateManager")?;
            Ok(AuthzEvent {
                account: Some(
                    addr_from_topic(&topics[1]).ok_or(DecodeError::Field { field: "_owner" })?,
                ),
                operator: Some(
                    addr_from_topic(&topics[2]).ok_or(DecodeError::Field { field: "_operator" })?,
                ),
                approved: Some(
                    first_word_bool(data).ok_or(DecodeError::Field { field: "_approved" })?,
                ),
                ..base(KIND_UPDATE_MANAGER)
            })
        }
        TOPIC_APPROVAL_FOR_ALL => {
            need(3, "ApprovalForAll")?;
            Ok(AuthzEvent {
                account: Some(
                    addr_from_topic(&topics[1]).ok_or(DecodeError::Field { field: "holder" })?,
                ),
                operator: Some(
                    addr_from_topic(&topics[2]).ok_or(DecodeError::Field { field: "operator" })?,
                ),
                approved: Some(first_word_bool(data).ok_or(DecodeError::Field {
                    field: "authorized",
                })?),
                ..base(KIND_APPROVED_FOR_ALL)
            })
        }
        other => Err(DecodeError::UnhandledTopic(other.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ZERO: &str = "0x0000000000000000000000000000000000000000000000000000000000000000";
    const ONE: &str = "0x0000000000000000000000000000000000000000000000000000000000000001";

    fn topic_addr(a: &str) -> String {
        format!("0x{:0>64}", a.trim_start_matches("0x"))
    }

    #[test]
    fn address_is_the_low_twenty_bytes() {
        assert_eq!(
            addr_from_topic(&topic_addr("0x4959f54f7b30224047ac3cae6bf147bbaee8b61c")).unwrap(),
            "0x4959f54f7b30224047ac3cae6bf147bbaee8b61c"
        );
    }

    #[test]
    fn u256_topic_decimal_conversion() {
        assert_eq!(u256_topic_to_decimal(ZERO).unwrap(), "0");
        assert_eq!(u256_topic_to_decimal(ONE).unwrap(), "1");
        let big = format!("0x{:0>64}", "de0b6b3a7640000");
        assert_eq!(u256_topic_to_decimal(&big).unwrap(), "1000000000000000000");
    }

    #[test]
    fn bool_word_is_any_nonzero_nibble() {
        assert!(!first_word_bool(ZERO).unwrap());
        assert!(first_word_bool(ONE).unwrap());
    }

    #[test]
    fn land_token_id_decodes_to_signed_coordinates() {
        let id = "0x0000000000000000000000000000002500000000000000000000000000000037";
        assert_eq!(decode_land_token_id(id).unwrap(), (37, 55));
        let negative = "0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe";
        assert_eq!(decode_land_token_id(negative).unwrap(), (-1, -2));
    }

    #[test]
    fn update_operator_decodes_asset_and_operator() {
        let ev = decode_log(
            LAND_REGISTRY_MAINNET,
            &[
                TOPIC_UPDATE_OPERATOR.to_string(),
                ONE.to_string(),
                topic_addr("0xabc0000000000000000000000000000000000001"),
            ],
            "0x",
            100,
            2,
            1700,
        )
        .unwrap();
        assert_eq!(ev.kind, KIND_UPDATE_OPERATOR);
        assert_eq!(ev.token_id.as_deref(), Some("1"));
        assert_eq!(
            ev.operator.as_deref(),
            Some("0xabc0000000000000000000000000000000000001")
        );
    }

    #[test]
    fn approval_for_all_carries_the_flag_from_data() {
        let revoked = decode_log(
            LAND_REGISTRY_MAINNET,
            &[
                TOPIC_APPROVAL_FOR_ALL.to_string(),
                topic_addr("0x1111111111111111111111111111111111111111"),
                topic_addr("0x2222222222222222222222222222222222222222"),
            ],
            ZERO,
            10,
            0,
            5,
        )
        .unwrap();
        assert_eq!(revoked.kind, KIND_APPROVED_FOR_ALL);
        assert_eq!(revoked.approved, Some(false));
    }

    #[test]
    fn a_truncated_log_is_an_error_not_a_silent_grant() {
        let err = decode_log(
            LAND_REGISTRY_MAINNET,
            &[TOPIC_UPDATE_OPERATOR.to_string()],
            "0x",
            1,
            0,
            0,
        )
        .unwrap_err();
        assert!(matches!(err, DecodeError::TopicArity { .. }));
    }
}
