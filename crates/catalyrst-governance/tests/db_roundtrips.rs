//! PG-gated: the store runs its migrations and the archive tables are mimicked in scratch schemas.
use catalyrst_governance::ports::archives::{self, ArchiveStatus};
use catalyrst_governance::ports::store::Store;
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;

async fn pool(label: &str) -> Option<PgPool> {
    let Ok(url) = std::env::var("CATALYRST_TEST_PG") else {
        eprintln!("CATALYRST_TEST_PG unset; skipping {label}");
        return None;
    };
    let schema = format!("gov_rt_{}_{}", label, std::process::id());
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .after_connect(move |conn, _| {
            let schema = schema.clone();
            Box::pin(async move {
                sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
                    "CREATE SCHEMA IF NOT EXISTS {schema}; SET search_path TO {schema}"
                )))
                .execute(conn)
                .await?;
                Ok(())
            })
        })
        .connect(&url)
        .await
        .expect("connect scratch pg");
    Some(pool)
}

/// Concurrent migrators in one process race on the shared advisory lock; run them one at a time.
static MIGRATE: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

async fn store_pool(label: &str) -> Option<PgPool> {
    let pool = pool(label).await?;
    {
        let _serial = MIGRATE.lock().await;
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .expect("migrations");
    }
    sqlx::raw_sql("TRUNCATE proposals, projects, project_updates")
        .execute(&pool)
        .await
        .unwrap();
    Some(pool)
}

async fn snapshot_pool(label: &str) -> Option<PgPool> {
    let pool = pool(label).await?;
    sqlx::raw_sql(
        "CREATE TABLE IF NOT EXISTS proposals (id text PRIMARY KEY, choices jsonb, scores jsonb, \
             scores_total float8, votes_count integer); \
         CREATE TABLE IF NOT EXISTS votes (proposal_id text, voter text, choice jsonb, vp float8, \
             created_ts bigint, reason text); \
         TRUNCATE proposals, votes",
    )
    .execute(&pool)
    .await
    .unwrap();
    Some(pool)
}

#[tokio::test]
async fn votes_head_and_votes_ride_one_statement() {
    let Some(pool) = snapshot_pool("votes").await else {
        return;
    };
    sqlx::query(
        "INSERT INTO proposals (id, choices, scores, scores_total, votes_count) \
         VALUES ('p1', $1, $2, 3.5, 3)",
    )
    .bind(serde_json::json!(["yes", "no"]))
    .bind(serde_json::json!([2.5, 1.0]))
    .execute(&pool)
    .await
    .unwrap();

    let payload = archives::proposal_votes(&pool, "p1").await.unwrap();
    assert!(matches!(payload.archive_status, ArchiveStatus::Ok));
    assert_eq!(payload.choices, vec!["yes", "no"]);
    assert_eq!(payload.scores, vec![2.5, 1.0]);
    assert_eq!(payload.scores_total, 3.5);
    assert_eq!(payload.votes_count, 3);
    assert!(payload.votes.is_empty());
    assert!(payload.series.is_none());

    for (voter, choice, vp, ts) in [
        ("0xa", 1, 5.0f64, 100i64),
        ("0xb", 2, 1.0, 200),
        ("0xc", 1, 3.0, 300),
    ] {
        sqlx::query(
            "INSERT INTO votes (proposal_id, voter, choice, vp, created_ts, reason) \
             VALUES ('p1', $1, $2, $3, $4, NULL)",
        )
        .bind(voter)
        .bind(serde_json::json!(choice))
        .bind(vp)
        .bind(ts)
        .execute(&pool)
        .await
        .unwrap();
    }
    let payload = archives::proposal_votes(&pool, "p1").await.unwrap();
    let voters: Vec<&str> = payload.votes.iter().map(|v| v.voter.as_str()).collect();
    assert_eq!(voters, vec!["0xa", "0xc", "0xb"]);
    assert_eq!(payload.votes[0].choice, 1);
    assert_eq!(payload.votes[0].created_ts, 100);
    assert_eq!(payload.votes_count, 3);
    assert!(payload.series.is_some());

    let missing = archives::proposal_votes(&pool, "nope").await.unwrap();
    assert!(matches!(missing.archive_status, ArchiveStatus::NotIndexed));
    assert!(missing.votes.is_empty());
}

#[tokio::test]
async fn comments_total_rides_the_page() {
    let Some(pool) = pool("comments").await else {
        return;
    };
    sqlx::raw_sql(
        "CREATE TABLE IF NOT EXISTS posts (topic_id bigint, post_number integer, \
             hidden boolean NOT NULL DEFAULT false, deleted_at timestamptz, username text, \
             created_at timestamptz, raw text); \
         TRUNCATE posts",
    )
    .execute(&pool)
    .await
    .unwrap();
    for (n, hidden, deleted) in [
        (1, false, false),
        (2, false, false),
        (3, true, false),
        (4, false, true),
        (5, false, false),
        (6, false, false),
    ] {
        sqlx::query(
            "INSERT INTO posts (topic_id, post_number, hidden, deleted_at, username, created_at, raw) \
             VALUES (7, $1, $2, CASE WHEN $3 THEN now() END, $4, to_timestamp(1000 + $1), $5)",
        )
        .bind(n)
        .bind(hidden)
        .bind(deleted)
        .bind(format!("u{n}"))
        .bind(format!("body{n}"))
        .execute(&pool)
        .await
        .unwrap();
    }

    let page = archives::comments_by_topic(&pool, 7, 2).await.unwrap();
    assert_eq!(page.total, 3);
    let names: Vec<&str> = page.comments.iter().map(|c| c.username.as_str()).collect();
    assert_eq!(names, vec!["u6", "u5"]);
    assert_eq!(page.comments[0].text, "body6");
    assert!(page.comments[0]
        .created_at
        .starts_with("1970-01-01T00:16:46"));

    let empty = archives::comments_by_topic(&pool, 8, 2).await.unwrap();
    assert_eq!(empty.total, 0);
    assert!(empty.comments.is_empty());
}

#[tokio::test]
async fn project_detail_is_one_statement() {
    let Some(pool) = store_pool("project").await else {
        return;
    };
    let store = Store::new(pool.clone());
    sqlx::raw_sql(
        "INSERT INTO proposals (id, raw, configuration) \
             VALUES ('pp1', '{}'::jsonb, '{\"budget\": 5}'::jsonb); \
         INSERT INTO projects (id, proposal_id, raw) \
             VALUES ('pr1', 'pp1', '{\"name\": \"one\"}'::jsonb), \
                    ('pr2', NULL, '{\"name\": \"two\"}'::jsonb), \
                    ('pr3', 'missing', '{\"name\": \"three\"}'::jsonb); \
         INSERT INTO project_updates (id, project_id, created_at, raw) \
             VALUES ('u1', 'pr1', to_timestamp(200), '{\"n\": 2}'::jsonb), \
                    ('u2', 'pr1', to_timestamp(100), '{\"n\": 1}'::jsonb), \
                    ('u3', 'pr1', NULL, '{\"n\": 3}'::jsonb), \
                    ('u4', 'pr2', to_timestamp(50), '{\"n\": 9}'::jsonb)",
    )
    .execute(&pool)
    .await
    .unwrap();

    let (raw, cfg, updates) = store.get_project_detail("pr1").await.unwrap().unwrap();
    assert_eq!(raw["name"], "one");
    assert_eq!(cfg.unwrap()["budget"], 5);
    let ns: Vec<i64> = updates.iter().map(|u| u["n"].as_i64().unwrap()).collect();
    assert_eq!(ns, vec![1, 2, 3]);

    let (_, cfg, updates) = store.get_project_detail("pr2").await.unwrap().unwrap();
    assert!(cfg.is_none());
    assert_eq!(updates.len(), 1);
    let (_, cfg, updates) = store.get_project_detail("pr3").await.unwrap().unwrap();
    assert!(cfg.is_none());
    assert!(updates.is_empty());
    assert!(store.get_project_detail("nope").await.unwrap().is_none());
}

#[tokio::test]
async fn activity_union_matches_the_three_feeds() {
    let Some(pool) = store_pool("activity").await else {
        return;
    };
    let store = Store::new(pool.clone());
    sqlx::raw_sql(
        "INSERT INTO proposals (id, title, \"user\", created_at, finish_at, raw) \
             VALUES ('a', 'A', '0xa', to_timestamp(100), to_timestamp(150), '{}'::jsonb), \
                    ('b', 'B', NULL, to_timestamp(200), now() + interval '1 day', '{}'::jsonb), \
                    ('c', NULL, '0xc', NULL, to_timestamp(120), '{}'::jsonb), \
                    ('d', 'D', '0xd', to_timestamp(50), to_timestamp(60), '{}'::jsonb); \
         INSERT INTO projects (id, proposal_id, title, raw) VALUES ('p', 'a', 'Proj A', '{}'::jsonb); \
         INSERT INTO project_updates (id, project_id, proposal_id, created_at, raw) \
             VALUES ('u1', 'p', 'a', to_timestamp(300), '{}'::jsonb), \
                    ('u2', NULL, 'b', to_timestamp(310), '{}'::jsonb), \
                    ('u3', NULL, NULL, to_timestamp(320), '{}'::jsonb), \
                    ('u4', 'p', 'a', NULL, '{}'::jsonb)",
    )
    .execute(&pool)
    .await
    .unwrap();

    type Item = (&'static str, Option<String>, String, Option<String>, i64);
    let got: Vec<Item> = store
        .recent_activity(2)
        .await
        .unwrap()
        .into_iter()
        .map(|r| (r.kind, r.proposal_id, r.title, r.address, r.ts))
        .collect();
    let mut want: Vec<Item> = Vec::new();
    for (id, title, user, ts) in store.recent_proposals(2).await.unwrap() {
        want.push(("proposal", Some(id), title, user, ts));
    }
    for (id, title, ts) in store.recently_finished(2).await.unwrap() {
        want.push(("finished", Some(id), title, None, ts));
    }
    for (proposal_id, title, ts) in store.recent_updates(2).await.unwrap() {
        want.push(("update", proposal_id, title, None, ts));
    }
    assert_eq!(got.len(), 6);
    assert_eq!(got, want);
}
