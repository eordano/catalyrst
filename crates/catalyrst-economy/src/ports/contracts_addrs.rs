use alloy::primitives::{address, Address};

#[derive(Debug, Clone, Copy)]
pub struct DclContracts {
    pub collection_store: Address,
    pub marketplace_v2: Address,
    pub bid_v2: Address,

    pub offchain_marketplace: Address,

    pub mana_token: Address,
}

impl DclContracts {
    pub fn for_chain(chain_id: u64) -> Option<Self> {
        match chain_id {
            137 => Some(Self {
                collection_store: address!("0x214ffc0f0103735728dc66b61a22e4f163e275ae"),
                marketplace_v2: address!("0x480a0f4e360e8964e68858dd231c2922f1df45ef"),
                bid_v2: address!("0xb96697fa4a3361ba35b774a42c58daccaad1b8e1"),

                offchain_marketplace: address!("0x540fb08eDb56AaE562864B390542C97F562825BA"),

                mana_token: address!("0xA1c57f48F0Deb89f569dFbE6E2B7f46D33606fD4"),
            }),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct NameContracts {
    pub registrar: Address,
    pub controller_v2: Address,
    pub marketplace: Address,
    pub mana_token: Address,
}

impl NameContracts {
    pub fn for_chain(chain_id: u64) -> Option<Self> {
        match chain_id {
            1 => Some(Self {
                registrar: address!("0x2a187453064356c898cae034eaed119e1663acb8"),
                controller_v2: address!("0xbe92b49aee993adea3a002adcda189a2b7dec56c"),
                marketplace: address!("0x8e5660b4ab70168b5a6feea0e0315cb49c8cd539"),
                mana_token: address!("0x0f5d2fb29fb7d3cfee444a200298f468908cc942"),
            }),
            _ => None,
        }
    }
}
