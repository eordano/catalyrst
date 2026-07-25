use alloy::primitives::{Address, Bytes, U256};
use alloy::sol_types::SolCall;

use crate::ports::abi::{reclaimCall, releaseCall};
use crate::ports::broker::BrokerCall;

pub fn build_reclaim(escrow: Address, collection: Address, token_id: U256) -> BrokerCall {
    let call = reclaimCall {
        collection,
        tokenId: token_id,
    };
    BrokerCall {
        to: escrow,
        data: Bytes::from(call.abi_encode()),
    }
}

pub fn build_release(
    escrow: Address,
    collection: Address,
    token_id: U256,
    buyer: Address,
) -> BrokerCall {
    let call = releaseCall {
        collection,
        tokenId: token_id,
        buyer,
    };
    BrokerCall {
        to: escrow,
        data: Bytes::from(call.abi_encode()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::broker::parse_address;

    fn addr(s: &str) -> Address {
        parse_address("t", s).unwrap()
    }

    #[test]
    fn reclaim_targets_escrow_and_encodes() {
        let escrow = addr("0xA1c57f48F0Deb89f569dFbE6E2B7f46D33606fD4");
        let collection = addr("0x214ffc0f0103735728dc66b61a22e4f163e275ae");
        let call = build_reclaim(escrow, collection, U256::from(7u64));
        assert_eq!(call.to, escrow);
        let decoded = reclaimCall::abi_decode(&call.data).unwrap();
        assert_eq!(decoded.collection, collection);
        assert_eq!(decoded.tokenId, U256::from(7u64));
    }

    #[test]
    fn release_targets_escrow_and_encodes() {
        let escrow = addr("0xA1c57f48F0Deb89f569dFbE6E2B7f46D33606fD4");
        let collection = addr("0x214ffc0f0103735728dc66b61a22e4f163e275ae");
        let buyer = addr("0x1111111111111111111111111111111111111111");
        let call = build_release(escrow, collection, U256::from(0u64), buyer);
        assert_eq!(call.to, escrow);
        let decoded = releaseCall::abi_decode(&call.data).unwrap();
        assert_eq!(decoded.collection, collection);
        assert_eq!(decoded.tokenId, U256::ZERO);
        assert_eq!(decoded.buyer, buyer);
    }
}
