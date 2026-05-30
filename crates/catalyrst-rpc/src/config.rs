use anyhow::{Context, Result};
use std::collections::HashMap;
use std::env;

#[derive(Clone, Debug)]
pub struct Config {
    pub http_host: String,
    pub http_port: u16,
    pub upstreams: HashMap<String, String>,
}

pub fn chain_id_for(network: &str) -> Option<u64> {
    match network.to_ascii_lowercase().as_str() {
        "mainnet" | "ethereum" => Some(1),
        "sepolia" => Some(11155111),
        "polygon" => Some(137),
        "amoy" => Some(80002),
        "arbitrum" => Some(42161),
        "optimism" => Some(10),
        "avalanche" => Some(43114),
        "binance" => Some(56),
        "fantom" => Some(250),
        _ => None,
    }
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let mut upstreams = HashMap::new();

        let pairs = [
            ("mainnet", "RPC_UPSTREAM_MAINNET", "https://rpc.decentraland.org/mainnet"),
            ("ethereum", "RPC_UPSTREAM_ETHEREUM", "https://rpc.decentraland.org/mainnet"),
            ("sepolia", "RPC_UPSTREAM_SEPOLIA", "https://rpc.decentraland.org/sepolia"),
            ("polygon", "RPC_UPSTREAM_POLYGON", "https://rpc.decentraland.org/polygon"),
            ("amoy", "RPC_UPSTREAM_AMOY", "https://rpc.decentraland.org/amoy"),
            ("arbitrum", "RPC_UPSTREAM_ARBITRUM", "https://rpc.decentraland.org/arbitrum"),
            ("optimism", "RPC_UPSTREAM_OPTIMISM", "https://rpc.decentraland.org/optimism"),
            ("avalanche", "RPC_UPSTREAM_AVALANCHE", "https://rpc.decentraland.org/avalanche"),
            ("binance", "RPC_UPSTREAM_BINANCE", "https://rpc.decentraland.org/binance"),
            ("fantom", "RPC_UPSTREAM_FANTOM", "https://rpc.decentraland.org/fantom"),
        ];
        for (net, var, default) in pairs {
            let url = env::var(var).unwrap_or_else(|_| default.to_string());
            upstreams.insert(net.to_string(), url);
        }

        Ok(Self {
            http_host: env::var("HTTP_SERVER_HOST").unwrap_or_else(|_| "127.0.0.1".into()),
            http_port: env::var("HTTP_SERVER_PORT")
                .unwrap_or_else(|_| "5153".into())
                .parse()
                .context("HTTP_SERVER_PORT must be u16")?,
            upstreams,
        })
    }

    pub fn upstream_for(&self, network: &str) -> Option<&str> {
        self.upstreams
            .get(&network.to_ascii_lowercase())
            .map(|s| s.as_str())
    }
}
