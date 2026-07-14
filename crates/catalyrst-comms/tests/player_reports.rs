use catalyrst_comms::ports::player_reports::{
    CreateReport, EvidenceRequest, EvidenceUploadError, PlayerReportsComponent, ReportWriteError,
    MAX_PENDING_EVIDENCE_SLOTS,
};
use catalyrst_contract_gate::pg::ScratchSchema;

const REPORTER: &str = "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const OTHER: &str = "0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const TARGET: &str = "0xcccccccccccccccccccccccccccccccccccccccc";

const PNG: &[u8] = b"\x89PNG\r\n\x1a\nfake-evidence-bytes";

async fn setup() -> Option<ScratchSchema> {
    let scratch = ScratchSchema::create("CATALYRST_COMMS_TEST_PG", "cg_comms_reports").await?;
    scratch
        .apply_sql(include_str!("../migrations/0008_player_reports.sql"))
        .await;
    Some(scratch)
}

fn one_file() -> Vec<EvidenceRequest> {
    vec![EvidenceRequest {
        filename: "shot.png".into(),
        content_type: "image/png".into(),
        file_size: PNG.len() as i64,
    }]
}

fn draft(report_id: uuid::Uuid, keys: Vec<String>) -> CreateReport {
    CreateReport {
        report_id,
        reporter: REPORTER.into(),
        reported: TARGET.into(),
        reason: "harassment".into(),
        description: "followed me across scenes".into(),
        additional_comments: Some("logs attached".into()),
        evidence_keys: keys,
    }
}

#[tokio::test]
async fn report_round_trips_from_presign_through_moderator_read() {
    let Some(scratch) = setup().await else {
        return;
    };
    let reports = PlayerReportsComponent::new(scratch.pool.clone());

    let (report_id, slots) = reports
        .create_evidence_slots(REPORTER, &one_file())
        .await
        .expect("presign");
    assert_eq!(slots.len(), 1);
    assert_eq!(slots[0].key, "0-shot.png");
    assert!(!slots[0].uploaded);

    reports
        .store_evidence(report_id, &slots[0].key, REPORTER, "image/png", PNG)
        .await
        .expect("upload");

    let created = reports
        .create_report(draft(report_id, vec![slots[0].key.clone()]))
        .await
        .expect("create");
    assert_eq!(created.id, report_id.to_string());
    assert_eq!(created.reporter_address, REPORTER);
    assert_eq!(created.reported_address, TARGET);
    assert_eq!(created.evidence_keys, vec!["0-shot.png".to_string()]);
    assert_eq!(created.status, "open");

    let fetched = reports
        .get_report(report_id)
        .await
        .expect("get")
        .expect("report exists");
    assert_eq!(fetched.description, "followed me across scenes");
    assert_eq!(
        fetched.additional_comments.as_deref(),
        Some("logs attached")
    );

    let listed = reports
        .list_reports(Some(TARGET), None, 100, 0)
        .await
        .expect("list");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, report_id.to_string());
    assert_eq!(reports.count_reports(Some(TARGET), None).await.unwrap(), 1);
    assert_eq!(reports.count_reports(Some(OTHER), None).await.unwrap(), 0);

    let evidence = reports.list_evidence(report_id).await.expect("evidence");
    assert_eq!(evidence.len(), 1);
    assert!(evidence[0].uploaded);

    let blob = reports
        .load_evidence(report_id, "0-shot.png")
        .await
        .expect("load")
        .expect("bytes stored");
    assert_eq!(blob.content, PNG);
    assert_eq!(blob.content_type, "image/png");
    assert_eq!(blob.filename, "shot.png");

    scratch.drop().await;
}

#[tokio::test]
async fn report_without_uploaded_evidence_is_rejected() {
    let Some(scratch) = setup().await else {
        return;
    };
    let reports = PlayerReportsComponent::new(scratch.pool.clone());

    let (report_id, slots) = reports
        .create_evidence_slots(REPORTER, &one_file())
        .await
        .expect("presign");

    match reports
        .create_report(draft(report_id, vec![slots[0].key.clone()]))
        .await
    {
        Err(ReportWriteError::EvidenceMissing(key)) => assert_eq!(key, "0-shot.png"),
        other => panic!("expected EvidenceMissing, got {other:?}"),
    }

    let stored = reports.count_reports(None, None).await.unwrap();
    assert_eq!(
        stored, 0,
        "a report with unuploaded evidence must not persist"
    );

    scratch.drop().await;
}

#[tokio::test]
async fn evidence_slots_are_bound_to_the_signer_that_presigned_them() {
    let Some(scratch) = setup().await else {
        return;
    };
    let reports = PlayerReportsComponent::new(scratch.pool.clone());

    let (report_id, slots) = reports
        .create_evidence_slots(REPORTER, &one_file())
        .await
        .expect("presign");

    match reports
        .store_evidence(report_id, &slots[0].key, OTHER, "image/png", PNG)
        .await
    {
        Err(EvidenceUploadError::NotOwner) => {}
        other => panic!("expected NotOwner, got {other:?}"),
    }

    reports
        .store_evidence(report_id, &slots[0].key, REPORTER, "image/png", PNG)
        .await
        .expect("owner upload");

    let mut hijack = draft(report_id, vec![slots[0].key.clone()]);
    hijack.reporter = OTHER.into();
    match reports.create_report(hijack).await {
        Err(ReportWriteError::NotOwner) => {}
        other => panic!("expected NotOwner, got {other:?}"),
    }

    scratch.drop().await;
}

#[tokio::test]
async fn evidence_upload_enforces_declared_size_type_and_single_write() {
    let Some(scratch) = setup().await else {
        return;
    };
    let reports = PlayerReportsComponent::new(scratch.pool.clone());

    let (report_id, slots) = reports
        .create_evidence_slots(REPORTER, &one_file())
        .await
        .expect("presign");
    let key = slots[0].key.clone();

    match reports
        .store_evidence(report_id, &key, REPORTER, "image/png", b"short")
        .await
    {
        Err(EvidenceUploadError::SizeMismatch { expected, actual }) => {
            assert_eq!(expected, PNG.len() as i64);
            assert_eq!(actual, 5);
        }
        other => panic!("expected SizeMismatch, got {other:?}"),
    }

    match reports
        .store_evidence(report_id, &key, REPORTER, "application/pdf", PNG)
        .await
    {
        Err(EvidenceUploadError::ContentTypeMismatch { expected }) => {
            assert_eq!(expected, "image/png")
        }
        other => panic!("expected ContentTypeMismatch, got {other:?}"),
    }

    match reports
        .store_evidence(report_id, "0-nope.png", REPORTER, "image/png", PNG)
        .await
    {
        Err(EvidenceUploadError::UnknownSlot) => {}
        other => panic!("expected UnknownSlot, got {other:?}"),
    }

    reports
        .store_evidence(report_id, &key, REPORTER, "image/png", PNG)
        .await
        .expect("first upload");
    match reports
        .store_evidence(report_id, &key, REPORTER, "image/png", PNG)
        .await
    {
        Err(EvidenceUploadError::AlreadyUploaded) => {}
        other => panic!("expected AlreadyUploaded, got {other:?}"),
    }

    scratch.drop().await;
}

#[tokio::test]
async fn a_report_id_can_only_be_submitted_once() {
    let Some(scratch) = setup().await else {
        return;
    };
    let reports = PlayerReportsComponent::new(scratch.pool.clone());

    let (report_id, slots) = reports
        .create_evidence_slots(REPORTER, &one_file())
        .await
        .expect("presign");
    reports
        .store_evidence(report_id, &slots[0].key, REPORTER, "image/png", PNG)
        .await
        .expect("upload");
    reports
        .create_report(draft(report_id, vec![slots[0].key.clone()]))
        .await
        .expect("create");

    match reports.create_report(draft(report_id, vec![])).await {
        Err(ReportWriteError::AlreadySubmitted) => {}
        other => panic!("expected AlreadySubmitted, got {other:?}"),
    }
    assert_eq!(reports.count_reports(None, None).await.unwrap(), 1);

    match reports
        .create_report(draft(uuid::Uuid::new_v4(), vec![]))
        .await
    {
        Err(ReportWriteError::UnknownReport) => {}
        other => panic!("expected UnknownReport, got {other:?}"),
    }

    scratch.drop().await;
}

#[tokio::test]
async fn unclaimed_evidence_slots_are_capped_per_reporter() {
    let Some(scratch) = setup().await else {
        return;
    };
    let reports = PlayerReportsComponent::new(scratch.pool.clone());

    let mut minted = 0i64;
    loop {
        match reports.create_evidence_slots(REPORTER, &one_file()).await {
            Ok(_) => minted += 1,
            Err(_) => break,
        }
        assert!(minted <= MAX_PENDING_EVIDENCE_SLOTS, "cap never tripped");
    }
    assert_eq!(minted, MAX_PENDING_EVIDENCE_SLOTS);

    reports
        .create_evidence_slots(OTHER, &one_file())
        .await
        .expect("the cap is per reporter");

    scratch.drop().await;
}
