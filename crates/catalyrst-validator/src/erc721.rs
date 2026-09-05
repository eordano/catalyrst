use serde::{Deserialize, Serialize};

use crate::types::Entity;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Erc721Trait {
    pub trait_type: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Erc721Metadata {
    pub id: String,
    pub name: Option<String>,
    pub description: String,
    pub language: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumbnail: Option<String>,
    pub attributes: Vec<Erc721Trait>,
}

fn rarity_emission(rarity: &str) -> Option<u64> {
    match rarity {
        "common" => Some(100_000),
        "uncommon" => Some(10_000),
        "rare" => Some(5_000),
        "epic" => Some(1_000),
        "legendary" => Some(100),
        "mythic" => Some(10),
        "unique" => Some(1),
        _ => None,
    }
}

pub fn format_erc721_entity(
    urn: &str,
    entity: &Entity,
    base_url: &str,
    emission: Option<&str>,
) -> Erc721Metadata {
    let metadata = entity.metadata.as_ref();

    let name = metadata
        .and_then(|m| m.get("i18n"))
        .and_then(|i18n| i18n.as_array())
        .and_then(|arr| {
            let english = arr.iter().find(|entry| {
                entry
                    .get("code")
                    .and_then(|c| c.as_str())
                    .map(|c| c.to_lowercase() == "en")
                    .unwrap_or(false)
            });
            let chosen = english.or_else(|| arr.first());
            chosen.and_then(|entry| entry.get("text").and_then(|t| t.as_str()))
        })
        .map(|s| s.to_string());

    let rarity = metadata
        .and_then(|m| m.get("rarity"))
        .and_then(|r| r.as_str())
        .unwrap_or("");

    let description = match emission {
        Some(em) => {
            let total = rarity_emission(rarity).unwrap_or(0);
            format!("DCL Wearable {em}/{total}")
        }
        None => String::new(),
    };

    let image_hash = find_content_hash(entity, "image");
    let thumbnail_hash = find_content_hash(entity, "thumbnail");

    let base = base_url.trim_end_matches('/');
    let image = image_hash.map(|h| format!("{base}/contents/{h}"));
    let thumbnail = thumbnail_hash.map(|h| format!("{base}/contents/{h}"));

    let mut attributes = Vec::new();
    attributes.push(Erc721Trait {
        trait_type: "Rarity".to_string(),
        value: rarity.to_string(),
    });

    let category = extract_item_category(metadata);
    if let Some(cat) = category {
        attributes.push(Erc721Trait {
            trait_type: "Category".to_string(),
            value: cat,
        });
    }

    let tags = extract_item_tags(metadata);
    for tag in tags {
        attributes.push(Erc721Trait {
            trait_type: "Tag".to_string(),
            value: tag,
        });
    }

    let body_shapes = extract_body_shapes(metadata);
    for shape in body_shapes {
        attributes.push(Erc721Trait {
            trait_type: "Body Shape".to_string(),
            value: shape,
        });
    }

    Erc721Metadata {
        id: urn.to_string(),
        name,
        description,
        language: "en-US".to_string(),
        image,
        thumbnail,
        attributes,
    }
}

fn find_content_hash(entity: &Entity, field_name: &str) -> Option<String> {
    let metadata = entity.metadata.as_ref()?;

    let file_path = metadata.get(field_name).and_then(|v| v.as_str())?;

    entity
        .content
        .iter()
        .find(|c| c.file == file_path)
        .map(|c| c.hash.clone())
}

fn extract_item_category(metadata: Option<&serde_json::Value>) -> Option<String> {
    metadata?
        .get("data")
        .or_else(|| metadata?.get("emoteDataADR74"))
        .and_then(|d| d.get("category"))
        .and_then(|c| c.as_str())
        .map(|s| s.to_string())
}

fn extract_item_tags(metadata: Option<&serde_json::Value>) -> Vec<String> {
    metadata
        .and_then(|m| m.get("data").or_else(|| m.get("emoteDataADR74")))
        .and_then(|d| d.get("tags"))
        .and_then(|t| t.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default()
}

fn extract_body_shapes(metadata: Option<&serde_json::Value>) -> Vec<String> {
    let data = metadata.and_then(|m| m.get("data").or_else(|| m.get("emoteDataADR74")));

    let representations = data
        .and_then(|d| d.get("representations"))
        .and_then(|r| r.as_array());

    let mut shapes = std::collections::BTreeSet::new();
    if let Some(reps) = representations {
        for rep in reps {
            if let Some(body_shapes) = rep.get("bodyShapes").and_then(|b| b.as_array()) {
                for shape in body_shapes {
                    if let Some(s) = shape.as_str() {
                        match s {
                            "urn:decentraland:off-chain:base-avatars:BaseMale" | "BaseMale" => {
                                shapes.insert("BaseMale".to_string());
                            }
                            "urn:decentraland:off-chain:base-avatars:BaseFemale" | "BaseFemale" => {
                                shapes.insert("BaseFemale".to_string());
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
    }

    shapes.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rarity_emissions() {
        assert_eq!(rarity_emission("common"), Some(100_000));
        assert_eq!(rarity_emission("unique"), Some(1));
        assert_eq!(rarity_emission("invalid"), None);
    }

    #[test]
    fn format_minimal_entity() {
        let entity = Entity {
            id: "bafkrei".to_string(),
            entity_type: crate::types::EntityType::Wearable,
            pointers: vec!["urn:decentraland:matic:collections-v2:0xabc:0".to_string()],
            timestamp: 1700000000000,
            content: vec![],
            version: "v3".to_string(),
            metadata: Some(serde_json::json!({
                "i18n": [{"code": "en", "text": "Cool Hat"}],
                "rarity": "epic",
                "data": {
                    "category": "hat",
                    "tags": ["cool", "hat"],
                    "representations": []
                }
            })),
        };

        let result = format_erc721_entity(
            "urn:decentraland:matic:collections-v2:0xabc:0",
            &entity,
            "https://peer.decentraland.org/content",
            Some("42"),
        );

        assert_eq!(result.name, Some("Cool Hat".to_string()));
        assert_eq!(result.description, "DCL Wearable 42/1000");
        assert_eq!(result.language, "en-US");
        assert!(result
            .attributes
            .iter()
            .any(|a| a.trait_type == "Rarity" && a.value == "epic"));
        assert!(result
            .attributes
            .iter()
            .any(|a| a.trait_type == "Category" && a.value == "hat"));
    }
}
