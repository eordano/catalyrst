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
        .filter_map(|r| {
            r.get(addr_field)
                .and_then(|v| v.as_str())
                .map(str::to_string)
        })
        .collect();

    let map = profiles.get_profiles(&addresses).await;

    for row in rows.iter_mut() {
        let Some(obj) = row.as_object_mut() else {
            continue;
        };
        let addr = obj
            .get(addr_field)
            .and_then(|v| v.as_str())
            .map(|s| s.to_lowercase())
            .unwrap_or_default();

        let info = map.get(&addr);
        let (name, pic, claimed) = match info {
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
        if let Some(nc) = info.and_then(|i| i.name_color.as_ref()) {
            if let Ok(v) = serde_json::to_value(nc) {
                obj.insert("nameColor".to_string(), v);
            }
        }
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
        .filter_map(|r| {
            r.get(addr_field)
                .and_then(|v| v.as_str())
                .map(str::to_string)
        })
        .collect();

    let map = profiles.get_profiles(&addresses).await;

    for row in rows.iter_mut() {
        let Some(obj) = row.as_object_mut() else {
            continue;
        };
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

#[cfg(test)]
mod tests {
    use super::*;

    fn poolless() -> ProfilesComponent {
        ProfilesComponent::new(None, "https://content".to_string())
    }

    #[tokio::test]
    async fn member_rows_get_all_nre_critical_profile_fields() {
        let mut rows = vec![serde_json::json!({
            "communityId": "c1",
            "memberAddress": "0xABC",
            "role": "member",
            "joinedAt": "2024-01-01T00:00:00",
        })];
        enrich_with_profiles(&poolless(), &mut rows, "memberAddress").await;

        let m = rows[0].as_object().unwrap();

        for key in [
            "memberAddress",
            "name",
            "profilePictureUrl",
            "hasClaimedName",
        ] {
            assert!(m.contains_key(key), "member missing {key}");
        }

        assert_eq!(m["name"], "");
        assert_eq!(m["profilePictureUrl"], "");
        assert_eq!(m["hasClaimedName"], false);
        assert!(
            !m.contains_key("nameColor"),
            "unresolved member must not carry a nameColor key"
        );
    }

    #[tokio::test]
    async fn post_rows_get_author_profile_fields() {
        let mut rows = vec![serde_json::json!({
            "id": "p1",
            "authorAddress": "0xABC",
            "content": "hi",
        })];
        enrich_posts_with_authors(&poolless(), &mut rows, "authorAddress").await;

        let m = rows[0].as_object().unwrap();
        for key in [
            "authorName",
            "authorProfilePictureUrl",
            "authorHasClaimedName",
        ] {
            assert!(m.contains_key(key), "post missing {key}");
        }
    }
}
