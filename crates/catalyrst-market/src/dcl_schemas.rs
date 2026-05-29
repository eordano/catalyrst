use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NftCategory {
    Parcel,
    Estate,
    Wearable,
    Ens,
    Emote,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Network {
    Ethereum,
    Matic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum SquidNetwork {
    Ethereum,
    Polygon,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(into = "u64", try_from = "u64")]
pub enum ChainId {
    EthereumMainnet = 1,
    EthereumSepolia = 11_155_111,
    MaticMainnet = 137,
    MaticAmoy = 80_002,
}

impl From<ChainId> for u64 {
    fn from(c: ChainId) -> u64 {
        c as u64
    }
}

impl TryFrom<u64> for ChainId {
    type Error = String;
    fn try_from(v: u64) -> Result<Self, Self::Error> {
        Ok(match v {
            1 => ChainId::EthereumMainnet,
            11_155_111 => ChainId::EthereumSepolia,
            137 => ChainId::MaticMainnet,
            80_002 => ChainId::MaticAmoy,
            _ => return Err(format!("unknown chain id: {}", v)),
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Contract {
    pub name: String,
    pub address: String,
    pub category: NftCategory,
    pub network: Network,
    #[serde(rename = "chainId")]
    pub chain_id: ChainId,
}

pub fn get_db_networks(network: Network) -> Vec<&'static str> {
    match network {
        Network::Ethereum => vec!["ETHEREUM"],
        Network::Matic => vec!["MATIC", "POLYGON"],
    }
}

pub fn ethereum_chain_id() -> ChainId {
    match std::env::var("ETHEREUM_CHAIN_ID").as_deref() {
        Ok("11155111") => ChainId::EthereumSepolia,
        _ => ChainId::EthereumMainnet,
    }
}

pub fn polygon_chain_id() -> ChainId {
    match std::env::var("POLYGON_CHAIN_ID").as_deref() {
        Ok("80002") => ChainId::MaticAmoy,
        _ => ChainId::MaticMainnet,
    }
}
