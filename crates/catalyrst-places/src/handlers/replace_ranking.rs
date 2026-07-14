use std::collections::HashSet;

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use serde_json::Value;

use crate::auth::require_ranking_token;
use crate::entity_id::EntityType;
use crate::http::errors::ApiError;
use crate::http::response::ApiData;
use crate::ports::places::{RankingEntry, ReplaceRankingResult};
use crate::AppState;

// A run of the automated score is one request, so the cap bounds how much a
// single call may rewrite. It sits far above the real set and exists because
// an unbounded bulk write that also clears what it omits is not something to
// leave open.
pub const MAX_RANKING_ENTRIES: usize = 5000;
const MAX_ID_LEN: usize = 255;

const BODY_SHAPE: &str = "Invalid ranking body. Expected { entries: [{ entity_type: \
     \"place\"|\"world\", id: string, ranking: number }] }.";

struct SubmittedEntry {
    entity_type: EntityType,
    id: String,
    ranking: f64,
}

fn entry_key(entry: &SubmittedEntry) -> String {
    format!("{}:{}", entry.entity_type.as_str(), entry.id)
}

fn parse_entry(value: &Value) -> Result<SubmittedEntry, ApiError> {
    let object = value
        .as_object()
        .ok_or_else(|| ApiError::bad_request(BODY_SHAPE))?;
    if object
        .keys()
        .any(|k| !matches!(k.as_str(), "entity_type" | "id" | "ranking"))
    {
        return Err(ApiError::bad_request(BODY_SHAPE));
    }
    let entity_type = match object.get("entity_type").and_then(Value::as_str) {
        Some("place") => EntityType::Place,
        Some("world") => EntityType::World,
        _ => return Err(ApiError::bad_request(BODY_SHAPE)),
    };
    let id = object
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| ApiError::bad_request(BODY_SHAPE))?;
    let length = id.chars().count();
    if length == 0 || length > MAX_ID_LEN {
        return Err(ApiError::bad_request(format!(
            "A destination id must be 1 to {MAX_ID_LEN} characters"
        )));
    }
    let ranking = object
        .get("ranking")
        .and_then(Value::as_f64)
        .ok_or_else(|| ApiError::bad_request(BODY_SHAPE))?;
    // A negative value from the automated score is always a broken run, so it
    // is rejected here rather than written.
    if !ranking.is_finite() || ranking < 0.0 {
        return Err(ApiError::bad_request(
            "A ranking must be a number greater than or equal to 0",
        ));
    }
    Ok(SubmittedEntry {
        entity_type,
        id: id.to_string(),
        ranking,
    })
}

fn parse_body(body: &Value) -> Result<Vec<SubmittedEntry>, ApiError> {
    let object = body
        .as_object()
        .ok_or_else(|| ApiError::bad_request(BODY_SHAPE))?;
    if object.keys().any(|k| k != "entries") {
        return Err(ApiError::bad_request(BODY_SHAPE));
    }
    let items = object
        .get("entries")
        .and_then(Value::as_array)
        .ok_or_else(|| ApiError::bad_request(BODY_SHAPE))?;
    if items.len() > MAX_RANKING_ENTRIES {
        return Err(ApiError::bad_request(format!(
            "Too many entries (max {MAX_RANKING_ENTRIES})"
        )));
    }
    items.iter().map(parse_entry).collect()
}

// An id sent twice means the caller computed two rankings for one destination
// and whichever landed last would win silently, so a duplicate is a broken run
// rather than a row to skip. Reported with the entity type, not the bare id:
// the type is half of what identifies the row. The id itself is compared as
// sent, because that is the key the caller's own export is unique on.
fn duplicated_keys(entries: &[SubmittedEntry]) -> Vec<String> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut duplicated: Vec<String> = Vec::new();
    for entry in entries {
        let key = entry_key(entry);
        if !seen.insert(key.clone()) && !duplicated.contains(&key) {
            duplicated.push(key);
        }
    }
    duplicated
}

fn leg(entries: &[SubmittedEntry], entity_type: EntityType) -> Vec<RankingEntry> {
    entries
        .iter()
        .filter(|e| e.entity_type == entity_type)
        .map(|e| RankingEntry {
            id: e.id.clone(),
            ranking: e.ranking,
        })
        .collect()
}

/// Replace the whole automated ranking set in one transaction.
///
/// The payload is the complete set for a run: every ranking it names is
/// written, and every automated ranking it does not name is cleared. Curated
/// destinations are never touched, not by the write and not by the clear, for
/// both callers -- this route captures no token identity, so the admin bearer
/// buys no override here the way it does on the single-destination routes.
#[utoipa::path(
    put,
    path = "/destinations/ranking",
    tag = "destinations",
    request_body = serde_json::Value,
    responses(
        (status = 201, body = ApiData<ReplaceRankingResult>),
        (status = 400, body = catalyrst_types::ApiErrorBody),
        (status = 401, body = catalyrst_types::ApiErrorBody),
        (status = 500, body = catalyrst_types::ApiErrorBody),
        (status = 503, body = catalyrst_types::ApiErrorBody)
    )
)]
pub async fn put_destinations_ranking(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Option<Json<Value>>,
) -> Result<(StatusCode, Json<ApiData<ReplaceRankingResult>>), ApiError> {
    require_ranking_token(
        &headers,
        state.data_team_auth_token.as_deref(),
        state.admin_auth_token.as_deref(),
    )?;
    let body = body
        .map(|Json(v)| v)
        .unwrap_or_else(|| Value::Object(Default::default()));
    let entries = parse_body(&body)?;
    let duplicated = duplicated_keys(&entries);
    if !duplicated.is_empty() {
        return Err(ApiError::bad_request(format!(
            "The same destination appears more than once: {}",
            duplicated.join(", ")
        )));
    }
    let result = state
        .places
        .replace_ranking(
            &leg(&entries, EntityType::Place),
            &leg(&entries, EntityType::World),
        )
        .await?;
    Ok((StatusCode::CREATED, Json(ApiData::ok(result))))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn parsed(body: Value) -> Result<Vec<SubmittedEntry>, ApiError> {
        parse_body(&body)
    }

    #[test]
    fn a_well_formed_payload_splits_into_its_two_legs() {
        let entries = parsed(json!({ "entries": [
            { "entity_type": "place", "id": "place-1", "ranking": 122 },
            { "entity_type": "world", "id": "named.dcl.eth", "ranking": 121.5 },
        ] }))
        .expect("valid body");
        let places = leg(&entries, EntityType::Place);
        let worlds = leg(&entries, EntityType::World);
        assert_eq!(places.len(), 1);
        assert_eq!(places[0].id, "place-1");
        assert_eq!(places[0].ranking, 122.0);
        assert_eq!(worlds.len(), 1);
        assert_eq!(worlds[0].id, "named.dcl.eth");
        assert_eq!(worlds[0].ranking, 121.5);
    }

    #[test]
    fn an_empty_set_is_a_valid_run() {
        assert!(parsed(json!({ "entries": [] }))
            .expect("valid body")
            .is_empty());
    }

    #[test]
    fn a_body_without_entries_is_rejected() {
        assert!(parsed(json!({})).is_err());
        assert!(parsed(json!({ "entries": {} })).is_err());
        assert!(parsed(json!([])).is_err());
    }

    #[test]
    fn an_unknown_key_is_rejected_on_the_envelope_and_on_an_entry() {
        assert!(parsed(json!({ "entries": [], "dry_run": true })).is_err());
        assert!(parsed(json!({ "entries": [
            { "entity_type": "place", "id": "p", "ranking": 1, "note": "x" }
        ] }))
        .is_err());
    }

    #[test]
    fn an_entry_missing_a_required_field_is_rejected() {
        assert!(parsed(json!({ "entries": [{ "entity_type": "place", "id": "p" }] })).is_err());
        assert!(parsed(json!({ "entries": [{ "id": "p", "ranking": 1 }] })).is_err());
        assert!(parsed(json!({ "entries": [{ "entity_type": "place", "ranking": 1 }] })).is_err());
    }

    // The single-destination route takes { ranking: number|null }; this one is
    // a complete set, where an unranked destination is the omission itself.
    #[test]
    fn a_null_or_non_numeric_ranking_is_rejected() {
        assert!(parsed(
            json!({ "entries": [{ "entity_type": "place", "id": "p", "ranking": null }] })
        )
        .is_err());
        assert!(parsed(
            json!({ "entries": [{ "entity_type": "place", "id": "p", "ranking": "7" }] })
        )
        .is_err());
    }

    #[test]
    fn a_negative_ranking_is_rejected() {
        assert!(parsed(
            json!({ "entries": [{ "entity_type": "place", "id": "p", "ranking": -1 }] })
        )
        .is_err());
        assert!(parsed(
            json!({ "entries": [{ "entity_type": "place", "id": "p", "ranking": 0 }] })
        )
        .is_ok());
    }

    #[test]
    fn an_unknown_entity_type_is_rejected() {
        assert!(parsed(
            json!({ "entries": [{ "entity_type": "scene", "id": "p", "ranking": 1 }] })
        )
        .is_err());
    }

    #[test]
    fn an_empty_or_overlong_id_is_rejected() {
        assert!(
            parsed(json!({ "entries": [{ "entity_type": "place", "id": "", "ranking": 1 }] }))
                .is_err()
        );
        let long = "a".repeat(MAX_ID_LEN + 1);
        assert!(parsed(
            json!({ "entries": [{ "entity_type": "place", "id": long, "ranking": 1 }] })
        )
        .is_err());
        let limit = "a".repeat(MAX_ID_LEN);
        assert!(parsed(
            json!({ "entries": [{ "entity_type": "place", "id": limit, "ranking": 1 }] })
        )
        .is_ok());
    }

    #[test]
    fn more_entries_than_the_cap_are_rejected() {
        let entries: Vec<Value> = (0..=MAX_RANKING_ENTRIES)
            .map(|i| json!({ "entity_type": "place", "id": i.to_string(), "ranking": 1 }))
            .collect();
        assert!(parsed(json!({ "entries": entries })).is_err());
    }

    #[test]
    fn a_repeated_destination_is_reported_once_with_its_entity_type() {
        let entries = parsed(json!({ "entries": [
            { "entity_type": "place", "id": "p", "ranking": 10 },
            { "entity_type": "place", "id": "p", "ranking": 20 },
            { "entity_type": "place", "id": "p", "ranking": 30 },
        ] }))
        .expect("valid body");
        assert_eq!(duplicated_keys(&entries), vec!["place:p".to_string()]);
    }

    // The two id spaces are a catalogue id and a world name, so the same
    // string under two entity types is two destinations, not one.
    #[test]
    fn the_same_id_under_two_entity_types_is_not_a_duplicate() {
        let entries = parsed(json!({ "entries": [
            { "entity_type": "place", "id": "shared", "ranking": 1 },
            { "entity_type": "world", "id": "shared", "ranking": 2 },
        ] }))
        .expect("valid body");
        assert!(duplicated_keys(&entries).is_empty());
    }

    // Case-sensitive, mirroring the caller's own uniqueness check: two casings
    // of one world name are not flagged here. They still refuse the run, one
    // layer down, where name resolution is what discovers they are one row --
    // ports/places/ranking_replace.rs::classify_worlds.
    #[test]
    fn two_casings_of_one_world_name_are_not_flagged_as_duplicates() {
        let entries = parsed(json!({ "entries": [
            { "entity_type": "world", "id": "World.dcl.eth", "ranking": 1 },
            { "entity_type": "world", "id": "world.dcl.eth", "ranking": 2 },
        ] }))
        .expect("valid body");
        assert!(duplicated_keys(&entries).is_empty());
    }
}
