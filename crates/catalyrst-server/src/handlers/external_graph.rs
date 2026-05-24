use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use tokio::sync::Mutex;

use crate::cache::ResponseCache;

pub struct SubgraphUrls {
    pub eth_collections: &'static str,
    pub matic_collections: &'static str,
    pub third_party_registry: &'static str,
    pub land: &'static str,
}

pub fn subgraph_urls(eth_network: &str) -> SubgraphUrls {
    match eth_network {
        "sepolia" => SubgraphUrls {
            eth_collections:
                "https://api.studio.thegraph.com/query/49472/collections-ethereum-sepolia/version/latest",
            matic_collections: "https://subgraph.decentraland.org/collections-matic-amoy",
            third_party_registry: "https://subgraph.decentraland.org/tpr-matic-amoy",
            land: "https://subgraph.decentraland.org/land-manager-sepolia",
        },

        _ => SubgraphUrls {
            eth_collections: "https://subgraph.decentraland.org/collections-ethereum-mainnet",
            matic_collections: "https://subgraph.decentraland.org/collections-matic-mainnet",
            third_party_registry: "https://subgraph.decentraland.org/tpr-matic-mainnet",
            land: "https://subgraph.decentraland.org/land-manager",
        },
    }
}

pub struct LandContracts {
    pub land: &'static str,
    pub estate: &'static str,
}

pub fn land_contracts(eth_network: &str) -> LandContracts {
    match eth_network {
        "sepolia" => LandContracts {
            land: "0x42f4ba48791e2de32f5fbf553441c2672864bb33",
            estate: "0x369a7fbe718c870c79f99fb423882e8dd8b20486",
        },
        _ => LandContracts {
            land: "0xf87e31492faf9a91b02ee0deaad50d51d56d5d4d",
            estate: "0x959e104e1a4db6317fa58f8295f586e1a978c297",
        },
    }
}

pub const THE_GRAPH_PAGE_SIZE: i64 = 1000;

fn nft_worker_base_url() -> String {
    std::env::var("NFT_WORKER_BASE_URL")
        .unwrap_or_else(|_| "https://nfts.decentraland.org".to_string())
}

pub fn client() -> &'static reqwest::Client {
    static CLIENT: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .timeout(Duration::from_secs(60))
            .build()
            .unwrap_or_default()
    })
}

pub async fn graph_query(
    url: &str,
    query: &str,
    variables: Value,
) -> Result<Value, String> {
    let resp = client()
        .post(url)
        .json(&json!({ "query": query, "variables": variables }))
        .send()
        .await
        .map_err(|e| format!("subgraph request failed: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("subgraph returned HTTP {}", resp.status()));
    }
    let body: Value = resp
        .json()
        .await
        .map_err(|e| format!("failed to parse subgraph response: {e}"))?;
    body.get("data")
        .cloned()
        .ok_or_else(|| format!("subgraph response without data: {body}"))
}

#[derive(Clone)]
pub struct TpContract {
    pub network: String,
    pub address: String,
}

#[derive(Clone)]
pub struct ThirdPartyProvider {
    pub id: String,
    pub contracts: Vec<TpContract>,
}

const TP_QUERY: &str = r#"
{
  thirdParties(where: {isApproved: true}) {
    id
    resolver
    metadata {
      thirdParty {
        name
        description
        contracts {
          network
          address
        }
      }
    }
  }
}"#;

struct ProviderCache {
    providers: Mutex<Option<(Vec<ThirdPartyProvider>, Instant)>>,
}

fn provider_cache() -> &'static ProviderCache {
    static C: std::sync::OnceLock<ProviderCache> = std::sync::OnceLock::new();
    C.get_or_init(|| ProviderCache {
        providers: Mutex::new(None),
    })
}

const PROVIDER_TTL: Duration = Duration::from_secs(6 * 60 * 60);

pub async fn third_party_providers(eth_network: &str) -> Vec<ThirdPartyProvider> {
    let cache = provider_cache();
    let mut guard = cache.providers.lock().await;
    if let Some((providers, at)) = guard.as_ref() {
        if at.elapsed() < PROVIDER_TTL {
            return providers.clone();
        }
    }

    let url = subgraph_urls(eth_network).third_party_registry;
    let providers = match graph_query(url, TP_QUERY, json!({})).await {
        Ok(data) => parse_providers(&data),
        Err(_) => {

            if let Some((p, _)) = guard.as_ref() {
                return p.clone();
            }
            Vec::new()
        }
    };

    *guard = Some((providers.clone(), Instant::now()));
    providers
}

fn parse_providers(data: &Value) -> Vec<ThirdPartyProvider> {
    let arr = data
        .get("thirdParties")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let mut out = Vec::with_capacity(arr.len());
    for p in arr {
        let id = p.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
        if id.is_empty() {
            continue;
        }
        let mut contracts = Vec::new();
        if let Some(cs) = p
            .get("metadata")
            .and_then(|m| m.get("thirdParty"))
            .and_then(|t| t.get("contracts"))
            .and_then(|c| c.as_array())
        {
            for c in cs {
                let network = c.get("network").and_then(|v| v.as_str()).unwrap_or("");
                let address = c.get("address").and_then(|v| v.as_str()).unwrap_or("");
                if !network.is_empty() && !address.is_empty() {
                    contracts.push(TpContract {
                        network: network.to_string(),

                        address: address.to_lowercase(),
                    });
                }
            }
        }
        out.push(ThirdPartyProvider { id, contracts });
    }
    out
}

const SUPPORTED_NETWORKS: [&str; 2] = ["mainnet", "matic"];

async fn owned_nfts_for_network(
    owner: &str,
    network: &str,
    contracts: &[String],
) -> Vec<String> {
    if !SUPPORTED_NETWORKS.contains(&network) {
        return Vec::new();
    }
    let url = format!(
        "{}/wallets/{}/networks/{}/nfts",
        nft_worker_base_url(),
        owner,
        network
    );
    let resp = match client()
        .post(&url)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .json(&contracts)
        .send()
        .await
    {
        Ok(r) => r,
        Err(_) => return Vec::new(),
    };
    if !resp.status().is_success() {
        return Vec::new();
    }
    let body: Value = match resp.json().await {
        Ok(b) => b,
        Err(_) => return Vec::new(),
    };
    body.get("data")
        .and_then(|d| d.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default()
}

pub async fn owned_nfts(
    owner: &str,
    contracts_by_network: &std::collections::HashMap<String, Vec<String>>,
) -> Vec<String> {

    let per_network = futures::future::join_all(
        contracts_by_network
            .iter()
            .map(|(network, contracts)| owned_nfts_for_network(owner, network, contracts)),
    )
    .await;
    let mut all = Vec::new();
    for urns in per_network {
        all.extend(urns);
    }
    all
}

pub fn mappings_includes_nft(
    mappings: &Value,
    network: &str,
    contract: &str,
    token_id: &str,
) -> bool {
    let list = match mappings
        .get(network)
        .and_then(|n| n.get(contract))
        .and_then(|c| c.as_array())
    {
        Some(l) => l,
        None => return false,
    };
    list.iter().any(|m| {
        match m.get("type").and_then(|t| t.as_str()) {
            Some("single") => m.get("id").and_then(|v| v.as_str()) == Some(token_id),
            Some("any") => true,
            Some("multiple") => m
                .get("ids")
                .and_then(|v| v.as_array())
                .map(|ids| ids.iter().any(|i| i.as_str() == Some(token_id)))
                .unwrap_or(false),
            Some("range") => {
                let from = m.get("from").and_then(|v| v.as_str());
                let to = m.get("to").and_then(|v| v.as_str());
                match (from, to, token_id.parse::<u128>().ok()) {
                    (Some(f), Some(t), Some(tk)) => {
                        match (f.parse::<u128>(), t.parse::<u128>()) {
                            (Ok(fv), Ok(tv)) => tk >= fv && tk <= tv,
                            _ => false,
                        }
                    }
                    _ => false,
                }
            }
            _ => false,
        }
    })
}

const QUERY_OPERATORS_PARCEL: &str = r#"
query fetchOperatorsOfParcel($x: Int, $y: Int){
  parcels(where: { and: [ { x: $x } { y: $y } ] }) {
    x
    y
    owner { address }
    operator
    updateOperator
  }
  estates(where: { parcels_: { x: $x, y: $y } }) {
    owner { address }
    operator
    updateOperator
  }
}"#;

const QUERY_AUTHORIZATIONS: &str = r#"
query fetchAuthorizations($address: String, $tokenAddress: String, $timestampFrom: BigInt) {
  authorizations(first: 1000, where: { owner_: { address: $address }, tokenAddress: $tokenAddress, timestamp_gt: $timestampFrom }, orderBy: timestamp, orderDirection: asc) {
    type
    operator
    isApproved
    timestamp
  }
}"#;

const QUERY_PARCELS_BY_UPDATE_OPERATOR: &str = r#"
query fetchParcelsByUpdateOperator($updateOperator: String, $first: Int, $skip: Int) {
  parcels(where: { updateOperator: $updateOperator } first: $first skip: $skip orderBy: id orderDirection: asc) {
    id
    x
    y
    owner { id }
    updateOperator
  }
}"#;

#[derive(Clone)]
pub struct ParcelOperators {
    pub owner: String,
    pub operator: Option<String>,
    pub update_operator: Option<String>,
    pub update_managers: Vec<String>,
    pub approved_for_all: Vec<String>,
}

const PARCEL_CACHE_TTL: Duration = Duration::from_secs(60);
const PARCEL_CACHE_MAX_ENTRIES: usize = 50_000;

fn parcel_cache() -> &'static Arc<ResponseCache<(String, i64, i64), Option<ParcelOperators>>> {
    static CACHE: std::sync::OnceLock<
        Arc<ResponseCache<(String, i64, i64), Option<ParcelOperators>>>,
    > = std::sync::OnceLock::new();
    CACHE.get_or_init(|| {
        Arc::new(ResponseCache::new(
            "parcel_operators",
            PARCEL_CACHE_TTL,
            PARCEL_CACHE_MAX_ENTRIES,
        ))
    })
}

pub async fn parcel_operators(
    eth_network: &str,
    x: i64,
    y: i64,
) -> Result<Option<ParcelOperators>, String> {
    let key = (eth_network.to_string(), x, y);
    let eth_network_owned = eth_network.to_string();
    parcel_cache()
        .get_or_fetch(key, move || async move {
            let url = subgraph_urls(&eth_network_owned).land;
            let data =
                graph_query(url, QUERY_OPERATORS_PARCEL, json!({ "x": x, "y": y })).await?;

            let estates = data.get("estates").and_then(|e| e.as_array());
            let parcels = data.get("parcels").and_then(|p| p.as_array());

            let first_estate = estates.and_then(|e| e.first());
            let first_parcel = parcels.and_then(|p| p.first());
            let belongs_to_estate = first_estate.is_some();

            let (owner, operator, update_operator) = if let Some(e) = first_estate {
                (
                    e.get("owner")
                        .and_then(|o| o.get("address"))
                        .and_then(|a| a.as_str())
                        .unwrap_or("")
                        .to_string(),
                    e.get("operator").and_then(|v| v.as_str()).map(|s| s.to_string()),
                    e.get("updateOperator")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string()),
                )
            } else if let Some(p) = first_parcel {
                (
                    p.get("owner")
                        .and_then(|o| o.get("address"))
                        .and_then(|a| a.as_str())
                        .unwrap_or("")
                        .to_string(),
                    p.get("operator").and_then(|v| v.as_str()).map(|s| s.to_string()),
                    p.get("updateOperator")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string()),
                )
            } else {
                // No estate or parcel — cache the negative result.
                return Ok::<Option<ParcelOperators>, String>(None);
            };

            let contracts = land_contracts(&eth_network_owned);
            let token_address = if belongs_to_estate {
                contracts.estate
            } else {
                contracts.land
            };

            let (update_managers, approved_for_all) =
                update_managers_and_approved_for_all(url, &owner, token_address).await?;

            Ok(Some(ParcelOperators {
                owner,
                operator,
                update_operator,
                update_managers,
                approved_for_all,
            }))
        })
        .await
}

async fn update_managers_and_approved_for_all(
    land_url: &str,
    address: &str,
    token_address: &str,
) -> Result<(Vec<String>, Vec<String>), String> {

    let mut update_managers: Vec<String> = Vec::new();
    let mut approved_for_all: Vec<String> = Vec::new();
    let mut timestamp_from: String = "0".to_string();

    loop {
        let data = graph_query(
            land_url,
            QUERY_AUTHORIZATIONS,
            json!({
                "address": address,
                "tokenAddress": token_address,
                "timestampFrom": timestamp_from,
            }),
        )
        .await?;

        let auths = data
            .get("authorizations")
            .and_then(|a| a.as_array())
            .cloned()
            .unwrap_or_default();

        for a in &auths {
            let kind = a.get("type").and_then(|v| v.as_str()).unwrap_or("");
            let operator = a.get("operator").and_then(|v| v.as_str()).unwrap_or("");
            let is_approved = a.get("isApproved").and_then(|v| v.as_bool()).unwrap_or(false);
            if operator.is_empty() {
                continue;
            }
            let target = match kind {
                "UpdateManager" => &mut update_managers,
                "Operator" => &mut approved_for_all,
                _ => continue,
            };
            if is_approved {
                if !target.iter().any(|o| o == operator) {
                    target.push(operator.to_string());
                }
            } else {
                target.retain(|o| o != operator);
            }
        }

        let len = auths.len() as i64;
        if len != THE_GRAPH_PAGE_SIZE {
            break;
        }

        timestamp_from = auths
            .last()
            .and_then(|a| a.get("timestamp"))
            .map(|t| match t {
                Value::String(s) => s.clone(),
                Value::Number(n) => n.to_string(),
                _ => "0".to_string(),
            })
            .unwrap_or_else(|| "0".to_string());
    }

    Ok((update_managers, approved_for_all))
}

pub async fn parcels_by_update_operator(
    eth_network: &str,
    update_operator: &str,
) -> Result<Vec<Value>, String> {
    let url = subgraph_urls(eth_network).land;
    let mut elements: Vec<Value> = Vec::new();
    let mut skip = 0i64;

    loop {
        let data = graph_query(
            url,
            QUERY_PARCELS_BY_UPDATE_OPERATOR,
            json!({
                "updateOperator": update_operator,
                "first": THE_GRAPH_PAGE_SIZE,
                "skip": skip,
            }),
        )
        .await?;

        let parcels = data
            .get("parcels")
            .and_then(|p| p.as_array())
            .cloned()
            .unwrap_or_default();

        if parcels.is_empty() {
            break;
        }

        for p in &parcels {
            elements.push(json!({
                "id": p.get("id").and_then(|v| v.as_str()).unwrap_or(""),
                "x": p.get("x").and_then(|v| v.as_str()).unwrap_or(""),
                "y": p.get("y").and_then(|v| v.as_str()).unwrap_or(""),
                "owner": p.get("owner").and_then(|o| o.get("id")).and_then(|v| v.as_str()).unwrap_or(""),
                "updateOperator": p.get("updateOperator").and_then(|v| v.as_str()).unwrap_or(""),
            }));
        }

        if (parcels.len() as i64) != THE_GRAPH_PAGE_SIZE {
            break;
        }
        skip += THE_GRAPH_PAGE_SIZE;
    }

    Ok(elements)
}

const QUERY_COLLECTIONS: &str =
    "{ collections (first: 1000, orderBy: urn, orderDirection: asc) { urn name } }";

pub async fn collections(url: &str) -> Result<Vec<(String, String)>, String> {
    let data = graph_query(url, QUERY_COLLECTIONS, json!({})).await?;
    let arr = data
        .get("collections")
        .and_then(|c| c.as_array())
        .cloned()
        .unwrap_or_default();
    Ok(arr
        .iter()
        .map(|c| {
            (
                c.get("urn").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                c.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            )
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn mappings_with(entry: Value) -> Value {
        json!({ "amoy": { "0xcontract": [entry] } })
    }

    #[test]
    fn single_matches_exact_id_only() {
        let m = mappings_with(json!({ "type": "single", "id": "42" }));
        assert!(mappings_includes_nft(&m, "amoy", "0xcontract", "42"));
        assert!(!mappings_includes_nft(&m, "amoy", "0xcontract", "43"));
    }

    #[test]
    fn any_matches_everything() {
        let m = mappings_with(json!({ "type": "any" }));
        assert!(mappings_includes_nft(&m, "amoy", "0xcontract", "0"));
        assert!(mappings_includes_nft(&m, "amoy", "0xcontract", "999999"));
    }

    #[test]
    fn multiple_matches_membership() {
        let m = mappings_with(json!({ "type": "multiple", "ids": ["1", "5", "9"] }));
        assert!(mappings_includes_nft(&m, "amoy", "0xcontract", "5"));
        assert!(!mappings_includes_nft(&m, "amoy", "0xcontract", "6"));
    }

    #[test]
    fn range_inclusive_endpoints() {
        let m = mappings_with(json!({ "type": "range", "from": "10", "to": "20" }));

        assert!(mappings_includes_nft(&m, "amoy", "0xcontract", "10"));
        assert!(mappings_includes_nft(&m, "amoy", "0xcontract", "20"));
        assert!(mappings_includes_nft(&m, "amoy", "0xcontract", "15"));

        assert!(!mappings_includes_nft(&m, "amoy", "0xcontract", "9"));
        assert!(!mappings_includes_nft(&m, "amoy", "0xcontract", "21"));
    }

    #[test]
    fn range_non_numeric_token_does_not_match() {
        let m = mappings_with(json!({ "type": "range", "from": "10", "to": "20" }));
        assert!(!mappings_includes_nft(&m, "amoy", "0xcontract", "fifteen"));
    }

    #[test]
    fn missing_network_or_contract_keys() {
        let m = mappings_with(json!({ "type": "any" }));
        assert!(!mappings_includes_nft(&m, "mainnet", "0xcontract", "1"));
        assert!(!mappings_includes_nft(&m, "amoy", "0xother", "1"));
    }

    #[test]
    fn unknown_type_does_not_match() {
        let m = mappings_with(json!({ "type": "bogus" }));
        assert!(!mappings_includes_nft(&m, "amoy", "0xcontract", "1"));
    }

    #[test]
    fn multiple_entries_any_match_wins() {
        let m = json!({
            "amoy": { "0xcontract": [
                { "type": "single", "id": "1" },
                { "type": "range", "from": "100", "to": "200" }
            ] }
        });
        assert!(mappings_includes_nft(&m, "amoy", "0xcontract", "1"));
        assert!(mappings_includes_nft(&m, "amoy", "0xcontract", "150"));
        assert!(!mappings_includes_nft(&m, "amoy", "0xcontract", "50"));
    }

    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc as StdArc;
    use std::time::Duration as StdDuration;

    /// Parcel cache shape mirrors `parcel_operators`: second call to the same
    /// (network, x, y) HITs and skips the upstream fetch.
    #[tokio::test]
    async fn parcel_operators_cache_second_call_is_a_hit() {
        let cache: ResponseCache<(String, i64, i64), Option<ParcelOperators>> =
            ResponseCache::new("parcel_test", StdDuration::from_secs(60), 100);
        let counter = StdArc::new(AtomicUsize::new(0));
        let key = ("mainnet".to_string(), 10, -5);

        let c = counter.clone();
        let v1 = cache
            .get_or_fetch(key.clone(), || async move {
                c.fetch_add(1, Ordering::SeqCst);
                Ok::<_, String>(Some(ParcelOperators {
                    owner: "0xowner".to_string(),
                    operator: None,
                    update_operator: None,
                    update_managers: vec![],
                    approved_for_all: vec![],
                }))
            })
            .await
            .unwrap();
        let c = counter.clone();
        let v2 = cache
            .get_or_fetch(key, || async move {
                c.fetch_add(1, Ordering::SeqCst);
                Ok::<_, String>(None)
            })
            .await
            .unwrap();
        assert_eq!(v1.as_ref().unwrap().owner, "0xowner");
        assert_eq!(v2.as_ref().unwrap().owner, "0xowner");
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    /// `None` (no estate/parcel found) is also a cacheable value.
    #[tokio::test]
    async fn parcel_operators_cache_caches_none() {
        let cache: ResponseCache<(String, i64, i64), Option<ParcelOperators>> =
            ResponseCache::new("parcel_test_none", StdDuration::from_secs(60), 100);
        let counter = StdArc::new(AtomicUsize::new(0));
        let key = ("mainnet".to_string(), 99, 99);

        let c = counter.clone();
        cache
            .get_or_fetch(key.clone(), || async move {
                c.fetch_add(1, Ordering::SeqCst);
                Ok::<_, String>(None)
            })
            .await
            .unwrap();
        let c = counter.clone();
        let v = cache
            .get_or_fetch(key, || async move {
                c.fetch_add(1, Ordering::SeqCst);
                Ok::<_, String>(Some(ParcelOperators {
                    owner: "0xowner".to_string(),
                    operator: None,
                    update_operator: None,
                    update_managers: vec![],
                    approved_for_all: vec![],
                }))
            })
            .await
            .unwrap();
        assert!(v.is_none());
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }
}
