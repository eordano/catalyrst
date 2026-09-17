use serde::Serialize;

use crate::ports::trades::{MATIC_AMOY, MATIC_MAINNET};

/// The off-chain marketplace version a manager is wired into: the only one that redeems coupons
/// signed against it. Spelled as decentraland-transactions `ContractName`, which is what the
/// wire value of `Coupon.marketplace` is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, utoipa::ToSchema)]
pub enum CouponMarketplace {
    OffChainMarketplaceV3,
    OffChainMarketplaceV2,
}

impl CouponMarketplace {
    pub fn as_str(&self) -> &'static str {
        match self {
            CouponMarketplace::OffChainMarketplaceV3 => "OffChainMarketplaceV3",
            CouponMarketplace::OffChainMarketplaceV2 => "OffChainMarketplaceV2",
        }
    }
}

impl std::fmt::Display for CouponMarketplace {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CouponManagerContract {
    pub address: &'static str,
    pub name: &'static str,
    pub version: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CouponContracts {
    pub marketplace: CouponMarketplace,
    pub coupon_manager: CouponManagerContract,
    pub collection_discount_coupon: &'static str,
}

const fn manager(address: &'static str) -> CouponManagerContract {
    CouponManagerContract {
        address,
        name: "CouponManager",
        version: "1.0.0",
    }
}

/// The coupon deployments of a chain, one per off-chain marketplace version, newest first
/// (decentraland-transactions `getCouponManager(marketplace, chainId)`).
///
/// Each marketplace version trusts its own CouponManager, so while two versions are live a chain
/// has two managers and a coupon belongs to whichever one it was signed against. The
/// CollectionDiscountCoupon is one per chain, shared by every manager. Empty on a chain without
/// collections, where nothing is deployed.
pub fn coupon_contracts(chain_id: i64) -> Vec<CouponContracts> {
    let (v3, v2, collection_discount_coupon) = match chain_id {
        MATIC_AMOY => (
            "0x6c956587d9fe70032781edcdc626310648575382",
            "0xa40b1d129b8906888720686f3a01921ddf37716f",
            "0x4ee8f6b87f4917a3bbc7c8bb3a06db8555f83db9",
        ),
        MATIC_MAINNET => (
            "0x655fdfa91d69ea49f4ce1a8f7f7e2622c8630813",
            "0x3fd3056ee72a2a85e9392fab3a450e7736536081",
            "0xc914507fe297b2dddd1232ac3a8903f1c125e794",
        ),
        _ => return Vec::new(),
    };
    vec![
        CouponContracts {
            marketplace: CouponMarketplace::OffChainMarketplaceV3,
            coupon_manager: manager(v3),
            collection_discount_coupon,
        },
        CouponContracts {
            marketplace: CouponMarketplace::OffChainMarketplaceV2,
            coupon_manager: manager(v2),
            collection_discount_coupon,
        },
    ]
}

/// The coupon deployment of `chain_id` whose manager is `address`, or None when the registry no
/// longer lists it.
pub fn find_coupon_contracts(chain_id: i64, address: &str) -> Option<CouponContracts> {
    coupon_contracts(chain_id).into_iter().find(|contracts| {
        contracts
            .coupon_manager
            .address
            .eq_ignore_ascii_case(address)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::trades::{ETHEREUM_MAINNET, ETHEREUM_SEPOLIA};

    #[test]
    fn each_polygon_chain_pairs_every_version_with_its_own_manager_newest_first() {
        let mainnet = coupon_contracts(MATIC_MAINNET);
        assert_eq!(
            mainnet
                .iter()
                .map(|c| (c.marketplace, c.coupon_manager.address))
                .collect::<Vec<_>>(),
            vec![
                (
                    CouponMarketplace::OffChainMarketplaceV3,
                    "0x655fdfa91d69ea49f4ce1a8f7f7e2622c8630813"
                ),
                (
                    CouponMarketplace::OffChainMarketplaceV2,
                    "0x3fd3056ee72a2a85e9392fab3a450e7736536081"
                ),
            ]
        );
        let amoy = coupon_contracts(MATIC_AMOY);
        assert_eq!(
            amoy.iter()
                .map(|c| (c.marketplace, c.coupon_manager.address))
                .collect::<Vec<_>>(),
            vec![
                (
                    CouponMarketplace::OffChainMarketplaceV3,
                    "0x6c956587d9fe70032781edcdc626310648575382"
                ),
                (
                    CouponMarketplace::OffChainMarketplaceV2,
                    "0xa40b1d129b8906888720686f3a01921ddf37716f"
                ),
            ]
        );
    }

    #[test]
    fn every_manager_of_a_chain_shares_its_one_discount_coupon() {
        for (chain, coupon) in [
            (MATIC_AMOY, "0x4ee8f6b87f4917a3bbc7c8bb3a06db8555f83db9"),
            (MATIC_MAINNET, "0xc914507fe297b2dddd1232ac3a8903f1c125e794"),
        ] {
            let contracts = coupon_contracts(chain);
            assert_eq!(contracts.len(), 2);
            for entry in contracts {
                assert_eq!(entry.collection_discount_coupon, coupon);
            }
        }
    }

    #[test]
    fn the_manager_name_and_version_are_what_the_domain_is_built_from() {
        for chain in [MATIC_AMOY, MATIC_MAINNET] {
            for contracts in coupon_contracts(chain) {
                assert_eq!(contracts.coupon_manager.name, "CouponManager");
                assert_eq!(contracts.coupon_manager.version, "1.0.0");
            }
        }
    }

    #[test]
    fn a_chain_without_collections_has_no_coupons() {
        for chain in [ETHEREUM_MAINNET, ETHEREUM_SEPOLIA, 42] {
            assert!(coupon_contracts(chain).is_empty());
        }
    }

    #[test]
    fn a_stored_manager_address_finds_the_deployment_that_signed_it() {
        let matched =
            find_coupon_contracts(MATIC_MAINNET, "0x3FD3056EE72A2A85E9392FAB3A450E7736536081")
                .unwrap();
        assert_eq!(
            matched.marketplace,
            CouponMarketplace::OffChainMarketplaceV2
        );
        assert!(
            find_coupon_contracts(MATIC_MAINNET, &format!("0x{}", "99".repeat(20))).is_none(),
            "a manager the registry no longer lists resolves nothing"
        );
        assert!(
            find_coupon_contracts(MATIC_AMOY, "0x3fd3056ee72a2a85e9392fab3a450e7736536081")
                .is_none()
        );
    }

    #[test]
    fn the_marketplace_is_spelled_as_the_transactions_library_names_it() {
        assert_eq!(
            CouponMarketplace::OffChainMarketplaceV3.to_string(),
            "OffChainMarketplaceV3"
        );
        assert_eq!(
            serde_json::to_value(CouponMarketplace::OffChainMarketplaceV2).unwrap(),
            serde_json::json!("OffChainMarketplaceV2")
        );
    }
}
