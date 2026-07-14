//! On-chain ownership of third-party wearables, used when no NFT indexer
//! (`NFT_WORKER_BASE_URL`) is configured: the entities' `mappings` name the
//! exact token ids a wearable is linked to, so the chain is asked about those
//! ids only, through the node's `RPC_ENDPOINT_ETH` / `RPC_ENDPOINT_POLYGON`.

use std::collections::{BTreeMap, BTreeSet};
use std::str::FromStr;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use alloy::primitives::{address, hex, Address, Bytes, U256};
use alloy::sol;
use alloy::sol_types::SolCall;
use catalyrst_commons::cache::TtlMap;
use serde_json::{json, Value};

use super::external_graph::client;

sol! {
    interface IERC165 {
        function supportsInterface(bytes4 interfaceId) external view returns (bool);
    }
    interface IERC721 {
        function balanceOf(address owner) external view returns (uint256);
        function ownerOf(uint256 tokenId) external view returns (address);
        function tokenOfOwnerByIndex(address owner, uint256 index) external view returns (uint256);
    }
    interface IERC1155 {
        function balanceOfBatch(address[] accounts, uint256[] ids) external view returns (uint256[]);
    }
    interface IMulticall3 {
        struct Call3 { address target; bool allowFailure; bytes callData; }
        struct Result3 { bool success; bytes returnData; }
        function aggregate3(Call3[] calls) external payable returns (Result3[] returnData);
    }
}

/// Same address on every chain it is deployed to, mainnet and polygon included.
const MULTICALL3: Address = address!("0xcA11bde05977b3631167028862bE2a173976CA11");
const IFACE_ERC1155: [u8; 4] = [0xd9, 0xb6, 0x7a, 0x26];
const IFACE_ERC721_ENUMERABLE: [u8; 4] = [0x78, 0x0e, 0x9d, 0x63];
/// A `range` mapping wider than this is not expanded into ids.
const RANGE_CAP: u64 = 2048;
/// Enumerable contracts are walked at most this far.
const ENUMERATE_CAP: u64 = 1000;
const MULTICALL_CHUNK: usize = 1000;
const RPC_BATCH: usize = 50;
const CACHE_TTL: Duration = Duration::from_secs(60);
const CACHE_MAX_ENTRIES: usize = 4096;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Candidates {
    /// Token ids exactly as the mappings spell them.
    pub ids: BTreeSet<String>,
    /// An `any` or an oversized `range` mapping: only enumerable contracts can answer.
    pub open: bool,
}

/// `(network, lowercase contract)` -> the token ids the mappings link to it.
pub type CandidateMap = BTreeMap<(String, String), Candidates>;

pub fn candidates_from_mappings<'a>(entities: impl IntoIterator<Item = &'a Value>) -> CandidateMap {
    let mut out = CandidateMap::new();
    for entity in entities {
        let Some(mappings) = entity
            .pointer("/metadata/mappings")
            .and_then(Value::as_object)
        else {
            continue;
        };
        for (network, contracts) in mappings {
            let Some(contracts) = contracts.as_object() else {
                continue;
            };
            for (contract, list) in contracts {
                let Some(list) = list.as_array() else {
                    continue;
                };
                let slot = out
                    .entry((network.clone(), contract.to_lowercase()))
                    .or_default();
                for m in list {
                    match m.get("type").and_then(Value::as_str) {
                        Some("single") => {
                            if let Some(id) = m.get("id").and_then(Value::as_str) {
                                slot.ids.insert(id.to_string());
                            }
                        }
                        Some("multiple") => {
                            for id in m.get("ids").and_then(Value::as_array).into_iter().flatten() {
                                if let Some(id) = id.as_str() {
                                    slot.ids.insert(id.to_string());
                                }
                            }
                        }
                        Some("range") => {
                            let bound = |k: &str| {
                                m.get(k)
                                    .and_then(Value::as_str)
                                    .and_then(|s| U256::from_str_radix(s, 10).ok())
                            };
                            match (bound("from"), bound("to")) {
                                (Some(from), Some(to))
                                    if to >= from && to - from < U256::from(RANGE_CAP) =>
                                {
                                    let mut id = from;
                                    loop {
                                        slot.ids.insert(id.to_string());
                                        if id == to {
                                            break;
                                        }
                                        id += U256::from(1);
                                    }
                                }
                                _ => slot.open = true,
                            }
                        }
                        Some("any") => slot.open = true,
                        _ => {}
                    }
                }
            }
        }
    }
    out
}

fn rpc_url_for(network: &str) -> Option<String> {
    let key = match network {
        "mainnet" => "RPC_ENDPOINT_ETH",
        "matic" => "RPC_ENDPOINT_POLYGON",
        _ => return None,
    };
    std::env::var(key)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

type CacheKey = (String, String, String);

fn cache() -> &'static TtlMap<CacheKey, Arc<Vec<String>>> {
    static C: OnceLock<TtlMap<CacheKey, Arc<Vec<String>>>> = OnceLock::new();
    C.get_or_init(|| TtlMap::bounded("third-party-onchain-owned", CACHE_TTL, CACHE_MAX_ENTRIES))
}

/// Owned `network:contract:tokenId` triples, the shape the NFT indexer path returns.
pub async fn owned_via_rpc(owner: &str, candidates: &CandidateMap) -> Vec<String> {
    let Ok(owner_addr) = Address::from_str(owner) else {
        return Vec::new();
    };
    let mut by_network: BTreeMap<&str, Vec<(String, Candidates)>> = BTreeMap::new();
    for ((network, contract), c) in candidates {
        by_network
            .entry(network.as_str())
            .or_default()
            .push((contract.clone(), c.clone()));
    }
    let mut all = Vec::new();
    for (network, contracts) in by_network {
        let Some(rpc) = rpc_url_for(network) else {
            tracing::warn!(
                network,
                "third-party wearable ownership: no NFT_WORKER_BASE_URL and no RPC endpoint \
                 for this network; reporting nothing owned"
            );
            continue;
        };
        let fingerprint = contracts
            .iter()
            .map(|(c, k)| format!("{c}/{}{}", k.ids.len(), if k.open { "+" } else { "" }))
            .collect::<Vec<_>>()
            .join(",");
        let key = (owner.to_lowercase(), network.to_string(), fingerprint);
        let network = network.to_string();
        let found = cache()
            .get_or_fetch(key, move || async move {
                let owned = owned_on_network(&rpc, owner_addr, &network, &contracts).await;
                Ok::<Arc<Vec<String>>, String>(Arc::new(owned))
            })
            .await
            .unwrap_or_default();
        all.extend(found.iter().cloned());
    }
    all
}

async fn owned_on_network(
    rpc: &str,
    owner: Address,
    network: &str,
    contracts: &[(String, Candidates)],
) -> Vec<String> {
    let mut probes = Vec::with_capacity(contracts.len() * 3);
    let mut addrs = Vec::with_capacity(contracts.len());
    for (contract, _) in contracts {
        let Ok(addr) = Address::from_str(contract) else {
            addrs.push(None);
            continue;
        };
        addrs.push(Some(addr));
        probes.push(RpcCall {
            to: addr,
            data: IERC721::balanceOfCall { owner }.abi_encode().into(),
        });
        probes.push(RpcCall {
            to: addr,
            data: IERC165::supportsInterfaceCall {
                interfaceId: IFACE_ERC1155.into(),
            }
            .abi_encode()
            .into(),
        });
        probes.push(RpcCall {
            to: addr,
            data: IERC165::supportsInterfaceCall {
                interfaceId: IFACE_ERC721_ENUMERABLE.into(),
            }
            .abi_encode()
            .into(),
        });
    }
    let answers = eth_call_batch(rpc, &probes).await;

    let mut out = Vec::new();
    let mut i = 0;
    for ((contract, cand), addr) in contracts.iter().zip(addrs) {
        let Some(addr) = addr else {
            continue;
        };
        let balance = answers
            .get(i)
            .and_then(Option::as_ref)
            .and_then(|b| IERC721::balanceOfCall::abi_decode_returns(b).ok());
        let is_1155 = decode_bool(answers.get(i + 1));
        let enumerable = decode_bool(answers.get(i + 2));
        i += 3;

        let holds_721 = balance.is_some_and(|b| !b.is_zero());
        let ids = if is_1155 {
            owned_1155(rpc, addr, owner, cand).await
        } else if holds_721 && enumerable {
            enumerate_721(rpc, addr, owner, balance.unwrap_or_default()).await
        } else if holds_721 {
            if cand.open {
                tracing::debug!(
                    contract,
                    "third-party mapping is open-ended on a non-enumerable ERC721; only the \
                     explicitly mapped ids are checked"
                );
            }
            check_721(rpc, addr, owner, cand).await
        } else {
            Vec::new()
        };
        out.extend(
            ids.into_iter()
                .map(|id| format!("{network}:{contract}:{id}")),
        );
    }
    out
}

fn decode_bool(answer: Option<&Option<Bytes>>) -> bool {
    answer
        .and_then(Option::as_ref)
        .and_then(|b| IERC165::supportsInterfaceCall::abi_decode_returns(b).ok())
        .unwrap_or(false)
}

fn parsed_ids(cand: &Candidates) -> Vec<(&str, U256)> {
    cand.ids
        .iter()
        .filter_map(|id| U256::from_str_radix(id, 10).ok().map(|u| (id.as_str(), u)))
        .collect()
}

async fn check_721(rpc: &str, addr: Address, owner: Address, cand: &Candidates) -> Vec<String> {
    let ids = parsed_ids(cand);
    let calldatas: Vec<Bytes> = ids
        .iter()
        .map(|(_, id)| IERC721::ownerOfCall { tokenId: *id }.abi_encode().into())
        .collect();
    let answers = multicall(rpc, addr, calldatas).await;
    ids.iter()
        .zip(answers)
        .filter(|(_, ret)| {
            ret.as_ref()
                .and_then(|b| IERC721::ownerOfCall::abi_decode_returns(b).ok())
                .is_some_and(|holder| holder == owner)
        })
        .map(|((id, _), _)| (*id).to_string())
        .collect()
}

async fn enumerate_721(rpc: &str, addr: Address, owner: Address, balance: U256) -> Vec<String> {
    let n = balance.min(U256::from(ENUMERATE_CAP)).to::<u64>();
    let calldatas: Vec<Bytes> = (0..n)
        .map(|i| {
            IERC721::tokenOfOwnerByIndexCall {
                owner,
                index: U256::from(i),
            }
            .abi_encode()
            .into()
        })
        .collect();
    multicall(rpc, addr, calldatas)
        .await
        .into_iter()
        .filter_map(|ret| {
            ret.and_then(|b| IERC721::tokenOfOwnerByIndexCall::abi_decode_returns(&b).ok())
        })
        .map(|id| id.to_string())
        .collect()
}

async fn owned_1155(rpc: &str, addr: Address, owner: Address, cand: &Candidates) -> Vec<String> {
    let ids = parsed_ids(cand);
    let calls: Vec<RpcCall> = ids
        .chunks(MULTICALL_CHUNK)
        .map(|chunk| {
            let ids: Vec<U256> = chunk.iter().map(|(_, u)| *u).collect();
            RpcCall {
                to: addr,
                data: IERC1155::balanceOfBatchCall {
                    accounts: vec![owner; ids.len()],
                    ids,
                }
                .abi_encode()
                .into(),
            }
        })
        .collect();
    let answers = eth_call_batch(rpc, &calls).await;
    let mut owned = Vec::new();
    for (chunk, answer) in ids.chunks(MULTICALL_CHUNK).zip(answers) {
        let Some(balances) = answer
            .as_ref()
            .and_then(|b| IERC1155::balanceOfBatchCall::abi_decode_returns(b).ok())
        else {
            continue;
        };
        for ((id, _), balance) in chunk.iter().zip(balances) {
            if !balance.is_zero() {
                owned.push((*id).to_string());
            }
        }
    }
    owned
}

/// One entry per calldata: the sub-call's return data when it succeeded.
async fn multicall(rpc: &str, target: Address, calldatas: Vec<Bytes>) -> Vec<Option<Bytes>> {
    let calls: Vec<RpcCall> = calldatas
        .chunks(MULTICALL_CHUNK)
        .map(|chunk| RpcCall {
            to: MULTICALL3,
            data: IMulticall3::aggregate3Call {
                calls: chunk
                    .iter()
                    .map(|d| IMulticall3::Call3 {
                        target,
                        allowFailure: true,
                        callData: d.clone(),
                    })
                    .collect(),
            }
            .abi_encode()
            .into(),
        })
        .collect();
    let answers = eth_call_batch(rpc, &calls).await;
    let mut out = Vec::with_capacity(calldatas.len());
    for (chunk, answer) in calldatas.chunks(MULTICALL_CHUNK).zip(answers) {
        out.extend(decode_aggregate3(answer.as_ref(), chunk.len()));
    }
    out
}

fn decode_aggregate3(answer: Option<&Bytes>, expected: usize) -> Vec<Option<Bytes>> {
    match answer.and_then(|b| IMulticall3::aggregate3Call::abi_decode_returns(b).ok()) {
        Some(results) if results.len() == expected => results
            .into_iter()
            .map(|r| r.success.then_some(r.returnData))
            .collect(),
        _ => (0..expected).map(|_| None).collect(),
    }
}

struct RpcCall {
    to: Address,
    data: Bytes,
}

/// JSON-RPC batches of `eth_call`; a failed or reverted call is `None`.
async fn eth_call_batch(rpc: &str, calls: &[RpcCall]) -> Vec<Option<Bytes>> {
    let mut out: Vec<Option<Bytes>> = vec![None; calls.len()];
    for (chunk_index, chunk) in calls.chunks(RPC_BATCH).enumerate() {
        let base = chunk_index * RPC_BATCH;
        let body: Vec<Value> = chunk
            .iter()
            .enumerate()
            .map(|(i, c)| {
                json!({
                    "jsonrpc": "2.0",
                    "id": base + i,
                    "method": "eth_call",
                    "params": [{ "to": c.to.to_string(), "data": c.data.to_string() }, "latest"],
                })
            })
            .collect();
        let resp = match client().post(rpc).json(&body).send().await {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!(error = %e, "eth_call batch failed");
                continue;
            }
        };
        if !resp.status().is_success() {
            tracing::warn!(status = %resp.status(), "eth_call batch rejected");
            continue;
        }
        let parsed: Value = match resp.json().await {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(error = %e, "eth_call batch: unreadable response");
                continue;
            }
        };
        let Some(items) = parsed.as_array() else {
            tracing::warn!("eth_call batch: endpoint answered a single object, not a batch");
            continue;
        };
        for item in items {
            let Some(idx) = item.get("id").and_then(Value::as_u64).map(|id| id as usize) else {
                continue;
            };
            if idx < base || idx >= base + chunk.len() {
                continue;
            }
            if let Some(hex_str) = item.get("result").and_then(Value::as_str) {
                if let Ok(bytes) = hex::decode(hex_str.trim_start_matches("0x")) {
                    out[idx] = Some(Bytes::from(bytes));
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy::sol_types::SolValue;

    fn entity(mappings: Value) -> Value {
        json!({ "metadata": { "mappings": mappings } })
    }

    #[test]
    fn candidates_follow_every_mapping_type() {
        let entities = [
            entity(json!({ "mainnet": { "0xAbC": [
                { "type": "single", "id": "4" },
                { "type": "multiple", "ids": ["502", "0503"] },
                { "type": "range", "from": "10", "to": "12" },
            ] } })),
            entity(
                json!({ "mainnet": { "0xabc": [{ "type": "single", "id": "7" }] },
                            "matic": { "0xdef": [{ "type": "any" }] } }),
            ),
            entity(
                json!({ "matic": { "0x123": [{ "type": "range", "from": "1", "to": "1000000" }] } }),
            ),
            json!({ "metadata": {} }),
        ];
        let got = candidates_from_mappings(entities.iter());

        let abc = &got[&("mainnet".into(), "0xabc".into())];
        let ids: Vec<&str> = abc.ids.iter().map(String::as_str).collect();
        assert_eq!(ids, ["0503", "10", "11", "12", "4", "502", "7"]);
        assert!(!abc.open);
        assert!(got[&("matic".into(), "0xdef".into())].open);
        let wide = &got[&("matic".into(), "0x123".into())];
        assert!(wide.open && wide.ids.is_empty());
        assert_eq!(got.len(), 3);
    }

    #[test]
    fn aggregate3_answers_map_back_per_subcall() {
        let holder = address!("0x29fb0d1b0836f9963f963b0bb07c49d2d61370b4");
        let results = vec![
            IMulticall3::Result3 {
                success: true,
                returnData: holder.abi_encode().into(),
            },
            IMulticall3::Result3 {
                success: false,
                returnData: Bytes::new(),
            },
            IMulticall3::Result3 {
                success: true,
                returnData: U256::from(4576).abi_encode().into(),
            },
        ];
        let blob = Bytes::from(results.abi_encode());

        let decoded = decode_aggregate3(Some(&blob), 3);
        assert_eq!(decoded.len(), 3);
        let owner = IERC721::ownerOfCall::abi_decode_returns(decoded[0].as_ref().unwrap()).unwrap();
        assert_eq!(owner, holder);
        assert!(decoded[1].is_none());
        let id = IERC721::tokenOfOwnerByIndexCall::abi_decode_returns(decoded[2].as_ref().unwrap())
            .unwrap();
        assert_eq!(id.to_string(), "4576");

        assert_eq!(decode_aggregate3(Some(&blob), 2), vec![None, None]);
        assert_eq!(decode_aggregate3(None, 1), vec![None]);
    }

    #[test]
    fn unknown_networks_have_no_rpc() {
        assert!(rpc_url_for("sepolia").is_none());
        assert!(rpc_url_for("").is_none());
    }

    /// Holdings of the conformance-suite address on 2026-09-08: an enumerable
    /// ERC721 (woodies #4576) and an ERC1155 (rekt id 4).
    #[tokio::test]
    #[ignore = "live chain reads; set CATALYRST_LIVE_RPC_ETH to a mainnet JSON-RPC URL"]
    async fn live_mainnet_holdings_of_the_conformance_address() {
        let Ok(rpc) = std::env::var("CATALYRST_LIVE_RPC_ETH") else {
            return;
        };
        let owner = address!("0x29fb0d1b0836f9963f963b0bb07c49d2d61370b4");
        let contracts = vec![
            (
                "0x134460d32fc66a6d84487c20dcd9fdcf92316017".to_string(),
                Candidates {
                    ids: BTreeSet::new(),
                    open: true,
                },
            ),
            (
                "0x4fe914990e7b463567ddbae6c00fde78de957f56".to_string(),
                Candidates {
                    ids: ["3", "4"].into_iter().map(String::from).collect(),
                    open: false,
                },
            ),
        ];
        let owned = owned_on_network(&rpc, owner, "mainnet", &contracts).await;
        assert!(
            owned.contains(&"mainnet:0x134460d32fc66a6d84487c20dcd9fdcf92316017:4576".to_string()),
            "{owned:?}"
        );
        assert!(
            owned.contains(&"mainnet:0x4fe914990e7b463567ddbae6c00fde78de957f56:4".to_string()),
            "{owned:?}"
        );
        assert!(
            !owned.contains(&"mainnet:0x4fe914990e7b463567ddbae6c00fde78de957f56:3".to_string()),
            "{owned:?}"
        );
    }
}
