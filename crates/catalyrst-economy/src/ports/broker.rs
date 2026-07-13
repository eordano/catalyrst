use std::str::FromStr;

use alloy::primitives::{Address, Bytes, B256, U256};
use alloy::sol_types::{SolCall, SolValue};

use crate::http::errors::ApiError;
use crate::ports::abi::{
    approveCall, buyCall, executeOrderCall, registerCall, safeTransferFromCall, ItemToBuy,
    ERC721_TRANSFER_TOPIC0,
};
use crate::ports::contracts_addrs::{DclContracts, NameContracts};
use crate::ports::signer::ReceiptLog;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PurchaseMode {
    Primary,

    Secondary,

    Trade,

    NameMint,

    NameSecondary,
}

impl PurchaseMode {
    pub fn as_str(self) -> &'static str {
        match self {
            PurchaseMode::Primary => "primary",
            PurchaseMode::Secondary => "secondary",
            PurchaseMode::Trade => "trade",
            PurchaseMode::NameMint => "name-mint",
            PurchaseMode::NameSecondary => "name-secondary",
        }
    }

    pub fn from_db_str(s: &str) -> Option<Self> {
        match s {
            "primary" => Some(PurchaseMode::Primary),
            "secondary" => Some(PurchaseMode::Secondary),
            "trade" => Some(PurchaseMode::Trade),
            "name-mint" => Some(PurchaseMode::NameMint),
            "name-secondary" => Some(PurchaseMode::NameSecondary),
            _ => None,
        }
    }

    pub fn has_forward_leg(self) -> bool {
        matches!(
            self,
            PurchaseMode::Primary | PurchaseMode::Secondary | PurchaseMode::Trade
        )
    }

    pub fn parse_wearable(s: &str) -> Result<Self, ApiError> {
        match s {
            "primary" => Ok(PurchaseMode::Primary),
            "secondary" => Ok(PurchaseMode::Secondary),
            "trade" => Ok(PurchaseMode::Trade),
            other => Err(ApiError::InvalidTransaction(format!(
                "invalid mode {other:?}: expected \"primary\", \"secondary\", or \"trade\""
            ))),
        }
    }

    pub fn parse_name(s: &str) -> Result<Self, ApiError> {
        match s {
            "mint" => Ok(PurchaseMode::NameMint),
            "secondary" => Ok(PurchaseMode::NameSecondary),
            other => Err(ApiError::InvalidTransaction(format!(
                "invalid mode {other:?}: expected \"mint\" or \"secondary\""
            ))),
        }
    }
}

#[derive(Debug, Clone)]
pub struct BrokerCall {
    pub to: Address,
    pub data: Bytes,
}

pub fn parse_address(label: &str, raw: &str) -> Result<Address, ApiError> {
    Address::from_str(raw.trim())
        .map_err(|e| ApiError::InvalidTransaction(format!("invalid {label} address {raw:?}: {e}")))
}

pub fn parse_wei(label: &str, raw: &str) -> Result<U256, ApiError> {
    let value = parse_wei_allow_zero(label, raw)?;
    if value.is_zero() {
        return Err(ApiError::InvalidTransaction(format!(
            "invalid {label}: must be a positive integer, got 0"
        )));
    }
    Ok(value)
}

pub fn parse_wei_allow_zero(label: &str, raw: &str) -> Result<U256, ApiError> {
    let trimmed = raw.trim();
    U256::from_str_radix(trimmed, 10).map_err(|e| {
        ApiError::InvalidTransaction(format!(
            "invalid {label} {raw:?}: not a decimal integer ({e})"
        ))
    })
}

pub fn parse_token_id(raw: &str) -> Result<U256, ApiError> {
    U256::from_str_radix(raw.trim(), 10).map_err(|e| {
        ApiError::InvalidTransaction(format!(
            "invalid tokenId {raw:?}: not a decimal integer ({e})"
        ))
    })
}

pub fn parse_item_id(raw: &str) -> Result<U256, ApiError> {
    U256::from_str_radix(raw.trim(), 10).map_err(|e| {
        ApiError::InvalidTransaction(format!(
            "invalid itemId {raw:?}: not a decimal integer ({e})"
        ))
    })
}

pub fn encode_buyer_data(buyer: Address) -> Bytes {
    Bytes::from(buyer.abi_encode())
}

pub fn decode_buyer_data(data: &[u8]) -> Result<Address, ApiError> {
    if data.len() != 32 {
        return Err(ApiError::InvalidTransaction(format!(
            "escrow buyer _data must be exactly 32 bytes (got {})",
            data.len()
        )));
    }
    Address::abi_decode(data).map_err(|e| {
        ApiError::InvalidTransaction(format!("could not decode buyer from _data: {e}"))
    })
}

pub fn build_primary(
    contracts: &DclContracts,
    collection: Address,
    item_id: U256,
    price_wei: U256,
    beneficiary: Address,
) -> BrokerCall {
    let call = buyCall {
        itemsToBuy: vec![ItemToBuy {
            collection,
            ids: vec![item_id],
            prices: vec![price_wei],
            beneficiaries: vec![beneficiary],
        }],
    };
    BrokerCall {
        to: contracts.collection_store,
        data: Bytes::from(call.abi_encode()),
    }
}

pub fn build_forward_to_escrow(
    collection: Address,
    from_relayer: Address,
    escrow: Address,
    token_id: U256,
    buyer: Address,
) -> BrokerCall {
    let call = safeTransferFromCall {
        from: from_relayer,
        to: escrow,
        tokenId: token_id,
        data: encode_buyer_data(buyer),
    };
    BrokerCall {
        to: collection,
        data: Bytes::from(call.abi_encode()),
    }
}

pub fn minted_token_id_from_logs(
    logs: &[ReceiptLog],
    collection: Address,
    relayer: Address,
) -> Option<U256> {
    let topic0 = B256::from(ERC721_TRANSFER_TOPIC0);
    let relayer_topic = B256::from_slice(&[&[0u8; 12][..], relayer.as_slice()].concat());
    for log in logs {
        if log.address == collection
            && log.topics.len() == 4
            && log.topics[0] == topic0
            && log.topics[1] == B256::ZERO
            && log.topics[2] == relayer_topic
        {
            return Some(U256::from_be_bytes(log.topics[3].0));
        }
    }
    None
}

pub fn build_secondary(
    contracts: &DclContracts,
    collection: Address,
    token_id: U256,
    price_wei: U256,
) -> BrokerCall {
    let call = executeOrderCall {
        nftAddress: collection,
        assetId: token_id,
        price: price_wei,
    };
    BrokerCall {
        to: contracts.marketplace_v2,
        data: Bytes::from(call.abi_encode()),
    }
}

pub fn validate_name(name: &str) -> Result<(), ApiError> {
    let n = name.trim();
    if n != name {
        return Err(ApiError::InvalidTransaction(format!(
            "invalid NAME {name:?}: leading/trailing whitespace"
        )));
    }
    if !(2..=15).contains(&n.len()) {
        return Err(ApiError::InvalidTransaction(format!(
            "invalid NAME {name:?}: length must be 2..=15 characters"
        )));
    }
    if !n.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Err(ApiError::InvalidTransaction(format!(
            "invalid NAME {name:?}: only ASCII letters and digits are allowed"
        )));
    }
    Ok(())
}

pub fn build_name_mint(
    contracts: &NameContracts,
    name: &str,
    beneficiary: Address,
) -> Result<BrokerCall, ApiError> {
    validate_name(name)?;
    let call = registerCall {
        _name: name.to_string(),
        _beneficiary: beneficiary,
    };
    Ok(BrokerCall {
        to: contracts.controller_v2,
        data: Bytes::from(call.abi_encode()),
    })
}

pub fn build_name_secondary(
    contracts: &NameContracts,
    token_id: U256,
    price_wei: U256,
) -> BrokerCall {
    let call = executeOrderCall {
        nftAddress: contracts.registrar,
        assetId: token_id,
        price: price_wei,
    };
    BrokerCall {
        to: contracts.marketplace,
        data: Bytes::from(call.abi_encode()),
    }
}

pub fn build_name_transfer(
    registrar: Address,
    from: Address,
    to: Address,
    token_id: U256,
) -> BrokerCall {
    let call = safeTransferFromCall {
        from,
        to,
        tokenId: token_id,
        data: Bytes::new(),
    };
    BrokerCall {
        to: registrar,
        data: Bytes::from(call.abi_encode()),
    }
}

pub fn build_mana_approve(mana_token: Address, spender: Address, amount: U256) -> BrokerCall {
    let call = approveCall { spender, amount };
    BrokerCall {
        to: mana_token,
        data: Bytes::from(call.abi_encode()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn contracts() -> DclContracts {
        DclContracts::for_chain(137).expect("polygon contracts")
    }

    #[test]
    fn mode_parse_round_trips() {
        assert_eq!(
            PurchaseMode::parse_wearable("primary").unwrap(),
            PurchaseMode::Primary
        );
        assert_eq!(
            PurchaseMode::parse_wearable("secondary").unwrap(),
            PurchaseMode::Secondary
        );
        assert!(PurchaseMode::parse_wearable("tertiary").is_err());
        assert!(PurchaseMode::parse_wearable("mint").is_err());
        assert_eq!(
            PurchaseMode::parse_name("mint").unwrap(),
            PurchaseMode::NameMint
        );
        assert_eq!(
            PurchaseMode::parse_name("secondary").unwrap(),
            PurchaseMode::NameSecondary
        );
        assert!(PurchaseMode::parse_name("primary").is_err());
        assert_eq!(
            PurchaseMode::parse_wearable("trade").unwrap(),
            PurchaseMode::Trade
        );
        for mode in [
            PurchaseMode::Primary,
            PurchaseMode::Secondary,
            PurchaseMode::Trade,
            PurchaseMode::NameMint,
            PurchaseMode::NameSecondary,
        ] {
            assert_eq!(PurchaseMode::from_db_str(mode.as_str()), Some(mode));
        }
        assert_eq!(PurchaseMode::from_db_str("land-lease"), None);
        assert!(PurchaseMode::Primary.has_forward_leg());
        assert!(PurchaseMode::Secondary.has_forward_leg());
        assert!(
            PurchaseMode::Trade.has_forward_leg(),
            "an accepted trade lands on the relayer and must be escrow-forwarded"
        );
        assert!(!PurchaseMode::NameMint.has_forward_leg());
        assert!(!PurchaseMode::NameSecondary.has_forward_leg());
    }

    #[test]
    fn parse_wei_rejects_zero_and_garbage() {
        assert!(parse_wei("priceWei", "0").is_err());
        assert!(parse_wei("priceWei", "").is_err());
        assert!(parse_wei("priceWei", "1.5").is_err());
        assert!(parse_wei("priceWei", "-1").is_err());
        let big = "1000000000000000000000";
        assert_eq!(
            parse_wei("priceWei", big).unwrap(),
            U256::from_str_radix(big, 10).unwrap()
        );
    }

    #[test]
    fn parse_token_id_allows_zero() {
        assert_eq!(parse_token_id("0").unwrap(), U256::ZERO);
        assert!(parse_token_id("nope").is_err());
    }

    #[test]
    fn parse_item_id_allows_zero_and_rejects_garbage() {
        assert_eq!(parse_item_id("0").unwrap(), U256::ZERO);
        assert_eq!(parse_item_id("7").unwrap(), U256::from(7u64));
        assert!(parse_item_id("1.5").is_err());
        assert!(parse_item_id("nope").is_err());
    }

    #[test]
    fn primary_targets_collection_store_and_mints_to_relayer() {
        let c = contracts();
        let collection = parse_address("c", "0x214ffc0f0103735728dc66b61a22e4f163e275ae").unwrap();
        let relayer = parse_address("r", "0xA1c57f48F0Deb89f569dFbE6E2B7f46D33606fD4").unwrap();
        let call = build_primary(
            &c,
            collection,
            U256::from(7u64),
            U256::from(123u64),
            relayer,
        );
        assert_eq!(call.to, c.collection_store);

        let decoded = buyCall::abi_decode(&call.data).unwrap();
        assert_eq!(decoded.itemsToBuy.len(), 1);
        assert_eq!(decoded.itemsToBuy[0].beneficiaries, vec![relayer]);
        assert_eq!(decoded.itemsToBuy[0].ids, vec![U256::from(7u64)]);
        assert_eq!(decoded.itemsToBuy[0].prices, vec![U256::from(123u64)]);
    }

    #[test]
    fn primary_calldata_matches_the_hand_computed_collection_store_abi() {
        let c = contracts();
        let collection = parse_address("c", "0x59a90bad9570ecd08895f132daf7b79696337f61").unwrap();
        let beneficiary = parse_address("b", "0x1111111111111111111111111111111111111111").unwrap();
        let call = build_primary(
            &c,
            collection,
            U256::from(7u64),
            U256::from(123u64),
            beneficiary,
        );
        assert_eq!(call.to, c.collection_store);

        let expected = concat!(
            "a4fdc78a",
            "0000000000000000000000000000000000000000000000000000000000000020",
            "0000000000000000000000000000000000000000000000000000000000000001",
            "0000000000000000000000000000000000000000000000000000000000000020",
            "00000000000000000000000059a90bad9570ecd08895f132daf7b79696337f61",
            "0000000000000000000000000000000000000000000000000000000000000080",
            "00000000000000000000000000000000000000000000000000000000000000c0",
            "0000000000000000000000000000000000000000000000000000000000000100",
            "0000000000000000000000000000000000000000000000000000000000000001",
            "0000000000000000000000000000000000000000000000000000000000000007",
            "0000000000000000000000000000000000000000000000000000000000000001",
            "000000000000000000000000000000000000000000000000000000000000007b",
            "0000000000000000000000000000000000000000000000000000000000000001",
            "0000000000000000000000001111111111111111111111111111111111111111",
        );
        assert_eq!(alloy::hex::encode(&call.data), expected);

        let independently_computed_selector = "a4fdc78a";
        assert_eq!(
            alloy::hex::encode(buyCall::SELECTOR),
            independently_computed_selector,
            "selector must be keccak256(\"buy((address,uint256[],uint256[],address[])[])\")[0..4]"
        );
    }

    #[test]
    fn minted_token_id_requires_a_mint_not_any_transfer_to_relayer() {
        let collection = parse_address("c", "0x214ffc0f0103735728dc66b61a22e4f163e275ae").unwrap();
        let relayer = parse_address("r", "0xA1c57f48F0Deb89f569dFbE6E2B7f46D33606fD4").unwrap();
        let other = parse_address("o", "0x2222222222222222222222222222222222222222").unwrap();
        let topic0 = B256::from(ERC721_TRANSFER_TOPIC0);
        let from_other = B256::from_slice(&[&[0u8; 12][..], other.as_slice()].concat());
        let to_relayer = B256::from_slice(&[&[0u8; 12][..], relayer.as_slice()].concat());
        let token_topic = B256::from_slice(&U256::from(42u64).to_be_bytes::<32>());
        let non_mint = ReceiptLog {
            address: collection,
            topics: vec![topic0, from_other, to_relayer, token_topic],
            data: Bytes::new(),
        };
        assert_eq!(
            minted_token_id_from_logs(&[non_mint], collection, relayer),
            None,
            "a secondary-market Transfer(other -> relayer) must not be mistaken for the mint"
        );
    }

    #[test]
    fn buyer_data_round_trips_through_escrow_decode() {
        let buyer = parse_address("b", "0x1234567890AbcdEF1234567890aBcdef12345678").unwrap();
        let data = encode_buyer_data(buyer);
        assert_eq!(data.len(), 32);
        assert_eq!(&data[..12], &[0u8; 12]);
        assert_eq!(decode_buyer_data(&data).unwrap(), buyer);
        assert!(decode_buyer_data(&data[..31]).is_err());
    }

    #[test]
    fn forward_to_escrow_carries_buyer_in_data() {
        let collection = parse_address("c", "0x214ffc0f0103735728dc66b61a22e4f163e275ae").unwrap();
        let relayer = parse_address("r", "0xA1c57f48F0Deb89f569dFbE6E2B7f46D33606fD4").unwrap();
        let escrow = parse_address("e", "0x000000000000000000000000000000000000dEaD").unwrap();
        let buyer = parse_address("b", "0x1111111111111111111111111111111111111111").unwrap();
        let call = build_forward_to_escrow(collection, relayer, escrow, U256::from(42u64), buyer);
        assert_eq!(call.to, collection);
        let decoded = safeTransferFromCall::abi_decode(&call.data).unwrap();
        assert_eq!(decoded.from, relayer);
        assert_eq!(decoded.to, escrow);
        assert_eq!(decoded.tokenId, U256::from(42u64));
        assert_eq!(decode_buyer_data(&decoded.data).unwrap(), buyer);
    }

    #[test]
    fn minted_token_id_parsed_from_transfer_log() {
        let collection = parse_address("c", "0x214ffc0f0103735728dc66b61a22e4f163e275ae").unwrap();
        let relayer = parse_address("r", "0xA1c57f48F0Deb89f569dFbE6E2B7f46D33606fD4").unwrap();
        let other = parse_address("o", "0x2222222222222222222222222222222222222222").unwrap();
        let topic0 = B256::from(ERC721_TRANSFER_TOPIC0);
        let zero_topic = B256::ZERO;
        let to_relayer = B256::from_slice(&[&[0u8; 12][..], relayer.as_slice()].concat());
        let to_other = B256::from_slice(&[&[0u8; 12][..], other.as_slice()].concat());
        let token = U256::from(987654321u64);
        let token_topic = B256::from_slice(&token.to_be_bytes::<32>());
        let log = |address, topics| ReceiptLog {
            address,
            topics,
            data: Bytes::new(),
        };

        let logs = vec![
            log(collection, vec![topic0, zero_topic, to_other, token_topic]),
            log(
                collection,
                vec![topic0, zero_topic, to_relayer, token_topic],
            ),
        ];
        assert_eq!(
            minted_token_id_from_logs(&logs, collection, relayer),
            Some(token)
        );

        let none = minted_token_id_from_logs(
            &[log(
                other,
                vec![topic0, zero_topic, to_relayer, token_topic],
            )],
            collection,
            relayer,
        );
        assert_eq!(none, None);
    }

    fn name_contracts() -> NameContracts {
        NameContracts::for_chain(1).expect("mainnet name contracts")
    }

    #[test]
    fn name_validation_enforces_controller_rules() {
        assert!(validate_name("ab").is_ok());
        assert!(validate_name("Neon42").is_ok());
        assert!(validate_name("exactlyfifteen1").is_ok());
        assert!(validate_name("a").is_err());
        assert!(validate_name("sixteencharslong").is_err());
        assert!(validate_name("has space").is_err());
        assert!(validate_name("émoji").is_err());
        assert!(validate_name("dash-ed").is_err());
        assert!(validate_name(" pad").is_err());
        assert!(validate_name("").is_err());
    }

    #[test]
    fn name_mint_targets_controller_and_encodes_register() {
        let c = name_contracts();
        let beneficiary = parse_address("b", "0x1111111111111111111111111111111111111111").unwrap();
        let call = build_name_mint(&c, "Neon42", beneficiary).unwrap();
        assert_eq!(call.to, c.controller_v2);
        let decoded = registerCall::abi_decode(&call.data).unwrap();
        assert_eq!(decoded._name, "Neon42");
        assert_eq!(decoded._beneficiary, beneficiary);
        assert!(build_name_mint(&c, "no good", beneficiary).is_err());
    }

    #[test]
    fn name_secondary_targets_eth_marketplace_with_registrar_nft() {
        let c = name_contracts();
        let call = build_name_secondary(&c, U256::from(42u64), U256::from(99u64));
        assert_eq!(call.to, c.marketplace);
        let decoded = executeOrderCall::abi_decode(&call.data).unwrap();
        assert_eq!(decoded.nftAddress, c.registrar);
        assert_eq!(decoded.assetId, U256::from(42u64));
        assert_eq!(decoded.price, U256::from(99u64));
    }

    #[test]
    fn name_transfer_is_plain_safe_transfer_on_registrar() {
        let c = name_contracts();
        let registrar = c.registrar;
        let from = parse_address("f", "0x1111111111111111111111111111111111111111").unwrap();
        let to = parse_address("t", "0x2222222222222222222222222222222222222222").unwrap();
        let call = build_name_transfer(registrar, from, to, U256::from(7u64));
        assert_eq!(call.to, registrar);
        let decoded = safeTransferFromCall::abi_decode(&call.data).unwrap();
        assert_eq!(decoded.from, from);
        assert_eq!(decoded.to, to);
        assert_eq!(decoded.tokenId, U256::from(7u64));
        assert!(decoded.data.is_empty());
    }

    #[test]
    fn mana_approve_encodes_spender_and_amount() {
        let c = name_contracts();
        let mana = c.mana_token;
        let spender = c.controller_v2;
        let call = build_mana_approve(mana, spender, U256::MAX);
        assert_eq!(call.to, mana);
        let decoded = approveCall::abi_decode(&call.data).unwrap();
        assert_eq!(decoded.spender, spender);
        assert_eq!(decoded.amount, U256::MAX);
    }

    #[test]
    fn secondary_targets_marketplace_v2_and_encodes_execute_order() {
        let c = contracts();
        let collection = parse_address("c", "0x214ffc0f0103735728dc66b61a22e4f163e275ae").unwrap();
        let call = build_secondary(&c, collection, U256::from(42u64), U256::from(99u64));
        assert_eq!(call.to, c.marketplace_v2);
        let decoded = executeOrderCall::abi_decode(&call.data).unwrap();
        assert_eq!(decoded.nftAddress, collection);
        assert_eq!(decoded.assetId, U256::from(42u64));
        assert_eq!(decoded.price, U256::from(99u64));
    }
}
