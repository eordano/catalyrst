//! Foundation discovery reads, using the same event and world contracts as Catalyrst.
use super::*;

#[derive(Deserialize)]
pub struct DiscoveryQuery {
    #[serde(default)]
    search: String,
    #[serde(default)]
    offset: u32,
    name: Option<String>,
}

pub async fn events(
    State(s): State<Arc<Store>>,
    Query(q): Query<DiscoveryQuery>,
) -> ApiResult<Json<Value>> {
    discover(&s, "https://events.decentraland.org/api/events", &q, false).await
}
pub async fn worlds(
    State(s): State<Arc<Store>>,
    Query(q): Query<DiscoveryQuery>,
) -> ApiResult<Json<Value>> {
    discover(&s, "https://places.decentraland.org/api/worlds", &q, true).await
}
async fn discover(
    s: &Store,
    url: &str,
    q: &DiscoveryQuery,
    worlds: bool,
) -> ApiResult<Json<Value>> {
    if q.search.len() > 200 || q.offset > 10000 {
        return Err(bad("Invalid discovery query"));
    }
    let mut request = s
        .client
        .get(url)
        .header("User-Agent", "dcl.social/1.0")
        .query(&[
            ("search", q.search.as_str()),
            ("limit", "24"),
            ("offset", &q.offset.to_string()),
        ]);
    let mut key = format!(
        "{}:{}:{}:",
        if worlds { "worlds" } else { "events" },
        q.offset,
        q.search
    );
    if worlds {
        request = request.query(&[("order_by", "most_active")]);
        if let Some(name) = &q.name {
            if !valid_world(name) {
                return Err(bad("Invalid world name"));
            }
            request = request.query(&[("names", name)]);
            key.push_str(name);
        }
    } else {
        request = request.query(&[("list", "upcoming"), ("order", "asc")]);
    }
    let value = memoized(s, key, 30_000, || async {
        let response = request.send().await.map_err(|_| unavailable())?;
        if !response.status().is_success() {
            return Err(unavailable());
        }
        bounded_json(response).await
    })
    .await?;
    Ok(Json(value))
}
pub fn valid_world(name: &str) -> bool {
    name.len() <= 253
        && name.contains('.')
        && name.split('.').all(|part| {
            !part.is_empty()
                && part.len() <= 63
                && !part.starts_with('-')
                && !part.ends_with('-')
                && part.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-')
        })
}

pub async fn event(State(s): State<Arc<Store>>, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    event_read(&s, &id, false).await
}
pub async fn attendees(
    State(s): State<Arc<Store>>,
    Path(id): Path<String>,
) -> ApiResult<Json<Value>> {
    event_read(&s, &id, true).await
}
async fn event_read(s: &Store, id: &str, attendees: bool) -> ApiResult<Json<Value>> {
    Uuid::parse_str(id).map_err(|_| bad("Invalid event ID"))?;
    let suffix = if attendees { "/attendees" } else { "" };
    let value = memoized(s, format!("event:{id}{suffix}"), 30_000, || async {
        let response = s
            .client
            .get(format!(
                "https://events.decentraland.org/api/events/{id}{suffix}"
            ))
            .header("User-Agent", "dcl.social/1.0")
            .send()
            .await
            .map_err(|_| unavailable())?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(ApiError(StatusCode::NOT_FOUND, "Event not found".into()));
        }
        if !response.status().is_success() {
            return Err(unavailable());
        }
        bounded_json(response).await
    })
    .await?;
    Ok(Json(value))
}
