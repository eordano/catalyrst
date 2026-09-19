use super::*;

pub async fn own(s: &Store, p: &Prepared) -> ApiResult<Value> {
    let response = s
        .client
        .get(&p.url)
        .send()
        .await
        .map_err(|_| unavailable())?;
    if response.status() == StatusCode::NOT_FOUND {
        return Ok(json!({"location":null}));
    }
    if !response.status().is_success() {
        return Err(unavailable());
    }
    let data = bounded_json(response).await?;
    Ok(json!({"location":fresh_location(&data, &p.wallet, now())}))
}
fn fresh_location(data: &Value, wallet: &str, at: i64) -> Option<Value> {
    peer_location(&data["peer"], wallet, at)
}
fn peer_location(peer: &Value, wallet: &str, at: i64) -> Option<Value> {
    let address = peer["id"].as_str().or_else(|| peer["address"].as_str())?;
    if !address.eq_ignore_ascii_case(wallet) {
        return None;
    }
    let updated = peer["lastPing"].as_i64()?;
    if !(0..30_000).contains(&(at - updated)) {
        return None;
    }
    let x = peer["parcel"][0].as_i64()?;
    let y = peer["parcel"][1].as_i64()?;
    if !(-150..=150).contains(&x) || !(-150..=150).contains(&y) {
        return None;
    }
    Some(json!({"x":x,"y":y,"updatedAt":updated}))
}

#[derive(Deserialize)]
pub struct LocationsQuery {
    addresses: String,
}

/// Public Archipelago presence, limited to the wallets requested by the client.
/// One shared snapshot avoids fetching the complete peer list for every friend.
pub async fn friends(
    State(s): State<Arc<Store>>,
    Query(query): Query<LocationsQuery>,
) -> ApiResult<Json<Value>> {
    let addresses: Vec<String> = query
        .addresses
        .split(',')
        .map(str::to_ascii_lowercase)
        .collect();
    if addresses.len() > 100
        || addresses.iter().any(|a| {
            a.len() != 42 || !a.starts_with("0x") || !a[2..].bytes().all(|b| b.is_ascii_hexdigit())
        })
    {
        return Err(bad("Provide up to 100 wallet addresses"));
    }
    let mut snapshot = s.locations.lock().await;
    if now() - snapshot.0 >= 10_000 {
        // Mark failed refreshes too, so an upstream outage cannot cause a request stampede.
        snapshot.0 = now();
        snapshot.1 = Value::Null;
        let response = s
            .client
            .get("https://archipelago-ea-stats.decentraland.org/peers")
            .send()
            .await
            .map_err(|_| unavailable())?;
        if !response.status().is_success() {
            return Err(unavailable());
        }
        snapshot.1 = bounded_json(response).await?;
    }
    Ok(Json(
        json!({"locations":select_locations(&snapshot.1, &addresses, now())}),
    ))
}

fn select_locations(data: &Value, addresses: &[String], at: i64) -> serde_json::Map<String, Value> {
    let mut result = serde_json::Map::new();
    if let Some(peers) = data["peers"].as_array() {
        for peer in peers {
            let Some(address) = peer["address"].as_str().or_else(|| peer["id"].as_str()) else {
                continue;
            };
            let address = address.to_ascii_lowercase();
            if addresses.contains(&address) {
                if let Some(location) = peer_location(peer, &address, at) {
                    result.insert(address, location);
                }
            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn snapshot_returns_only_requested_fresh_peers() {
        let data = json!({"peers":[
            {"id":"0xABC","lastPing":100_000,"parcel":[-2,45]},
            {"id":"0xDEF","lastPing":100_000,"parcel":[1,2]},
            {"id":"0xOLD","lastPing":60_000,"parcel":[3,4]}
        ]});
        let selected = select_locations(&data, &["0xabc".into(), "0xold".into()], 110_000);
        assert_eq!(selected.len(), 1);
        assert_eq!(selected["0xabc"]["x"], -2);
        assert!(select_locations(&data, &["0xabc".into()], 130_000).is_empty());
    }
    #[test]
    fn only_returns_own_fresh_valid_parcel() {
        let data = json!({"peer":{"id":"0xABC","lastPing":100_000,"parcel":[-2,45]}});
        assert_eq!(
            fresh_location(&data, "0xabc", 110_000),
            Some(json!({"x":-2,"y":45,"updatedAt":100_000}))
        );
        assert!(fresh_location(&data, "0xother", 110_000).is_none());
        assert!(fresh_location(&data, "0xabc", 130_000).is_none());
        assert!(fresh_location(&data, "0xabc", 99_000).is_none());
        assert!(fresh_location(
            &json!({"peer":{"id":"0xabc","lastPing":100_000,"parcel":[999,0]}}),
            "0xabc",
            110_000
        )
        .is_none());
        assert!(fresh_location(&json!({}), "0xabc", 110_000).is_none());
    }
}
