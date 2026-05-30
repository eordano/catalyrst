use crate::ports::profiles::ProfilesComponent;

pub async fn enrich_with_profiles(
    profiles: &ProfilesComponent,
    rows: &mut [serde_json::Value],
    addr_field: &str,
) {
    if rows.is_empty() {
        return;
    }

    let addresses: Vec<String> = rows
        .iter()
        .filter_map(|r| r.get(addr_field).and_then(|v| v.as_str()).map(str::to_string))
        .collect();

    let map = profiles.get_profiles(&addresses).await;

    for row in rows.iter_mut() {
        let Some(obj) = row.as_object_mut() else { continue };
        let addr = obj
            .get(addr_field)
            .and_then(|v| v.as_str())
            .map(|s| s.to_lowercase())
            .unwrap_or_default();

        let (name, pic, claimed) = match map.get(&addr) {
            Some(info) => (
                info.name.clone(),
                info.profile_picture_url.clone(),
                info.has_claimed_name,
            ),
            None => (String::new(), String::new(), false),
        };

        obj.insert("name".to_string(), serde_json::Value::String(name));
        obj.insert(
            "profilePictureUrl".to_string(),
            serde_json::Value::String(pic),
        );
        obj.insert(
            "hasClaimedName".to_string(),
            serde_json::Value::Bool(claimed),
        );
    }
}

pub async fn enrich_posts_with_authors(
    profiles: &ProfilesComponent,
    rows: &mut [serde_json::Value],
    addr_field: &str,
) {
    if rows.is_empty() {
        return;
    }

    let addresses: Vec<String> = rows
        .iter()
        .filter_map(|r| r.get(addr_field).and_then(|v| v.as_str()).map(str::to_string))
        .collect();

    let map = profiles.get_profiles(&addresses).await;

    for row in rows.iter_mut() {
        let Some(obj) = row.as_object_mut() else { continue };
        let addr = obj
            .get(addr_field)
            .and_then(|v| v.as_str())
            .map(|s| s.to_lowercase())
            .unwrap_or_default();

        let (name, pic, claimed) = match map.get(&addr) {
            Some(info) => (
                info.name.clone(),
                info.profile_picture_url.clone(),
                info.has_claimed_name,
            ),
            None => (addr.clone(), String::new(), false),
        };

        obj.insert("authorName".to_string(), serde_json::Value::String(name));
        obj.insert(
            "authorProfilePictureUrl".to_string(),
            serde_json::Value::String(pic),
        );
        obj.insert(
            "authorHasClaimedName".to_string(),
            serde_json::Value::Bool(claimed),
        );
    }
}
