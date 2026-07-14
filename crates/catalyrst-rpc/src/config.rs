use anyhow::Result;
use catalyrst_envcfg::{get_port, optional_endpoint};
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

        "polygon" | "matic" => Some(137),
        "amoy" => Some(80002),
        "mumbai" => Some(80001),
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
            ("mainnet", "RPC_UPSTREAM_MAINNET"),
            ("ethereum", "RPC_UPSTREAM_ETHEREUM"),
            ("sepolia", "RPC_UPSTREAM_SEPOLIA"),
            ("polygon", "RPC_UPSTREAM_POLYGON"),
            ("matic", "RPC_UPSTREAM_MATIC"),
            ("amoy", "RPC_UPSTREAM_AMOY"),
            ("mumbai", "RPC_UPSTREAM_MUMBAI"),
            ("arbitrum", "RPC_UPSTREAM_ARBITRUM"),
            ("optimism", "RPC_UPSTREAM_OPTIMISM"),
            ("avalanche", "RPC_UPSTREAM_AVALANCHE"),
            ("binance", "RPC_UPSTREAM_BINANCE"),
            ("fantom", "RPC_UPSTREAM_FANTOM"),
        ];
        for (net, var) in pairs {
            if let Some(url) = optional_endpoint(var) {
                upstreams.insert(net.to_string(), url);
            }
        }

        Ok(Self {
            http_host: env::var("HTTP_SERVER_HOST").unwrap_or_else(|_| "127.0.0.1".into()),
            http_port: get_port("HTTP_SERVER_PORT", 5153)?,
            upstreams,
        })
    }

    pub fn upstream_for(&self, network: &str) -> Option<&str> {
        self.upstreams
            .get(&network.to_ascii_lowercase())
            .map(|s| s.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SUPPORTED: &[(&str, u64)] = &[
        ("mainnet", 1),
        ("ethereum", 1),
        ("sepolia", 11155111),
        ("polygon", 137),
        ("matic", 137),
        ("amoy", 80002),
        ("mumbai", 80001),
        ("arbitrum", 42161),
        ("optimism", 10),
        ("avalanche", 43114),
        ("binance", 56),
        ("fantom", 250),
    ];

    #[test]
    fn chain_id_covers_full_client_network_set() {
        for (net, want) in SUPPORTED {
            assert_eq!(
                chain_id_for(net),
                Some(*want),
                "network {net} should map to chain id {want}"
            );
        }
    }

    #[test]
    fn chain_id_is_case_insensitive() {
        assert_eq!(chain_id_for("MAINNET"), Some(1));
        assert_eq!(chain_id_for("Polygon"), Some(137));
        assert_eq!(chain_id_for("MATIC"), Some(137));
    }

    #[test]
    fn unknown_network_has_no_chain_id() {
        assert_eq!(chain_id_for("solana"), None);
        assert_eq!(chain_id_for("goerli"), None);
        assert_eq!(chain_id_for(""), None);
        assert_eq!(chain_id_for("mainet"), None);
    }

    /// One test, not two: env vars are process-global and parallel test
    /// threads would race on them.
    #[test]
    fn upstreams_come_only_from_configuration() {
        for (net, _) in SUPPORTED {
            std::env::remove_var(format!("RPC_UPSTREAM_{}", net.to_uppercase()));
        }
        let bare = Config::from_env().expect("config");
        for (net, _) in SUPPORTED {
            assert_eq!(
                bare.upstream_for(net),
                None,
                "{net} must have no upstream until one is configured; a default here \
                 would relay to production"
            );
        }

        std::env::set_var("RPC_UPSTREAM_MAINNET", "http://127.0.0.1:8545");
        let configured = Config::from_env().expect("config");
        std::env::remove_var("RPC_UPSTREAM_MAINNET");
        assert_eq!(
            configured.upstream_for("mainnet"),
            Some("http://127.0.0.1:8545")
        );
        assert_eq!(configured.upstream_for("polygon"), None);
        for net in configured.upstreams.keys() {
            assert!(
                chain_id_for(net).is_some(),
                "configured network {net} has no known chain id"
            );
        }
    }

    #[test]
    fn upstream_lookup_is_case_insensitive_and_rejects_unknown() {
        let mut upstreams = HashMap::new();
        upstreams.insert("polygon".to_string(), "https://example/polygon".to_string());
        let cfg = Config {
            http_host: "127.0.0.1".into(),
            http_port: 0,
            upstreams,
        };
        assert_eq!(cfg.upstream_for("POLYGON"), Some("https://example/polygon"));
        assert_eq!(cfg.upstream_for("does-not-exist"), None);
    }
}
