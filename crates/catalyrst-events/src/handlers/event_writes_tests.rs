use super::*;
use crate::clients::test_support::stub_places;
use crate::clients::{is_world_event, Places};

const FEATURED_ITEM: &str =
    "urn:decentraland:matic:collections-v2:0x1234567890abcdef1234567890abcdef12345678:1";
const OTHER_ITEM: &str =
    "urn:decentraland:matic:collections-v2:0x1234567890abcdef1234567890abcdef12345678";

fn create_body(extra: Value) -> CreateEventBody {
    let mut body = json!({
        "name": "Created Event",
        "start_at": "2030-01-01T00:00:00Z",
        "duration": 3_600_000,
        "x": 0,
        "y": 0,
    });
    body.as_object_mut()
        .unwrap()
        .extend(extra.as_object().cloned().unwrap_or_default());
    serde_json::from_value(body).unwrap()
}

fn update_body(body: Value) -> UpdateEventBody {
    serde_json::from_value(body).unwrap()
}

fn create_raw(body: &CreateEventBody) -> Map<String, Value> {
    create_raw_at(body, None)
}

fn create_raw_at(body: &CreateEventBody, place_id: Option<&str>) -> Map<String, Value> {
    let start_at = validate_create(body).unwrap();
    let world = is_world_event(body.world.unwrap_or(false), body.server.as_deref());
    match build_create_raw(body, "0xabc", start_at, place_id, world).unwrap() {
        Value::Object(m) => m,
        other => panic!("create raw must be an object: {other}"),
    }
}

#[test]
fn create_carries_the_resolved_place_id_and_null_on_a_miss() {
    let raw = create_raw_at(&create_body(json!({})), Some("place-uuid"));
    assert_eq!(raw["place_id"], json!("place-uuid"));
    let raw = create_raw_at(
        &create_body(json!({ "world": true, "server": "my-world.dcl.eth" })),
        Some("my-world.dcl.eth"),
    );
    assert_eq!(raw["place_id"], json!("my-world.dcl.eth"));
    let raw = create_raw(&create_body(json!({})));
    assert_eq!(raw["place_id"], Value::Null);
}

#[tokio::test]
async fn the_stored_world_flag_follows_the_resolution_not_the_request() {
    let places = Places::new(stub_places().await);
    let body = create_body(json!({ "world": true, "server": null, "x": 1, "y": 2 }));
    let raw = build_create(&places, &body, "0xabc", validate_create(&body).unwrap())
        .await
        .unwrap();
    assert_eq!(raw["world"], json!(false));
    assert_eq!(raw["place_id"], json!("place-uuid"));
    assert_eq!(
        raw["url"],
        json!("https://decentraland.org/play/?position=1%2C2")
    );

    let body = create_body(json!({ "world": true, "server": "my-world.dcl.eth", "x": 1, "y": 2 }));
    let raw = build_create(&places, &body, "0xabc", validate_create(&body).unwrap())
        .await
        .unwrap();
    assert_eq!(raw["world"], json!(true));
    assert_eq!(raw["place_id"], json!("my-world.dcl.eth"));
}

fn stored_world_event() -> Map<String, Value> {
    let mut raw = stored(None);
    raw.insert("x".into(), json!(1));
    raw.insert("y".into(), json!(2));
    raw.insert("server".into(), json!("my-world.dcl.eth"));
    raw.insert("world".into(), json!(true));
    raw.insert("place_id".into(), json!("my-world.dcl.eth"));
    raw
}

#[tokio::test]
async fn an_explicit_null_location_key_moves_a_world_event_back_to_its_parcel() {
    let places = Places::new(stub_places().await);
    for cleared in [json!({ "server": null }), json!({ "world": null })] {
        let mut raw = stored_world_event();
        let body = update_body(cleared.clone());
        assert!(location_touched(&body), "{cleared}");
        apply_owner_edit(&mut raw, &body, false).unwrap();
        resolve_location(&places, &mut raw).await;
        assert_eq!(raw["world"], json!(false), "{cleared}");
        assert_eq!(raw["place_id"], json!("place-uuid"), "{cleared}");
    }
    let mut raw = stored_world_event();
    apply_owner_edit(&mut raw, &update_body(json!({ "server": null })), false).unwrap();
    assert_eq!(raw["server"], Value::Null);
}

#[test]
fn only_a_location_edit_re_resolves_the_place() {
    for touched in [
        json!({ "x": 1 }),
        json!({ "y": 1 }),
        json!({ "server": "my-world.dcl.eth" }),
        json!({ "server": null }),
        json!({ "world": true }),
        json!({ "world": null }),
    ] {
        assert!(location_touched(&update_body(touched.clone())), "{touched}");
    }
    for untouched in [
        json!({}),
        json!({ "name": "Renamed" }),
        json!({ "featured_item": FEATURED_ITEM }),
        json!({ "start_at": "2030-01-01T00:00:00Z" }),
    ] {
        assert!(
            !location_touched(&update_body(untouched.clone())),
            "{untouched}"
        );
    }
}

fn stored(featured_item: Option<&str>) -> Map<String, Value> {
    let mut raw = Map::new();
    raw.insert("name".into(), json!("Stored"));
    raw.insert("approved".into(), json!(true));
    raw.insert("featured_item".into(), json!(featured_item));
    raw
}

#[test]
fn create_accepts_featured_item_and_stores_it_in_raw() {
    let raw = create_raw(&create_body(json!({ "featured_item": FEATURED_ITEM })));
    assert_eq!(raw["featured_item"], json!(FEATURED_ITEM));
}

#[test]
fn create_normalizes_empty_and_missing_featured_item_to_null() {
    let raw = create_raw(&create_body(json!({ "featured_item": "" })));
    assert_eq!(raw["featured_item"], Value::Null);
    let raw = create_raw(&create_body(json!({})));
    assert_eq!(raw["featured_item"], Value::Null);
    let raw = create_raw(&create_body(json!({ "featured_item": null })));
    assert_eq!(raw["featured_item"], Value::Null);
}

#[test]
fn create_rejects_featured_items_outside_the_collections_v2_urn_shape() {
    for bad in [
        "my favourite wearable".to_string(),
        "urn:decentraland:mainnet:collections-v2:0x1234567890abcdef1234567890abcdef12345678:1"
            .into(),
        "urn:decentraland:matic:collections-v1:0x1234567890abcdef1234567890abcdef12345678:1".into(),
        format!("{FEATURED_ITEM}{}", "1".repeat(130)),
    ] {
        let body = create_body(json!({ "featured_item": bad }));
        assert!(validate_create(&body).is_err(), "{bad}");
    }
}

#[test]
fn update_sets_featured_item() {
    let mut raw = stored(None);
    let body = update_body(json!({ "featured_item": FEATURED_ITEM }));
    apply_owner_edit(&mut raw, &body, false).unwrap();
    assert_eq!(raw["featured_item"], json!(FEATURED_ITEM));
}

#[test]
fn update_clears_featured_item_with_empty_string_or_null() {
    for cleared in [json!(""), Value::Null] {
        let mut raw = stored(Some(FEATURED_ITEM));
        let body = update_body(json!({ "featured_item": cleared }));
        apply_owner_edit(&mut raw, &body, false).unwrap();
        assert_eq!(raw["featured_item"], Value::Null, "{cleared}");
    }
}

#[test]
fn update_without_featured_item_keeps_the_stored_value() {
    let mut raw = stored(Some(FEATURED_ITEM));
    let body = update_body(json!({ "name": "Renamed" }));
    apply_owner_edit(&mut raw, &body, false).unwrap();
    assert_eq!(raw["featured_item"], json!(FEATURED_ITEM));
}

#[test]
fn update_rejects_invalid_featured_item_and_leaves_raw_untouched() {
    for bad in [
        "my favourite wearable".to_string(),
        "urn:decentraland:mainnet:collections-v2:0x1234567890abcdef1234567890abcdef12345678:1"
            .into(),
        "urn:decentraland:matic:collections-v1:0x1234567890abcdef1234567890abcdef12345678:1".into(),
        format!("{FEATURED_ITEM}{}", "1".repeat(130)),
    ] {
        let mut raw = stored(None);
        let body = update_body(json!({ "featured_item": bad }));
        assert!(apply_owner_edit(&mut raw, &body, false).is_err(), "{bad}");
        assert_eq!(raw["featured_item"], Value::Null, "{bad}");
    }
}

#[test]
fn featured_item_change_is_judged_against_the_stored_value() {
    let raw = stored(None);
    assert!(!featured_item_changed(&raw, &update_body(json!({}))));
    assert!(!featured_item_changed(
        &raw,
        &update_body(json!({ "featured_item": "" }))
    ));
    assert!(!featured_item_changed(
        &raw,
        &update_body(json!({ "featured_item": null }))
    ));
    assert!(featured_item_changed(
        &raw,
        &update_body(json!({ "featured_item": FEATURED_ITEM }))
    ));

    let raw = stored(Some(FEATURED_ITEM));
    assert!(!featured_item_changed(
        &raw,
        &update_body(json!({ "featured_item": FEATURED_ITEM }))
    ));
    assert!(featured_item_changed(
        &raw,
        &update_body(json!({ "featured_item": OTHER_ITEM }))
    ));
    assert!(featured_item_changed(
        &raw,
        &update_body(json!({ "featured_item": "" }))
    ));
    assert!(featured_item_changed(
        &raw,
        &update_body(json!({ "featured_item": null }))
    ));
}

#[test]
fn update_rejects_unsafe_url_and_leaves_raw_untouched() {
    for bad in [
        "javascript:alert(1)",
        "http://localhost:8080/",
        "http://169.254.169.254/",
        "file:///etc/passwd",
    ] {
        let mut raw = stored(None);
        raw.insert("url".into(), json!("https://decentraland.org/events"));
        let body = update_body(json!({ "url": bad }));
        assert!(apply_owner_edit(&mut raw, &body, false).is_err(), "{bad}");
        assert_eq!(
            raw["url"],
            json!("https://decentraland.org/events"),
            "{bad}"
        );
    }
}

#[test]
fn update_stores_safe_trimmed_url() {
    let mut raw = stored(None);
    let body = update_body(json!({ "url": "  https://decentraland.org/events  " }));
    apply_owner_edit(&mut raw, &body, false).unwrap();
    assert_eq!(raw["url"], json!("https://decentraland.org/events"));
}

#[test]
fn update_rejects_unsafe_image_and_image_vertical() {
    for field in ["image", "image_vertical"] {
        let mut raw = stored(None);
        let body = update_body(json!({ field: "javascript:alert(1)" }));
        assert!(apply_owner_edit(&mut raw, &body, false).is_err(), "{field}");
    }
}

#[test]
fn featured_item_alone_does_not_count_as_touched_content() {
    assert!(!content_touched(&update_body(
        json!({ "featured_item": FEATURED_ITEM })
    )));
    assert!(content_touched(&update_body(
        json!({ "community_id": "c1" })
    )));
}

fn dated(start_at: &str) -> Map<String, Value> {
    let start = parse_rfc3339(start_at).unwrap();
    let mut raw = stored(None);
    raw.insert("start_at".into(), json!(start.to_rfc3339()));
    raw.insert("duration".into(), json!(3_600_000));
    raw.insert(
        "finish_at".into(),
        json!((start + Duration::milliseconds(3_600_000)).to_rfc3339()),
    );
    raw
}

#[test]
fn create_rejects_an_event_that_already_ended() {
    let body = create_body(json!({ "start_at": "2020-01-01T00:00:00Z" }));
    assert_eq!(
        validate_create(&body).unwrap_err().to_string(),
        PAST_FINISH_AT_MESSAGE
    );
}

#[test]
fn create_accepts_an_event_still_running() {
    let start_at = (Utc::now() - Duration::minutes(30)).to_rfc3339();
    let body = create_body(json!({ "start_at": start_at, "duration": 3_600_000 }));
    assert!(validate_create(&body).is_ok());
}

#[test]
fn create_rejects_a_recurrence_that_ends_in_the_past() {
    let body = create_body(json!({
        "recurrent": true,
        "recurrent_frequency": "WEEKLY",
        "recurrent_until": "2020-06-01T00:00:00Z",
    }));
    assert_eq!(
        validate_create(&body).unwrap_err().to_string(),
        PAST_RECURRENT_UNTIL_MESSAGE
    );
}

#[test]
fn create_accepts_a_recurrence_that_ends_in_the_future() {
    let body = create_body(json!({
        "recurrent": true,
        "recurrent_frequency": "WEEKLY",
        "recurrent_until": "2031-06-01T00:00:00Z",
    }));
    assert!(validate_create(&body).is_ok());
}

#[test]
fn a_metadata_only_edit_of_a_finished_event_is_allowed() {
    let mut raw = dated("2020-01-01T00:00:00Z");
    for untouched in [
        json!({ "name": "Renamed" }),
        json!({ "description": "Still over" }),
        json!({ "featured_item": FEATURED_ITEM }),
    ] {
        let edit = apply_owner_edit(&mut raw, &update_body(untouched.clone()), false);
        assert!(edit.is_ok(), "{untouched}");
    }
    assert_eq!(raw["name"], json!("Renamed"));
}

#[test]
fn an_edit_that_leaves_the_end_date_in_the_past_is_rejected() {
    let mut raw = dated("2030-01-01T00:00:00Z");
    let body = update_body(json!({ "start_at": "2020-01-01T00:00:00Z" }));
    assert_eq!(
        apply_owner_edit(&mut raw, &body, false)
            .unwrap_err()
            .to_string(),
        PAST_FINISH_AT_MESSAGE
    );

    let mut raw = dated("2020-01-01T00:00:00Z");
    let body = update_body(json!({ "duration": 3_600_000 }));
    assert_eq!(
        apply_owner_edit(&mut raw, &body, false)
            .unwrap_err()
            .to_string(),
        PAST_FINISH_AT_MESSAGE
    );
}

#[test]
fn an_edit_that_moves_the_recurrence_end_into_the_past_is_rejected() {
    let mut raw = dated("2030-01-01T00:00:00Z");
    raw.insert("recurrent".into(), json!(true));
    raw.insert("recurrent_frequency".into(), json!("WEEKLY"));
    raw.insert("recurrent_until".into(), json!("2031-12-31T00:00:00Z"));
    let body = update_body(json!({ "recurrent_until": "2020-01-01T00:00:00Z" }));
    assert_eq!(
        apply_owner_edit(&mut raw, &body, false)
            .unwrap_err()
            .to_string(),
        PAST_RECURRENT_UNTIL_MESSAGE
    );
}

fn past_series(until: &str) -> Map<String, Value> {
    let mut raw = dated("2020-01-01T00:00:00Z");
    raw.insert("recurrent".into(), json!(true));
    raw.insert("recurrent_frequency".into(), json!("WEEKLY"));
    raw.insert("recurrent_until".into(), json!(until));
    raw
}

#[test]
fn a_selector_only_edit_of_a_series_whose_dates_are_past_is_rejected() {
    for selector in [
        json!({ "recurrent_weekday_mask": 0 }),
        json!({ "recurrent_monthday": 1 }),
        json!({ "recurrent_setpos": 1 }),
        json!({ "recurrent_month_mask": 0 }),
    ] {
        let mut raw = past_series("2020-06-01T00:00:00Z");
        let body = update_body(selector.clone());
        assert_eq!(
            apply_owner_edit(&mut raw, &body, false)
                .unwrap_err()
                .to_string(),
            PAST_FINISH_AT_MESSAGE,
            "{selector}"
        );
    }
}

#[test]
fn a_selector_only_edit_that_empties_a_running_series_is_accepted() {
    let mut raw = past_series("2031-12-31T00:00:00Z");
    let body = update_body(json!({ "recurrent_weekday_mask": 0 }));
    assert!(apply_owner_edit(&mut raw, &body, false).is_ok());
}

#[test]
fn create_accepts_a_running_series_that_started_in_the_past() {
    let body = create_body(json!({
        "start_at": "2020-01-01T00:00:00Z",
        "recurrent": true,
        "recurrent_frequency": "WEEKLY",
        "recurrent_until": "2031-01-01T00:00:00Z",
    }));
    assert!(validate_create(&body).is_ok());
}

#[test]
fn an_edit_extending_a_running_series_is_allowed() {
    let mut raw = past_series("2031-01-01T00:00:00Z");
    let body = update_body(json!({ "recurrent_until": "2032-01-01T00:00:00Z" }));
    assert!(apply_owner_edit(&mut raw, &body, false).is_ok());
    assert_eq!(raw["recurrent_until"], json!("2032-01-01T00:00:00Z"));
}

#[test]
fn a_series_without_a_frequency_is_not_treated_as_recurrent() {
    let body = create_body(json!({
        "recurrent": true,
        "recurrent_until": "2020-06-01T00:00:00Z",
    }));
    assert!(validate_create(&body).is_ok());
}

#[test]
fn create_rejects_a_duration_past_the_maximum() {
    let body = create_body(json!({ "duration": MAX_EVENT_DURATION_MS + 1 }));
    assert_eq!(
        validate_create(&body).unwrap_err().to_string(),
        "Maximum allowed duration 24Hrs"
    );
    let body = create_body(json!({ "duration": MAX_EVENT_DURATION_MS }));
    assert!(validate_create(&body).is_ok());
}

#[test]
fn a_duration_that_would_overflow_the_clock_is_a_400_not_a_panic() {
    let body = create_body(json!({ "duration": i64::MAX }));
    assert_eq!(
        validate_create(&body).unwrap_err().to_string(),
        "Maximum allowed duration 24Hrs"
    );
}

#[test]
fn an_edit_cannot_stretch_the_duration_past_the_maximum() {
    let mut raw = dated("2030-01-01T00:00:00Z");
    let body = update_body(json!({ "duration": MAX_EVENT_DURATION_MS + 1 }));
    assert_eq!(
        apply_owner_edit(&mut raw, &body, false)
            .unwrap_err()
            .to_string(),
        "Maximum allowed duration 24Hrs"
    );

    let mut raw = dated("2030-01-01T00:00:00Z");
    raw.insert("duration".into(), json!(MAX_EVENT_DURATION_MS * 2));
    let body = update_body(json!({ "duration": MAX_EVENT_DURATION_MS * 2 }));
    assert!(apply_owner_edit(&mut raw, &body, false).is_ok());
}

#[test]
fn a_null_only_date_field_still_triggers_the_past_date_check() {
    let body = update_body(json!({ "recurrent_weekday_mask": null }));
    assert!(!recurrence_touched(&body));
    assert!(date_key_present(&Bytes::from_static(
        b"{\"recurrent_weekday_mask\":null}"
    )));
    let mut raw = dated("2020-01-01T00:00:00Z");
    assert_eq!(
        apply_owner_edit(&mut raw, &body, true)
            .unwrap_err()
            .to_string(),
        PAST_FINISH_AT_MESSAGE
    );
}
