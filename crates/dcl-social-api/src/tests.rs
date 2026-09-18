use super::*;
use axum::{
    body::{to_bytes, Body},
    http::{HeaderMap, Request},
};
use catalyrst_crypto::{create_simple_auth_chain, Wallet};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use tower::ServiceExt;
const COMMUNITY: &str = "11111111-1111-4111-8111-111111111111";
const KEY: &str = "0000000000000000000000000000000000000000000000000000000000000001";
struct Fixture {
    state: Arc<Store>,
    router: Router,
    member: Arc<AtomicBool>,
    writes: Arc<AtomicUsize>,
    reads: Arc<AtomicUsize>,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.task.abort();
    }
}
async fn fixture(path: &str) -> Fixture {
    let member = Arc::new(AtomicBool::new(true));
    let writes = Arc::new(AtomicUsize::new(0));
    let reads = Arc::new(AtomicUsize::new(0));
    let m = member.clone();
    let w = writes.clone();
    let r = reads.clone();
    let upstream=Router::new().route("/v1/communities",get(move||{let r=r.clone();async move{
        r.fetch_add(1,Ordering::SeqCst);
        Json(json!({"data":{"results":[{"id":COMMUNITY,"name":"Builders"}],"total":1}}))
    }})).route("/v1/communities/{id}",get(move|headers:HeaderMap|{let m=m.clone();async move {
        assert!(headers.contains_key("x-identity-auth-chain-1"));
        Json(json!({"data":{"id":COMMUNITY,"active":true,"name":"Builders","membersCount":2,"role":if m.load(Ordering::SeqCst){"member"}else{"none"}}}))
    }})).route("/v1/communities/{id}/posts",post(move|headers:HeaderMap,Json(body):Json<Value>|{let w=w.clone();async move{
        let links:AuthChain=(0..2).map(|i|serde_json::from_str(headers.get(format!("x-identity-auth-chain-{i}")).unwrap().to_str().unwrap()).unwrap()).collect();
        let payload=build_payload_v6("POST",&format!("/v1/communities/{COMMUNITY}/posts"),headers["x-identity-timestamp"].to_str().unwrap(),headers["x-identity-metadata"].to_str().unwrap());
        catalyrst_crypto::verify::verify_auth_chain(&links,&payload,Some(now())).unwrap();
        w.fetch_add(1,Ordering::SeqCst);
        Json(json!({"data":{"id":"post-1","content":body["content"]}}))
    }}));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move { axum::serve(listener, upstream).await.unwrap() });
    let state = Arc::new(Store::open(path, &origin).unwrap());
    let router = app(state.clone(), PathBuf::from("/tmp/no-social-assets"));
    Fixture {
        state,
        router,
        member,
        writes,
        reads,
        task,
    }
}
async fn request(
    router: &Router,
    method: &str,
    path: &str,
    body: Value,
    token: Option<&str>,
) -> (StatusCode, Value) {
    let mut req = Request::builder()
        .method(method)
        .uri(path)
        .header("content-type", "application/json");
    if let Some(token) = token {
        req = req.header("authorization", format!("Bearer {token}"));
    }
    let response = router
        .clone()
        .oneshot(
            req.body(if method == "GET" {
                Body::empty()
            } else {
                Body::from(body.to_string())
            })
            .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 2 * 1024 * 1024)
        .await
        .unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(json!(null)),
    )
}
async fn prepare_action(f: &Fixture, op: Value) -> Value {
    let (status, p) = request(
        &f.router,
        "POST",
        "/api/actions",
        json!({"wallet":Wallet::from_hex(KEY).unwrap().address(),"operation":op}),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{p}");
    p
}
async fn submit(f: &Fixture, p: &Value, key: &str) -> (StatusCode, Value) {
    let chain = create_simple_auth_chain(
        &Wallet::from_hex(key).unwrap(),
        p["payload"].as_str().unwrap(),
    )
    .unwrap();
    request(
        &f.router,
        "POST",
        &format!("/api/actions/{}/complete", p["id"].as_str().unwrap()),
        json!({"authChain":chain}),
        None,
    )
    .await
}
fn send(text: &str) -> Value {
    json!({"type":"send_message","community_id":COMMUNITY,"text":text,"scene":{"x":0,"y":0}})
}
#[tokio::test]
async fn signatures_replay_expiry_and_membership_are_enforced() {
    let f = fixture(":memory:").await;
    let p = prepare_action(&f, send("hello")).await;
    assert_eq!(
        submit(
            &f,
            &p,
            "0000000000000000000000000000000000000000000000000000000000000002"
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(submit(&f, &p, KEY).await.0, StatusCode::OK);
    assert_eq!(submit(&f, &p, KEY).await.0, StatusCode::CONFLICT);
    assert_eq!(messages(&f.state, COMMUNITY, 0).unwrap().len(), 1);
    let expired = prepare_action(&f, send("expired")).await;
    let mut stored: Prepared = serde_json::from_value(expired.clone()).unwrap();
    stored.expires_at = now() - 1;
    f.state
        .db
        .lock()
        .unwrap()
        .execute(
            "UPDATE actions SET prepared=? WHERE id=?",
            params![serde_json::to_string(&stored).unwrap(), stored.id],
        )
        .unwrap();
    assert_eq!(submit(&f, &expired, KEY).await.0, StatusCode::GONE);
    f.member.store(false, Ordering::SeqCst);
    let denied = prepare_action(&f, send("not a member")).await;
    assert_eq!(submit(&f, &denied, KEY).await.0, StatusCode::FORBIDDEN);
    assert_eq!(messages(&f.state, COMMUNITY, 0).unwrap().len(), 1);
}
#[tokio::test]
async fn read_capability_rechecks_membership_and_scope() {
    let f = fixture(":memory:").await;
    let p = prepare_action(
        &f,
        json!({"type":"open_community","community_id":COMMUNITY}),
    )
    .await;
    let (status, opened) = submit(&f, &p, KEY).await;
    assert_eq!(status, StatusCode::OK);
    let token = opened["readToken"].as_str().unwrap();
    let path = format!("/api/communities/{COMMUNITY}/messages");
    assert_eq!(
        request(&f.router, "GET", &path, Value::Null, None).await.0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        request(
            &f.router,
            "GET",
            "/api/communities/other/messages",
            Value::Null,
            Some(token)
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        request(&f.router, "GET", &path, Value::Null, Some(token))
            .await
            .0,
        StatusCode::OK
    );
    f.member.store(false, Ordering::SeqCst);
    assert_eq!(
        request(&f.router, "GET", &path, Value::Null, Some(token))
            .await
            .0,
        StatusCode::FORBIDDEN
    );
}
#[tokio::test]
async fn relay_binds_exact_body_and_is_single_use() {
    let f = fixture(":memory:").await;
    let p = prepare_action(
        &f,
        json!({"type":"publish_post","community_id":COMMUNITY,"content":"Mixed CASE matters"}),
    )
    .await;
    assert_eq!(p["method"], "POST");
    let chain = create_simple_auth_chain(
        &Wallet::from_hex(KEY).unwrap(),
        p["payload"].as_str().unwrap(),
    )
    .unwrap();
    let path = format!("/api/actions/{}/complete", p["id"].as_str().unwrap());
    assert_eq!(
        request(
            &f.router,
            "POST",
            &path,
            json!({"authChain":chain,"body":{"content":"tampered"}}),
            None
        )
        .await
        .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let (status, result) = submit(&f, &p, KEY).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(result["data"]["content"], "Mixed CASE matters");
    assert_eq!(submit(&f, &p, KEY).await.0, StatusCode::CONFLICT);
    assert_eq!(f.writes.load(Ordering::SeqCst), 1);
}
#[tokio::test]
async fn restart_keeps_messages_and_rejects_arbitrary_proxy() {
    let path = format!("/tmp/dcl-social-test-{}.sqlite", Uuid::new_v4());
    let f = fixture(&path).await;
    let p = prepare_action(&f, send("persistent")).await;
    assert_eq!(submit(&f, &p, KEY).await.0, StatusCode::OK);
    let (status,_)=request(&f.router,"POST","/api/actions",json!({"wallet":Wallet::from_hex(KEY).unwrap().address(),"operation":{"type":"proxy","url":"http://127.0.0.1/admin"}}),None).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    drop(f);
    let reopened = Store::open(&path, "http://127.0.0.1:1").unwrap();
    let history = messages(&reopened, COMMUNITY, 0).unwrap();
    assert_eq!(history[0]["text"], "persistent");
    assert_eq!(history[0]["scene"]["x"], 0);
    drop(reopened);
    std::fs::remove_file(path).unwrap();
}

#[tokio::test]
async fn replies_reactions_and_pins_obey_membership_and_message_scope() {
    let f = fixture(":memory:").await;
    let p = prepare_action(&f, send("Parent message")).await;
    assert_eq!(submit(&f, &p, KEY).await.0, StatusCode::OK);
    let message = p["id"].as_str().unwrap();
    let reaction = json!({"type":"message_action","community_id":COMMUNITY,"message_id":message,"action":"heart"});
    let action = prepare_action(&f, reaction.clone()).await;
    assert_eq!(submit(&f, &action, KEY).await.0, StatusCode::OK);
    assert_eq!(submit(&f, &action, KEY).await.0, StatusCode::CONFLICT);
    assert_eq!(
        messages(&f.state, COMMUNITY, 0).unwrap()[0]["reactions"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let remove = prepare_action(&f, reaction.clone()).await;
    assert_eq!(submit(&f, &remove, KEY).await.0, StatusCode::OK);
    assert!(messages(&f.state, COMMUNITY, 0).unwrap()[0]["reactions"]
        .as_array()
        .unwrap()
        .is_empty());
    let reply = prepare_action(
        &f,
        json!({"type":"reply","community_id":COMMUNITY,"message_id":message,"text":"In a thread"}),
    )
    .await;
    assert_eq!(submit(&f, &reply, KEY).await.0, StatusCode::OK);
    let history = messages(&f.state, COMMUNITY, 0).unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0]["replies"][0]["text"], "In a thread");
    let pin = prepare_action(&f, json!({"type":"message_action","community_id":COMMUNITY,"message_id":message,"action":"pin"})).await;
    assert_eq!(submit(&f, &pin, KEY).await.0, StatusCode::FORBIDDEN);
    let missing = prepare_action(&f, json!({"type":"reply","community_id":COMMUNITY,"message_id":Uuid::new_v4().to_string(),"text":"Missing parent"})).await;
    assert_eq!(submit(&f, &missing, KEY).await.0, StatusCode::BAD_REQUEST);
    let other = "22222222-2222-4222-8222-222222222222";
    assert!(conversation::act(
        &f.state,
        other,
        message,
        "wallet",
        &conversation::Action::Heart,
        "member"
    )
    .is_err());
    f.member.store(false, Ordering::SeqCst);
    let removed = prepare_action(&f, reaction).await;
    assert_eq!(submit(&f, &removed, KEY).await.0, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn custom_channels_keep_history_and_private_access_separate() {
    let f = fixture(":memory:").await;
    conversation::create_channel(&f.state, COMMUNITY, "world-building", false).unwrap();
    conversation::create_channel(&f.state, COMMUNITY, "moderators", true).unwrap();
    let mut op = send("Only in world-building");
    op["channel"] = json!("world-building");
    let p = prepare_action(&f, op).await;
    assert_eq!(submit(&f, &p, KEY).await.0, StatusCode::OK);
    assert!(messages(&f.state, COMMUNITY, 0).unwrap().is_empty());
    let read = prepare_action(
        &f,
        json!({"type":"open_community","community_id":COMMUNITY,"channel":"world-building"}),
    )
    .await;
    let (status, opened) = submit(&f, &read, KEY).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(opened["messages"][0]["text"], "Only in world-building");
    assert_eq!(opened["channels"].as_array().unwrap().len(), 1);
    let private = prepare_action(
        &f,
        json!({"type":"open_community","community_id":COMMUNITY,"channel":"moderators"}),
    )
    .await;
    assert_eq!(submit(&f, &private, KEY).await.0, StatusCode::FORBIDDEN);
    let create = prepare_action(&f, json!({"type":"create_channel","community_id":COMMUNITY,"name":"member-made","private":false})).await;
    assert_eq!(submit(&f, &create, KEY).await.0, StatusCode::FORBIDDEN);
    let token = opened["readToken"].as_str().unwrap();
    let (_, history) = request(
        &f.router,
        "GET",
        &format!("/api/communities/{COMMUNITY}/messages?channel=moderators"),
        json!(null),
        Some(token),
    )
    .await;
    assert_eq!(history["messages"][0]["text"], "Only in world-building");
    f.member.store(false, Ordering::SeqCst);
    assert_eq!(
        request(
            &f.router,
            "GET",
            &format!("/api/communities/{COMMUNITY}/messages"),
            json!(null),
            Some(token)
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
async fn older_history_search_pins_and_threads_are_scoped_and_paginated() {
    let f = fixture(":memory:").await;
    conversation::create_channel(&f.state, COMMUNITY, "other", false).unwrap();
    let parent = Uuid::new_v4().to_string();
    {
        let db = f.state.db.lock().unwrap();
        for i in 0..205 {
            let id = if i == 0 {
                parent.clone()
            } else {
                Uuid::new_v4().to_string()
            };
            db.execute("INSERT INTO messages(id,community,wallet,text,created,channel) VALUES(?1,?2,'wallet',?3,1,'general')",params![id,COMMUNITY,format!("Message {i}")]).unwrap();
        }
        db.execute("INSERT INTO pins(message) VALUES(?1)", [&parent])
            .unwrap();
        for i in 0..205 {
            db.execute(
                "INSERT INTO replies(id,parent,wallet,text,created) VALUES(?1,?2,'wallet',?3,1)",
                params![Uuid::new_v4().to_string(), parent, format!("Reply {i}")],
            )
            .unwrap();
        }
    }
    let p = prepare_action(
        &f,
        json!({"type":"open_community","community_id":COMMUNITY}),
    )
    .await;
    let (_, opened) = submit(&f, &p, KEY).await;
    let token = opened["readToken"].as_str().unwrap();
    let endpoint = format!("/api/communities/{COMMUNITY}/messages");
    let (_, latest) = request(&f.router, "GET", &endpoint, json!(null), Some(token)).await;
    assert_eq!(latest["messages"].as_array().unwrap().len(), 100);
    assert_eq!(latest["hasMore"], true);
    let before = latest["messages"][0]["seq"].as_i64().unwrap();
    let (_, older) = request(
        &f.router,
        "GET",
        &format!("{endpoint}?before={before}"),
        json!(null),
        Some(token),
    )
    .await;
    assert_eq!(older["messages"].as_array().unwrap().len(), 100);
    assert!(older["messages"][99]["seq"].as_i64().unwrap() < before);
    for query in ["search=Reply%200", "pinned=true"] {
        let (_, found) = request(
            &f.router,
            "GET",
            &format!("{endpoint}?{query}"),
            json!(null),
            Some(token),
        )
        .await;
        assert_eq!(found["messages"].as_array().unwrap().len(), 1);
        assert_eq!(found["messages"][0]["id"], parent);
        assert_eq!(found["messages"][0]["replyCount"], 205);
    }
    let (_, thread) = request(
        &f.router,
        "GET",
        &format!("{endpoint}?thread={parent}"),
        json!(null),
        Some(token),
    )
    .await;
    assert_eq!(thread["replies"].as_array().unwrap().len(), 100);
    assert_eq!(thread["hasMore"], true);
    let before = thread["replies"][0]["seq"].as_i64().unwrap();
    let (_, older) = request(
        &f.router,
        "GET",
        &format!("{endpoint}?thread={parent}&reply_before={before}"),
        json!(null),
        Some(token),
    )
    .await;
    assert_eq!(older["replies"].as_array().unwrap().len(), 100);
    assert!(older["replies"][99]["seq"].as_i64().unwrap() < before);
    let p = prepare_action(
        &f,
        json!({"type":"open_community","community_id":COMMUNITY,"channel":"other"}),
    )
    .await;
    let (_, opened) = submit(&f, &p, KEY).await;
    assert_eq!(
        request(
            &f.router,
            "GET",
            &format!("{endpoint}?thread={parent}"),
            json!(null),
            opened["readToken"].as_str()
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
async fn enrichment_keeps_replies_reactions_and_pins_per_message() {
    let f = fixture(":memory:").await;
    let ids: Vec<String> = (0..3).map(|_| Uuid::new_v4().to_string()).collect();
    {
        let db = f.state.db.lock().unwrap();
        for (i, id) in ids.iter().enumerate() {
            db.execute("INSERT INTO messages(id,community,wallet,text,created,channel) VALUES(?1,?2,'wallet',?3,1,'general')",params![id,COMMUNITY,format!("Message {i}")]).unwrap();
        }
        for i in 0..101 {
            db.execute(
                "INSERT INTO replies(id,parent,wallet,text,created) VALUES(?1,?2,'wallet',?3,1)",
                params![Uuid::new_v4().to_string(), ids[0], format!("Reply {i}")],
            )
            .unwrap();
        }
        for i in 0..2 {
            db.execute(
                "INSERT INTO replies(id,parent,wallet,text,created) VALUES(?1,?2,'wallet',?3,1)",
                params![Uuid::new_v4().to_string(), ids[1], format!("Other {i}")],
            )
            .unwrap();
        }
        db.execute("INSERT INTO pins(message) VALUES(?1)", [&ids[0]])
            .unwrap();
        db.execute(
            "INSERT INTO reactions(message,wallet,emoji) VALUES(?1,'0xb','heart'),(?1,'0xa','heart'),(?1,'0xa','celebrate')",
            [&ids[0]],
        )
        .unwrap();
    }
    let history = messages(&f.state, COMMUNITY, 0).unwrap();
    assert_eq!(history.len(), 3);
    let first = &history[0];
    assert_eq!(first["id"], ids[0]);
    assert_eq!(first["replyCount"], 101);
    assert_eq!(first["hasMoreReplies"], true);
    let replies = first["replies"].as_array().unwrap();
    assert_eq!(replies.len(), 100);
    assert_eq!(replies[0]["text"], "Reply 1");
    assert_eq!(replies[99]["text"], "Reply 100");
    assert!(replies
        .windows(2)
        .all(|w| w[0]["seq"].as_i64() < w[1]["seq"].as_i64()));
    assert_eq!(first["pinned"], true);
    assert_eq!(
        first["reactions"],
        json!([{"emoji":"celebrate","wallet":"0xa"},{"emoji":"heart","wallet":"0xa"},{"emoji":"heart","wallet":"0xb"}])
    );
    let second = &history[1];
    assert_eq!(second["replyCount"], 2);
    assert_eq!(second["hasMoreReplies"], false);
    assert_eq!(second["replies"][1]["text"], "Other 1");
    assert_eq!(second["pinned"], false);
    assert_eq!(second["reactions"], json!([]));
    let third = &history[2];
    assert_eq!(third["replyCount"], 0);
    assert_eq!(third["replies"], json!([]));
    assert_eq!(third["pinned"], false);
}

#[tokio::test]
async fn discovery_reads_are_memoized_per_query() {
    let f = fixture(":memory:").await;
    for _ in 0..2 {
        let (status, body) = request(
            &f.router,
            "GET",
            "/api/communities?search=build",
            json!(null),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["data"]["results"][0]["name"], "Builders");
    }
    assert_eq!(f.reads.load(Ordering::SeqCst), 1);
    request(
        &f.router,
        "GET",
        "/api/communities?search=other",
        json!(null),
        None,
    )
    .await;
    assert_eq!(f.reads.load(Ordering::SeqCst), 2);
    f.state.memo.forget("communities:");
    request(
        &f.router,
        "GET",
        "/api/communities?search=build",
        json!(null),
        None,
    )
    .await;
    assert_eq!(f.reads.load(Ordering::SeqCst), 3);
}

#[tokio::test]
async fn channel_settings_enforce_invites_readonly_and_slow_mode() {
    let f = fixture(":memory:").await;
    let wallet = Wallet::from_hex(KEY).unwrap().address();
    conversation::create_channel(&f.state, COMMUNITY, "private", true).unwrap();
    let op = json!({"type":"open_community","community_id":COMMUNITY,"channel":"private"});
    let p = prepare_action(&f, op.clone()).await;
    assert_eq!(submit(&f, &p, KEY).await.0, StatusCode::FORBIDDEN);
    let mut config = conversation::ChannelConfig {
        private: true,
        members: vec![wallet.to_string()],
        read_only: false,
        slow_mode: 10,
    };
    conversation::configure(&f.state, COMMUNITY, "private", &config).unwrap();
    let p = prepare_action(&f, op.clone()).await;
    let (status, opened) = submit(&f, &p, KEY).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(opened["channels"][0]["name"], "private");
    let mut send = send("Slow mode message");
    send["channel"] = json!("private");
    let p = prepare_action(&f, send.clone()).await;
    assert_eq!(submit(&f, &p, KEY).await.0, StatusCode::OK);
    let p = prepare_action(&f, send.clone()).await;
    assert_eq!(submit(&f, &p, KEY).await.0, StatusCode::TOO_MANY_REQUESTS);
    config.read_only = true;
    conversation::configure(&f.state, COMMUNITY, "private", &config).unwrap();
    let p = prepare_action(&f, send).await;
    assert_eq!(submit(&f, &p, KEY).await.0, StatusCode::FORBIDDEN);
    config.members.clear();
    conversation::configure(&f.state, COMMUNITY, "private", &config).unwrap();
    assert_eq!(
        request(
            &f.router,
            "GET",
            &format!("/api/communities/{COMMUNITY}/messages"),
            json!(null),
            opened["readToken"].as_str()
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    let p=prepare_action(&f,json!({"type":"configure_channel","community_id":COMMUNITY,"channel":"private","config":config})).await;
    assert_eq!(submit(&f, &p, KEY).await.0, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn report_review_requires_moderator_and_removes_content() {
    let f = fixture(":memory:").await;
    let p = prepare_action(&f, send("Reported content")).await;
    assert_eq!(submit(&f, &p, KEY).await.0, StatusCode::OK);
    let id = p["id"].as_str().unwrap();
    for _ in 0..2 {
        let p=prepare_action(&f,json!({"type":"message_action","community_id":COMMUNITY,"message_id":id,"action":"report"})).await;
        assert_eq!(submit(&f, &p, KEY).await.0, StatusCode::OK);
    }
    assert_eq!(
        conversation::reports(&f.state, COMMUNITY).unwrap()[0]["reports"],
        1
    );
    let p = prepare_action(
        &f,
        json!({"type":"community_reports","community_id":COMMUNITY}),
    )
    .await;
    assert_eq!(submit(&f, &p, KEY).await.0, StatusCode::FORBIDDEN);
    let p = prepare_action(
        &f,
        json!({"type":"message_action","community_id":COMMUNITY,"message_id":id,"action":"remove"}),
    )
    .await;
    assert_eq!(submit(&f, &p, KEY).await.0, StatusCode::FORBIDDEN);
    conversation::act(
        &f.state,
        COMMUNITY,
        id,
        "moderator",
        &conversation::Action::Remove,
        "moderator",
    )
    .unwrap();
    assert!(conversation::reports(&f.state, COMMUNITY)
        .unwrap()
        .is_empty());
    let history = messages(&f.state, COMMUNITY, 0).unwrap();
    assert_eq!(history[0]["text"], "[Message removed]");
    assert!(history[0]["scene"].is_null());
}

#[tokio::test]
async fn community_invitations_and_cancellation_bind_upstream_intent() {
    let f = fixture(":memory:").await;
    let wallet = Wallet::from_hex(KEY)
        .unwrap()
        .address()
        .to_string()
        .to_lowercase();
    let invitations =
        prepare_action(&f, json!({"type":"my_community_invitations","offset":100})).await;
    assert_eq!(invitations["method"], "GET");
    assert!(invitations["url"].as_str().unwrap().ends_with(&format!(
        "/v1/members/{wallet}/requests?type=invite&limit=100&offset=100"
    )));
    let requests = prepare_action(&f, json!({"type":"my_join_requests"})).await;
    assert!(requests["url"]
        .as_str()
        .unwrap()
        .ends_with("?type=request_to_join&limit=100&offset=0"));
    let cancel = prepare_action(
        &f,
        json!({"type":"cancel_community_request","community_id":COMMUNITY,"request_id":COMMUNITY}),
    )
    .await;
    assert_eq!(cancel["method"], "PATCH");
    assert!(cancel["url"]
        .as_str()
        .unwrap()
        .ends_with(&format!("/v1/communities/{COMMUNITY}/requests/{COMMUNITY}")));
    assert_eq!(
        serde_json::from_str::<Value>(cancel["body"].as_str().unwrap()).unwrap(),
        json!({"intention":"cancelled"})
    );
    for operation in [
        json!({"type":"my_community_invitations","offset":100001}),
        json!({"type":"cancel_community_request","community_id":COMMUNITY,"request_id":"not-a-request"}),
    ] {
        let (status, _) = request(
            &f.router,
            "POST",
            "/api/actions",
            json!({"wallet":wallet,"operation":operation}),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }
}

#[tokio::test]
async fn event_rsvp_binds_method_and_world_shares_keep_realm() {
    let path = format!("/tmp/dcl-social-world-test-{}.sqlite", Uuid::new_v4());
    let f = fixture(&path).await;
    let attending = prepare_action(&f, json!({"type":"my_events"})).await;
    assert_eq!(attending["method"], "GET");
    assert_eq!(
        attending["url"],
        "https://events.decentraland.org/api/events/attending"
    );
    for (kind, attending, method) in [
        ("event_attendees", None, "GET"),
        ("event_rsvp", Some(true), "POST"),
        ("event_rsvp", Some(false), "DELETE"),
    ] {
        let mut op = json!({"type":kind,"event_id":COMMUNITY});
        if let Some(attending) = attending {
            op["attending"] = json!(attending);
        }
        let p = prepare_action(&f, op).await;
        assert_eq!(p["method"], method);
        assert_eq!(
            p["url"],
            format!("https://events.decentraland.org/api/events/{COMMUNITY}/attendees")
        );
        let prepared: Prepared = serde_json::from_value(p).unwrap();
        assert_eq!(
            prepared.payload,
            build_payload_v6(
                method,
                &format!("/api/events/{COMMUNITY}/attendees"),
                &prepared.timestamp,
                &prepared.metadata
            )
        );
    }
    let p=prepare_action(&f,json!({"type":"send_message","community_id":COMMUNITY,"text":"Join my world","scene":{"x":0,"y":0,"world":"builders.dcl.eth"}})).await;
    assert_eq!(submit(&f, &p, KEY).await.0, StatusCode::OK);
    assert_eq!(
        messages(&f.state, COMMUNITY, 0).unwrap()[0]["scene"]["world"],
        "builders.dcl.eth"
    );
    for name in [
        "https://evil.example",
        "world.dcl.eth/../secret",
        "",
        "-world.eth",
    ] {
        let op = json!({"type":"send_message","community_id":COMMUNITY,"text":"","scene":{"x":0,"y":0,"world":name}});
        let (status, _) = request(
            &f.router,
            "POST",
            "/api/actions",
            json!({"wallet":Wallet::from_hex(KEY).unwrap().address(),"operation":op}),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }
    drop(f);
    let reopened = Store::open(&path, "http://127.0.0.1:1").unwrap();
    assert_eq!(
        messages(&reopened, COMMUNITY, 0).unwrap()[0]["scene"]["world"],
        "builders.dcl.eth"
    );
    drop(reopened);
    std::fs::remove_file(path).unwrap();
}

#[tokio::test]
async fn community_voice_places_and_picture_bind_foundation_contracts() {
    use base64::{engine::general_purpose::STANDARD, Engine};
    let f = fixture(":memory:").await;
    let live = prepare_action(&f, json!({"type":"active_community_voice_chats"})).await;
    assert_eq!(live["method"], "GET");
    assert!(live["url"]
        .as_str()
        .unwrap()
        .ends_with("/v1/community-voice-chats/active"));
    let places = prepare_action(
        &f,
        json!({"type":"community_places","community_id":COMMUNITY,"offset":100}),
    )
    .await;
    assert_eq!(places["method"], "GET");
    assert!(places["url"].as_str().unwrap().ends_with(&format!(
        "/v1/communities/{COMMUNITY}/places?limit=100&offset=100"
    )));
    let add = prepare_action(
        &f,
        json!({"type":"add_community_places","community_id":COMMUNITY,"place_ids":[COMMUNITY]}),
    )
    .await;
    assert_eq!(add["method"], "POST");
    assert_eq!(
        serde_json::from_str::<Value>(add["body"].as_str().unwrap()).unwrap(),
        json!({"placeIds":[COMMUNITY]})
    );
    let remove = prepare_action(
        &f,
        json!({"type":"remove_community_place","community_id":COMMUNITY,"place_id":COMMUNITY}),
    )
    .await;
    assert_eq!(remove["method"], "DELETE");
    assert!(remove["url"]
        .as_str()
        .unwrap()
        .ends_with(&format!("/places/{COMMUNITY}")));
    let mut picture = b"\x89PNG\r\n\x1a\n".to_vec();
    picture.resize(100_000, 255);
    let encoded = STANDARD.encode(&picture);
    let op = json!({"type":"update_community","community_id":COMMUNITY,"name":"Builders","description":"Let's build","privacy":"public","thumbnail":encoded});
    let prepared = prepare_action(&f, op).await;
    assert_eq!(prepared["operation"]["thumbnail"], encoded);
    let multipart = community_assets::multipart(
        prepared["body"].as_str().unwrap(),
        prepared["id"].as_str().unwrap(),
        Some(&encoded),
    )
    .unwrap();
    assert!(multipart.windows(picture.len()).any(|part| part == picture));
    for invalid in [
        json!({"type":"add_community_places","community_id":COMMUNITY,"place_ids":["../../other"]}),
        json!({"type":"remove_community_place","community_id":COMMUNITY,"place_id":"bad"}),
        json!({"type":"community_places","community_id":COMMUNITY,"offset":100001}),
        json!({"type":"update_community","community_id":COMMUNITY,"name":"Builders","description":"Let's build","privacy":"public","thumbnail":"<svg>"}),
    ] {
        let (status, _) = request(
            &f.router,
            "POST",
            "/api/actions",
            json!({"wallet":Wallet::from_hex(KEY).unwrap().address(),"operation":invalid}),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }
}

#[tokio::test]
async fn event_creation_validates_dates_and_binds_destination_and_body() {
    let path = format!("/tmp/dcl-social-event-test-{}.sqlite", Uuid::new_v4());
    let f = fixture(&path).await;
    let start = (chrono::Utc::now() + chrono::Duration::hours(1)).to_rfc3339();
    let draft = json!({"name":"Meet friends", "description":"A community evening", "start_at":start,
        "duration":3_600_000,"scene":{"x":0,"y":0,"world":"party.dcl.eth"}});
    let prepared = prepare_action(&f, json!({"type":"create_event","event":draft})).await;
    assert_eq!(prepared["method"], "POST");
    assert_eq!(
        prepared["url"],
        "https://events.decentraland.org/api/events"
    );
    let body: Value = serde_json::from_str(prepared["body"].as_str().unwrap()).unwrap();
    assert_eq!(body["server"], "party.dcl.eth");
    assert_eq!(body["world"], true);
    assert_eq!(body["duration"], 3_600_000);
    assert_eq!(body["categories"], json!([]));
    assert!(body.get("approved").is_none());
    let mut changed = draft.clone();
    changed["name"] = json!("Different event");
    let other = prepare_action(&f, json!({"type":"create_event","event":changed})).await;
    assert_ne!(prepared["metadata"], other["metadata"]);
    for (field, value) in [
        ("name", json!(" ")),
        ("duration", json!(0)),
        ("duration", json!(86_400_001)),
        ("start_at", json!("invalid")),
        ("start_at", json!("2000-01-01T00:00:00Z")),
        ("scene", json!({"x":151,"y":0})),
        ("scene", json!({"x":0,"y":0,"world":"https://evil.example"})),
    ] {
        let mut invalid = draft.clone();
        invalid[field] = value;
        let (status, _) = request(&f.router,"POST","/api/actions",
            json!({"wallet":prepared["wallet"],"operation":{"type":"create_event","event":invalid}}),None).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{field}");
    }
    let _ = std::fs::remove_file(path);
}

#[tokio::test]
async fn security_headers_cover_success_and_error_responses() {
    let f = fixture(":memory:").await;
    for (path, expected) in [
        ("/api/health", StatusCode::OK),
        ("/api/missing", StatusCode::NOT_FOUND),
    ] {
        let response = f
            .router
            .clone()
            .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), expected);
        let headers = response.headers();
        let csp = headers["content-security-policy"].to_str().unwrap();
        assert!(csp.contains("script-src 'self' 'wasm-unsafe-eval';"));
        assert!(!csp.contains("'unsafe-eval'"));
        assert!(csp.contains("wss://rpc-social-service-ea.decentraland.org"));
        assert!(csp.contains("wss://*.livekit.cloud"));
        assert!(headers["permissions-policy"]
            .to_str()
            .unwrap()
            .contains("microphone=(self)"));
        assert_eq!(
            headers["cross-origin-opener-policy"],
            "same-origin-allow-popups"
        );
        assert_eq!(headers["cross-origin-resource-policy"], "same-origin");
        assert_eq!(headers["cache-control"], "no-store");
    }
}
