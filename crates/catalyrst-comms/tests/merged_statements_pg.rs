//! Row shapes and outcomes of the statements that replaced multi-round-trip
//! sequences: the connection ban gate, ban creation with a device snapshot,
//! paged scene bans, private/community voice joins and teardown.

use catalyrst_comms::livekit::community_voice_chat_room_name;
use catalyrst_comms::ports::player_connection::{
    PlayerConnectionComponent, UpsertPlayerConnection,
};
use catalyrst_comms::ports::player_reports::{
    CreateReport, EvidenceRequest, PlayerReportsComponent,
};
use catalyrst_comms::ports::scene_bans::SceneBansComponent;
use catalyrst_comms::ports::user_bans::{BanWriteError, CreateBan, UserBansComponent};
use catalyrst_comms::voice_db::{
    DeleteRoomError, PrivateJoin, VoiceChatUserStatus, VoiceDb, VoiceDbConfig,
};
use catalyrst_contract_gate::pg::ScratchSchema;

const A: &str = "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const B: &str = "0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const C: &str = "0xcccccccccccccccccccccccccccccccccccccccc";
const D: &str = "0xdddddddddddddddddddddddddddddddddddddddd";
const E: &str = "0xeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee";
const MOD: &str = "0x9999999999999999999999999999999999999999";

async fn setup(prefix: &str) -> Option<ScratchSchema> {
    let scratch = ScratchSchema::create("CATALYRST_COMMS_TEST_PG", prefix).await?;
    for sql in [
        include_str!("../migrations/0001_comms.sql"),
        include_str!("../migrations/0002_user_moderation.sql"),
        include_str!("../migrations/0006_player_connection_and_device_bans.sql"),
        include_str!("../migrations/0007_community_voice_chat_sid.sql"),
        include_str!("../migrations/0008_player_reports.sql"),
    ] {
        scratch.apply_sql(sql).await;
    }
    Some(scratch)
}

fn ban(address: &str, device: Option<&str>) -> CreateBan {
    CreateBan {
        banned_address: address.into(),
        banned_by: MOD.into(),
        reason: "abuse".into(),
        custom_message: None,
        banned_device_id: device.map(String::from),
        duration_ms: None,
    }
}

async fn record_device(pc: &PlayerConnectionComponent, address: &str, device: &str) {
    pc.upsert(UpsertPlayerConnection {
        address: address.into(),
        ip_address: None,
        device_id: Some(device.into()),
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn connection_gate_answers_platform_and_scene_bans_in_one_row() {
    let Some(scratch) = setup("cg_comms_gate").await else {
        return;
    };
    let pool = scratch.pool.clone();
    let pc = PlayerConnectionComponent::new(pool.clone());
    let bans = UserBansComponent::new(pool.clone());
    let scene_bans = SceneBansComponent::new(pool.clone());

    record_device(&pc, A, "dev-a").await;
    bans.create_ban(ban(A, Some("dev-a"))).await.unwrap();
    scene_bans.ban("p1", B, MOD).await.unwrap();

    assert_eq!(
        bans.connection_gate(A, None, "p1").await.unwrap(),
        (true, false)
    );
    assert_eq!(
        bans.connection_gate(B, None, "p1").await.unwrap(),
        (false, true)
    );
    assert_eq!(
        bans.connection_gate(C, Some("dev-a"), "p1").await.unwrap(),
        (true, false),
        "a sent device id matches the banned device"
    );
    assert_eq!(
        bans.connection_gate(C, None, "p1").await.unwrap(),
        (false, false)
    );
    assert_eq!(
        bans.connection_gate(&A.to_uppercase(), Some(""), "p1")
            .await
            .unwrap(),
        (true, false),
        "an empty device id falls back to the recorded one; case is folded"
    );

    assert_eq!(
        bans.first_banned_for_connection(&[C, B, A]).await.unwrap(),
        Some(2),
        "the first banned address in request order"
    );
    assert_eq!(bans.first_banned_for_connection(&[]).await.unwrap(), None);
    assert_eq!(
        bans.first_banned_for_connection(&[C, B]).await.unwrap(),
        None
    );
    record_device(&pc, C, "dev-a").await;
    assert_eq!(
        bans.first_banned_for_connection(&[C, B]).await.unwrap(),
        Some(0),
        "a recorded device shared with a banned player counts"
    );

    scratch.drop().await;
}

#[tokio::test]
async fn ban_insert_snapshots_the_recorded_device_only_when_asked() {
    let Some(scratch) = setup("cg_comms_bansnap").await else {
        return;
    };
    let pool = scratch.pool.clone();
    let pc = PlayerConnectionComponent::new(pool.clone());
    let bans = UserBansComponent::new(pool.clone());

    record_device(&pc, D, "dev-d").await;
    record_device(&pc, E, "dev-e").await;

    let snap = bans
        .create_ban_with_recorded_device(ban(D, None))
        .await
        .unwrap();
    assert_eq!(snap.banned_device_id.as_deref(), Some("dev-d"));
    let plain = bans.create_ban(ban(E, None)).await.unwrap();
    assert_eq!(
        plain.banned_device_id, None,
        "create_ban keeps the caller's None"
    );

    match bans.create_ban_with_recorded_device(ban(D, None)).await {
        Err(BanWriteError::AlreadyBanned(addr)) => assert_eq!(addr, D),
        other => panic!("second ban must conflict, got {other:?}"),
    }
    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM user_bans WHERE banned_address = $1")
        .bind(D)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(rows, 1);

    scratch.drop().await;
}

#[tokio::test]
async fn scene_ban_page_carries_its_total_and_falls_back_past_the_end() {
    let Some(scratch) = setup("cg_comms_banpage").await else {
        return;
    };
    let scene_bans = SceneBansComponent::new(scratch.pool.clone());
    for addr in [A, B, C] {
        scene_bans.ban("p2", addr, MOD).await.unwrap();
    }

    let (page, total) = scene_bans
        .list_addresses_page_with_total("p2", 2, 0)
        .await
        .unwrap();
    assert_eq!((page.len(), total), (2, 3));
    let (page, total) = scene_bans
        .list_addresses_page_with_total("p2", 2, 10)
        .await
        .unwrap();
    assert_eq!(
        (page.len(), total),
        (0, 3),
        "an empty page past the end keeps the total"
    );
    let (page, total) = scene_bans
        .list_addresses_page_with_total("p-none", 2, 0)
        .await
        .unwrap();
    assert_eq!((page.len(), total), (0, 0));

    scratch.drop().await;
}

#[tokio::test]
async fn private_voice_join_resolves_activity_and_previous_room_in_one_statement() {
    let Some(scratch) = setup("cg_comms_pjoin").await else {
        return;
    };
    let pool = scratch.pool.clone();
    let db = VoiceDb::new(pool.clone(), VoiceDbConfig::default());

    db.create_voice_chat_room("room-ab", &[A.into(), B.into(), A.into()])
        .await
        .unwrap();
    let members: i64 =
        sqlx::query_scalar("SELECT count(*) FROM voice_chat_users WHERE room_name = 'room-ab'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(members, 2, "the unnest insert folds duplicate addresses");
    db.create_voice_chat_room("room-solo", &[E.into()])
        .await
        .unwrap();
    db.create_voice_chat_room("room-ac", &[A.into(), C.into()])
        .await
        .unwrap();
    sqlx::query(
        "UPDATE voice_chat_users SET status = $1 WHERE room_name = 'room-ac' AND address = $2",
    )
    .bind(VoiceChatUserStatus::Connected.as_str())
    .bind(A)
    .execute(&pool)
    .await
    .unwrap();

    assert_eq!(
        db.join_user_to_active_room(E, "room-solo").await.unwrap(),
        PrivateJoin::Inactive,
        "a one-user room is not active"
    );
    assert_eq!(
        db.join_user_to_active_room(D, "room-ab").await.unwrap(),
        PrivateJoin::NotInRoom
    );
    assert_eq!(
        db.join_user_to_active_room(A, "room-ab").await.unwrap(),
        PrivateJoin::Joined {
            old_room: "room-ac".into()
        },
        "the connected room wins over a pending one as the previous room"
    );
    let statuses: Vec<(String, String)> = sqlx::query_as(
        "SELECT room_name, status FROM voice_chat_users WHERE address = $1 ORDER BY room_name",
    )
    .bind(A)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        statuses,
        vec![
            (
                "room-ab".to_string(),
                VoiceChatUserStatus::Connected.as_str().to_string()
            ),
            (
                "room-ac".to_string(),
                VoiceChatUserStatus::Disconnected.as_str().to_string()
            ),
        ]
    );

    match db
        .delete_private_voice_chat_user_is_or_was_in("room-ab", D)
        .await
    {
        Err(DeleteRoomError::RoomDoesNotExist) => {}
        other => panic!("a stranger's room does not exist for them, got {other:?}"),
    }
    let mut deleted = db
        .delete_private_voice_chat_user_is_or_was_in("room-ab", A)
        .await
        .unwrap();
    deleted.sort();
    assert_eq!(deleted, vec![A.to_string(), B.to_string()]);

    scratch.drop().await;
}

#[tokio::test]
async fn community_voice_counts_and_teardown_report_row_counts() {
    let Some(scratch) = setup("cg_comms_cvoice").await else {
        return;
    };
    let pool = scratch.pool.clone();
    let db = VoiceDb::new(pool.clone(), VoiceDbConfig::default());
    let live = community_voice_chat_room_name("live");
    let orphan = community_voice_chat_room_name("orphan");
    for (room, addr, moderator) in [(&live, A, true), (&live, B, false), (&orphan, C, false)] {
        sqlx::query(
            "INSERT INTO community_voice_chat_users (address, room_name, is_moderator, status) \
             VALUES ($1, $2, $3, $4)",
        )
        .bind(addr)
        .bind(room)
        .bind(moderator)
        .bind(VoiceChatUserStatus::Connected.as_str())
        .execute(&pool)
        .await
        .unwrap();
    }

    let counts = db
        .get_active_community_voice_chat_participant_counts()
        .await
        .unwrap();
    assert_eq!(counts.get("live"), Some(&2));
    assert_eq!(
        counts.get("orphan"),
        None,
        "a room with no connected moderator is not active"
    );

    assert_eq!(db.delete_community_voice_chat(&live).await.unwrap(), 2);
    assert_eq!(db.delete_community_voice_chat(&live).await.unwrap(), 0);

    scratch.drop().await;
}

#[tokio::test]
async fn report_page_carries_its_total() {
    let Some(scratch) = setup("cg_comms_reppage").await else {
        return;
    };
    let reports = PlayerReportsComponent::new(scratch.pool.clone());
    let bytes = b"\x89PNG\r\n\x1a\nfake";
    let (report_id, slots) = reports
        .create_evidence_slots(
            A,
            &[EvidenceRequest {
                filename: "shot.png".into(),
                content_type: "image/png".into(),
                file_size: bytes.len() as i64,
            }],
        )
        .await
        .unwrap();
    reports
        .store_evidence(report_id, &slots[0].key, A, "image/png", bytes)
        .await
        .unwrap();
    reports
        .create_report(CreateReport {
            report_id,
            reporter: A.into(),
            reported: B.into(),
            reason: "harassment".into(),
            description: "spam".into(),
            additional_comments: None,
            evidence_keys: vec![slots[0].key.clone()],
        })
        .await
        .unwrap();

    let (page, total) = reports
        .list_reports_page(Some(B), None, 10, 0)
        .await
        .unwrap();
    assert_eq!((page.len(), total), (1, 1));
    assert_eq!(page[0].evidence_keys, vec![slots[0].key.clone()]);
    let (page, total) = reports
        .list_reports_page(Some(B), None, 10, 5)
        .await
        .unwrap();
    assert_eq!((page.len(), total), (0, 1));
    let (page, total) = reports
        .list_reports_page(Some(C), None, 10, 0)
        .await
        .unwrap();
    assert_eq!((page.len(), total), (0, 0));
    let (report, evidence) = reports
        .get_report_with_evidence(report_id)
        .await
        .unwrap()
        .expect("report");
    assert_eq!(report.reported_address, B);
    assert!(evidence[0].uploaded);

    scratch.drop().await;
}
