use super::*;
use catalyrst_pulse::application_relay::auth::PROOF_DOMAIN;
use std::sync::atomic::{AtomicUsize, Ordering};
use tower::ServiceExt;
use wire::{authority_request::Operation, authority_response::Result as Reply};

const SERVICE_KEY: [u8; 32] = [9; 32];
const LIVEKIT_SECRET: [u8; 32] = [7; 32];

#[test]
fn replay_capacity_covers_one_hertz_renewals_plus_a_full_lease_burst() {
    assert!(MAX_REPLAYS >= MAX_LEASES * (REPLAY_TTL.as_secs() as usize + 1));
    assert_eq!(MAX_REPLAYS, 262_144);
}

#[derive(Default)]
struct Policy {
    mode: AtomicUsize,
    calls: AtomicUsize,
    entered: tokio::sync::Notify,
    release: tokio::sync::Notify,
}

#[async_trait]
impl RelayPolicy for Policy {
    async fn authorize(&self, room: &VerifiedRoom) -> Result<(), DenialReason> {
        assert_eq!(room.wallet, format!("0x{}", "11".repeat(20)));
        assert_eq!(room.session, format!("0x{}", "22".repeat(20)));
        assert_eq!(room.nonce, [3; 32]);
        assert_eq!(room.proof.len(), 32);
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.entered.notify_one();
        match self.mode.load(Ordering::SeqCst) {
            1 => Err(DenialReason::NotAuthorized),
            2 => Err(DenialReason::Unavailable),
            3 => {
                self.release.notified().await;
                Ok(())
            }
            4 => {
                std::future::pending::<()>().await;
                Ok(())
            }
            _ => Ok(()),
        }
    }
}

fn authority() -> (Arc<RelayAuthority>, Arc<Policy>) {
    let policy = Arc::new(Policy::default());
    let authority = RelayAuthority::new(
        policy.clone(),
        "livekit-key".into(),
        LIVEKIT_SECRET.to_vec(),
        SERVICE_KEY,
    )
    .unwrap();
    (Arc::new(authority), policy)
}

fn join_with_claims(claims: serde_json::Value) -> wire::Join {
    let hp = format!(
        "{}.{}",
        URL_SAFE_NO_PAD.encode(br#"{"alg":"HS256"}"#),
        URL_SAFE_NO_PAD.encode(serde_json::to_vec(&claims).unwrap())
    );
    let mut signature = HmacSha256::new_from_slice(&LIVEKIT_SECRET).unwrap();
    signature.update(hp.as_bytes());
    let mut proof = HmacSha256::new_from_slice(&signature.finalize().into_bytes()).unwrap();
    proof.update(PROOF_DOMAIN);
    proof.update(&[3; 32]);
    proof.update(&[0x11; 20]);
    proof.update(&[0x22; 20]);
    proof.update(&Sha256::digest(hp.as_bytes()));
    wire::Join {
        pulse_instance: vec![1; 16],
        connection_generation: vec![2; 16],
        wallet: vec![0x11; 20],
        session: vec![0x22; 20],
        header_payload: hp,
        nonce: vec![3; 32],
        proof: proof.finalize().into_bytes().to_vec(),
    }
}

fn claims(exp: u64) -> serde_json::Value {
    serde_json::json!({
        "iss": "livekit-key", "sub": format!("0x{}", "11".repeat(20)), "exp": exp,
        "video": {"room": "scene-room", "roomJoin": true, "canPublishData": true}
    })
}

fn request(operation: Operation) -> wire::AuthorityRequest {
    wire::AuthorityRequest {
        operation: Some(operation),
    }
}

async fn execute(authority: &RelayAuthority, operation: Operation, now: u64) -> Reply {
    authority
        .execute(request(operation), Duration::from_secs(now), Instant::now())
        .await
        .result
        .unwrap()
}

fn lease(reply: Reply) -> wire::Lease {
    match reply {
        Reply::Lease(lease) => lease,
        _ => panic!("expected lease"),
    }
}

fn assert_denied(reply: Reply, expected: DenialReason) {
    match reply {
        Reply::Denied(denial) => assert_eq!(denial.reason, expected as i32),
        _ => panic!("expected denial"),
    }
}

fn renewal(lease: &wire::Lease) -> wire::Renew {
    wire::Renew {
        pulse_instance: vec![1; 16],
        connection_generation: vec![2; 16],
        lease_id: lease.lease_id.clone(),
        authority_incarnation: lease.authority_incarnation.clone(),
    }
}

fn release(renew: &wire::Renew) -> wire::Release {
    wire::Release {
        pulse_instance: renew.pulse_instance.clone(),
        connection_generation: renew.connection_generation.clone(),
        lease_id: renew.lease_id.clone(),
        authority_incarnation: renew.authority_incarnation.clone(),
    }
}

fn signed_headers(
    body: &[u8],
    timestamp: &str,
    nonce: [u8; 16],
    method: &str,
    path: &str,
) -> HeaderMap {
    let nonce = URL_SAFE_NO_PAD.encode(nonce);
    let preimage = format!(
        "{method}\n{path}\n{timestamp}\n{nonce}\n{}",
        hex::encode(Sha256::digest(body))
    );
    let mut mac = HmacSha256::new_from_slice(&SERVICE_KEY).unwrap();
    mac.update(preimage.as_bytes());
    let mut headers = HeaderMap::new();
    headers.insert("x-pulse-authority-timestamp", timestamp.parse().unwrap());
    headers.insert("x-pulse-authority-nonce", nonce.parse().unwrap());
    headers.insert(
        "x-pulse-authority-signature",
        URL_SAFE_NO_PAD
            .encode(mac.finalize().into_bytes())
            .parse()
            .unwrap(),
    );
    headers.insert(header::CONTENT_TYPE, CONTENT_TYPE.parse().unwrap());
    headers.insert(header::ACCEPT, CONTENT_TYPE.parse().unwrap());
    headers
}

fn http_request(body: Vec<u8>, nonce: [u8; 16]) -> Request<Body> {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
        .to_string();
    let headers = signed_headers(&body, &timestamp, nonce, "POST", ENDPOINT);
    let mut request = Request::builder()
        .method("POST")
        .uri(ENDPOINT)
        .body(Body::from(body))
        .unwrap();
    *request.headers_mut() = headers;
    request
}

async fn reply(response: Response) -> Reply {
    assert_eq!(response.headers()[header::CONTENT_TYPE], CONTENT_TYPE);
    assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
    let bytes = to_bytes(response.into_body(), MAX_BODY).await.unwrap();
    wire::AuthorityResponse::decode(bytes)
        .unwrap()
        .result
        .unwrap()
}

#[tokio::test]
async fn protobuf_endpoint_independently_verifies_proof_and_service_authentication() {
    let (authority, policy) = authority();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let join = join_with_claims(claims(now + 60));
    let body = request(Operation::Join(join)).encode_to_vec();
    let app = router::<()>(authority.clone());
    let response = app
        .clone()
        .oneshot(http_request(body.clone(), [1; 16]))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let granted = lease(reply(response).await);
    assert_eq!(granted.ttl_ms, 2000);
    assert_eq!(granted.lease_id.len(), 16);
    assert_eq!(granted.authority_incarnation.len(), 16);
    assert_eq!(policy.calls.load(Ordering::SeqCst), 1);
    let replay = app.oneshot(http_request(body, [1; 16])).await.unwrap();
    assert_eq!(replay.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(policy.calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn authentication_binds_raw_body_method_path_timestamp_and_canonical_headers() {
    let (authority, _) = authority();
    let body = [10, 0];
    for (index, (method, path, timestamp, received_body)) in [
        ("GET", ENDPOINT, "100", body.as_slice()),
        ("POST", "/wrong", "100", body.as_slice()),
        ("POST", ENDPOINT, "100", &[10, 1][..]),
        ("POST", ENDPOINT, "0100", body.as_slice()),
        ("POST", ENDPOINT, "+100", body.as_slice()),
        ("POST", ENDPOINT, "94", body.as_slice()),
        ("POST", ENDPOINT, "106", body.as_slice()),
    ]
    .into_iter()
    .enumerate()
    {
        let headers = signed_headers(&body, timestamp, [index as u8; 16], method, path);
        assert!(authority
            .authenticate(&headers, received_body, 100, Instant::now())
            .is_err());
    }
    for timestamp in ["95", "100", "105"] {
        let headers = signed_headers(
            &body,
            timestamp,
            [timestamp.parse::<u8>().unwrap(); 16],
            "POST",
            ENDPOINT,
        );
        assert!(authority
            .authenticate(&headers, &body, 100, Instant::now())
            .is_ok());
    }
    let mut duplicate = signed_headers(&body, "100", [9; 16], "POST", ENDPOINT);
    duplicate.append("x-pulse-authority-timestamp", "100".parse().unwrap());
    assert!(authority
        .authenticate(&duplicate, &body, 100, Instant::now())
        .is_err());
    let mut padded = signed_headers(&body, "100", [10; 16], "POST", ENDPOINT);
    let nonce = format!("{}==", padded["x-pulse-authority-nonce"].to_str().unwrap());
    padded.insert("x-pulse-authority-nonce", nonce.parse().unwrap());
    assert!(authority
        .authenticate(&padded, &body, 100, Instant::now())
        .is_err());
}

#[tokio::test(start_paused = true)]
async fn replay_registry_is_bounded_and_does_not_evict_live_nonces() {
    let (authority, _) = authority();
    let now = Instant::now();
    {
        let mut entries = authority.replays.lock().unwrap();
        for id in 0..MAX_REPLAYS {
            entries.insert((id as u128).to_be_bytes(), now + REPLAY_TTL);
        }
    }
    let headers = signed_headers(&[], "100", [0xff; 16], "POST", ENDPOINT);
    assert_eq!(
        authority.authenticate(&headers, &[], 100, now),
        Err(DenialReason::Capacity)
    );
    assert_eq!(authority.replays.lock().unwrap().len(), MAX_REPLAYS);
    tokio::time::advance(REPLAY_TTL).await;
    assert!(authority
        .authenticate(&headers, &[], 100, Instant::now())
        .is_ok());
    assert_eq!(authority.replays.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn malformed_frames_and_wrong_http_contract_fail_closed() {
    let (authority, policy) = authority();
    for method in [Method::GET, Method::PUT] {
        let mut request = http_request(vec![], [1; 16]);
        *request.method_mut() = method;
        assert_eq!(
            authority.handle(request).await.status(),
            StatusCode::METHOD_NOT_ALLOWED
        );
    }
    let mut wrong_path = http_request(vec![], [2; 16]);
    *wrong_path.uri_mut() = format!("{ENDPOINT}?alias=1").parse().unwrap();
    assert_eq!(
        authority.handle(wrong_path).await.status(),
        StatusCode::NOT_FOUND
    );
    let mut wrong_type = http_request(vec![], [3; 16]);
    wrong_type
        .headers_mut()
        .insert(header::CONTENT_TYPE, "application/json".parse().unwrap());
    assert_eq!(
        authority.handle(wrong_type).await.status(),
        StatusCode::UNSUPPORTED_MEDIA_TYPE
    );
    let too_large = http_request(vec![0; MAX_BODY + 1], [4; 16]);
    assert_eq!(
        authority.handle(too_large).await.status(),
        StatusCode::PAYLOAD_TOO_LARGE
    );
    let malformed = authority.handle(http_request(vec![0xff], [5; 16])).await;
    assert_eq!(malformed.status(), StatusCode::BAD_REQUEST);
    let empty = authority.handle(http_request(vec![], [6; 16])).await;
    assert_denied(reply(empty).await, DenialReason::Invalid);
    assert_eq!(policy.calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn join_never_bypasses_expiry_grants_proof_or_identity() {
    let (authority, policy) = authority();
    let mut invalid_claims = vec![];
    let mut value = claims(100);
    value["exp"] = 100.into();
    invalid_claims.push(value);
    let mut value = claims(200);
    value["iss"] = "wrong".into();
    invalid_claims.push(value);
    let mut value = claims(200);
    value["video"]["roomJoin"] = false.into();
    invalid_claims.push(value);
    for value in invalid_claims {
        assert_denied(
            execute(&authority, Operation::Join(join_with_claims(value)), 100).await,
            DenialReason::NotAuthorized,
        );
    }
    for mutate in [
        (|j: &mut wire::Join| j.proof[0] ^= 1) as fn(&mut wire::Join),
        |j| j.wallet[0] ^= 1,
        |j| j.session[0] ^= 1,
        |j| j.nonce[0] ^= 1,
        |j| j.header_payload.push('x'),
    ] {
        let mut join = join_with_claims(claims(200));
        mutate(&mut join);
        assert_denied(
            execute(&authority, Operation::Join(join), 100).await,
            DenialReason::NotAuthorized,
        );
    }
    let mut malformed = join_with_claims(claims(200));
    malformed.connection_generation.pop();
    assert_denied(
        execute(&authority, Operation::Join(malformed), 100).await,
        DenialReason::Invalid,
    );
    assert_eq!(policy.calls.load(Ordering::SeqCst), 0);
}

#[tokio::test(start_paused = true)]
async fn live_lease_can_outlive_jwt_but_expired_lease_and_rejoin_cannot() {
    let (authority, _) = authority();
    let original = join_with_claims(claims(101));
    let first = lease(execute(&authority, Operation::Join(original.clone()), 100).await);
    tokio::time::advance(Duration::from_millis(1500)).await;
    let renewed = lease(execute(&authority, Operation::Renew(renewal(&first)), 102).await);
    assert_eq!(first.lease_id, renewed.lease_id);
    assert_denied(
        execute(&authority, Operation::Join(original), 102).await,
        DenialReason::NotAuthorized,
    );
    tokio::time::advance(LEASE_TTL).await;
    assert_denied(
        execute(&authority, Operation::Renew(renewal(&first)), 104).await,
        DenialReason::Expired,
    );
    assert!(authority.leases.lock().unwrap().entries.is_empty());
}

#[tokio::test]
async fn policy_change_or_dependency_error_revokes_exact_lease() {
    for (mode, reason) in [
        (1, DenialReason::NotAuthorized),
        (2, DenialReason::Unavailable),
    ] {
        let (authority, policy) = authority();
        let first = lease(
            execute(
                &authority,
                Operation::Join(join_with_claims(claims(200))),
                100,
            )
            .await,
        );
        policy.mode.store(mode, Ordering::SeqCst);
        assert_denied(
            execute(&authority, Operation::Renew(renewal(&first)), 100).await,
            reason,
        );
        policy.mode.store(0, Ordering::SeqCst);
        assert_denied(
            execute(&authority, Operation::Renew(renewal(&first)), 100).await,
            DenialReason::Expired,
        );
    }
}

#[tokio::test]
async fn incarnation_instance_and_generation_bind_renewal_and_release() {
    let (authority, _) = authority();
    let first = lease(
        execute(
            &authority,
            Operation::Join(join_with_claims(claims(200))),
            100,
        )
        .await,
    );
    let (restarted, _) = super::tests::authority();
    assert_denied(
        execute(&restarted, Operation::Renew(renewal(&first)), 100).await,
        DenialReason::Expired,
    );
    for field in 0..3 {
        let mut changed = renewal(&first);
        match field {
            0 => changed.pulse_instance[0] ^= 1,
            1 => changed.connection_generation[0] ^= 1,
            _ => changed.authority_incarnation[0] ^= 1,
        }
        assert!(matches!(
            execute(&authority, Operation::Renew(changed.clone()), 100).await,
            Reply::Denied(_)
        ));
        assert!(matches!(
            execute(&authority, Operation::Release(release(&changed)), 100).await,
            Reply::Denied(_)
        ));
        assert_eq!(authority.leases.lock().unwrap().entries.len(), 1);
    }
    assert!(matches!(
        execute(
            &authority,
            Operation::Release(release(&renewal(&first))),
            100
        )
        .await,
        Reply::Released(_)
    ));
    assert!(authority.leases.lock().unwrap().entries.is_empty());
}

#[tokio::test(start_paused = true)]
async fn delayed_renewal_cannot_revive_expiry_or_release() {
    for remove in [false, true] {
        let (authority, policy) = authority();
        let first = lease(
            execute(
                &authority,
                Operation::Join(join_with_claims(claims(200))),
                100,
            )
            .await,
        );
        tokio::time::advance(Duration::from_millis(1800)).await;
        policy.mode.store(3, Ordering::SeqCst);
        let context = renewal(&first);
        let task_authority = authority.clone();
        let task_context = context.clone();
        let task = tokio::spawn(async move {
            execute(&task_authority, Operation::Renew(task_context), 101).await
        });
        while policy.calls.load(Ordering::SeqCst) < 2 {
            tokio::task::yield_now().await;
        }
        if remove {
            assert!(matches!(
                execute(&authority, Operation::Release(release(&context)), 101).await,
                Reply::Released(_)
            ));
        } else {
            tokio::time::advance(Duration::from_millis(201)).await;
        }
        policy.release.notify_one();
        assert_denied(task.await.unwrap(), DenialReason::Expired);
        assert!(authority.leases.lock().unwrap().entries.is_empty());
    }
}

#[tokio::test(start_paused = true)]
async fn concurrent_renewal_denial_cannot_be_undone_by_earlier_success() {
    let (authority, policy) = authority();
    let first = lease(
        execute(
            &authority,
            Operation::Join(join_with_claims(claims(200))),
            100,
        )
        .await,
    );
    policy.mode.store(3, Ordering::SeqCst);
    let task_authority = authority.clone();
    let context = renewal(&first);
    let task_context = context.clone();
    let task =
        tokio::spawn(
            async move { execute(&task_authority, Operation::Renew(task_context), 100).await },
        );
    while policy.calls.load(Ordering::SeqCst) < 2 {
        tokio::task::yield_now().await;
    }
    assert_denied(
        execute(&authority, Operation::Renew(context), 100).await,
        DenialReason::Capacity,
    );
    policy.release.notify_one();
    assert_denied(task.await.unwrap(), DenialReason::Expired);
}

#[tokio::test(start_paused = true)]
async fn policy_timeout_and_check_pressure_deny_and_delete_renewal() {
    let (authority, policy) = authority();
    let first = lease(
        execute(
            &authority,
            Operation::Join(join_with_claims(claims(200))),
            100,
        )
        .await,
    );
    policy.mode.store(4, Ordering::SeqCst);
    let started = Instant::now();
    assert_denied(
        execute(&authority, Operation::Renew(renewal(&first)), 100).await,
        DenialReason::Unavailable,
    );
    assert_eq!(started.elapsed(), CHECK_TIMEOUT);
    assert!(authority.leases.lock().unwrap().entries.is_empty());
    policy.mode.store(0, Ordering::SeqCst);
    let first = lease(
        execute(
            &authority,
            Operation::Join(join_with_claims(claims(200))),
            100,
        )
        .await,
    );
    let permits = authority
        .checks
        .acquire_many(MAX_CHECKS as u32)
        .await
        .unwrap();
    assert_denied(
        execute(&authority, Operation::Renew(renewal(&first)), 100).await,
        DenialReason::Capacity,
    );
    assert_denied(
        execute(
            &authority,
            Operation::Join(join_with_claims(claims(200))),
            100,
        )
        .await,
        DenialReason::Capacity,
    );
    assert!(authority.leases.lock().unwrap().entries.is_empty());
    drop(permits);
}

#[tokio::test(start_paused = true)]
async fn lease_pressure_denies_without_evicting_live_entries() {
    let (authority, _) = authority();
    let first = lease(
        execute(
            &authority,
            Operation::Join(join_with_claims(claims(200))),
            100,
        )
        .await,
    );
    {
        let mut leases = authority.leases.lock().unwrap();
        let stored = leases
            .entries
            .get(&fixed::<16>(&first.lease_id).unwrap())
            .unwrap();
        let room = stored.room.clone();
        leases.entries.clear();
        for id in 0..MAX_LEASES {
            leases.entries.insert(
                (id as u128).to_be_bytes(),
                StoredLease {
                    instance: [1; 16],
                    generation: [2; 16],
                    room: room.clone(),
                    expires: Instant::now() + LEASE_TTL,
                    checking: false,
                },
            );
        }
    }
    assert_denied(
        execute(
            &authority,
            Operation::Join(join_with_claims(claims(200))),
            100,
        )
        .await,
        DenialReason::Capacity,
    );
    assert_eq!(authority.leases.lock().unwrap().entries.len(), MAX_LEASES);
    tokio::time::advance(LEASE_TTL).await;
    lease(
        execute(
            &authority,
            Operation::Join(join_with_claims(claims(200))),
            102,
        )
        .await,
    );
    assert_eq!(authority.leases.lock().unwrap().entries.len(), 1);
}

#[test]
fn service_key_must_be_dedicated() {
    assert!(RelayAuthority::new(
        Arc::new(Policy::default()),
        "key".into(),
        LIVEKIT_SECRET.to_vec(),
        LIVEKIT_SECRET
    )
    .is_err());
}

#[tokio::test]
async fn request_pressure_is_rejected_before_body_authentication_or_policy() {
    let (authority, policy) = authority();
    let permits = authority
        .requests
        .acquire_many(MAX_CHECKS as u32)
        .await
        .unwrap();
    let response = authority.handle(http_request(vec![0xff], [1; 16])).await;
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_denied(reply(response).await, DenialReason::Capacity);
    assert!(authority.replays.lock().unwrap().is_empty());
    assert_eq!(policy.calls.load(Ordering::SeqCst), 0);
    drop(permits);
}

#[tokio::test(start_paused = true)]
async fn lease_ttl_is_anchored_before_policy_and_jwt_is_rechecked_after_policy() {
    for expires_during_check in [false, true] {
        let (authority, policy) = authority();
        policy.mode.store(3, Ordering::SeqCst);
        let task_authority = authority.clone();
        let started = Instant::now();
        let expiry = if expires_during_check { 101 } else { 200 };
        let task = tokio::spawn(async move {
            task_authority
                .execute(
                    request(Operation::Join(join_with_claims(claims(expiry)))),
                    Duration::from_millis(100_900),
                    started,
                )
                .await
                .result
                .unwrap()
        });
        while policy.calls.load(Ordering::SeqCst) == 0 {
            tokio::task::yield_now().await;
        }
        tokio::time::advance(Duration::from_millis(200)).await;
        policy.release.notify_one();
        let result = task.await.unwrap();
        if expires_during_check {
            assert_denied(result, DenialReason::Expired);
            assert!(authority.leases.lock().unwrap().entries.is_empty());
        } else {
            let granted = lease(result);
            let id = fixed::<16>(&granted.lease_id).unwrap();
            assert_eq!(
                authority.leases.lock().unwrap().entries[&id].expires,
                started + LEASE_TTL
            );
            tokio::time::advance(Duration::from_millis(1800)).await;
            assert_denied(
                execute(&authority, Operation::Renew(renewal(&granted)), 102).await,
                DenialReason::Expired,
            );
        }
    }
}
