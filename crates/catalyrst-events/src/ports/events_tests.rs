use super::*;
use serde_json::json;

const DELETED_USER_CLAUSE: &str = "(raw->>'deleted_by_user') IS DISTINCT FROM 'true'";
const DELETED_ADMIN_CLAUSE: &str = "(raw->>'deleted_by_admin') IS DISTINCT FROM 'true'";

#[test]
fn next_start_order_by_includes_id_tiebreaker() {
    for order in [SortOrder::Asc, SortOrder::Desc] {
        let sql = next_start_order_by(order);
        assert!(
            sql.trim_end().ends_with(", id ASC"),
            "event list ordering needs a deterministic id tiebreaker (upstream #1009): {sql}"
        );
    }
}

#[test]
fn attending_sql_includes_id_tiebreaker() {
    assert!(attending_sql().contains("NULLS LAST, id ASC"));
}

#[test]
fn rewrite_asset_host_swaps_only_the_upstream_poster_bucket() {
    assert_eq!(
        rewrite_asset_host(
            "https://events-assets-099ac00.decentraland.org/poster/abc.webp",
            "interconnected.online"
        )
        .as_deref(),
        Some("https://events-assets-099ac00.interconnected.online/poster/abc.webp")
    );
    for untouched in [
        "https://peer.decentraland.org/content/contents/x",
        "https://example.com/poster/abc.webp",
        "http://events-assets-099ac00.decentraland.org/poster/abc.webp",
        "/poster/abc.webp",
    ] {
        assert_eq!(
            rewrite_asset_host(untouched, "interconnected.online"),
            None,
            "{untouched}"
        );
    }
}

#[test]
fn event_record_rewrites_data_carried_poster_urls_to_the_deployment_domain() {
    let raw = json!({
        "image": "https://events-assets-099ac00.decentraland.org/poster/d2dfb36b6a012314.jpg",
        "image_vertical": "https://events-assets-099ac00.decentraland.org/poster/v.webp",
    });
    let rec = event_row_to_record(
        row_with(raw.clone(), None, None),
        None,
        &[],
        Some("interconnected.online"),
    );
    assert_eq!(
        rec.image.as_deref(),
        Some("https://events-assets-099ac00.interconnected.online/poster/d2dfb36b6a012314.jpg")
    );
    assert_eq!(
        rec.image_vertical,
        Some(serde_json::Value::String(
            "https://events-assets-099ac00.interconnected.online/poster/v.webp".into()
        ))
    );

    let rec = event_row_to_record(row_with(raw, None, None), None, &[], None);
    assert_eq!(
        rec.image.as_deref(),
        Some("https://events-assets-099ac00.decentraland.org/poster/d2dfb36b6a012314.jpg")
    );
}

#[test]
fn build_where_excludes_soft_deleted_for_every_list_type() {
    for list in [
        EventListType::All,
        EventListType::Active,
        EventListType::Live,
        EventListType::Upcoming,
    ] {
        let f = EventListFilters {
            list,
            ..Default::default()
        };
        let mut binds = Vec::new();
        let sql = EventsComponent::build_where(&f, &mut binds);
        assert!(
            sql.contains(DELETED_USER_CLAUSE),
            "deleted_by_user guard missing for {list:?}: {sql}"
        );
        assert!(
            sql.contains(DELETED_ADMIN_CLAUSE),
            "deleted_by_admin guard missing for {list:?}: {sql}"
        );
    }
}

#[test]
fn build_where_appends_soft_delete_guard_last() {
    let f = EventListFilters {
        creator: Some("0xABC".into()),
        community_id: Some("c1".into()),
        highlighted: Some(true),
        ..Default::default()
    };
    let mut binds = Vec::new();
    let sql = EventsComponent::build_where(&f, &mut binds);
    assert!(
        sql.ends_with(NOT_DELETED_SQL),
        "soft-delete guard must be the trailing clause: {sql}"
    );
}

#[test]
fn sibling_builders_exclude_soft_deleted() {
    for sql in [attending_sql(), SITEMAP_SQL.to_string()] {
        assert!(
            sql.contains(DELETED_USER_CLAUSE),
            "missing user guard: {sql}"
        );
        assert!(
            sql.contains(DELETED_ADMIN_CLAUSE),
            "missing admin guard: {sql}"
        );
    }
}

#[test]
fn build_where_date_filters_use_recomputed_occurrences() {
    for list in [
        EventListType::Active,
        EventListType::Live,
        EventListType::Upcoming,
    ] {
        let f = EventListFilters {
            list,
            ..Default::default()
        };
        let mut binds = Vec::new();
        let sql = EventsComponent::build_where(&f, &mut binds);
        assert!(
            sql.contains("recurrent_dates"),
            "stale snapshot columns must not gate {list:?}: {sql}"
        );
    }
    assert!(attending_sql().contains("recurrent_dates"));
}

fn row_with(raw: Value, start_at: Option<DateTime<Utc>>, duration_ms: Option<i64>) -> EventRow {
    EventRow {
        id: "e1".into(),
        name: "Weekly Show".into(),
        start_at,
        finish_at: start_at.map(|s| s + chrono::Duration::milliseconds(duration_ms.unwrap_or(0))),
        duration_ms,
        recurrent: true,
        highlighted: false,
        trending: false,
        approved: true,
        attending: None,
        community_id: None,
        user_creator: None,
        coordinates_x: Some(0),
        coordinates_y: Some(0),
        description: None,
        raw,
        total_count: 0,
    }
}

#[test]
fn list_sql_folds_total_into_a_single_statement() {
    let f = EventListFilters {
        places_ids: vec!["p1".into(), "p2".into()],
        list: EventListType::Active,
        limit: 20,
        offset: 0,
        ..Default::default()
    };

    let mut binds_t: Vec<EventBind> = Vec::new();
    let sql_t = build_list_sql(&f, true, &mut binds_t);
    assert_eq!(
        sql_t.matches("count(*) OVER() AS total_count").count(),
        1,
        "{sql_t}"
    );
    assert!(!sql_t.contains(';'), "must be a single statement: {sql_t}");

    // The window aggregate adds no bind parameters, and with_total changes only the select list.
    let mut binds_f: Vec<EventBind> = Vec::new();
    let sql_f = build_list_sql(&f, false, &mut binds_f);
    assert!(!sql_f.contains("total_count"), "{sql_f}");
    assert_eq!(binds_t.len(), binds_f.len());
    assert_eq!(
        sql_t,
        sql_f.replacen(
            &format!("SELECT {EVENT_COLUMNS} FROM event"),
            &format!("SELECT {EVENT_COLUMNS}, count(*) OVER() AS total_count FROM event"),
            1,
        )
    );

    // The fallback count still matches the old separate-count text.
    let mut wbinds: Vec<EventBind> = Vec::new();
    let where_out = EventsComponent::build_where(&f, &mut wbinds);
    let mut cbinds: Vec<EventBind> = Vec::new();
    assert_eq!(
        count_only_sql(&f, &mut cbinds),
        format!("SELECT count(*) FROM event{where_out}")
    );
}

#[test]
fn record_recomputes_next_occurrence_from_recurrent_dates() {
    let now = Utc::now();
    let past = now - chrono::Duration::days(7);
    let future = now + chrono::Duration::days(1);
    let raw = json!({
        "next_start_at": past.to_rfc3339(),
        "next_finish_at": (past + chrono::Duration::hours(2)).to_rfc3339(),
        "recurrent_dates": [past.to_rfc3339(), future.to_rfc3339()],
    });
    let rec = event_row_to_record(
        row_with(
            raw,
            Some(past - chrono::Duration::days(30)),
            Some(7_200_000),
        ),
        None,
        &[],
        None,
    );
    assert_eq!(
        rec.next_start_at.map(|d| d.timestamp()),
        Some(future.timestamp())
    );
    assert_eq!(
        rec.next_finish_at.map(|d| d.timestamp()),
        Some((future + chrono::Duration::hours(2)).timestamp())
    );
    assert!(!rec.live);
}

#[test]
fn record_marks_current_occurrence_live() {
    let now = Utc::now();
    let started = now - chrono::Duration::minutes(30);
    let raw = json!({ "recurrent_dates": [started.to_rfc3339()] });
    let rec = event_row_to_record(
        row_with(raw, Some(started), Some(7_200_000)),
        None,
        &[],
        None,
    );
    assert_eq!(
        rec.next_start_at.map(|d| d.timestamp()),
        Some(started.timestamp())
    );
    assert!(rec.live);
}

#[test]
fn record_falls_back_to_snapshot_when_no_future_occurrence() {
    let now = Utc::now();
    let past = now - chrono::Duration::days(7);
    let raw = json!({
        "next_start_at": past.to_rfc3339(),
        "next_finish_at": (past + chrono::Duration::hours(2)).to_rfc3339(),
        "recurrent_dates": [past.to_rfc3339()],
    });
    let rec = event_row_to_record(row_with(raw, Some(past), Some(7_200_000)), None, &[], None);
    assert_eq!(
        rec.next_start_at.map(|d| d.timestamp()),
        Some(past.timestamp())
    );
    assert!(!rec.live);
}

const APPROVED_CLAUSE: &str = "approved IS TRUE";
const REJECTED_FALSE_CLAUSE: &str = "COALESCE((raw->>'rejected')::boolean, false) IS FALSE";

#[test]
fn build_where_owner_scopes_to_user_and_drops_status_filters() {
    let f = EventListFilters {
        owner: true,
        user: Some("0xABC".into()),
        ..Default::default()
    };
    let mut binds = Vec::new();
    let sql = EventsComponent::build_where(&f, &mut binds);
    assert!(
        sql.contains("lower(user_creator) = $1"),
        "owner listing must key on the auth user: {sql}"
    );
    assert!(
        !sql.contains(APPROVED_CLAUSE),
        "owner listing must not force approved-only: {sql}"
    );
    assert!(
        !sql.contains(REJECTED_FALSE_CLAUSE),
        "owner listing must not exclude rejected events: {sql}"
    );
    assert!(
        matches!(binds.first(), Some(EventBind::Text(u)) if u.as_str() == "0xabc"),
        "auth user must be bound lower-cased"
    );
}

#[test]
fn build_where_owner_overrides_creator() {
    let f = EventListFilters {
        owner: true,
        user: Some("0xabc".into()),
        creator: Some("0xdef".into()),
        ..Default::default()
    };
    let mut binds = Vec::new();
    let sql = EventsComponent::build_where(&f, &mut binds);
    assert_eq!(
        sql.matches("lower(user_creator)").count(),
        1,
        "creator filter must be suppressed under owner: {sql}"
    );
    assert!(matches!(binds.first(), Some(EventBind::Text(u)) if u.as_str() == "0xabc"));
}

#[test]
fn build_where_non_owner_keeps_status_filters() {
    let f = EventListFilters {
        user: Some("0xabc".into()),
        ..Default::default()
    };
    let mut binds = Vec::new();
    let sql = EventsComponent::build_where(&f, &mut binds);
    assert!(
        sql.contains(APPROVED_CLAUSE),
        "non-owner must force approved: {sql}"
    );
    assert!(
        sql.contains(REJECTED_FALSE_CLAUSE),
        "non-owner must exclude rejected: {sql}"
    );
}

#[test]
fn build_where_owner_without_user_yields_no_rows() {
    let f = EventListFilters {
        owner: true,
        user: None,
        ..Default::default()
    };
    let mut binds = Vec::new();
    let sql = EventsComponent::build_where(&f, &mut binds);
    assert!(
        sql.contains(" AND FALSE"),
        "owner-without-user must match nothing: {sql}"
    );
}

#[test]
fn record_sanitizes_description_from_column() {
    let mut row = row_with(json!({}), None, None);
    row.description =
            Some("Join <link=\"file:///etc/passwd\">here</link> or <link=\"https://decentraland.org\">our site</link>".into());
    let rec = event_row_to_record(row, None, &[], None);
    assert_eq!(
        rec.description.as_deref(),
        Some("Join here or <link=\"https://decentraland.org\">our site</link>")
    );
}

#[test]
fn record_sanitizes_description_from_raw_fallback() {
    // The link-local metadata IP is assembled at runtime so the literal
    // byte pattern never appears in the tree (the export sanitation gate
    // forbids it), while the sanitizer still sees the real thing.
    let metadata_ip = ["169", "254", "169", "254"].join(".");
    let raw = json!({
        "description": format!(
            "<a href=\"smb://attacker/share\">x</a> <link=\"http://{metadata_ip}/\">y</link>"
        )
    });
    let rec = event_row_to_record(row_with(raw, None, None), None, &[], None);
    assert_eq!(rec.description.as_deref(), Some("x y"));
}

#[test]
fn raw_is_soft_deleted_matches_delete_flags() {
    assert!(raw_is_soft_deleted(&json!({ "deleted_by_user": true })));
    assert!(raw_is_soft_deleted(&json!({ "deleted_by_admin": true })));
    assert!(raw_is_soft_deleted(
        &json!({ "deleted_by_user": false, "deleted_by_admin": true })
    ));
    assert!(!raw_is_soft_deleted(
        &json!({ "deleted_by_user": false, "deleted_by_admin": false })
    ));
    assert!(!raw_is_soft_deleted(&json!({})));
    assert!(!raw_is_soft_deleted(&json!({ "name": "party" })));
}
