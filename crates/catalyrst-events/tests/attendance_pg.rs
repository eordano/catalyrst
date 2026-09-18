use catalyrst_contract_gate::pg::ScratchSchema;
use catalyrst_events::mirror::{parse_page, upsert_event};
use catalyrst_events::ports::attendees::AttendeesComponent;
use catalyrst_events::ports::events::{EventListFilters, EventListType, EventsComponent};
use serde_json::{json, Value};

const ACTIVE_PAGE: &str = include_str!("fixtures/upstream_events_active_page0.json");
const USER: &str = "0x000000000000000000000000000000000000BEEF";
const OTHER: &str = "0x000000000000000000000000000000000000cafe";

async fn setup() -> Option<ScratchSchema> {
    let scratch = ScratchSchema::create("CATALYRST_EVENTS_TEST_PG", "cg_events_attend").await?;
    for sql in [
        include_str!("../migrations/0001_federation.sql"),
        include_str!("../migrations/0002_event_catalog.sql"),
        include_str!("../migrations/0003_local_overlays.sql"),
    ] {
        scratch.apply_sql(sql).await;
    }
    Some(scratch)
}

fn fixture_events() -> Vec<Value> {
    let body: Value = serde_json::from_str(ACTIVE_PAGE).unwrap();
    parse_page(&body).unwrap()
}

#[tokio::test]
async fn an_rsvp_and_its_reply_share_one_statement() {
    let Some(scratch) = setup().await else {
        return;
    };
    let pool = scratch.pool.clone();
    let events = fixture_events();
    let event = &events[0];
    upsert_event(&pool, event).await.unwrap();
    let id = event["id"].as_str().unwrap();
    let upstream: Vec<String> = event["latest_attendees"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    let attendees = AttendeesComponent::new(pool.clone());

    let baseline = attendees.list_for_event(id).await.unwrap();
    assert_eq!(
        baseline.iter().map(|a| a.user.as_str()).collect::<Vec<_>>(),
        upstream.iter().map(String::as_str).collect::<Vec<_>>()
    );

    let after_going = attendees
        .rsvp_going(id, USER, Some("Beef"), Value::Null)
        .await
        .unwrap();
    assert_eq!(after_going.len(), upstream.len() + 1);
    let mine = after_going
        .iter()
        .find(|a| a.user.eq_ignore_ascii_case(USER))
        .expect("the fresh RSVP is in the reply");
    assert_eq!(mine.user_name.as_deref(), Some("Beef"));
    assert_eq!(
        after_going[0].user, mine.user,
        "the newest RSVP sorts first"
    );
    let listed = attendees.list_for_event(id).await.unwrap();
    assert_eq!(listed.len(), after_going.len());

    let again = attendees
        .rsvp_going(id, USER, Some("Beef2"), json!({ "user_name": "Beef2" }))
        .await
        .unwrap();
    assert_eq!(again.len(), upstream.len() + 1, "a repeat RSVP is one row");
    assert_eq!(again[0].user_name.as_deref(), Some("Beef2"));

    let dup_upstream = attendees
        .rsvp_going(id, &upstream[0].to_uppercase(), None, Value::Null)
        .await
        .unwrap();
    assert_eq!(
        dup_upstream.len(),
        upstream.len() + 1,
        "a local RSVP by an upstream attendee dedupes case-insensitively"
    );

    let after_cancel = attendees.rsvp_cancel(id, USER).await.unwrap();
    assert!(!after_cancel
        .iter()
        .any(|a| a.user.eq_ignore_ascii_case(USER)));
    assert_eq!(after_cancel.len(), upstream.len());
    assert_eq!(
        attendees
            .list_for_event("no-such-event")
            .await
            .unwrap()
            .len(),
        0
    );

    scratch.drop().await;
}

#[tokio::test]
async fn the_viewers_attendance_rides_in_the_list_and_get_statements() {
    let Some(scratch) = setup().await else {
        return;
    };
    let pool = scratch.pool.clone();
    let events = fixture_events();
    for e in &events {
        upsert_event(&pool, e).await.unwrap();
    }
    let id = events[0]["id"].as_str().unwrap();
    let upstream_user = events[0]["latest_attendees"][0].as_str().unwrap();
    let attendees = AttendeesComponent::new(pool.clone());
    attendees
        .rsvp_going(id, USER, None, Value::Null)
        .await
        .unwrap();
    let component = EventsComponent::new(pool.clone(), None);

    let filters = EventListFilters {
        limit: 200,
        list: EventListType::All,
        user: Some(USER.to_string()),
        ..Default::default()
    };
    let (page, _) = component.query(&filters, false).await.unwrap();
    let mine = page.iter().find(|e| e.id == id).unwrap();
    assert!(mine.attending, "a local RSVP marks the viewer attending");
    assert!(page.iter().filter(|e| e.attending).count() >= 1);

    let other = EventListFilters {
        user: Some(OTHER.to_string()),
        ..filters.clone()
    };
    let (page, _) = component.query(&other, false).await.unwrap();
    assert!(!page.iter().find(|e| e.id == id).unwrap().attending);

    let only = EventListFilters {
        only_attendee: true,
        ..filters.clone()
    };
    let (page, _) = component.query(&only, false).await.unwrap();
    assert_eq!(page.len(), 1);
    assert_eq!(page[0].id, id);

    let got = component
        .get_for_viewer(id, Some(USER))
        .await
        .unwrap()
        .unwrap();
    assert!(got.attending);
    let got = component
        .get_for_viewer(id, Some(&upstream_user.to_uppercase()))
        .await
        .unwrap()
        .unwrap();
    assert!(got.attending, "an upstream attendee is attending too");
    let got = component
        .get_for_viewer(id, Some(OTHER))
        .await
        .unwrap()
        .unwrap();
    assert!(!got.attending);
    let anon = component.get_for_viewer(id, None).await.unwrap().unwrap();
    assert!(!anon.attending);
    assert!(component
        .get_for_viewer("no-such-event", Some(USER))
        .await
        .unwrap()
        .is_none());

    sqlx::query("INSERT INTO moderators (address, added_at) VALUES ($1, 0)")
        .bind(OTHER)
        .execute(&pool)
        .await
        .unwrap();
    assert!(component
        .viewer_is_moderator(&OTHER.to_uppercase())
        .await
        .unwrap());
    assert!(!component.viewer_is_moderator(USER).await.unwrap());

    scratch.drop().await;
}
