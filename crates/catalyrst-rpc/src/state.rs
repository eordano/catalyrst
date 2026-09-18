use crate::config::Config;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

const MEMO_MAX_ENTRIES: usize = 256;

// Short-lived memo of successful responses for param-less methods.
#[derive(Default)]
pub struct RpcMemo {
    entries: Mutex<HashMap<String, (Instant, Value)>>,
}

impl RpcMemo {
    pub fn ttl_for(method: &str) -> Option<Duration> {
        match method {
            "eth_blockNumber" => Some(Duration::from_secs(1)),
            "net_version" | "web3_clientVersion" => Some(Duration::from_secs(2)),
            _ => None,
        }
    }

    pub fn get(&self, key: &str) -> Option<Value> {
        let map = self.entries.lock().expect("memo lock poisoned");
        let (until, body) = map.get(key)?;
        (*until > Instant::now()).then(|| body.clone())
    }

    pub fn put(&self, key: String, ttl: Duration, body: &Value) {
        if body.get("result").is_none() {
            return;
        }
        let mut map = self.entries.lock().expect("memo lock poisoned");
        let now = Instant::now();
        if map.len() >= MEMO_MAX_ENTRIES {
            map.retain(|_, (until, _)| *until > now);
            if map.len() >= MEMO_MAX_ENTRIES {
                map.clear();
            }
        }
        map.insert(key, (now + ttl, body.clone()));
    }
}

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

    pub memo: RpcMemo,
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
