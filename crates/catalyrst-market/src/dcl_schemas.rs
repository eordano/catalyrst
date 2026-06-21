use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "market/", rename_all = "snake_case")
)]
#[serde(rename_all = "snake_case")]
pub enum NftCategory {
    Parcel,
    Estate,
    Wearable,
    Ens,
    Emote,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "market/", rename_all = "UPPERCASE")
)]
#[serde(rename_all = "UPPERCASE")]
pub enum Network {
    Ethereum,
    Matic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum SquidNetwork {
    Ethereum,
    Polygon,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
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
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "market/", rename_all = "camelCase")
)]
pub struct Contract {
    pub name: String,
    pub address: String,
    pub category: NftCategory,
    pub network: Network,
    #[serde(rename = "chainId")]
    #[cfg_attr(feature = "ts", ts(type = "number"))]
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

pub fn peer_base_url() -> Option<String> {
    match std::env::var("PEER_BASE_URL") {
        Ok(v) if !v.trim().is_empty() => Some(v.trim().trim_end_matches('/').to_string()),
        _ => None,
    }
}

pub fn repoint_content_url(url: &str) -> String {
    match peer_base_url() {
        Some(base) => repoint_content_url_to(url, &base),
        None => url.to_string(),
    }
}

pub fn repoint_content_url_to(url: &str, base: &str) -> String {
    let Some(scheme_end) = url.find("://") else {
        return url.to_string();
    };
    match url[scheme_end + 3..].find('/') {
        Some(path_at) => format!("{base}{}", &url[scheme_end + 3 + path_at..]),
        None => url.to_string(),
    }
}

#[cfg(test)]
mod repoint_tests {
    use super::repoint_content_url_to;

    #[test]
    fn repoints_host_keeps_path() {
        assert_eq!(
            repoint_content_url_to(
                "https://peer.decentraland.org/lambdas/collections/contents/urn:x/thumbnail",
                "https://catalyst.example.org",
            ),
            "https://catalyst.example.org/lambdas/collections/contents/urn:x/thumbnail"
        );
    }

    #[test]
    fn leaves_non_urls_untouched() {
        assert_eq!(
            repoint_content_url_to("urn:decentraland:x", "https://c"),
            "urn:decentraland:x"
        );
        assert_eq!(repoint_content_url_to("", "https://c"), "");
        assert_eq!(
            repoint_content_url_to("https://host-no-path", "https://c"),
            "https://host-no-path"
        );
    }
}
