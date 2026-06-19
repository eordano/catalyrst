use crate::config::Config;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, RwLock};

/// Default read-only JSON-RPC method allowlist. Used to seed the runtime-mutable
/// allowlist held in [`AppStateInner::allowed_methods`]. Operators may amend the
/// live set via the bearer-gated `/admin/rpc/methods` routes without restarting.
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
    /// Runtime-mutable JSON-RPC method allowlist (seeded from
    /// [`READ_ONLY_METHODS`]). Read on the hot relay path; amended by admins.
    pub allowed_methods: RwLock<BTreeSet<String>>,
    /// Runtime-mutable network → upstream URL map (seeded from `cfg.upstreams`).
    /// Read on the hot relay path; amended by admins.
    pub upstreams: RwLock<BTreeMap<String, String>>,
    /// Bearer token gating the `/admin/rpc/*` routes. `None` ⇒ admin routes
    /// fail closed (403). Sourced from `CATALYRST_RPC_ADMIN_TOKEN`.
    pub admin_token: Option<String>,
}

impl AppStateInner {
    /// Snapshot of the current allowlist as a sorted vec.
    pub fn methods_snapshot(&self) -> Vec<String> {
        self.allowed_methods
            .read()
            .expect("allowed_methods lock poisoned")
            .iter()
            .cloned()
            .collect()
    }

    /// Whether `method` is currently allowed on the relay.
    pub fn is_method_allowed(&self, method: &str) -> bool {
        self.allowed_methods
            .read()
            .expect("allowed_methods lock poisoned")
            .contains(method)
    }

    /// Snapshot of the current network → upstream map.
    pub fn upstreams_snapshot(&self) -> BTreeMap<String, String> {
        self.upstreams
            .read()
            .expect("upstreams lock poisoned")
            .clone()
    }

    /// Resolve the upstream URL for `network` (case-insensitive), honouring any
    /// runtime amendments made via the admin routes.
    pub fn upstream_for(&self, network: &str) -> Option<String> {
        self.upstreams
            .read()
            .expect("upstreams lock poisoned")
            .get(&network.to_ascii_lowercase())
            .cloned()
    }
}

pub type AppState = Arc<AppStateInner>;
