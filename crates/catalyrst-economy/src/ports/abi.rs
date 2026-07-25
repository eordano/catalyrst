use alloy::primitives::U256;
use alloy::sol;
use alloy::sol_types::SolCall;

sol! {
    function executeMetaTransaction(
        address userAddress,
        bytes functionSignature,
        bytes32 sigR,
        bytes32 sigS,
        uint8 sigV
    ) external returns (bytes);

    struct ItemToBuy {
        address collection;
        uint256[] ids;
        uint256[] prices;
        address[] beneficiaries;
    }
    function buy(ItemToBuy[] itemsToBuy) external;

    function executeOrder(address nftAddress, uint256 assetId, uint256 price) external;

    function placeBid(
        address _tokenAddress,
        uint256 _tokenId,
        uint256 _price,
        uint256 _duration
    ) external;

    function reclaim(address collection, uint256 tokenId) external;
    function release(address collection, uint256 tokenId, address buyer) external;

    function safeTransferFrom(address from, address to, uint256 tokenId, bytes data) external;

    function register(string _name, address _beneficiary) external;

    function approve(address spender, uint256 amount) external returns (bool);

    function getNonce(address user) external view returns (uint256 nonce);
}

pub const ERC721_TRANSFER_TOPIC0: [u8; 32] = [
    0xdd, 0xf2, 0x52, 0xad, 0x1b, 0xe2, 0xc8, 0x9b, 0x69, 0xc2, 0xb0, 0x68, 0xfc, 0x37, 0x8d, 0xaa,
    0x95, 0x2b, 0xa7, 0xf1, 0x63, 0xc4, 0xa1, 0x16, 0x28, 0xf5, 0x5a, 0x4d, 0xf5, 0x23, 0xb3, 0xef,
];

pub enum SaleKind {
    CollectionStore,
    MarketplaceV2,
    BidV2,
}

pub const EXEC_META_TX_SELECTORS: [[u8; 4]; 2] =
    [[0x0c, 0x53, 0xc5, 0x1c], [0xd8, 0xed, 0x1a, 0xcc]];

pub fn is_execute_meta_tx(full_data: &[u8]) -> bool {
    full_data.len() >= 4
        && EXEC_META_TX_SELECTORS.contains(&[
            full_data[0],
            full_data[1],
            full_data[2],
            full_data[3],
        ])
}

pub fn decode_meta_tx(full_data: &[u8]) -> Option<Vec<u8>> {
    executeMetaTransactionCall::abi_decode(full_data)
        .ok()
        .map(|c| c.functionSignature.to_vec())
}

pub fn get_sale_price(full_data: &[u8], kind: SaleKind) -> Option<U256> {
    let inner = decode_meta_tx(full_data)?;
    match kind {
        SaleKind::CollectionStore => {
            let call = buyCall::abi_decode(&inner).ok()?;
            call.itemsToBuy
                .first()
                .and_then(|i| i.prices.first())
                .copied()
        }
        SaleKind::MarketplaceV2 => {
            let call = executeOrderCall::abi_decode(&inner).ok()?;
            Some(call.price)
        }
        SaleKind::BidV2 => {
            let call = placeBidCall::abi_decode(&inner).ok()?;
            Some(call._price)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_both_execute_meta_tx_selectors() {
        assert!(is_execute_meta_tx(&[0x0c, 0x53, 0xc5, 0x1c, 0x00]));
        assert!(is_execute_meta_tx(&[0xd8, 0xed, 0x1a, 0xcc]));
    }

    #[test]
    fn rejects_other_selectors() {
        assert!(!is_execute_meta_tx(&[0xa9, 0x05, 0x9c, 0xbb]));
        assert!(!is_execute_meta_tx(&[0x0c, 0x53, 0xc5]));
        assert!(!is_execute_meta_tx(&[]));
    }

    #[test]
    fn erc721_transfer_topic0_matches_keccak() {
        let computed = alloy::primitives::keccak256("Transfer(address,address,uint256)".as_bytes());
        assert_eq!(computed.as_slice(), &ERC721_TRANSFER_TOPIC0);
    }
}
