use serde_json::Value;
use sqlx::PgPool;

fn is_base_wearable(urn: &str) -> bool {
    urn.contains("base-avatars")
}

fn is_base_emote(urn: &str) -> bool {
    urn.starts_with("urn:decentraland:off-chain:")
        && !urn.ends_with(':')
        && catalyrst_types::item_schema::is_base_emote(urn)
}

fn split_urn_and_token_id(urn: &str) -> (&str, Option<&str>) {
    let segment_count = urn.split(':').count();
    if segment_count == 7 && !urn.contains("collections-thirdparty") {
        if let Some(last_colon) = urn.rfind(':') {
            return (&urn[..last_colon], Some(&urn[last_colon + 1..]));
        }
    }
    (urn, None)
}

fn normalize_urn(urn: &str) -> String {
    urn.replacen(":ethereum:", ":mainnet:", 1)
}

#[cfg(test)]
fn resolve_owned(
    normalized_urns: &[String],
    owned_exact: &std::collections::HashSet<String>,
    owned_prefixes: &std::collections::HashSet<String>,
) -> std::collections::HashSet<String> {
    let mut owned = std::collections::HashSet::new();
    for urn in normalized_urns {
        if owned_exact.contains(urn) || owned_prefixes.contains(urn) {
            owned.insert(urn.clone());
        }
    }
    owned
}

/// `[lo, hi)` per item urn: `lo` is the token prefix `urn:`, `hi` swaps its last byte for
/// the next one (':' -> ';'), so a bytewise (C collation) walk of nft_owner_urn between
/// them visits only the token urns under that item instead of every urn the owner holds.
fn prefix_bounds(urns: &[String]) -> (Vec<String>, Vec<String>) {
    urns.iter()
        .map(|u| (format!("{u}:"), format!("{u};")))
        .unzip()
}

/// One row per owned `(address, urn)` pair (exact urn or a token urn under it), one round trip per batch.
fn ownership_sql(overlay: bool) -> &'static str {
    if overlay {
        "SELECT DISTINCT u.address, u.urn \
         FROM unnest($1::text[], $2::text[], $3::text[], $4::text[]) AS u(address, urn, lo, hi) \
         CROSS JOIN LATERAL ( \
             (SELECT 1 FROM squid_marketplace.nft n \
              WHERE n.owner_address = u.address AND n.urn = u.urn \
              LIMIT 1) \
             UNION ALL \
             (SELECT 1 FROM marketplace.usage_grants ug \
              WHERE ug.status = 'active' \
                AND ug.grantee_address = u.address AND ug.urn = u.urn \
              LIMIT 1) \
             UNION ALL \
             (SELECT 1 FROM squid_marketplace.nft n \
              WHERE n.owner_address = u.address \
                AND n.urn >= u.lo AND n.urn < u.hi \
                AND left(n.urn, length(u.lo)) = u.lo \
              LIMIT 1) \
             UNION ALL \
             (SELECT 1 FROM marketplace.usage_grants ug \
              WHERE ug.status = 'active' \
                AND ug.grantee_address = u.address \
                AND ug.urn >= u.lo AND ug.urn < u.hi \
                AND left(ug.urn, length(u.lo)) = u.lo \
              LIMIT 1) \
             LIMIT 1 \
         ) owned"
    } else {
        "SELECT DISTINCT u.address, u.urn \
         FROM unnest($1::text[], $2::text[], $3::text[], $4::text[]) AS u(address, urn, lo, hi) \
         CROSS JOIN LATERAL ( \
             (SELECT 1 FROM squid_marketplace.nft n \
              WHERE n.owner_address = u.address AND n.urn = u.urn \
              LIMIT 1) \
             UNION ALL \
             (SELECT 1 FROM squid_marketplace.nft n \
              WHERE n.owner_address = u.address \
                AND n.urn >= u.lo AND n.urn < u.hi \
                AND left(n.urn, length(u.lo)) = u.lo \
              LIMIT 1) \
             LIMIT 1 \
         ) owned"
    }
}

async fn resolve_ownership_batch(
    pool: &PgPool,
    requested: &std::collections::HashSet<(String, String)>,
) -> std::collections::HashMap<String, std::collections::HashSet<String>> {
    use std::collections::{HashMap, HashSet};
    if requested.is_empty() {
        return HashMap::new();
    }
    let (addresses, urns): (Vec<_>, Vec<_>) = requested.iter().cloned().unzip();
    let (lows, highs) = prefix_bounds(&urns);
    let overlay = super::lease_overlay::usage_grants_present(pool).await;
    let rows: Vec<(String, String)> = sqlx::query_as(ownership_sql(overlay))
        .bind(&addresses)
        .bind(&urns)
        .bind(&lows)
        .bind(&highs)
        .fetch_all(pool)
        .await
        .unwrap_or_default();
    let mut owned: HashMap<String, HashSet<String>> = HashMap::new();
    for (address, urn) in rows {
        owned.entry(address).or_default().insert(urn);
    }
    owned
}

async fn fetch_batch_ens_names(
    pool: &PgPool,
    addresses: &[String],
) -> std::collections::HashMap<String, Vec<String>> {
    use std::collections::{HashMap, HashSet};
    let rows: Vec<(String, Option<String>)> = sqlx::query_as(
        "SELECT owner_address, name FROM squid_marketplace.nft
         WHERE category = 'ens' AND owner_address = ANY($1) ORDER BY id ASC",
    )
    .bind(addresses)
    .fetch_all(pool)
    .await
    .unwrap_or_default();
    let mut names: HashMap<String, Vec<String>> = HashMap::new();
    let mut invalid = HashSet::new();
    for (address, name) in rows {
        if let Some(name) = name {
            names.entry(address).or_default().push(name);
        } else {
            invalid.insert(address);
        }
    }
    for address in invalid {
        names.remove(&address);
    }
    names
}

pub async fn fetch_owned_ens_names(pool: &PgPool, address: &str) -> Vec<String> {
    sqlx::query_scalar::<_, String>(
        "SELECT name FROM squid_marketplace.nft \
         WHERE category = 'ens' AND owner_address = lower($1) \
         ORDER BY id ASC",
    )
    .bind(address)
    .fetch_all(pool)
    .await
    .unwrap_or_default()
}

pub fn apply_claimed_name(metadata: &mut Value, owned_names: &[String]) {
    let avatars = match metadata.get_mut("avatars").and_then(|v| v.as_array_mut()) {
        Some(arr) => arr,
        None => return,
    };

    for avatar_val in avatars.iter_mut() {
        let claimed = avatar_val
            .get("name")
            .and_then(|v| v.as_str())
            .map(|name| owned_names.iter().any(|owned| owned == name))
            .unwrap_or(false);
        if let Some(obj) = avatar_val.as_object_mut() {
            obj.insert("hasClaimedName".to_string(), Value::Bool(claimed));
        }
    }
}

fn collect_ownership_urns(metadata: &Value) -> Vec<String> {
    let mut to_check: Vec<String> = Vec::new();

    let avatars = match metadata.get("avatars").and_then(|v| v.as_array()) {
        Some(arr) => arr,
        None => return to_check,
    };

    for avatar_val in avatars.iter() {
        let avatar_obj = match avatar_val.get("avatar") {
            Some(a) => a,
            None => continue,
        };

        if let Some(wearables) = avatar_obj.get("wearables").and_then(|w| w.as_array()) {
            for wearable_val in wearables {
                let wearable = match wearable_val.as_str() {
                    Some(s) => s,
                    None => continue,
                };
                if is_base_wearable(wearable) {
                    continue;
                }
                let (urn, _token_id) = split_urn_and_token_id(wearable);
                to_check.push(normalize_urn(urn));
            }
        }

        if let Some(emotes) = avatar_obj.get("emotes").and_then(|e| e.as_array()) {
            for emote_val in emotes {
                let emote_urn = match emote_val.get("urn").and_then(|u| u.as_str()) {
                    Some(s) => s,
                    None => continue,
                };
                if !emote_urn.contains(':') || is_base_emote(emote_urn) {
                    continue;
                }
                let (urn, _token_id) = split_urn_and_token_id(emote_urn);
                to_check.push(normalize_urn(urn));
            }
        }
    }

    to_check
}

fn filter_avatars_by_ownership(avatars: &mut [Value], owned: &std::collections::HashSet<String>) {
    for avatar_val in avatars.iter_mut() {
        let avatar_obj = match avatar_val.get_mut("avatar") {
            Some(a) => a,
            None => continue,
        };

        if let Some(wearables) = avatar_obj
            .get("wearables")
            .and_then(|w| w.as_array())
            .cloned()
        {
            let mut validated: Vec<Value> = Vec::new();
            for wearable_val in &wearables {
                let wearable = match wearable_val.as_str() {
                    Some(s) => s,
                    None => continue,
                };

                if is_base_wearable(wearable) {
                    validated.push(wearable_val.clone());
                    continue;
                }

                let (urn, _token_id) = split_urn_and_token_id(wearable);
                if owned.contains(&normalize_urn(urn)) {
                    validated.push(wearable_val.clone());
                }
            }
            avatar_obj["wearables"] = Value::Array(validated);
        }

        if let Some(emotes) = avatar_obj.get("emotes").and_then(|e| e.as_array()).cloned() {
            let mut validated: Vec<Value> = Vec::new();
            for emote_val in &emotes {
                let emote_urn = match emote_val.get("urn").and_then(|u| u.as_str()) {
                    Some(s) => s,
                    None => {
                        validated.push(emote_val.clone());
                        continue;
                    }
                };

                if !emote_urn.contains(':') || is_base_emote(emote_urn) {
                    validated.push(emote_val.clone());
                    continue;
                }

                let (urn, _token_id) = split_urn_and_token_id(emote_urn);
                if owned.contains(&normalize_urn(urn)) {
                    validated.push(emote_val.clone());
                }
            }
            avatar_obj["emotes"] = Value::Array(validated);
        }
    }
}

pub fn rewrite_snapshot_urls(entity_id: &str, metadata: &mut Value, cdn_base: &str) {
    let avatars = match metadata.get_mut("avatars").and_then(|v| v.as_array_mut()) {
        Some(arr) => arr,
        None => return,
    };

    let base = if cdn_base.ends_with('/') {
        cdn_base.to_string()
    } else {
        format!("{cdn_base}/")
    };

    for avatar_val in avatars.iter_mut() {
        let avatar_obj = match avatar_val.get_mut("avatar").and_then(|a| a.as_object_mut()) {
            Some(o) => o,
            None => continue,
        };
        avatar_obj.insert(
            "snapshots".to_string(),
            serde_json::json!({
                "face256": format!("{}entities/{}/face.png", base, entity_id),
                "body": format!("{}entities/{}/body.png", base, entity_id),
            }),
        );
    }
}

fn link_url_regex() -> &'static regex::Regex {
    use std::sync::OnceLock;
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    RE.get_or_init(|| {
        regex::Regex::new(r"(?i)^(?:https?)://[^\s/$.?#].[^\s]*$").expect("LinkUrl regex is valid")
    })
}

pub fn sanitize_links(metadata: &mut Value) {
    let avatars = match metadata.get_mut("avatars").and_then(|v| v.as_array_mut()) {
        Some(arr) => arr,
        None => return,
    };

    let re = link_url_regex();

    for avatar_val in avatars.iter_mut() {
        let links = match avatar_val.get_mut("links").and_then(|l| l.as_array_mut()) {
            Some(arr) => arr,
            None => continue,
        };

        let mut sanitized: Vec<Value> = Vec::new();
        for link in links.iter() {
            let url = match link.get("url").and_then(|u| u.as_str()) {
                Some(s) => s,
                None => continue,
            };

            if re.is_match(url) && catalyrst_types::sanitize::is_safe_link_target(url) {
                sanitized.push(link.clone());
                continue;
            }

            if let Ok(decoded) = urlencoding::decode(url) {
                if re.is_match(&decoded) && catalyrst_types::sanitize::is_safe_link_target(&decoded)
                {
                    let mut link_clone = link.clone();
                    if let Some(obj) = link_clone.as_object_mut() {
                        obj.insert("url".to_string(), Value::String(decoded.into_owned()));
                    }
                    sanitized.push(link_clone);
                }
            }
        }

        avatar_val["links"] = Value::Array(sanitized);
    }
}

pub fn ensure_profile_shape(entity: &Value, metadata: &mut Value) {
    if metadata.get("timestamp").is_none() {
        if let Some(ts) = entity.get("timestamp") {
            metadata["timestamp"] = ts.clone();
        }
    }

    if metadata.get("avatars").is_none() {
        metadata["avatars"] = Value::Array(vec![]);
    }
}

fn apply_pointer_identity(metadata: &mut Value, eth_address: &str) {
    if let Some(avatars) = metadata.get_mut("avatars").and_then(|v| v.as_array_mut()) {
        for avatar in avatars.iter_mut().filter(|a| a.is_object()) {
            avatar["userId"] = Value::String(eth_address.to_string());
            avatar["ethAddress"] = Value::String(eth_address.to_string());
        }
    }
}

pub fn entity_id(entity: &Value) -> Option<&str> {
    entity.get("id").and_then(|v| v.as_str())
}

pub fn entity_eth_address(entity: &Value) -> Option<String> {
    entity
        .get("pointers")
        .and_then(|v| v.as_array())
        .and_then(|arr| arr.first())
        .and_then(|v| v.as_str())
        .map(|s| s.to_lowercase())
}

pub async fn process_profile(
    entity: &Value,
    squid_pool: Option<&PgPool>,
    cdn_base: &str,
) -> Option<Value> {
    process_profiles_positional(std::slice::from_ref(entity), squid_pool, cdn_base)
        .await
        .pop()
        .flatten()
}

pub async fn process_profiles(
    entities: &[Value],
    squid_pool: Option<&PgPool>,
    cdn_base: &str,
) -> Vec<Value> {
    process_profiles_positional(entities, squid_pool, cdn_base)
        .await
        .into_iter()
        .flatten()
        .collect()
}

/// One slot per entity (`None` without metadata); each chunk of 128 costs two concurrent round trips.
pub async fn process_profiles_positional(
    entities: &[Value],
    squid_pool: Option<&PgPool>,
    cdn_base: &str,
) -> Vec<Option<Value>> {
    use std::collections::HashSet;
    let mut profiles = Vec::with_capacity(entities.len());
    for chunk in entities.chunks(128) {
        let mut prepared: Vec<Option<(String, Value)>> = chunk
            .iter()
            .map(|entity| {
                let mut metadata = entity.get("metadata")?.clone();
                let eid = entity_id(entity).unwrap_or("");
                let address = entity_eth_address(entity).unwrap_or_default();
                ensure_profile_shape(entity, &mut metadata);
                sanitize_links(&mut metadata);
                rewrite_snapshot_urls(eid, &mut metadata, cdn_base);
                if !address.is_empty() && !address.starts_with("default") {
                    apply_pointer_identity(&mut metadata, &address);
                }
                Some((address, metadata))
            })
            .collect();
        if let Some(pool) = squid_pool {
            let mut addresses = HashSet::new();
            let mut requested = HashSet::new();
            for (address, metadata) in prepared.iter().flatten() {
                if !address.starts_with("default") {
                    addresses.insert(address.clone());
                    requested.extend(
                        collect_ownership_urns(metadata)
                            .into_iter()
                            .map(|urn| (address.clone(), urn)),
                    );
                }
            }
            if !addresses.is_empty() {
                let addresses: Vec<_> = addresses.into_iter().collect();
                let (owned, names) = tokio::join!(
                    resolve_ownership_batch(pool, &requested),
                    fetch_batch_ens_names(pool, &addresses),
                );
                let empty = HashSet::new();
                for (address, metadata) in prepared.iter_mut().flatten() {
                    if address.starts_with("default") {
                        continue;
                    }
                    if let Some(avatars) = metadata.get_mut("avatars").and_then(Value::as_array_mut)
                    {
                        filter_avatars_by_ownership(avatars, owned.get(address).unwrap_or(&empty));
                    }
                    apply_claimed_name(
                        metadata,
                        names.get(address).map(Vec::as_slice).unwrap_or(&[]),
                    );
                }
            }
        }
        profiles.extend(
            prepared
                .into_iter()
                .map(|p| p.map(|(_, metadata)| metadata)),
        );
    }
    profiles
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::collections::HashSet;

    fn owned_set(urns: &[&str]) -> HashSet<String> {
        urns.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn split_urn_strips_token_id_for_7segment_collections_v2() {
        let (urn, tok) = split_urn_and_token_id("urn:decentraland:matic:collections-v2:0xabc:0:42");
        assert_eq!(urn, "urn:decentraland:matic:collections-v2:0xabc:0");
        assert_eq!(tok, Some("42"));

        let (urn, tok) = split_urn_and_token_id("urn:decentraland:matic:collections-v2:0xabc:0");
        assert_eq!(urn, "urn:decentraland:matic:collections-v2:0xabc:0");
        assert_eq!(tok, None);

        let tp = "urn:decentraland:matic:collections-thirdparty:tp:coll:item";
        assert_eq!(split_urn_and_token_id(tp).1, None);
    }

    #[test]
    fn normalize_urn_maps_ethereum_to_mainnet() {
        assert_eq!(
            normalize_urn("urn:decentraland:ethereum:collections-v1:0xabc:hat"),
            "urn:decentraland:mainnet:collections-v1:0xabc:hat"
        );

        assert!(normalize_urn("a:mainnet:b").contains(":mainnet:"));
    }

    #[test]
    fn ownership_filter_keeps_base_keeps_owned_drops_unowned() {
        let owned = owned_set(&["urn:decentraland:matic:collections-v2:0xowned:0"]);
        let mut avatars = vec![json!({
            "avatar": {
                "wearables": [
                    "urn:decentraland:off-chain:base-avatars:eyebrows_00",
                    "urn:decentraland:matic:collections-v2:0xowned:0",
                    "urn:decentraland:matic:collections-v2:0xowned:0:99",
                    "urn:decentraland:matic:collections-v2:0xnope:0"
                ]
            }
        })];
        filter_avatars_by_ownership(&mut avatars, &owned);
        let ws: Vec<&str> = avatars[0]["avatar"]["wearables"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert_eq!(
            ws.len(),
            3,
            "unowned must be stripped, base+owned kept: {ws:?}"
        );
        assert!(
            ws.iter().all(|w| !w.contains("0xnope")),
            "unowned leaked: {ws:?}"
        );
        assert!(ws.contains(&"urn:decentraland:off-chain:base-avatars:eyebrows_00"));
    }

    #[test]
    fn ownership_filter_handles_emotes_and_ethereum_normalization() {
        let owned = owned_set(&["urn:decentraland:mainnet:collections-v1:0xc:dance"]);
        let mut avatars = vec![json!({
            "avatar": {
                "emotes": [
                    {"slot": 0, "urn": "handsair"},
                    {"slot": 1, "urn": "urn:decentraland:off-chain:base-emotes:wave"},
                    {"slot": 2, "urn": "urn:decentraland:ethereum:collections-v1:0xc:dance"},
                    {"slot": 3, "urn": "urn:decentraland:matic:collections-v2:0xx:1"}
                ]
            }
        })];
        filter_avatars_by_ownership(&mut avatars, &owned);
        let es = avatars[0]["avatar"]["emotes"].as_array().unwrap();
        let urns: Vec<&str> = es.iter().map(|e| e["urn"].as_str().unwrap()).collect();
        assert_eq!(es.len(), 3, "unowned emote must be stripped: {urns:?}");
        assert!(
            urns.iter().all(|u| !u.contains("0xx")),
            "unowned emote leaked: {urns:?}"
        );
        assert!(
            urns.contains(&"handsair")
                && urns.contains(&"urn:decentraland:off-chain:base-emotes:wave")
        );
    }

    #[test]
    fn prefix_bounds_bracket_each_item_urn_bytewise() {
        let item = "urn:decentraland:matic:collections-v2:0xabc:0";
        let (lo, hi) = prefix_bounds(&[item.to_string()]);
        assert_eq!(lo, vec![format!("{item}:")]);
        assert_eq!(hi, vec![format!("{item};")]);
        assert_eq!(b';', b':' + 1);
        let inside = |s: &str| s.as_bytes() >= lo[0].as_bytes() && s.as_bytes() < hi[0].as_bytes();
        assert!(inside(&format!("{item}:42")));
        assert!(inside(&format!("{item}:")));
        assert!(
            !inside(item),
            "the item urn itself is an exact match, not a prefix one"
        );
        assert!(!inside("urn:decentraland:matic:collections-v2:0xabc:01:7"));
        assert!(!inside("urn:decentraland:matic:collections-v2:0xabc:10:7"));
    }

    #[test]
    fn ownership_sql_pairs_each_address_with_its_own_urns() {
        for overlay in [false, true] {
            let sql = ownership_sql(overlay);
            assert!(
                sql.contains(
                    "FROM unnest($1::text[], $2::text[], $3::text[], $4::text[]) AS u(address, urn, lo, hi)"
                ),
                "{sql}"
            );
            assert!(sql.contains("CROSS JOIN LATERAL ("), "{sql}");
            assert!(
                sql.contains("n.owner_address = u.address AND n.urn = u.urn"),
                "exact leg scoped to the pair's address: {sql}"
            );
            assert!(sql.contains("n.urn >= u.lo AND n.urn < u.hi"), "{sql}");
            assert!(sql.contains("LIMIT 1"), "{sql}");
            assert!(!sql.contains("EXISTS"), "{sql}");
            assert!(!sql.contains("ANY("), "no cross-address ANY() match: {sql}");
            assert!(sql.contains("left(n.urn, length(u.lo)) = u.lo"), "{sql}");
            assert_eq!(sql.contains("marketplace.usage_grants"), overlay);
            assert_eq!(sql.contains("ug.urn >= u.lo AND ug.urn < u.hi"), overlay);
            assert_eq!(
                sql.contains("ug.grantee_address = u.address AND ug.urn = u.urn"),
                overlay
            );
        }
    }

    #[tokio::test]
    async fn process_profiles_is_positional_and_pure_without_a_pool() {
        let entities = vec![
            json!({
                "id": "Qm1",
                "pointers": ["0xAAAA"],
                "timestamp": 5,
                "metadata": { "avatars": [{ "name": "one", "avatar": { "wearables": [] } }] }
            }),
            json!({ "id": "Qm2", "pointers": ["0xbbbb"], "timestamp": 6 }),
            json!({
                "id": "Qm3",
                "pointers": ["0xcccc"],
                "timestamp": 7,
                "metadata": { "avatars": [{ "name": "three" }] }
            }),
        ];
        let out = process_profiles_positional(&entities, None, "https://cdn.example").await;
        assert_eq!(out.len(), 3);
        assert!(out[1].is_none(), "no metadata, no profile");
        assert_eq!(out[0].as_ref().unwrap()["avatars"][0]["userId"], "0xaaaa");
        assert_eq!(out[0].as_ref().unwrap()["timestamp"], 5);
        assert_eq!(
            out[2].as_ref().unwrap()["avatars"][0]["ethAddress"],
            "0xcccc"
        );
        let single = process_profile(&entities[0], None, "https://cdn.example").await;
        assert_eq!(single, out[0]);
        let flat = process_profiles(&entities, None, "https://cdn.example").await;
        assert_eq!(flat, out.iter().flatten().cloned().collect::<Vec<_>>());
    }

    const ALICE: &str = "0xaaaa000000000000000000000000000000000001";
    const BOB: &str = "0xbbbb000000000000000000000000000000000002";
    const CARL: &str = "0xcccc000000000000000000000000000000000003";
    const ITEM1: &str = "urn:decentraland:matic:collections-v2:0xc0ffee:1";
    const ITEM2: &str = "urn:decentraland:matic:collections-v2:0xc0ffee:2";
    const ITEM3: &str = "urn:decentraland:matic:collections-v2:0xc0ffee:3";

    const SQUID_DDL: &str = "
        CREATE TABLE squid_marketplace.nft (
            id TEXT PRIMARY KEY, owner_address TEXT NOT NULL, urn TEXT, category TEXT, name TEXT);
        INSERT INTO squid_marketplace.nft VALUES
            ('w-1', '0xaaaa000000000000000000000000000000000001', 'urn:decentraland:matic:collections-v2:0xc0ffee:1', 'wearable', NULL),
            ('w-2', '0xaaaa000000000000000000000000000000000001', 'urn:decentraland:matic:collections-v2:0xc0ffee:2:77', 'wearable', NULL),
            ('w-3', '0xaaaa000000000000000000000000000000000001', 'urn:decentraland:matic:collections-v2:0xc0ffee:30:1', 'wearable', NULL),
            ('w-4', '0xbbbb000000000000000000000000000000000002', 'urn:decentraland:matic:collections-v2:0xc0ffee:1:5', 'wearable', NULL),
            ('ens-1', '0xaaaa000000000000000000000000000000000001', NULL, 'ens', 'alice'),
            ('ens-2', '0xaaaa000000000000000000000000000000000001', NULL, 'ens', 'aaa'),
            ('ens-3', '0xbbbb000000000000000000000000000000000002', NULL, 'ens', 'bob');
    ";

    fn pair(addr: &str, urn: &str) -> (String, String) {
        (addr.to_string(), urn.to_string())
    }

    #[tokio::test]
    async fn ownership_and_names_batches_match_the_single_lookups() {
        let Some(db) = super::pg_scratch::ScratchSquid::new(SQUID_DDL).await else {
            return;
        };
        let pool = db.pool.clone();

        let pairs = [
            pair(ALICE, ITEM1),
            pair(ALICE, ITEM2),
            pair(ALICE, ITEM3),
            pair(BOB, ITEM1),
            pair(BOB, ITEM2),
            pair(CARL, ITEM1),
        ];
        let owned = resolve_ownership_batch(&pool, &pairs.iter().cloned().collect()).await;
        let expect: std::collections::HashMap<String, HashSet<String>> = [
            (ALICE.to_string(), owned_set(&[ITEM1, ITEM2])),
            (BOB.to_string(), owned_set(&[ITEM1])),
        ]
        .into_iter()
        .collect();
        assert_eq!(
            owned, expect,
            "exact, token-prefix, and per-address scoping"
        );
        assert!(resolve_ownership_batch(&pool, &HashSet::new())
            .await
            .is_empty());

        let names = fetch_batch_ens_names(&pool, &[ALICE.into(), BOB.into(), CARL.into()]).await;
        for addr in [ALICE, BOB, CARL] {
            let single = fetch_owned_ens_names(&pool, addr).await;
            assert_eq!(
                names.get(addr).cloned().unwrap_or_default(),
                single,
                "{addr}"
            );
        }
        assert_eq!(
            names[ALICE],
            vec!["alice", "aaa"],
            "id order like the single lookup"
        );

        let entities = vec![
            json!({
                "id": "Qa", "pointers": [ALICE], "timestamp": 1,
                "metadata": { "avatars": [{ "name": "aaa", "avatar": {
                    "wearables": [ITEM1, format!("{ITEM2}:77"), ITEM3] } }] }
            }),
            json!({
                "id": "Qb", "pointers": [BOB], "timestamp": 2,
                "metadata": { "avatars": [{ "name": "alice", "avatar": {
                    "wearables": [ITEM1, ITEM2] } }] }
            }),
            json!({
                "id": "Qc", "pointers": [CARL], "timestamp": 3,
                "metadata": { "avatars": [{ "name": "carl", "avatar": {
                    "wearables": [ITEM1] } }] }
            }),
        ];
        let out = process_profiles_positional(&entities, Some(&pool), "https://cdn.example").await;
        let wearables = |i: usize| -> Vec<String> {
            out[i].as_ref().unwrap()["avatars"][0]["avatar"]["wearables"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap().to_string())
                .collect()
        };
        assert_eq!(wearables(0), vec![ITEM1.to_string(), format!("{ITEM2}:77")]);
        assert_eq!(wearables(1), vec![ITEM1.to_string()]);
        assert!(wearables(2).is_empty(), "carl holds nothing");
        assert_eq!(
            out[0].as_ref().unwrap()["avatars"][0]["hasClaimedName"],
            true
        );
        assert_eq!(
            out[1].as_ref().unwrap()["avatars"][0]["hasClaimedName"],
            false,
            "bob does not hold the name alice"
        );
        assert_eq!(
            out[2].as_ref().unwrap()["avatars"][0]["hasClaimedName"],
            false
        );
        for (i, entity) in entities.iter().enumerate() {
            let single = process_profile(entity, Some(&pool), "https://cdn.example").await;
            assert_eq!(single, out[i], "batch and single agree for entity {i}");
        }

        drop(pool);
        db.drop().await;
    }

    #[test]
    fn test_is_base_wearable() {
        assert!(is_base_wearable(
            "urn:decentraland:off-chain:base-avatars:green_hoodie"
        ));
        assert!(!is_base_wearable(
            "urn:decentraland:matic:collections-v2:0xabc:0"
        ));
    }

    #[test]
    fn test_is_base_emote() {
        assert!(is_base_emote("urn:decentraland:off-chain:base-emotes:wave"));
        assert!(is_base_emote(
            "urn:decentraland:off-chain:base-scene-emotes:wave"
        ));
        assert!(!is_base_emote(
            "urn:decentraland:off-chain:base-scene-emotes:"
        ));
        assert!(!is_base_emote(
            "urn:decentraland:off-chain:base-emotes-fake:wave"
        ));
        assert!(!is_base_emote(
            "fake:urn:decentraland:off-chain:base-emotes:wave"
        ));
        assert!(!is_base_emote(
            "urn:decentraland:matic:collections-v2:0xabc:0"
        ));
    }

    #[test]
    fn test_split_urn_and_token_id() {
        let (urn, token) =
            split_urn_and_token_id("urn:decentraland:matic:collections-v2:0xabc:0:12345");
        assert_eq!(urn, "urn:decentraland:matic:collections-v2:0xabc:0");
        assert_eq!(token, Some("12345"));

        let (urn, token) = split_urn_and_token_id("urn:decentraland:matic:collections-v2:0xabc:0");
        assert_eq!(urn, "urn:decentraland:matic:collections-v2:0xabc:0");
        assert_eq!(token, None);

        let (urn, token) = split_urn_and_token_id(
            "urn:decentraland:matic:collections-thirdparty:provider:collection:item",
        );
        assert_eq!(
            urn,
            "urn:decentraland:matic:collections-thirdparty:provider:collection:item"
        );
        assert_eq!(token, None);
    }

    #[test]
    fn test_rewrite_snapshot_urls() {
        let mut metadata = json!({
            "avatars": [{
                "avatar": {
                    "snapshots": {
                        "face256": "bafybeifoo",
                        "body": "bafybeibar"
                    }
                }
            }]
        });

        rewrite_snapshot_urls("entity123", &mut metadata, "https://cdn.example.com");

        let snapshots = &metadata["avatars"][0]["avatar"]["snapshots"];
        assert_eq!(
            snapshots["face256"],
            "https://cdn.example.com/entities/entity123/face.png"
        );
        assert_eq!(
            snapshots["body"],
            "https://cdn.example.com/entities/entity123/body.png"
        );
    }

    #[test]
    fn test_rewrite_snapshot_urls_trailing_slash() {
        let mut metadata = json!({
            "avatars": [{
                "avatar": {
                    "snapshots": {
                        "face256": "bafybeifoo",
                        "body": "bafybeibar"
                    }
                }
            }]
        });

        rewrite_snapshot_urls("e1", &mut metadata, "https://cdn.example.com/");

        let snapshots = &metadata["avatars"][0]["avatar"]["snapshots"];
        assert_eq!(
            snapshots["face256"],
            "https://cdn.example.com/entities/e1/face.png"
        );
        assert_eq!(
            snapshots["body"],
            "https://cdn.example.com/entities/e1/body.png"
        );
    }

    #[test]
    fn test_sanitize_links_valid() {
        let mut metadata = json!({
            "avatars": [{
                "links": [
                    {"title": "Twitter", "url": "https://twitter.com/user"},
                    {"title": "Bad", "url": "not-a-url"}
                ]
            }]
        });

        sanitize_links(&mut metadata);

        let links = metadata["avatars"][0]["links"].as_array().unwrap();
        assert_eq!(links.len(), 1);
        assert_eq!(links[0]["url"], "https://twitter.com/user");
    }

    #[test]
    fn test_sanitize_links_url_decode() {
        let mut metadata = json!({
            "avatars": [{
                "links": [
                    {"title": "Site", "url": "https%3A%2F%2Fexample.com%2Fpath"}
                ]
            }]
        });

        sanitize_links(&mut metadata);

        let links = metadata["avatars"][0]["links"].as_array().unwrap();
        assert_eq!(links.len(), 1);
        assert_eq!(links[0]["url"], "https://example.com/path");
    }

    #[test]
    fn test_sanitize_links_keeps_public_strips_unsafe_hosts() {
        let mut metadata = json!({
            "avatars": [{
                "links": [
                    {"title": "Public", "url": "https://twitter.com/user"},
                    {"title": "Js", "url": "javascript:alert(1)"},
                    {"title": "Localhost", "url": "http://localhost"},
                    {"title": "Private", "url": "http://10.0.0.1"},
                    {"title": "Metadata", "url": "http://169.254.169.254"},
                    {"title": "Internal", "url": "http://svc.internal"},
                    {"title": "Obfuscated", "url": "http://2130706433"}
                ]
            }]
        });

        sanitize_links(&mut metadata);

        let links = metadata["avatars"][0]["links"].as_array().unwrap();
        assert_eq!(
            links.len(),
            1,
            "only the public https link survives: {links:?}"
        );
        assert_eq!(links[0]["url"], "https://twitter.com/user");
    }

    #[test]
    fn test_sanitize_links_empty() {
        let mut metadata = json!({
            "avatars": [{
                "links": []
            }]
        });

        sanitize_links(&mut metadata);

        let links = metadata["avatars"][0]["links"].as_array().unwrap();
        assert_eq!(links.len(), 0);
    }

    #[test]
    fn test_sanitize_links_missing() {
        let mut metadata = json!({
            "avatars": [{}]
        });

        sanitize_links(&mut metadata);
    }

    #[test]
    fn test_ensure_profile_shape() {
        let entity = json!({
            "timestamp": 1234567890
        });
        let mut metadata = json!({});

        ensure_profile_shape(&entity, &mut metadata);

        assert_eq!(metadata["timestamp"], 1234567890);
        assert!(metadata["avatars"].as_array().unwrap().is_empty());
    }

    #[test]
    fn test_normalize_urn() {
        assert_eq!(
            normalize_urn("urn:decentraland:ethereum:collections-v1:0xabc:hat"),
            "urn:decentraland:mainnet:collections-v1:0xabc:hat"
        );

        assert_eq!(
            normalize_urn("urn:decentraland:matic:collections-v2:0xabc:0"),
            "urn:decentraland:matic:collections-v2:0xabc:0"
        );

        assert_eq!(normalize_urn(":ethereum::ethereum:"), ":mainnet::ethereum:");
    }

    fn old_owns(stored: &std::collections::HashSet<String>, urn: &str) -> bool {
        if stored.contains(urn) {
            return true;
        }
        let prefix = format!("{urn}:");
        stored.iter().any(|s| s.starts_with(&prefix))
    }

    fn simulate_query_sets(
        stored: &std::collections::HashSet<String>,
        candidates: &[String],
    ) -> (
        std::collections::HashSet<String>,
        std::collections::HashSet<String>,
    ) {
        use std::collections::HashSet;

        let owned_exact: HashSet<String> = candidates
            .iter()
            .filter(|u| stored.contains(*u))
            .cloned()
            .collect();

        let owned_prefixes: HashSet<String> = candidates
            .iter()
            .filter(|u| !owned_exact.contains(*u))
            .filter(|u| {
                let prefix = format!("{u}:");
                stored.iter().any(|s| s.starts_with(&prefix))
            })
            .cloned()
            .collect();
        (owned_exact, owned_prefixes)
    }

    #[test]
    fn test_resolve_owned_matches_old_logic() {
        use std::collections::HashSet;

        let stored: HashSet<String> = [
            "urn:decentraland:matic:collections-v2:0xexact:0",
            "urn:decentraland:matic:collections-v2:0xprefix:1:987654321",
            "urn:decentraland:mainnet:collections-v1:0xl1:hat",
            "urn:decentraland:matic:collections-v2:0xother:5",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();

        let candidates: Vec<String> = [
            "urn:decentraland:matic:collections-v2:0xexact:0",
            "urn:decentraland:matic:collections-v2:0xprefix:1",
            "urn:decentraland:mainnet:collections-v1:0xl1:hat",
            "urn:decentraland:matic:collections-v2:0xnope:9",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();

        let (owned_exact, owned_prefixes) = simulate_query_sets(&stored, &candidates);
        let batched = resolve_owned(&candidates, &owned_exact, &owned_prefixes);

        let expected: HashSet<String> = candidates
            .iter()
            .filter(|u| old_owns(&stored, u))
            .cloned()
            .collect();

        assert_eq!(batched, expected);
        assert!(batched.contains("urn:decentraland:matic:collections-v2:0xexact:0"));
        assert!(batched.contains("urn:decentraland:matic:collections-v2:0xprefix:1"));
        assert!(batched.contains("urn:decentraland:mainnet:collections-v1:0xl1:hat"));
        assert!(!batched.contains("urn:decentraland:matic:collections-v2:0xnope:9"));
    }

    #[test]
    fn test_resolve_owned_exact_takes_priority_over_prefix() {
        use std::collections::HashSet;

        let stored: HashSet<String> = [
            "urn:decentraland:matic:collections-v2:0xboth:0",
            "urn:decentraland:matic:collections-v2:0xboth:0:42",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let candidates = vec!["urn:decentraland:matic:collections-v2:0xboth:0".to_string()];

        let (owned_exact, owned_prefixes) = simulate_query_sets(&stored, &candidates);
        assert!(owned_exact.contains(&candidates[0]));
        assert!(!owned_prefixes.contains(&candidates[0]));

        let batched = resolve_owned(&candidates, &owned_exact, &owned_prefixes);
        let expected: HashSet<String> = candidates
            .iter()
            .filter(|u| old_owns(&stored, u))
            .cloned()
            .collect();
        assert_eq!(batched, expected);
        assert_eq!(batched.len(), 1);
    }

    #[test]
    fn test_resolve_owned_empty_and_none_owned() {
        use std::collections::HashSet;
        let stored: HashSet<String> = HashSet::new();
        let candidates = vec![
            "urn:decentraland:matic:collections-v2:0xa:0".to_string(),
            "urn:decentraland:matic:collections-v2:0xb:1".to_string(),
        ];
        let (e, p) = simulate_query_sets(&stored, &candidates);
        let batched = resolve_owned(&candidates, &e, &p);
        assert!(batched.is_empty());

        let empty = resolve_owned(&[], &HashSet::new(), &HashSet::new());
        assert!(empty.is_empty());
    }

    #[test]
    fn test_resolve_owned_prefix_must_be_colon_delimited() {
        use std::collections::HashSet;

        let stored: HashSet<String> = ["urn:decentraland:matic:collections-v2:0xabcdef:0"]
            .iter()
            .map(|s| s.to_string())
            .collect();

        let candidates = vec!["urn:decentraland:matic:collections-v2:0xabc".to_string()];

        let (e, p) = simulate_query_sets(&stored, &candidates);
        let batched = resolve_owned(&candidates, &e, &p);
        let expected: HashSet<String> = candidates
            .iter()
            .filter(|u| old_owns(&stored, u))
            .cloned()
            .collect();
        assert_eq!(batched, expected);
        assert!(batched.is_empty());
    }

    #[test]
    fn apply_claimed_name_overrides_stale_true_when_name_not_owned() {
        let mut metadata = json!({
            "avatars": [{"name": "Genius", "hasClaimedName": true}]
        });

        apply_claimed_name(&mut metadata, &["OtherName".to_string()]);

        assert_eq!(metadata["avatars"][0]["hasClaimedName"], false);
    }

    #[test]
    fn apply_claimed_name_sets_true_when_missing_and_owned() {
        let mut metadata = json!({
            "avatars": [{"name": "iMoo"}]
        });

        apply_claimed_name(&mut metadata, &["iMoo".to_string()]);

        assert_eq!(metadata["avatars"][0]["hasClaimedName"], true);
    }

    #[test]
    fn apply_claimed_name_is_case_sensitive() {
        let mut metadata = json!({
            "avatars": [{"name": "imoo", "hasClaimedName": true}]
        });

        apply_claimed_name(&mut metadata, &["iMoo".to_string()]);

        assert_eq!(metadata["avatars"][0]["hasClaimedName"], false);
    }

    #[test]
    fn apply_claimed_name_handles_missing_name_and_empty_avatars() {
        let mut metadata = json!({
            "avatars": [{"avatar": {}}]
        });
        apply_claimed_name(&mut metadata, &["iMoo".to_string()]);
        assert_eq!(metadata["avatars"][0]["hasClaimedName"], false);

        let mut no_avatars = json!({});
        apply_claimed_name(&mut no_avatars, &["iMoo".to_string()]);
        assert!(no_avatars.get("avatars").is_none());
    }

    #[test]
    fn test_ensure_profile_shape_preserves_existing() {
        let entity = json!({
            "timestamp": 1234567890
        });
        let mut metadata = json!({
            "timestamp": 9999,
            "avatars": [{"name": "test"}]
        });

        ensure_profile_shape(&entity, &mut metadata);

        assert_eq!(metadata["timestamp"], 9999);
        assert_eq!(metadata["avatars"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn test_apply_pointer_identity_overwrites_and_backfills() {
        let pointer = "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let mut metadata = json!({
            "avatars": [
                {
                    "name": "spoofed",
                    "userId": "0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                    "ethAddress": "0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
                },
                { "name": "bare" }
            ]
        });

        apply_pointer_identity(&mut metadata, pointer);

        for avatar in metadata["avatars"].as_array().unwrap() {
            assert_eq!(avatar["userId"], pointer);
            assert_eq!(avatar["ethAddress"], pointer);
        }
    }

    #[test]
    fn test_apply_pointer_identity_tolerates_malformed_shapes() {
        let pointer = "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let mut no_avatars = json!({});
        apply_pointer_identity(&mut no_avatars, pointer);
        assert!(no_avatars.get("avatars").is_none());

        let mut mixed = json!({ "avatars": ["not-an-object", { "name": "ok" }] });
        apply_pointer_identity(&mut mixed, pointer);
        assert_eq!(mixed["avatars"][0], "not-an-object");
        assert_eq!(mixed["avatars"][1]["userId"], pointer);
    }
}

#[cfg(test)]
#[path = "profile_batch_tests.rs"]
mod batch_tests;

/// A throwaway database with empty `squid_marketplace` and `marketplace.usage_grants` for PG-gated tests.
#[cfg(test)]
pub(crate) mod pg_scratch {
    use sqlx::postgres::PgPoolOptions;
    use sqlx::PgPool;

    pub(crate) struct ScratchSquid {
        admin: PgPool,
        pub(crate) pool: PgPool,
        database: String,
    }

    impl ScratchSquid {
        pub(crate) async fn new(ddl: &'static str) -> Option<Self> {
            let url = catalyrst_testgate::require_pg("CATALYRST_SERVER_TEST_PG")?;
            let admin = PgPoolOptions::new()
                .max_connections(1)
                .connect(&url)
                .await
                .unwrap();
            let database = format!("squid_scratch_{}", uuid::Uuid::new_v4().simple());
            sqlx::query(sqlx::AssertSqlSafe(format!("CREATE DATABASE {database}")))
                .execute(&admin)
                .await
                .unwrap();
            let (base, _) = url.rsplit_once('/').unwrap();
            let pool = PgPoolOptions::new()
                .max_connections(4)
                .connect(&format!("{base}/{database}"))
                .await
                .unwrap();
            sqlx::raw_sql(
                "CREATE SCHEMA squid_marketplace; CREATE SCHEMA marketplace;
                 CREATE TABLE marketplace.usage_grants (
                     id BIGSERIAL PRIMARY KEY, grantee_address TEXT NOT NULL, urn TEXT NOT NULL,
                     token_id TEXT, category TEXT NOT NULL, escrow_ref TEXT,
                     granted_at TIMESTAMPTZ NOT NULL DEFAULT now(), unlock_at TIMESTAMPTZ NOT NULL,
                     status TEXT NOT NULL DEFAULT 'active')",
            )
            .execute(&pool)
            .await
            .unwrap();
            sqlx::raw_sql(ddl).execute(&pool).await.unwrap();
            Some(Self {
                admin,
                pool,
                database,
            })
        }

        pub(crate) async fn drop(self) {
            self.pool.close().await;
            sqlx::query(sqlx::AssertSqlSafe(format!(
                "DROP DATABASE {}",
                self.database
            )))
            .execute(&self.admin)
            .await
            .unwrap();
        }
    }
}
