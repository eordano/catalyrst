use super::types::Trade;

pub(super) struct AssetMeta {
    pub(super) image: String,

    pub(super) seller: String,
    pub(super) category: String,
    pub(super) rarity: Option<String>,
    pub(super) name: Option<String>,
    pub(super) contract_address: String,
    pub(super) token_id: Option<String>,
    pub(super) item_id: Option<String>,
}

impl AssetMeta {
    fn link(&self) -> String {
        let base = std::env::var("MARKETPLACE_BASE_URL").unwrap_or_default();
        if let Some(token_id) = &self.token_id {
            format!(
                "{}/contracts/{}/tokens/{}",
                base, self.contract_address, token_id
            )
        } else {
            format!(
                "{}/contracts/{}/items/{}",
                base,
                self.contract_address,
                self.item_id.as_deref().unwrap_or_default()
            )
        }
    }

    fn token_or_item_id(&self) -> String {
        self.token_id
            .clone()
            .or_else(|| self.item_id.clone())
            .unwrap_or_default()
    }
}

fn insert_opt(map: &mut serde_json::Map<String, serde_json::Value>, key: &str, v: &Option<String>) {
    if let Some(val) = v {
        map.insert(key.into(), serde_json::json!(val));
    }
}

pub(super) fn bid_accepted_event(bid: &Trade, assets: &[&AssetMeta]) -> Option<serde_json::Value> {
    if assets.len() != 1 {
        return None;
    }
    let asset = assets[0];
    let price = bid
        .sent
        .first()
        .and_then(|a| a.amount.clone())
        .unwrap_or_default();
    let mut metadata = serde_json::Map::new();
    metadata.insert("address".into(), serde_json::json!(bid.signer));
    metadata.insert("image".into(), serde_json::json!(asset.image));
    metadata.insert("seller".into(), serde_json::json!(asset.seller));
    metadata.insert("category".into(), serde_json::json!(asset.category));
    insert_opt(&mut metadata, "rarity", &asset.rarity);
    metadata.insert("link".into(), serde_json::json!(asset.link()));
    insert_opt(&mut metadata, "nftName", &asset.name);
    metadata.insert("price".into(), serde_json::json!(price));
    metadata.insert("title".into(), serde_json::json!("Bid Accepted"));
    metadata.insert(
        "description".into(),
        serde_json::json!(format!(
            "Your bid for {} MANA for this {} was accepted.",
            crate::logic::numeric::format_ether(&price),
            asset.name.as_deref().unwrap_or_default()
        )),
    );
    metadata.insert("network".into(), serde_json::json!(bid.network));

    Some(serde_json::json!({
        "type": "blockchain",
        "subType": "bid-accepted",
        "key": format!("bid-accepted-{}", bid.id),

        "timestamp": 0,
        "metadata": serde_json::Value::Object(metadata),
    }))
}

pub(super) fn item_sold_event(
    trade: &Trade,
    assets: &[&AssetMeta],
    caller: &str,
) -> Option<serde_json::Value> {
    if assets.len() != 1 {
        return None;
    }
    let asset = assets[0];
    let mut metadata = serde_json::Map::new();
    metadata.insert("address".into(), serde_json::json!(trade.signer));
    metadata.insert("image".into(), serde_json::json!(asset.image));
    metadata.insert("seller".into(), serde_json::json!(asset.seller));
    metadata.insert("buyer".into(), serde_json::json!(caller));
    metadata.insert("category".into(), serde_json::json!(asset.category));
    insert_opt(&mut metadata, "rarity", &asset.rarity);
    metadata.insert("link".into(), serde_json::json!(asset.link()));
    insert_opt(&mut metadata, "nftName", &asset.name);
    metadata.insert("title".into(), serde_json::json!("Item Sold"));
    metadata.insert(
        "description".into(),
        serde_json::json!(format!(
            "Someone just bought your {}",
            asset.name.as_deref().unwrap_or_default()
        )),
    );
    metadata.insert("network".into(), serde_json::json!(trade.network));
    metadata.insert(
        "tokenId".into(),
        serde_json::json!(asset.token_or_item_id()),
    );

    Some(serde_json::json!({
        "type": "blockchain",
        "subType": "item-sold",
        "key": format!("item-sold-{}", trade.id),
        "timestamp": 0,
        "metadata": serde_json::Value::Object(metadata),
    }))
}
