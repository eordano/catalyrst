use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::extract::State;
use axum::response::IntoResponse;
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::sync::Mutex;

use crate::state::AppState;

struct ContractAddrs {
    catalyst: &'static str,
    name_denylist: &'static str,
    poi: &'static str,
    tpr_subgraph: &'static str,
}

fn contracts_for(network: &str) -> ContractAddrs {
    match network {
        "sepolia" => ContractAddrs {
            catalyst: "0x9b5091588a4bae0a5ea54a35af3c31f57a68ed37",
            name_denylist: "0x6082b0b10b0fe9040652e35acbf3a22fe6764f27",
            poi: "0x7a0fad6854de8df1245da952cd3ae7f6893154c1",
            tpr_subgraph: "https://subgraph.decentraland.org/tpr-matic-amoy",
        },

        _ => ContractAddrs {
            catalyst: "0x4a2f10076101650f40342885b99b6b101d83c486",
            name_denylist: "0x0c4c90a4f29872a2e9ef4c4be3d419792bca9a36",
            poi: "0xFEC09d5C192aaf7Ec7E2C89Cc8D3224138391B2E",
            tpr_subgraph: "https://subgraph.decentraland.org/tpr-matic-mainnet",
        },
    }
}

const SEL_CATALYST_COUNT: &str = "18becc10";
const SEL_CATALYST_IDS: &str = "7b9b4f2c";
const SEL_CATALYST_BY_ID: &str = "c9038ce9";
const SEL_SIZE: &str = "949d225d";
const SEL_GET: &str = "9507d39a";

fn eth_rpc_url() -> String {
    std::env::var("RPC_ENDPOINT_ETH")
        .unwrap_or_else(|_| "https://rpc.decentraland.org/mainnet".to_string())
}

fn polygon_rpc_url() -> String {
    std::env::var("RPC_ENDPOINT_POLYGON")
        .unwrap_or_else(|_| "https://rpc.decentraland.org/polygon".to_string())
}

#[derive(Deserialize)]
struct RpcResponse {
    result: Option<Value>,
    error: Option<RpcError>,
}

#[derive(Deserialize)]
struct RpcError {
    code: i64,
    message: String,
}

async fn eth_call(
    client: &reqwest::Client,
    rpc_url: &str,
    to: &str,
    data_hex: &str,
) -> Result<String, String> {
    let req = json!({
        "jsonrpc": "2.0",
        "method": "eth_call",
        "params": [ { "to": to, "data": format!("0x{}", data_hex) }, "latest" ],
        "id": 1
    });

    let resp = client
        .post(rpc_url)
        .json(&req)
        .send()
        .await
        .map_err(|e| format!("RPC request failed: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!("RPC returned HTTP {}", resp.status()));
    }

    let body: RpcResponse = resp
        .json()
        .await
        .map_err(|e| format!("failed to parse RPC response: {e}"))?;

    if let Some(err) = body.error {
        return Err(format!("RPC error {}: {}", err.code, err.message));
    }

    match body.result {
        Some(Value::String(s)) => Ok(s),
        Some(other) => Err(format!("unexpected RPC result type: {other}")),
        None => Err("RPC returned null result with no error".into()),
    }
}

fn strip0x(s: &str) -> &str {
    s.strip_prefix("0x").unwrap_or(s)
}

fn hex_decode(s: &str) -> Result<Vec<u8>, String> {
    if !s.len().is_multiple_of(2) {
        return Err("odd-length hex".into());
    }
    let nibble = |c: u8| -> Result<u8, String> {
        match c {
            b'0'..=b'9' => Ok(c - b'0'),
            b'a'..=b'f' => Ok(c - b'a' + 10),
            b'A'..=b'F' => Ok(c - b'A' + 10),
            _ => Err(format!("invalid hex char: {}", c as char)),
        }
    };

    s.as_bytes()
        .chunks_exact(2)
        .map(|pair| Ok((nibble(pair[0])? << 4) | nibble(pair[1])?))
        .collect()
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        s.push(HEX[(b >> 4) as usize] as char);
        s.push(HEX[(b & 0x0f) as usize] as char);
    }
    s
}

fn encode_uint256(n: u64) -> String {
    format!("{:064x}", n)
}

const KECCAK_ROUND_CONSTANTS: [u64; 24] = [
    0x0000000000000001,
    0x0000000000008082,
    0x800000000000808a,
    0x8000000080008000,
    0x000000000000808b,
    0x0000000080000001,
    0x8000000080008081,
    0x8000000000008009,
    0x000000000000008a,
    0x0000000000000088,
    0x0000000080008009,
    0x000000008000000a,
    0x000000008000808b,
    0x800000000000008b,
    0x8000000000008089,
    0x8000000000008003,
    0x8000000000008002,
    0x8000000000000080,
    0x000000000000800a,
    0x800000008000000a,
    0x8000000080008081,
    0x8000000000008080,
    0x0000000080000001,
    0x8000000080008008,
];

const KECCAK_ROTATIONS: [u32; 24] = [
    1, 3, 6, 10, 15, 21, 28, 36, 45, 55, 2, 14, 27, 41, 56, 8, 25, 43, 62, 18, 39, 61, 20, 44,
];

const KECCAK_PI: [usize; 24] = [
    10, 7, 11, 17, 18, 3, 5, 16, 8, 21, 24, 4, 15, 23, 19, 13, 12, 2, 20, 14, 22, 9, 6, 1,
];

fn keccak_f1600(state: &mut [u64; 25]) {
    for &rc in &KECCAK_ROUND_CONSTANTS[..24] {
        let mut c = [0u64; 5];
        for x in 0..5 {
            c[x] = state[x] ^ state[x + 5] ^ state[x + 10] ^ state[x + 15] ^ state[x + 20];
        }
        let mut d = [0u64; 5];
        for x in 0..5 {
            d[x] = c[(x + 4) % 5] ^ c[(x + 1) % 5].rotate_left(1);
        }
        for x in 0..5 {
            for y in 0..5 {
                state[x + 5 * y] ^= d[x];
            }
        }

        let mut last = state[1];
        for i in 0..24 {
            let j = KECCAK_PI[i];
            let tmp = state[j];
            state[j] = last.rotate_left(KECCAK_ROTATIONS[i]);
            last = tmp;
        }

        for y in 0..5 {
            let row = [
                state[5 * y],
                state[5 * y + 1],
                state[5 * y + 2],
                state[5 * y + 3],
                state[5 * y + 4],
            ];
            for x in 0..5 {
                state[5 * y + x] = row[x] ^ ((!row[(x + 1) % 5]) & row[(x + 2) % 5]);
            }
        }

        state[0] ^= rc;
    }
}

fn keccak256(input: &[u8]) -> [u8; 32] {
    const RATE: usize = 136;
    let mut state = [0u64; 25];
    let mut offset = 0;

    while input.len() - offset >= RATE {
        absorb_block(&mut state, &input[offset..offset + RATE]);
        keccak_f1600(&mut state);
        offset += RATE;
    }

    let mut block = [0u8; RATE];
    let rem = &input[offset..];
    block[..rem.len()].copy_from_slice(rem);
    block[rem.len()] ^= 0x01;
    block[RATE - 1] ^= 0x80;
    absorb_block(&mut state, &block);
    keccak_f1600(&mut state);

    let mut out = [0u8; 32];
    for (i, chunk) in out.chunks_mut(8).enumerate() {
        chunk.copy_from_slice(&state[i].to_le_bytes());
    }
    out
}

fn absorb_block(state: &mut [u64; 25], block: &[u8]) {
    for (i, word) in block.chunks_exact(8).enumerate() {
        let mut w = [0u8; 8];
        w.copy_from_slice(word);
        state[i] ^= u64::from_le_bytes(w);
    }
}

fn to_checksum_address(addr: &str) -> String {
    let lower = strip0x(addr).to_ascii_lowercase();
    let hash = keccak256(lower.as_bytes());
    let mut out = String::with_capacity(42);
    out.push_str("0x");
    for (i, ch) in lower.chars().enumerate() {
        if ch.is_ascii_digit() {
            out.push(ch);
        } else {

            let byte = hash[i / 2];
            let nibble = if i % 2 == 0 { byte >> 4 } else { byte & 0x0f };
            if nibble >= 8 {
                out.push(ch.to_ascii_uppercase());
            } else {
                out.push(ch);
            }
        }
    }
    out
}

fn decode_uint_word(hex_word: &str) -> Result<u64, String> {
    let h = strip0x(hex_word);
    if h.len() < 64 {
        return Err(format!("word too short: {h}"));
    }

    let tail = &h[64 - 16..64];
    u64::from_str_radix(tail, 16).map_err(|e| format!("bad uint word: {e}"))
}

fn decode_string_return(hex: &str) -> Result<String, String> {
    let h = strip0x(hex);
    let bytes = hex_decode(h).map_err(|e| format!("bad hex: {e}"))?;
    decode_string_at(&bytes, 0)
}

fn decode_string_at(bytes: &[u8], head_word_idx: usize) -> Result<String, String> {
    let head_off = head_word_idx * 32;
    if bytes.len() < head_off + 32 {
        return Err("truncated head".into());
    }
    let offset = be_word_to_usize(&bytes[head_off..head_off + 32])?;
    if bytes.len() < offset + 32 {
        return Err("truncated string length".into());
    }
    let len = be_word_to_usize(&bytes[offset..offset + 32])?;
    let start = offset + 32;
    if bytes.len() < start + len {
        return Err("truncated string data".into());
    }
    String::from_utf8(bytes[start..start + len].to_vec())
        .map_err(|e| format!("string not utf8: {e}"))
}

fn be_word_to_usize(word: &[u8]) -> Result<usize, String> {
    if word.len() != 32 {
        return Err("word not 32 bytes".into());
    }

    let mut v: usize = 0;
    for &b in &word[24..32] {
        v = (v << 8) | b as usize;
    }
    Ok(v)
}

async fn fetch_servers(client: &reqwest::Client, network: &str) -> Result<Vec<Value>, String> {
    let c = contracts_for(network);
    let rpc = eth_rpc_url();

    let count_hex = eth_call(client, &rpc, c.catalyst, SEL_CATALYST_COUNT).await?;
    let count = decode_uint_word(&count_hex)?;

    let mut servers = Vec::with_capacity(count as usize);
    for i in 0..count {
        let ids_data = format!("{}{}", SEL_CATALYST_IDS, encode_uint256(i));
        let id_hex = eth_call(client, &rpc, c.catalyst, &ids_data).await?;
        let id_word = strip0x(&id_hex);
        if id_word.len() < 64 {
            return Err(format!("bad catalystIds return: {id_hex}"));
        }
        let id = format!("0x{}", &id_word[..64]);

        let by_id_data = format!("{}{}", SEL_CATALYST_BY_ID, &id_word[..64]);
        let rec_hex = eth_call(client, &rpc, c.catalyst, &by_id_data).await?;
        let rec = hex_decode(strip0x(&rec_hex)).map_err(|e| format!("bad hex: {e}"))?;
        if rec.len() < 96 {
            return Err(format!("catalystById return too short: {rec_hex}"));
        }

        let owner = to_checksum_address(&hex_encode(&rec[44..64]));
        let domain = decode_string_at(&rec, 2)?;

        if domain.starts_with("http://") {
            continue;
        }
        let mut address = domain.clone();
        if !address.starts_with("https://") {
            address = format!("https://{address}");
        }
        let address = address.trim().to_string();

        servers.push(json!({
            "baseUrl": address,
            "owner": owner,
            "id": id,
        }));
    }
    Ok(servers)
}

async fn fetch_list(
    client: &reqwest::Client,
    rpc: &str,
    contract: &str,
) -> Result<Vec<String>, String> {
    let size_hex = eth_call(client, rpc, contract, SEL_SIZE).await?;
    let size = decode_uint_word(&size_hex)?;

    let mut out = Vec::with_capacity(size as usize);
    for i in 0..size {
        let data = format!("{}{}", SEL_GET, encode_uint256(i));
        let val_hex = eth_call(client, rpc, contract, &data).await?;
        out.push(decode_string_return(&val_hex)?);
    }
    Ok(out)
}

async fn fetch_pois(client: &reqwest::Client, network: &str) -> Result<Vec<String>, String> {
    let c = contracts_for(network);
    fetch_list(client, &polygon_rpc_url(), c.poi).await
}

async fn fetch_denylisted_names(
    client: &reqwest::Client,
    network: &str,
) -> Result<Vec<String>, String> {
    let c = contracts_for(network);
    fetch_list(client, &eth_rpc_url(), c.name_denylist).await
}

async fn fetch_third_party_integrations(
    client: &reqwest::Client,
    network: &str,
) -> Result<Vec<Value>, String> {
    let c = contracts_for(network);
    let query = r#"{ thirdParties(where: {isApproved: true}, first: 1000) { id metadata { thirdParty { name description } } } }"#;
    let resp = client
        .post(c.tpr_subgraph)
        .json(&json!({ "query": query }))
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
    let providers = body
        .get("data")
        .and_then(|d| d.get("thirdParties"))
        .and_then(|t| t.as_array())
        .ok_or_else(|| format!("unexpected subgraph response: {body}"))?;

    let mut out = Vec::with_capacity(providers.len());
    for p in providers {
        let id = p.get("id").and_then(|v| v.as_str()).unwrap_or("");
        let tp = p
            .get("metadata")
            .and_then(|m| m.get("thirdParty"));
        let name = tp
            .and_then(|t| t.get("name"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let description = tp
            .and_then(|t| t.get("description"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        out.push(json!({
            "name": name,
            "description": description,
            "urn": id,
        }));
    }
    Ok(out)
}

const CACHE_TTL: Duration = Duration::from_secs(6 * 60 * 60);

struct Cached {
    value: Value,
    fetched_at: Instant,
}

struct ContractCaches {
    servers: Mutex<Option<Cached>>,
    pois: Mutex<Option<Cached>>,
    denylisted_names: Mutex<Option<Cached>>,
    third_party: Mutex<Option<Cached>>,
    client: reqwest::Client,
}

impl ContractCaches {
    fn new() -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(60))
            .build()
            .unwrap_or_else(|e| {
                tracing::warn!("failed to build reqwest client with timeout ({e}); using default");
                reqwest::Client::new()
            });
        Self {
            servers: Mutex::new(None),
            pois: Mutex::new(None),
            denylisted_names: Mutex::new(None),
            third_party: Mutex::new(None),
            client,
        }
    }
}

fn caches() -> &'static ContractCaches {
    static CACHES: std::sync::OnceLock<ContractCaches> = std::sync::OnceLock::new();
    CACHES.get_or_init(ContractCaches::new)
}

async fn cached_or_fetch<F, Fut>(
    slot: &Mutex<Option<Cached>>,
    fetch: F,
) -> Result<Value, String>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<Value, String>>,
{

    let cached_snapshot: Option<(Value, Instant)> = {
        let g = slot.lock().await;
        g.as_ref().map(|c| (c.value.clone(), c.fetched_at))
    };

    if let Some((ref value, fetched_at)) = cached_snapshot {
        if fetched_at.elapsed() < CACHE_TTL {
            return Ok(value.clone());
        }
    }

    let new_value = match fetch().await {
        Ok(v) => v,
        Err(e) => {

            if let Some((value, _)) = cached_snapshot {
                tracing::warn!("contract fetch failed ({e}); serving stale cache");
                return Ok(value);
            }
            return Err(e);
        }
    };

    {
        let mut g = slot.lock().await;
        *g = Some(Cached {
            value: new_value.clone(),
            fetched_at: Instant::now(),
        });
    }
    Ok(new_value)
}

pub async fn contracts_servers(State(s): State<Arc<AppState>>) -> impl IntoResponse {
    let c = caches();
    let network = s.eth_network.clone();
    match cached_or_fetch(&c.servers, || async {
        fetch_servers(&c.client, &network).await.map(Value::Array)
    })
    .await
    {
        Ok(v) => Json(v),
        Err(e) => {
            tracing::error!("contracts/servers failed: {e}");
            Json(json!([]))
        }
    }
}

pub async fn contracts_pois(State(s): State<Arc<AppState>>) -> impl IntoResponse {
    let c = caches();
    let network = s.eth_network.clone();
    match cached_or_fetch(&c.pois, || async {
        let pois = fetch_pois(&c.client, &network).await?;
        Ok(Value::Array(pois.into_iter().map(Value::String).collect()))
    })
    .await
    {
        Ok(v) => Json(v),
        Err(e) => {
            tracing::error!("contracts/pois failed: {e}");
            Json(json!([]))
        }
    }
}

pub async fn contracts_denylisted_names(State(s): State<Arc<AppState>>) -> impl IntoResponse {
    let c = caches();
    let network = s.eth_network.clone();
    match cached_or_fetch(&c.denylisted_names, || async {
        let names = fetch_denylisted_names(&c.client, &network).await?;
        Ok(Value::Array(names.into_iter().map(Value::String).collect()))
    })
    .await
    {
        Ok(v) => Json(v),
        Err(e) => {
            tracing::error!("contracts/denylisted-names failed: {e}");
            Json(json!([]))
        }
    }
}

pub async fn third_party_integrations(State(s): State<Arc<AppState>>) -> impl IntoResponse {
    let c = caches();
    let network = s.eth_network.clone();
    match cached_or_fetch(&c.third_party, || async {
        fetch_third_party_integrations(&c.client, &network)
            .await
            .map(Value::Array)
    })
    .await
    {
        Ok(v) => Json(json!({ "data": v })),
        Err(e) => {
            tracing::error!("third-party-integrations failed: {e}");
            Json(json!({ "data": [] }))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keccak_empty() {

        let h = keccak256(b"");
        assert_eq!(
            hex_encode(&h),
            "c5d2460186f7233c927e7db2dcc703c0e500b653ca82273b7bfad8045d85a470"
        );
    }

    #[test]
    fn keccak_abc() {
        let h = keccak256(b"abc");
        assert_eq!(
            hex_encode(&h),
            "4e03657aea45a94fc7d47ba826c8d667c0d1e6e33a64a036ec44f58fa12d6c45"
        );
    }

    #[test]
    fn eip55_checksum() {

        assert_eq!(
            to_checksum_address("0x75e1d32289679dfcb2f01fbc0e043b3d7f9cd443"),
            "0x75e1d32289679dfcB2F01fBc0e043B3d7F9Cd443"
        );

        assert_eq!(
            to_checksum_address("0x5aaeb6053f3e94c9b9a09f33669435e7ef1beaed"),
            "0x5aAeb6053F3E94C9b9A09f33669435E7Ef1BeAed"
        );
    }

    #[test]
    fn decode_string_simple() {

        let hexed = "0000000000000000000000000000000000000000000000000000000000000020\
                     0000000000000000000000000000000000000000000000000000000000000005\
                     68656c6c6f000000000000000000000000000000000000000000000000000000";
        assert_eq!(decode_string_return(hexed).unwrap(), "hello");
    }

    #[test]
    fn decode_uint_word_works() {
        assert_eq!(
            decode_uint_word(
                "0x0000000000000000000000000000000000000000000000000000000000000008"
            )
            .unwrap(),
            8
        );
    }
}
