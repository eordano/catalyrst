use alloy::primitives::{keccak256, Address, B256, U256};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use super::{drafts::ItemDraft, items::ItemsComponent};
use crate::http::errors::ApiError;

pub const COLLECTION_MANAGER: &str = "0x9d32aac179153a991e832550d9f96441ea27763a";
pub const COLLECTION_FACTORY: &str = "0x3195e88ae10704b359764cb38e429d24f1c2f781";
pub const COLLECTION_FORWARDER: &str = "0xbf6755a83c0dcdbb2933a96ea778e00b717d7004";
pub const COLLECTION_BASE_URI: &str =
    "https://peer.decentraland.org/lambdas/collections/standard/erc721/";

#[derive(Clone, Debug, Deserialize, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "builder/"))]
pub struct PublicationItem {
    pub id: String,
    pub rarity: String,
    pub price: String,
    pub beneficiary: String,
    pub metadata: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "builder/"))]
pub struct PublicationPreparation {
    pub id: String,
    pub revision: String,
    pub chain_id: u32,
    pub manager: String,
    pub factory: String,
    pub forwarder: String,
    pub salt: String,
    pub name: String,
    pub symbol: String,
    pub base_uri: String,
    pub creator: String,
    pub items: Vec<PublicationItem>,
}

impl ItemsComponent {
    pub async fn prepare_publication(
        &self,
        signer: &str,
        id: Uuid,
    ) -> Result<PublicationPreparation, ApiError> {
        if let Some(state) = self.publication_state(signer, id).await? {
            if state.status != "reverted" {
                return Ok(state.preparation);
            }
        }
        let snapshot: Value = sqlx::query_scalar(SNAPSHOT_QUERY)
            .bind(id)
            .bind(signer)
            .fetch_optional(&self.pool)
            .await?
            .ok_or_else(|| ApiError::not_found("Collection not found"))?;
        prepare(snapshot, signer)
    }
}

pub(super) fn prepare(snapshot: Value, signer: &str) -> Result<PublicationPreparation, ApiError> {
    let collection = &snapshot["collection"];
    let id = text(collection, "id")?;
    let name = text(collection, "name")?;
    if collection["is_published"] == true || !collection["contract_address"].is_null() {
        return Err(ApiError::bad_request(
            "Collection has already been published",
        ));
    }
    if !collection["third_party_id"].is_null() {
        return Err(ApiError::bad_request(
            "Linked collections use their registered provider's publication flow",
        ));
    }
    if name.trim().is_empty() || name.encode_utf16().count() > 42 {
        return Err(ApiError::bad_request(
            "Collection name must contain 1 to 42 characters",
        ));
    }
    let creator: Address = signer
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid creator address"))?;
    if !text(collection, "eth_address")?.eq_ignore_ascii_case(signer) {
        return Err(ApiError::forbidden("Collection belongs to another wallet"));
    }
    let salt = match collection["salt"].as_str().filter(|s| !s.is_empty()) {
        Some(salt) => salt
            .parse::<B256>()
            .map_err(|_| ApiError::bad_request("Collection salt must be 32 bytes"))?,
        None => keccak256(id.as_bytes()),
    };
    let rows = snapshot["items"]
        .as_array()
        .ok_or_else(|| ApiError::bad_request("Collection items are missing"))?;
    if rows.is_empty() || rows.len() > 50 {
        return Err(ApiError::bad_request(
            "Publish a collection containing 1 to 50 items",
        ));
    }
    let items = rows
        .iter()
        .map(|row| prepare_item(row, signer))
        .collect::<Result<_, _>>()?;
    Ok(PublicationPreparation {
        id: id.into(),
        revision: catalyrst_hashing::hash_bytes_v1(snapshot.to_string().as_bytes()),
        chain_id: 137,
        manager: COLLECTION_MANAGER.into(),
        factory: COLLECTION_FACTORY.into(),
        forwarder: COLLECTION_FORWARDER.into(),
        salt: format!("{salt:#x}"),
        name: name.into(),
        symbol: format!(
            "DCL-{}",
            name.chars()
                .filter(|c| !c.is_whitespace() && !"aeiou".contains(*c))
                .collect::<String>()
                .to_uppercase()
        ),
        base_uri: COLLECTION_BASE_URI.into(),
        creator: format!("{creator:#x}"),
        items,
    })
}

fn text<'a>(value: &'a Value, field: &str) -> Result<&'a str, ApiError> {
    value[field]
        .as_str()
        .ok_or_else(|| ApiError::bad_request(format!("Missing {field}")))
}

fn prepare_item(row: &Value, signer: &str) -> Result<PublicationItem, ApiError> {
    let item: ItemDraft = serde_json::from_value(row.clone())
        .map_err(|_| ApiError::bad_request("Invalid collection item"))?;
    item.validate(item.id, signer)?;
    let invalid = |reason: &str| ApiError::bad_request(format!("{}: {reason}", item.name));
    if row["missing_content"] == true {
        return Err(invalid("upload missing content before publishing"));
    }
    if item.name.encode_utf16().count() > 32 || item.description.encode_utf16().count() > 64 {
        return Err(invalid("publication requires a name of at most 32 characters and a description of at most 64 characters"));
    }
    if item.name.contains(':') || item.description.contains(':') {
        return Err(invalid("name and description cannot contain colons"));
    }
    let rarity = item
        .rarity
        .as_ref()
        .ok_or_else(|| invalid("choose a rarity"))?;
    let price = item
        .price
        .as_ref()
        .ok_or_else(|| invalid("set an item price"))?;
    let price = U256::from_str_radix(price, 10).map_err(|_| invalid("price exceeds uint256"))?;
    let beneficiary: Address = (if price == U256::ZERO {
        "0x0000000000000000000000000000000000000000"
    } else {
        item.beneficiary.as_deref().unwrap_or(signer)
    })
    .parse()
    .map_err(|_| invalid("invalid beneficiary"))?;
    if price != U256::ZERO && beneficiary == Address::ZERO {
        return Err(invalid("a priced item needs a beneficiary"));
    }
    let thumbnail = item
        .thumbnail
        .as_deref()
        .ok_or_else(|| invalid("add a thumbnail"))?;
    if !item.contents.contains_key(thumbnail) {
        return Err(invalid("upload the thumbnail before publishing"));
    }
    let category = item.data["category"].as_str().unwrap_or_default();
    let categories: &[&str] = if item.item_type == "emote" {
        &[
            "dance",
            "stunt",
            "greetings",
            "fun",
            "poses",
            "reactions",
            "horror",
            "miscellaneous",
        ]
    } else {
        &[
            "eyebrows",
            "eyes",
            "facial_hair",
            "hair",
            "body_shape",
            "mouth",
            "upper_body",
            "lower_body",
            "feet",
            "earring",
            "eyewear",
            "hat",
            "helmet",
            "mask",
            "tiara",
            "top_head",
            "skin",
            "hands_wear",
        ]
    };
    if !categories.contains(&category) {
        return Err(invalid("choose a valid category"));
    }
    let reps = item.data["representations"]
        .as_array()
        .filter(|r| !r.is_empty())
        .ok_or_else(|| invalid("add an avatar representation"))?;
    let mut shapes = Vec::new();
    for rep in reps {
        let files = rep["contents"]
            .as_array()
            .filter(|f| !f.is_empty())
            .ok_or_else(|| invalid("representation content is missing"))?;
        let main = rep["mainFile"]
            .as_str()
            .ok_or_else(|| invalid("representation main file is missing"))?;
        if !files.iter().any(|f| f.as_str() == Some(main))
            || files
                .iter()
                .any(|f| f.as_str().is_none_or(|f| !item.contents.contains_key(f)))
        {
            return Err(invalid("upload all representation files before publishing"));
        }
        let bodies = rep["bodyShapes"]
            .as_array()
            .filter(|b| !b.is_empty())
            .ok_or_else(|| invalid("choose avatar body shapes"))?;
        for body in bodies {
            let shape = match body.as_str() {
                Some("urn:decentraland:off-chain:base-avatars:BaseMale") => "BaseMale",
                Some("urn:decentraland:off-chain:base-avatars:BaseFemale") => "BaseFemale",
                _ => return Err(invalid("unsupported avatar body shape")),
            };
            if !shapes.contains(&shape) {
                shapes.push(shape);
            }
        }
    }
    let kind = if item.item_type == "emote" {
        "e"
    } else if item.contents.keys().any(|p| p.ends_with(".js")) {
        "sw"
    } else {
        "w"
    };
    let mut metadata = format!(
        "1:{kind}:{}:{}:{category}:{}",
        item.name,
        item.description,
        shapes.join(",")
    );
    if kind == "e" {
        let looped = item.data["loop"]
            .as_bool()
            .ok_or_else(|| invalid("choose an emote play mode"))?;
        metadata.push_str(if looped { ":1" } else { ":0" });
        let mut props = String::new();
        if item.contents.keys().any(|p| p.contains(".mp3")) {
            props.push('s');
        }
        if row["metrics"]["props"].as_u64().unwrap_or(0) > 0 {
            props.push('g');
        }
        if !props.is_empty() {
            metadata.push(':');
            metadata.push_str(&props);
        }
        if let Some(outcomes) = item.data["outcomes"].as_array().filter(|a| !a.is_empty()) {
            metadata.push_str(if outcomes.len() == 1 {
                ":so"
            } else if item.data["randomizeOutcomes"] == true {
                ":ro"
            } else {
                ":mo"
            });
        }
    }
    Ok(PublicationItem {
        id: item.id.to_string(),
        rarity: rarity.clone(),
        price: price.to_string(),
        beneficiary: format!("{beneficiary:#x}"),
        metadata,
    })
}

#[cfg(test)]
mod tests;

pub(super) const SNAPSHOT_QUERY: &str = "SELECT jsonb_build_object('collection', to_jsonb(c), 'items', \
             COALESCE((SELECT jsonb_agg(to_jsonb(i) || jsonb_build_object('contents', \
               COALESCE((SELECT jsonb_object_agg(ic.file, ic.hash) FROM item_contents ic \
                 WHERE ic.item_id=i.id), '{}'::jsonb), 'missing_content', \
               EXISTS (SELECT 1 FROM item_contents ic WHERE ic.item_id=i.id AND NOT EXISTS \
                 (SELECT 1 FROM uploaded_contents u WHERE u.hash=ic.hash))) ORDER BY i.created_at, i.id) \
               FROM items i WHERE i.collection_id=c.id), '[]'::jsonb)) \
             FROM collections c WHERE c.id=$1 AND lower(c.eth_address)=$2";
