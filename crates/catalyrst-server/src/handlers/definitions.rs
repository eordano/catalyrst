use serde_json::{json, Map, Value};

pub const SORTED_RARITIES: &[&str] = &[
    "common",
    "uncommon",
    "rare",
    "epic",
    "legendary",
    "exotic",
    "mythic",
    "unique",
];

pub fn rarity_rank(rarity: &str) -> i64 {
    SORTED_RARITIES
        .iter()
        .position(|r| *r == rarity)
        .map(|p| p as i64)
        .unwrap_or(-1)
}

pub fn content_url(entity: &Value, file_name: &str, content_public_url: &str) -> Option<String> {
    let hash = entity
        .get("content")
        .and_then(|c| c.as_array())
        .and_then(|arr| {
            arr.iter().find_map(|item| {
                let f = item.get("file").and_then(|v| v.as_str())?;
                if f == file_name {
                    item.get("hash").and_then(|v| v.as_str())
                } else {
                    None
                }
            })
        })?;
    let base = if content_public_url.ends_with('/') {
        content_public_url.to_string()
    } else {
        format!("{content_public_url}/")
    };
    Some(format!("{base}contents/{hash}"))
}

pub fn map_representations(reps: &mut Value, entity: &Value, content_public_url: &str) {
    if let Some(arr) = reps.as_array_mut() {
        for rep in arr.iter_mut() {
            let new_contents: Vec<Value> = rep
                .get("contents")
                .and_then(|c| c.as_array())
                .map(|files| {
                    files
                        .iter()
                        .filter_map(|f| f.as_str())
                        .map(|file_name| {
                            let url = content_url(entity, file_name, content_public_url)
                                .unwrap_or_default();
                            json!({ "key": file_name, "url": url })
                        })
                        .collect()
                })
                .unwrap_or_default();
            if let Some(obj) = rep.as_object_mut() {
                obj.insert("contents".into(), Value::Array(new_contents));
            }
        }
    }
}

pub fn rewrite_image_thumbnail(
    def: &mut Map<String, Value>,
    entity: &Value,
    content_public_url: &str,
) {
    if let Some(image_file) = def
        .get("image")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
    {
        if let Some(url) = content_url(entity, &image_file, content_public_url) {
            def.insert("image".into(), Value::String(url));
        }
    }
    if let Some(thumb_file) = def
        .get("thumbnail")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
    {
        if let Some(url) = content_url(entity, &thumb_file, content_public_url) {
            def.insert("thumbnail".into(), Value::String(url));
        }
    }
}

pub fn extract_wearable_definition(entity: &Value, content_public_url: &str) -> Option<Value> {
    let metadata = entity.get("metadata")?.clone();
    let mut def = metadata.as_object()?.clone();

    if let Some(data) = def.get_mut("data").and_then(|d| d.as_object_mut()) {
        if let Some(reps) = data.get_mut("representations") {
            map_representations(reps, entity, content_public_url);
        }
    }
    rewrite_image_thumbnail(&mut def, entity, content_public_url);
    Some(Value::Object(def))
}

pub fn extract_emote_definition(entity: &Value, content_public_url: &str) -> Option<Value> {
    let metadata = entity.get("metadata")?.clone();
    let mut meta = metadata.as_object()?.clone();

    let emote_data_adr74: Value = if meta.contains_key("emoteDataADR74") {
        let mut adr = meta.remove("emoteDataADR74").unwrap();
        if let Some(reps) = adr.get_mut("representations") {
            map_representations(reps, entity, content_public_url);
        }
        adr
    } else {
        let data = meta.remove("data").unwrap_or_else(|| json!({}));
        let emote_data_v0 = meta.remove("emoteDataV0");
        let loop_val = emote_data_v0
            .as_ref()
            .and_then(|v| v.get("loop"))
            .cloned()
            .unwrap_or(Value::Bool(false));
        let tags = data.get("tags").cloned().unwrap_or_else(|| json!([]));
        let mut reps = data
            .get("representations")
            .cloned()
            .unwrap_or_else(|| json!([]));
        map_representations(&mut reps, entity, content_public_url);
        json!({
            "category": "dance",
            "tags": tags,
            "loop": loop_val,
            "representations": reps,
        })
    };

    rewrite_image_thumbnail(&mut meta, entity, content_public_url);
    meta.insert("emoteDataADR74".into(), emote_data_adr74);
    Some(Value::Object(meta))
}

thread_local! {

    static COLLATOR: std::cell::RefCell<feruca::Collator> =
        std::cell::RefCell::new(feruca::Collator::default());
}

pub(crate) fn locale_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    COLLATOR.with(|c| c.borrow_mut().collate(a, b))
}
