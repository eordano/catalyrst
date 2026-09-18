use catalyrst_contract_gate::pg::ScratchDb;
use catalyrst_credits::ports::credits::CreditsComponent;

#[tokio::test]
async fn progress_keeps_missing_rows_and_balances_in_one_query() {
    let Some(db) = ScratchDb::create("CATALYRST_TEST_PG", "credits_progress").await else {
        return;
    };
    sqlx::migrate!("./migrations").run(&db.pool).await.unwrap();
    sqlx::query("INSERT INTO user_program (address,has_started_program) VALUES ('program',true),('both',false)")
        .execute(&db.pool).await.unwrap();
    sqlx::query("INSERT INTO user_credits (address,available,earned_available,is_blocked_for_claiming) VALUES ('credit',12.5,2.5,true),('both',10,4,false)")
        .execute(&db.pool).await.unwrap();
    let credits = CreditsComponent::new(db.pool.clone());
    let capture = catalyrst_testgate::sql_capture::sql_capture();
    for (address, started, values) in [
        ("missing", false, None),
        ("program", true, None),
        ("credit", false, Some((12.5, 2.5, true))),
        ("both", false, Some((10., 4., false))),
    ] {
        capture.reset();
        let (actual_started, actual_credits) = credits.user_progress(address).await.unwrap();
        assert_eq!(capture.count(), 1);
        assert_eq!(actual_started, started);
        assert_eq!(
            actual_credits.map(|c| (c.available, c.earned_available, c.is_blocked_for_claiming)),
            values
        );
    }
    db.drop().await;
}
