#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OffChainMarketplace {
    pub name: &'static str,
    pub version: &'static str,
    pub address: &'static str,
    /// V3 keys a cancellation on the trade's EIP-712 digest; V1 and V2 key it on keccak256 of
    /// the signature bytes, which `hashed_signature` already stores. Also decides whether a
    /// digest is recorded at all -- the indexer only writes one for the versions that use it,
    /// and storing one for a V2 trade would leave the join columns meaning different things
    /// on each side.
    pub cancels_by_digest: bool,
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
            cancels_by_digest: false,
        }),
        MATIC_MAINNET => Some(OffChainMarketplace {
            name: "DecentralandMarketplacePolygon",
            version: "1.0.0",
            address: "0xa40b1d129b8906888720686f3a01921ddf37716f",
            cancels_by_digest: false,
        }),
        MATIC_AMOY => Some(OffChainMarketplace {
            name: "DecentralandMarketplacePolygon",
            version: "1.0.0",
            address: "0x1b67d0e31eeb6b52d8eeed71d3616c2f5b33b8e7",
            cancels_by_digest: false,
        }),
        _ => None,
    }
}

/// decentraland-transactions 3.3.0 `offChainMarketplaceV3`, now deployed on both mainnets as
/// well: while two versions are live a trade signed against either one verifies, and a new one
/// settles on V3.
pub fn offchain_marketplace_v3(chain_id: i64) -> Option<OffChainMarketplace> {
    match chain_id {
        ETHEREUM_MAINNET => Some(OffChainMarketplace {
            name: "DecentralandMarketplaceEthereum",
            version: "1.0.0",
            address: "0x0f11d0d1671519683bd48abf3dbe779e300941cd",
            cancels_by_digest: true,
        }),
        MATIC_MAINNET => Some(OffChainMarketplace {
            name: "DecentralandMarketplacePolygon",
            version: "1.0.0",
            address: "0xe38ef22abe871513555cba89adfe45ab4f548ada",
            cancels_by_digest: true,
        }),
        ETHEREUM_SEPOLIA => Some(OffChainMarketplace {
            name: "DecentralandMarketplaceEthereum",
            version: "1.0.0",
            address: "0x257db44ac97789c16ab277eae87dcde0c246cc9f",
            cancels_by_digest: true,
        }),
        MATIC_AMOY => Some(OffChainMarketplace {
            name: "DecentralandMarketplacePolygon",
            version: "1.0.0",
            address: "0x36fd1434a6c4b8ade80c9847c1d15033ce34488c",
            cancels_by_digest: true,
        }),
        _ => None,
    }
}

/// Newest first. The EIP-712 domain names its verifying contract, so the version is part of
/// what the signer signed: pinning one would reject every trade signed against the other
/// while clients roll over, and newest-first settles an ambiguous signature on the newest
/// deployment.
pub fn offchain_marketplaces(chain_id: i64) -> Vec<OffChainMarketplace> {
    [
        offchain_marketplace_v3(chain_id),
        offchain_marketplace_v2(chain_id),
    ]
    .into_iter()
    .flatten()
    .collect()
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
        assert_eq!(
            offchain_marketplace_v3(MATIC_AMOY).unwrap().name,
            offchain_marketplace_v2(MATIC_AMOY).unwrap().name
        );
    }

    #[test]
    fn an_unknown_chain_has_no_marketplace() {
        assert!(offchain_marketplace_v2(42).is_none());
        assert!(offchain_marketplace_v3(42).is_none());
        assert!(offchain_marketplaces(42).is_empty());
        assert!(network_for_chain(42).is_none());
    }

    #[test]
    fn candidates_run_newest_first_on_every_chain_that_has_two() {
        let amoy = offchain_marketplaces(MATIC_AMOY);
        assert_eq!(amoy.len(), 2);
        assert!(amoy[0].cancels_by_digest);
        assert_eq!(
            amoy[0].address,
            "0x36fd1434a6c4b8ade80c9847c1d15033ce34488c"
        );
        assert!(!amoy[1].cancels_by_digest);
        let sepolia = offchain_marketplaces(ETHEREUM_SEPOLIA);
        assert_eq!(
            sepolia[0].address,
            "0x257db44ac97789c16ab277eae87dcde0c246cc9f"
        );
        for (mainnet, v3) in [
            (
                ETHEREUM_MAINNET,
                "0x0f11d0d1671519683bd48abf3dbe779e300941cd",
            ),
            (MATIC_MAINNET, "0xe38ef22abe871513555cba89adfe45ab4f548ada"),
        ] {
            let candidates = offchain_marketplaces(mainnet);
            assert_eq!(
                candidates,
                vec![
                    offchain_marketplace_v3(mainnet).unwrap(),
                    offchain_marketplace_v2(mainnet).unwrap()
                ]
            );
            assert_eq!(candidates[0].address, v3);
            assert!(candidates[0].cancels_by_digest);
        }
    }

    #[test]
    fn a_chain_without_an_off_chain_marketplace_lists_nothing() {
        assert!(offchain_marketplaces(5).is_empty());
    }

    #[test]
    fn only_ethereum_carries_estates() {
        assert!(is_estate_chain(ETHEREUM_MAINNET));
        assert!(!is_estate_chain(MATIC_MAINNET));
    }
}
