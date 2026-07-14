#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OffChainMarketplace {
    pub name: &'static str,
    pub version: &'static str,
    pub address: &'static str,
}

pub const ETHEREUM_MAINNET: i64 = 1;
pub const ETHEREUM_SEPOLIA: i64 = 11155111;
pub const MATIC_MAINNET: i64 = 137;
pub const MATIC_AMOY: i64 = 80002;

pub fn offchain_marketplace_v2(chain_id: i64) -> Option<OffChainMarketplace> {
    match chain_id {
        ETHEREUM_MAINNET | ETHEREUM_SEPOLIA => Some(OffChainMarketplace {
            name: "DecentralandMarketplaceEthereum",
            version: "1.0.0",
            address: "0x1b67d0e31eeb6b52d8eeed71d3616c2f5b33b8e7",
        }),
        MATIC_MAINNET => Some(OffChainMarketplace {
            name: "DecentralandMarketplacePolygon",
            version: "1.0.0",
            address: "0xa40b1d129b8906888720686f3a01921ddf37716f",
        }),
        MATIC_AMOY => Some(OffChainMarketplace {
            name: "DecentralandMarketplacePolygon",
            version: "1.0.0",
            address: "0x1b67d0e31eeb6b52d8eeed71d3616c2f5b33b8e7",
        }),
        _ => None,
    }
}

pub fn network_for_chain(chain_id: i64) -> Option<&'static str> {
    match chain_id {
        ETHEREUM_MAINNET | ETHEREUM_SEPOLIA => Some("ETHEREUM"),
        MATIC_MAINNET | MATIC_AMOY => Some("MATIC"),
        _ => None,
    }
}

pub fn is_estate_chain(chain_id: i64) -> bool {
    matches!(chain_id, ETHEREUM_MAINNET | ETHEREUM_SEPOLIA)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn polygon_mainnet_uses_its_own_v2_address() {
        let eth = offchain_marketplace_v2(ETHEREUM_MAINNET).unwrap();
        let matic = offchain_marketplace_v2(MATIC_MAINNET).unwrap();
        assert_ne!(eth.address, matic.address);
        assert_eq!(matic.address, "0xa40b1d129b8906888720686f3a01921ddf37716f");
        assert_eq!(matic.name, "DecentralandMarketplacePolygon");
    }

    #[test]
    fn testnets_mirror_their_mainnet_names() {
        assert_eq!(
            offchain_marketplace_v2(ETHEREUM_SEPOLIA).unwrap().name,
            offchain_marketplace_v2(ETHEREUM_MAINNET).unwrap().name
        );
        assert_eq!(
            offchain_marketplace_v2(MATIC_AMOY).unwrap().name,
            offchain_marketplace_v2(MATIC_MAINNET).unwrap().name
        );
    }

    #[test]
    fn an_unknown_chain_has_no_marketplace() {
        assert!(offchain_marketplace_v2(42).is_none());
        assert!(network_for_chain(42).is_none());
    }

    #[test]
    fn only_ethereum_carries_estates() {
        assert!(is_estate_chain(ETHEREUM_MAINNET));
        assert!(!is_estate_chain(MATIC_MAINNET));
    }
}
