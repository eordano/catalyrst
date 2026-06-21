#[derive(Debug, Clone)]
pub struct DclContracts {
    pub collection_store: String,
    pub marketplace_v2: String,
    pub bid_v2: String,
}

impl DclContracts {
    pub fn for_chain(chain_id: u64) -> Option<Self> {
        match chain_id {
            137 => Some(Self {
                collection_store: "0x214ffc0f0103735728dc66b61a22e4f163e275ae".into(),
                marketplace_v2: "0x480a0f4e360e8964e68858dd231c2922f1df45ef".into(),
                bid_v2: "0xb96697fa4a3361ba35b774a42c58daccaad1b8e1".into(),
            }),
            _ => None,
        }
    }
}
