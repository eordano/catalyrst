//! Grant and revoke each run as one statement; these pin the outcomes the
//! multi-statement version produced: unknown badge, unresolvable tier (with and
//! without a requested tierId), idempotent re-grant, and the audit rows.
//! DB-gated via `CATALYRST_BADGES_TEST_PG` / `CATALYRST_TEST_PG`.

use catalyrst_badges::config::DEFAULT_ASSET_BASE_URL;
use catalyrst_badges::http::errors::ApiError;
use catalyrst_badges::ports::badges::BadgesComponent;
use catalyrst_contract_gate::pg::ScratchSchema;
use chrono::{DateTime, Utc};
use sqlx::PgPool;

const ADDR: &str = "0xf39fd6e51aad88f6f4ce6ab8827279cfffb92266";
const ACTOR: &str = "tests";

macro_rules! gate {
    () => {
        match ScratchSchema::create("CATALYRST_BADGES_TEST_PG", "cg_badges_grant").await {
            Some(s) => s,
            None => return,
        }
    };
}

async fn component(pool: &PgPool) -> BadgesComponent {
    sqlx::migrate!("./migrations").run(pool).await.unwrap();
    BadgesComponent::new(pool.clone(), DEFAULT_ASSET_BASE_URL.to_string())
}

async fn audit(pool: &PgPool) -> Vec<(String, String, Option<String>, String)> {
    sqlx::query_as(
        "SELECT action, badge_id, tier_id, actor FROM badge_admin_audit \
         WHERE address = $1 ORDER BY id",
    )
    .bind(ADDR)
    .fetch_all(pool)
    .await
    .unwrap()
}

async fn progress(
    pool: &PgPool,
    badge_id: &str,
) -> Option<(i32, Option<DateTime<Utc>>, Option<String>, Option<String>)> {
    sqlx::query_as(
        "SELECT steps_done, completed_at, last_completed_tier_id, granted_by \
         FROM user_badge_progress WHERE address = $1 AND badge_id = $2",
    )
    .bind(ADDR)
    .bind(badge_id)
    .fetch_optional(pool)
    .await
    .unwrap()
}

async fn achieved(pool: &PgPool, badge_id: &str) -> Vec<(String, Option<String>)> {
    sqlx::query_as(
        "SELECT tier_id, granted_by FROM user_achieved_tiers \
         WHERE address = $1 AND badge_id = $2 ORDER BY completed_at, tier_id",
    )
    .bind(ADDR)
    .bind(badge_id)
    .fetch_all(pool)
    .await
    .unwrap()
}

fn status(err: &ApiError) -> u16 {
    match err {
        ApiError::Http { status, .. } => *status,
        other => panic!("unexpected error variant: {other:?}"),
    }
}

#[tokio::test]
async fn unknown_badge_is_false_and_writes_nothing() {
    let scratch = gate!();
    let badges = component(&scratch.pool).await;
    assert!(!badges
        .grant_badge(ADDR, "doesnotexist", None, ACTOR)
        .await
        .unwrap());
    assert!(!badges
        .revoke_badge(ADDR, "doesnotexist", ACTOR)
        .await
        .unwrap());
    assert!(audit(&scratch.pool).await.is_empty());
}

#[tokio::test]
async fn plain_badge_grant_is_idempotent_and_audited() {
    let scratch = gate!();
    let badges = component(&scratch.pool).await;
    assert!(badges
        .grant_badge(ADDR, "open_for_business", Some("ignored-on-plain"), ACTOR)
        .await
        .unwrap());
    let first = progress(&scratch.pool, "open_for_business")
        .await
        .expect("progress row");
    assert_eq!(first.0, 1);
    assert!(first.1.is_some());
    assert_eq!(first.2, None);
    assert_eq!(first.3.as_deref(), Some(ACTOR));
    assert!(achieved(&scratch.pool, "open_for_business")
        .await
        .is_empty());

    assert!(badges
        .grant_badge(ADDR, "open_for_business", None, "again")
        .await
        .unwrap());
    let second = progress(&scratch.pool, "open_for_business").await.unwrap();
    assert_eq!((second.0, second.1, second.2), (1, first.1, None));
    assert_eq!(second.3.as_deref(), Some("again"));

    assert_eq!(
        audit(&scratch.pool).await,
        vec![
            (
                "grant".into(),
                "open_for_business".into(),
                Some("ignored-on-plain".into()),
                ACTOR.into()
            ),
            (
                "grant".into(),
                "open_for_business".into(),
                None,
                "again".into()
            ),
        ]
    );

    let (ach, _) = badges.user_badges(ADDR, true).await.unwrap();
    assert_eq!(
        ach.iter().map(|b| b.id.as_str()).collect::<Vec<_>>(),
        vec!["open_for_business"]
    );
}

#[tokio::test]
async fn tiered_grant_resolves_the_top_tier_or_the_requested_one() {
    let scratch = gate!();
    let badges = component(&scratch.pool).await;

    assert!(badges
        .grant_badge(ADDR, "walkabout", None, ACTOR)
        .await
        .unwrap());
    let p = progress(&scratch.pool, "walkabout").await.unwrap();
    assert_eq!(
        (p.0, p.2.as_deref()),
        (1_000_000, Some("walkabout-gold")),
        "no tierId grants the highest ordinal"
    );
    assert_eq!(
        achieved(&scratch.pool, "walkabout").await,
        vec![("walkabout-gold".to_string(), Some(ACTOR.to_string()))]
    );

    assert!(badges
        .grant_badge(ADDR, "walkabout", Some("walkabout-starter"), "later")
        .await
        .unwrap());
    let p = progress(&scratch.pool, "walkabout").await.unwrap();
    assert_eq!(
        (p.0, p.2.as_deref(), p.3.as_deref()),
        (1_000_000, Some("walkabout-starter"), Some("later")),
        "steps never drop; the last granted tier wins"
    );
    assert_eq!(
        achieved(&scratch.pool, "walkabout")
            .await
            .into_iter()
            .map(|(t, _)| t)
            .collect::<Vec<_>>(),
        vec![
            "walkabout-gold".to_string(),
            "walkabout-starter".to_string()
        ]
    );

    let err = badges
        .grant_badge(ADDR, "walkabout", Some("walkabout-platinum"), ACTOR)
        .await
        .unwrap_err();
    assert_eq!(status(&err), 404);
    assert!(err.to_string().contains("walkabout-platinum"));

    sqlx::query(
        "INSERT INTO badge_definitions (id, name, is_tier) VALUES ('lonely', 'Lonely', true)",
    )
    .execute(&scratch.pool)
    .await
    .unwrap();
    let err = badges
        .grant_badge(ADDR, "lonely", None, ACTOR)
        .await
        .unwrap_err();
    assert_eq!(status(&err), 400);
    assert!(progress(&scratch.pool, "lonely").await.is_none());

    assert_eq!(
        audit(&scratch.pool)
            .await
            .into_iter()
            .map(|(a, b, t, _)| (a, b, t))
            .collect::<Vec<_>>(),
        vec![
            ("grant".to_string(), "walkabout".to_string(), None),
            (
                "grant".to_string(),
                "walkabout".to_string(),
                Some("walkabout-starter".to_string())
            ),
        ],
        "refused grants leave no audit row"
    );

    let (ach, _) = badges.user_badges(ADDR, false).await.unwrap();
    let walkabout = ach.iter().find(|b| b.id == "walkabout").expect("achieved");
    assert_eq!(
        walkabout
            .progress
            .achieved_tiers
            .iter()
            .map(|t| t.tier_id.as_str())
            .collect::<Vec<_>>(),
        vec!["walkabout-gold", "walkabout-starter"]
    );
    let latest = badges.latest_achieved(ADDR, 5).await.unwrap();
    assert_eq!(latest[0].id, "walkabout");
    assert_eq!(latest[0].tier_name.as_deref(), Some("Starter"));
}

#[tokio::test]
async fn revoke_clears_both_tables_and_audits_once() {
    let scratch = gate!();
    let badges = component(&scratch.pool).await;
    badges
        .grant_badge(ADDR, "walkabout", Some("walkabout-bronze"), ACTOR)
        .await
        .unwrap();
    badges
        .grant_badge(ADDR, "open_for_business", None, ACTOR)
        .await
        .unwrap();

    assert!(badges.revoke_badge(ADDR, "walkabout", "mod").await.unwrap());
    assert!(achieved(&scratch.pool, "walkabout").await.is_empty());
    assert!(progress(&scratch.pool, "walkabout").await.is_none());
    assert!(
        progress(&scratch.pool, "open_for_business").await.is_some(),
        "other badges untouched"
    );
    assert!(
        badges.revoke_badge(ADDR, "walkabout", "mod").await.unwrap(),
        "known badge with nothing to delete still reports true"
    );

    let rows = audit(&scratch.pool).await;
    assert_eq!(rows.len(), 4);
    assert_eq!(
        (
            rows[2].0.as_str(),
            rows[2].1.as_str(),
            rows[2].2.as_deref(),
            rows[2].3.as_str()
        ),
        ("revoke", "walkabout", None, "mod")
    );
    let (ach, _) = badges.user_badges(ADDR, false).await.unwrap();
    assert_eq!(
        ach.iter().map(|b| b.id.as_str()).collect::<Vec<_>>(),
        vec!["open_for_business"]
    );
}
