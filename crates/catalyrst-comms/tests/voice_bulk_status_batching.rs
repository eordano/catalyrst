//! Perf regression: `community_voice_chat_bulk_status` must resolve every
//! requested community with ONE aggregate query over `community_voice_chat_users`,
//! not one SELECT per id. Also pins result equivalence with the old per-id loop,
//! including the bulk endpoint's no-zeroing semantics (a room with connected
//! non-moderators but no active moderator reports `active=false` with a nonzero
//! `participant_count`).
//!
//! DB-gated via the shared scratch cluster (`CATALYRST_COMMS_TEST_PG` /
//! `CATALYRST_TEST_PG`); skips cleanly when unset.
//!
//! MUST be a current-thread `#[tokio::test]` -- see `support`'s SQL-counter docs.

mod support;

use axum::extract::State;
use axum::Json;
use catalyrst_comms::handlers::voice::{community_voice_chat_bulk_status, BulkCommunityStatusBody};
use catalyrst_contract_gate::pg::ScratchSchema;

const K: usize = 50;
const MOD_ROOMS: usize = 25;

async fn setup() -> Option<ScratchSchema> {
    let scratch = ScratchSchema::create("CATALYRST_COMMS_TEST_PG", "cg_comms_bulkvoice").await?;
    scratch
        .apply_sql(include_str!("../migrations/0001_comms.sql"))
        .await;
    scratch
        .apply_sql(include_str!(
            "../migrations/0007_community_voice_chat_sid.sql"
        ))
        .await;
    Some(scratch)
}

fn room_name(r: usize) -> String {
    format!("voice-chat-community-c{r:02}")
}

/// Hand-computed expectation for room `r` from the seed below.
fn expected(r: usize) -> serde_json::Value {
    let has_mod = r < MOD_ROOMS;
    // connected non-mods (2) + interrupted-active (1) + not_connected-active (1)
    // + (moderator when present).
    let participant_count = if has_mod { 5 } else { 4 };
    let moderator_count = if has_mod { 1 } else { 0 };
    serde_json::json!({
        "community_id": format!("c{r:02}"),
        "active": moderator_count > 0,
        "participant_count": participant_count,
        "moderator_count": moderator_count,
    })
}

/// One big multi-row INSERT so seeding is a single round trip; all values are
/// controlled test constants, so literal interpolation is safe.
async fn seed(pool: &sqlx::PgPool) {
    let mut rows: Vec<String> = Vec::new();
    let mut push =
        |addr: String, room: &str, is_mod: bool, status: &str, joined: &str, updated: &str| {
            rows.push(format!(
                "('{addr}','{room}',{is_mod},'{status}',{joined},{updated})"
            ));
        };
    for r in 0..K {
        let room = room_name(r);
        if r < MOD_ROOMS {
            push(
                format!("0xr{r:02}mod"),
                &room,
                true,
                "connected",
                "now()",
                "now()",
            );
        }
        push(
            format!("0xr{r:02}u1"),
            &room,
            false,
            "connected",
            "now()",
            "now()",
        );
        push(
            format!("0xr{r:02}u2"),
            &room,
            false,
            "connected",
            "now()",
            "now()",
        );
        push(
            format!("0xr{r:02}dis"),
            &room,
            false,
            "disconnected",
            "now()",
            "now()",
        );
        // connection_interrupted with a fresh update = active.
        push(
            format!("0xr{r:02}intA"),
            &room,
            false,
            "connection_interrupted",
            "now()",
            "now()",
        );
        // connection_interrupted stale by an hour = inactive (TTL is 300s).
        push(
            format!("0xr{r:02}intB"),
            &room,
            false,
            "connection_interrupted",
            "now()",
            "now() - interval '1 hour'",
        );
        // not_connected joined just now = active.
        push(
            format!("0xr{r:02}nc"),
            &room,
            false,
            "not_connected",
            "now()",
            "now()",
        );
    }
    let sql = format!(
        "INSERT INTO community_voice_chat_users \
         (address, room_name, is_moderator, status, joined_at, status_updated_at) VALUES {}",
        rows.join(", ")
    );
    sqlx::query(sqlx::AssertSqlSafe(sql))
        .execute(pool)
        .await
        .expect("seed insert");
}

#[tokio::test]
async fn bulk_status_is_one_query_and_matches_per_id_semantics() {
    let Some(scratch) = setup().await else {
        return;
    };
    let pool = scratch.pool.clone();
    seed(&pool).await;

    let state = support::test_state(pool.clone(), None, None, "squid_marketplace");

    // Install AFTER seeding so the seed INSERT is outside the counting window.
    let (counter, _guard) = support::install_sql_counter();

    let resp = community_voice_chat_bulk_status(
        State(state.clone()),
        Json(BulkCommunityStatusBody {
            community_ids: (0..K).map(|i| format!("c{i:02}")).collect(),
        }),
    )
    .await
    .expect("bulk status");

    assert_eq!(
        counter.count_containing("community_voice_chat_users"),
        1,
        "bulk status must hit community_voice_chat_users exactly once"
    );

    let expected_data: Vec<serde_json::Value> = (0..K).map(expected).collect();
    assert_eq!(
        resp.0["data"],
        serde_json::Value::Array(expected_data),
        "bulk result must equal the hand-computed per-id result (order preserved)"
    );

    // Pin the no-zeroing bulk semantics: a room with participants but no active
    // moderator reports active=false AND participant_count>0.
    let no_mod = &resp.0["data"][MOD_ROOMS];
    assert_eq!(no_mod["active"], serde_json::json!(false));
    assert_eq!(no_mod["participant_count"], serde_json::json!(4));
    assert_eq!(no_mod["moderator_count"], serde_json::json!(0));

    // Duplicate + unknown ids: request order preserved, duplicates each emitted,
    // absent room defaults to (false, 0, 0). Still one query.
    counter.reset();
    let resp2 = community_voice_chat_bulk_status(
        State(state),
        Json(BulkCommunityStatusBody {
            community_ids: vec!["c00".into(), "c00".into(), "zz".into()],
        }),
    )
    .await
    .expect("bulk status dup");
    assert_eq!(
        counter.count_containing("community_voice_chat_users"),
        1,
        "duplicate/unknown ids must still be one query"
    );
    let data2 = resp2.0["data"].as_array().unwrap();
    assert_eq!(data2.len(), 3);
    assert_eq!(data2[0], expected(0));
    assert_eq!(data2[1], expected(0));
    assert_eq!(
        data2[2],
        serde_json::json!({
            "community_id": "zz",
            "active": false,
            "participant_count": 0,
            "moderator_count": 0,
        })
    );

    scratch.drop().await;
}
