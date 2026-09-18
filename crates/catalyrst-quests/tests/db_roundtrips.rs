//! PG-gated: each test builds the quests schema in its own search_path on CATALYRST_TEST_PG.
use catalyrst_quests::db::{
    CreateQuest, CreateReward, CreateRewardHook, CreateRewardItem, Db, DbError, ToggleOutcome,
    UpdateOutcome,
};
use catalyrst_quests::proto::{ProtocolMessage, QuestDefinition};
use sqlx::postgres::PgPoolOptions;
use uuid::Uuid;

const CREATOR: &str = "0xcreator";
const OTHER: &str = "0xother";

async fn db(label: &str) -> Option<Db> {
    let Ok(url) = std::env::var("CATALYRST_TEST_PG") else {
        eprintln!("CATALYRST_TEST_PG unset; skipping {label}");
        return None;
    };
    let schema = format!("quests_rt_{}_{}", label, std::process::id());
    let pool = PgPoolOptions::new()
        .max_connections(2)
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
    Some(Db::from_pool(pool).await.expect("schema"))
}

fn quest(with_reward: bool) -> CreateQuest {
    CreateQuest {
        name: "A quest".into(),
        description: "A description".into(),
        image_url: "https://img.example/x.png".into(),
        definition: QuestDefinition::default().encode_to_vec(),
        reward: with_reward.then(|| CreateReward {
            hook: CreateRewardHook {
                webhook_url: "https://hooks.example/x".into(),
                request_body: Some(serde_json::json!({ "k": "{user_address}" })),
            },
            items: vec![
                CreateRewardItem {
                    name: "hat".into(),
                    image_link: "https://img.example/hat.png".into(),
                },
                CreateRewardItem {
                    name: "cape".into(),
                    image_link: "https://img.example/cape.png".into(),
                },
            ],
        }),
    }
}

#[tokio::test]
async fn reward_join_reports_items_hook_and_creator() {
    let Some(db) = db("reward").await else { return };
    let id = db.create_quest(&quest(true), CREATOR).await.unwrap();

    let r = db.get_quest_reward(&id, Some(CREATOR)).await.unwrap();
    assert_eq!(r.items.len(), 2);
    assert!(r.is_creator);
    let hook = r.hook.expect("hook");
    assert_eq!(hook.webhook_url, "https://hooks.example/x");
    assert_eq!(hook.request_body.unwrap()["k"], "{user_address}");

    let r = db.get_quest_reward(&id, Some(OTHER)).await.unwrap();
    assert!(!r.is_creator);
    let r = db.get_quest_reward(&id, None).await.unwrap();
    assert!(!r.is_creator);

    let bare = db.create_quest(&quest(false), CREATOR).await.unwrap();
    let r = db.get_quest_reward(&bare, Some(CREATOR)).await.unwrap();
    assert!(r.items.is_empty() && r.hook.is_none());
    assert!(matches!(
        db.get_quest_reward_hook(&bare).await,
        Err(DbError::NotFound)
    ));
}

#[tokio::test]
async fn stats_aggregate_matches_per_instance_semantics() {
    let Some(db) = db("stats").await else { return };
    let id = db.create_quest(&quest(false), CREATOR).await.unwrap();
    let a = db.start_quest(&id, "0xa").await.unwrap();
    let b = db.start_quest(&id, "0xb").await.unwrap();
    let c = db.start_quest(&id, "0xc").await.unwrap();
    db.abandon_quest_instance(&c).await.unwrap();
    db.complete_quest_instance(&a).await.unwrap();
    db.complete_quest_instance(&c).await.unwrap();

    let since = chrono::Utc::now().naive_utc() - chrono::Duration::hours(24);
    let s = db.quest_stats(&id, CREATOR, since).await.unwrap().unwrap();
    assert!(s.is_creator);
    assert_eq!((s.active, s.abandoned, s.completed), (2, 1, 1));
    assert_eq!(s.started_since, 2);

    let future = chrono::Utc::now().naive_utc() + chrono::Duration::hours(1);
    let s = db.quest_stats(&id, OTHER, future).await.unwrap().unwrap();
    assert!(!s.is_creator);
    assert_eq!(s.started_since, 0);
    assert!(db
        .quest_stats(&Uuid::new_v4().to_string(), CREATOR, since)
        .await
        .unwrap()
        .is_none());
    let _ = b;
}

#[tokio::test]
async fn update_and_toggle_outcomes_are_distinguishable() {
    let Some(db) = db("update").await else { return };
    let id = db.create_quest(&quest(true), CREATOR).await.unwrap();

    assert_eq!(
        db.update_quest(&id, &quest(false), OTHER).await.unwrap(),
        UpdateOutcome::NotCreator
    );
    let UpdateOutcome::Updated(new_id) = db.update_quest(&id, &quest(true), CREATOR).await.unwrap()
    else {
        panic!("creator update must succeed")
    };
    assert_ne!(new_id, id);
    assert!(!db.get_stored_quest(&id).await.unwrap().active);
    assert!(db.get_stored_quest(&new_id).await.unwrap().active);
    assert_eq!(
        db.get_old_quest_versions(&new_id).await.unwrap(),
        vec![id.clone()]
    );
    assert_eq!(
        db.get_quest_reward(&new_id, None)
            .await
            .unwrap()
            .items
            .len(),
        2
    );
    assert_eq!(
        db.update_quest(&id, &quest(false), CREATOR).await.unwrap(),
        UpdateOutcome::NotUpdatable
    );
    assert_eq!(
        db.activate_quest(&id, CREATOR).await.unwrap(),
        ToggleOutcome::NotApplicable
    );

    assert_eq!(
        db.deactivate_quest(&new_id, OTHER).await.unwrap(),
        ToggleOutcome::NotCreator
    );
    assert_eq!(
        db.activate_quest(&new_id, CREATOR).await.unwrap(),
        ToggleOutcome::NotApplicable
    );
    assert_eq!(
        db.deactivate_quest(&new_id, CREATOR).await.unwrap(),
        ToggleOutcome::Done
    );
    assert_eq!(
        db.deactivate_quest(&new_id, CREATOR).await.unwrap(),
        ToggleOutcome::NotApplicable
    );
    assert!(!db.is_active_quest(&new_id).await.unwrap());
    assert_eq!(
        db.activate_quest(&new_id, CREATOR).await.unwrap(),
        ToggleOutcome::Done
    );
    assert!(db.is_active_quest(&new_id).await.unwrap());
}

#[tokio::test]
async fn instance_join_events_and_resets() {
    let Some(db) = db("instances").await else {
        return;
    };
    let id = db.create_quest(&quest(false), CREATOR).await.unwrap();
    let inst = db.start_quest(&id, "0xa").await.unwrap();

    let (instance, stored) = db.get_quest_instance_with_quest(&inst).await.unwrap();
    assert_eq!(instance.id, inst);
    assert_eq!(instance.quest_id, id);
    assert_eq!(stored.id, id);
    assert_eq!(stored.creator_address, CREATOR);
    assert!(stored.active);
    assert!(matches!(
        db.get_quest_instance_with_quest(&Uuid::new_v4().to_string())
            .await,
        Err(DbError::NotFound)
    ));

    let e1 = Uuid::new_v4().to_string();
    let e2 = Uuid::new_v4().to_string();
    db.add_event(&e1, "0xa", b"one", &inst).await.unwrap();
    db.add_event(&e2, "0xa", b"two", &inst).await.unwrap();
    db.complete_quest_instance(&inst).await.unwrap();

    assert!(matches!(
        db.remove_event(&Uuid::new_v4().to_string(), &inst).await,
        Err(DbError::NotFound)
    ));
    let completed: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM completed_quest_instances WHERE quest_instance_id = $1)",
    )
    .bind(Uuid::parse_str(&inst).unwrap())
    .fetch_one(db_pool(&db))
    .await
    .unwrap();
    assert!(completed, "a missing event must not clear the completion");

    db.remove_event(&e1, &inst).await.unwrap();
    assert_eq!(db.get_events(&inst).await.unwrap().len(), 1);
    let completed: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM completed_quest_instances WHERE quest_instance_id = $1)",
    )
    .bind(Uuid::parse_str(&inst).unwrap())
    .fetch_one(db_pool(&db))
    .await
    .unwrap();
    assert!(!completed);

    db.complete_quest_instance(&inst).await.unwrap();
    db.reset_quest_instance(&inst).await.unwrap();
    assert!(db.get_events(&inst).await.unwrap().is_empty());
    let completed: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM completed_quest_instances WHERE quest_instance_id = $1)",
    )
    .bind(Uuid::parse_str(&inst).unwrap())
    .fetch_one(db_pool(&db))
    .await
    .unwrap();
    assert!(!completed);
}

#[tokio::test]
async fn pages_carry_totals_and_fall_back_past_the_end() {
    let Some(db) = db("pages").await else { return };
    let mut ids = Vec::new();
    for _ in 0..3 {
        ids.push(db.create_quest(&quest(false), CREATOR).await.unwrap());
    }
    db.deactivate_quest(&ids[0], CREATOR).await.unwrap();

    let (page, total) = db.get_active_quests_page(0, 1).await.unwrap();
    assert_eq!((page.len(), total), (1, 2));
    let (page, total) = db.get_active_quests_page(10, 1).await.unwrap();
    assert_eq!((page.len(), total), (0, 2));

    let (page, total) = db.get_quests_by_creator_page(CREATOR, 0, 2).await.unwrap();
    assert_eq!((page.len(), total), (2, 3));
    let (page, total) = db.get_quests_by_creator_page(CREATOR, 5, 2).await.unwrap();
    assert_eq!((page.len(), total), (0, 3));
    let (page, total) = db.get_quests_by_creator_page(OTHER, 0, 2).await.unwrap();
    assert_eq!((page.len(), total), (0, 0));

    let a = db.start_quest(&ids[1], "0xa").await.unwrap();
    db.start_quest(&ids[1], "0xb").await.unwrap();
    db.abandon_quest_instance(&a).await.unwrap();
    let (page, total) = db
        .get_active_quest_instances_by_quest_id_page(&ids[1], 0, 10)
        .await
        .unwrap();
    assert_eq!((page.len(), total), (1, 1));
    let (page, total) = db
        .get_active_quest_instances_by_quest_id_page(&ids[1], 3, 10)
        .await
        .unwrap();
    assert_eq!((page.len(), total), (0, 1));
}

fn db_pool(db: &Db) -> &sqlx::PgPool {
    db.pool()
}
