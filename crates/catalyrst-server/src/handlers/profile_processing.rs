use serde_json::Value;
use sqlx::PgPool;

fn is_base_wearable(urn: &str) -> bool {
    urn.contains("base-avatars")
}

fn is_base_emote(urn: &str) -> bool {
    urn.contains("urn:decentraland:off-chain:base-emotes")
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

async fn resolve_ownership_batch(
    pool: &PgPool,
    address: &str,
    normalized_urns: &[String],
) -> std::collections::HashSet<String> {
    use std::collections::HashSet;

    let mut owned_exact: HashSet<String> = HashSet::new();
    let mut owned_prefixes: HashSet<String> = HashSet::new();

    if normalized_urns.is_empty() {
        return HashSet::new();
    }

    let unique: Vec<String> = {
        let mut seen = HashSet::new();
        normalized_urns
            .iter()
            .filter(|u| seen.insert((*u).clone()))
            .cloned()
            .collect()
    };

    let exact_rows: Vec<(String,)> = sqlx::query_as(
        "SELECT DISTINCT urn FROM squid_marketplace.nft \
         WHERE owner_address = lower($1) AND urn = ANY($2)",
    )
    .bind(address)
    .bind(&unique)
    .fetch_all(pool)
    .await
    .unwrap_or_default();
    for (urn,) in exact_rows {
        owned_exact.insert(urn);
    }

    let fallback: Vec<String> = unique
        .iter()
        .filter(|u| !owned_exact.contains(*u))
        .cloned()
        .collect();

    if !fallback.is_empty() {

        let prefixes: Vec<String> = fallback.iter().map(|u| format!("{u}:")).collect();

        let prefix_rows: Vec<(String,)> = sqlx::query_as(
            "SELECT DISTINCT p AS matched_prefix \
             FROM unnest($2::text[]) AS p \
             WHERE EXISTS ( \
                 SELECT 1 FROM squid_marketplace.nft n \
                 WHERE n.owner_address = lower($1) \
                   AND left(n.urn, length(p)) = p \
             )",
        )
        .bind(address)
        .bind(&prefixes)
        .fetch_all(pool)
        .await
        .unwrap_or_default();

        for (matched_prefix,) in prefix_rows {

            if let Some(urn) = matched_prefix.strip_suffix(':') {
                owned_prefixes.insert(urn.to_string());
            }
        }
    }

    resolve_owned(&unique, &owned_exact, &owned_prefixes)
}

pub async fn validate_ownership(
    squid_pool: Option<&PgPool>,
    eth_address: &str,
    metadata: &mut Value,
) {
    let pool = match squid_pool {
        Some(p) => p,
        None => return,
    };

    let avatars = match metadata.get_mut("avatars").and_then(|v| v.as_array_mut()) {
        Some(arr) => arr,
        None => return,
    };

    let mut to_check: Vec<String> = Vec::new();
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

    let owned = resolve_ownership_batch(pool, eth_address, &to_check).await;

    for avatar_val in avatars.iter_mut() {
        let avatar_obj = match avatar_val.get_mut("avatar") {
            Some(a) => a,
            None => continue,
        };

        if let Some(wearables) = avatar_obj.get("wearables").and_then(|w| w.as_array()).cloned() {
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
        regex::Regex::new(r"(?i)^(?:https?)://[^\s/$.?#].[^\s]*$")
            .expect("LinkUrl regex is valid")
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

            if re.is_match(url) {
                sanitized.push(link.clone());
                continue;
            }

            if let Ok(decoded) = urlencoding::decode(url) {
                if re.is_match(&decoded) {
                    let mut link_clone = link.clone();
                    if let Some(obj) = link_clone.as_object_mut() {
                        obj.insert(
                            "url".to_string(),
                            Value::String(decoded.into_owned()),
                        );
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
    let mut metadata = entity.get("metadata")?.clone();

    let eid = entity_id(entity).unwrap_or("");
    let eth_address = entity_eth_address(entity).unwrap_or_default();

    ensure_profile_shape(entity, &mut metadata);

    sanitize_links(&mut metadata);

    rewrite_snapshot_urls(eid, &mut metadata, cdn_base);

    if !eth_address.starts_with("default") {
        validate_ownership(squid_pool, &eth_address, &mut metadata).await;
    }

    Some(metadata)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

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

        let (urn, token) =
            split_urn_and_token_id("urn:decentraland:matic:collections-v2:0xabc:0");
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
        assert_eq!(snapshots["face256"], "https://cdn.example.com/entities/e1/face.png");
        assert_eq!(snapshots["body"], "https://cdn.example.com/entities/e1/body.png");
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

        assert_eq!(
            normalize_urn(":ethereum::ethereum:"),
            ":mainnet::ethereum:"
        );
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
}
