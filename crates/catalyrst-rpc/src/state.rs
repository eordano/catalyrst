use crate::config::Config;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, RwLock};

pub const READ_ONLY_METHODS: &[&str] = &[
    "eth_getTransactionReceipt",
    "eth_estimateGas",
    "eth_call",
    "eth_getBalance",
    "eth_getStorageAt",
    "eth_blockNumber",
    "eth_gasPrice",
    "eth_protocolVersion",
    "net_version",
    "web3_sha3",
    "web3_clientVersion",
    "eth_getTransactionCount",
    "eth_getBlockByNumber",
    "eth_getCode",
];

pub struct AppStateInner {
    pub cfg: Config,
    pub http: reqwest::Client,

    pub allowed_methods: RwLock<BTreeSet<String>>,

    pub upstreams: RwLock<BTreeMap<String, String>>,

    pub admin_token: Option<String>,
}

impl AppStateInner {
    pub fn methods_snapshot(&self) -> Vec<String> {
        self.allowed_methods
            .read()
            .expect("allowed_methods lock poisoned")
            .iter()
            .cloned()
            .collect()
    }

    pub fn is_method_allowed(&self, method: &str) -> bool {
        self.allowed_methods
            .read()
            .expect("allowed_methods lock poisoned")
            .contains(method)
    }

    pub fn upstreams_snapshot(&self) -> BTreeMap<String, String> {
        self.upstreams
            .read()
            .expect("upstreams lock poisoned")
            .clone()
    }

    pub fn upstream_for(&self, network: &str) -> Option<String> {
        self.upstreams
            .read()
            .expect("upstreams lock poisoned")
            .get(&network.to_ascii_lowercase())
            .cloned()
    }
}

pub type AppState = Arc<AppStateInner>;
