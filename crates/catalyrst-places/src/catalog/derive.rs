use chrono::{DateTime, Utc};
use serde_json::{json, Value};
use uuid::Uuid;

pub struct DerivedPlace {
    pub id: String,
    pub base_position: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub creator_address: Option<String>,
    pub content_rating: Option<String>,
    pub deployed_at: Option<DateTime<Utc>>,
    pub raw: Value,
}

fn scene_json(metadata: &Value) -> &Value {
    metadata.get("v").unwrap_or(metadata)
}

fn non_empty(s: Option<&str>) -> Option<String> {
    s.map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

fn str_array(v: Option<&Value>) -> Vec<String> {
    v.and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

pub fn derive(
    deployer_address: &str,
    pointers: &[String],
    deployed_at: Option<DateTime<Utc>>,
    metadata: &Value,
    thumbnail_hash: Option<&str>,
    content_public_url: &str,
) -> Option<DerivedPlace> {
    let m = scene_json(metadata);
    let scene = m.get("scene")?;
    let base = non_empty(scene.get("base").and_then(Value::as_str))
        .or_else(|| pointers.first().cloned())?;

    let display = m.get("display");
    let title = non_empty(display.and_then(|d| d.get("title")).and_then(Value::as_str));
    let description = non_empty(
        display
            .and_then(|d| d.get("description"))
            .and_then(Value::as_str),
    );

    let contact = m.get("contact");
    let contact_name = non_empty(contact.and_then(|c| c.get("name")).and_then(Value::as_str));
    let contact_email = non_empty(contact.and_then(|c| c.get("email")).and_then(Value::as_str));

    let content_rating = non_empty(
        m.get("policy")
            .and_then(|p| p.get("contentRating"))
            .and_then(Value::as_str),
    );

    let mut positions = str_array(scene.get("parcels"));
    if positions.is_empty() {
        positions.push(base.clone());
    }
    let tags = str_array(m.get("tags"));

    let image = thumbnail_hash.map(|h| {
        format!(
            "{}/contents/{}",
            content_public_url.trim_end_matches('/'),
            h
        )
    });

    let creator_address = non_empty(Some(deployer_address)).map(|s| s.to_lowercase());

    let id = Uuid::new_v5(&Uuid::NAMESPACE_URL, base.as_bytes()).to_string();

    let raw = json!({
        "id": id,
        "title": title,
        "description": description,
        "owner": creator_address,
        "positions": positions,
        "base_position": base,
        "contact_name": contact_name,
        "contact_email": contact_email,
        "image": image,
        "tags": tags,
        "categories": [],
        "disabled": false,
        "world": false,
        "deployed_at": deployed_at,
        "source": "content",
    });

    Some(DerivedPlace {
        id,
        base_position: base,
        title,
        description,
        creator_address,
        content_rating,
        deployed_at,
        raw,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wrapped() -> Value {
        json!({ "v": {
            "scene": { "base": "135,2", "parcels": ["135,2", "135,3"] },
            "display": { "title": "My Scene", "description": "hello", "navmapThumbnail": "t.png" },
            "contact": { "name": "author", "email": "a@b.c" },
            "policy": { "contentRating": "E" },
            "tags": ["game", "art"]
        }})
    }

    #[test]
    fn derives_from_v_wrapped_scene() {
        let p = derive(
            "0xABC",
            &["135,2".to_string()],
            None,
            &wrapped(),
            Some("hash123"),
            "/content",
        )
        .expect("derives");
        assert_eq!(p.base_position, "135,2");
        assert_eq!(p.title.as_deref(), Some("My Scene"));
        assert_eq!(p.creator_address.as_deref(), Some("0xabc"));
        assert_eq!(p.content_rating.as_deref(), Some("E"));
        assert_eq!(p.raw["image"], "/content/contents/hash123");
        assert_eq!(p.raw["positions"], json!(["135,2", "135,3"]));
        assert_eq!(p.raw["source"], "content");
        assert_eq!(p.raw["world"], false);
    }

    #[test]
    fn id_is_stable_for_base_position() {
        let a = derive("0x1", &[], None, &wrapped(), None, "/content").unwrap();
        let b = derive("0x2", &[], None, &wrapped(), None, "/content").unwrap();
        assert_eq!(a.id, b.id);
    }

    #[test]
    fn no_scene_yields_none() {
        assert!(derive("0x1", &[], None, &json!({"v": {}}), None, "/content").is_none());
    }

    #[test]
    fn falls_back_to_pointer_when_base_missing() {
        let meta = json!({ "v": { "scene": { "parcels": [] } } });
        let p = derive("0x1", &["10,10".to_string()], None, &meta, None, "/content").unwrap();
        assert_eq!(p.base_position, "10,10");
        assert_eq!(p.raw["positions"], json!(["10,10"]));
    }
}
