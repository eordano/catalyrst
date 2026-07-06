use super::*;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;

#[test]
fn jwt_has_three_dot_parts() {
    let tok = AccessToken::new("devkey", "devsecret", "0xabc", VideoGrants::join("room1"))
        .to_jwt()
        .unwrap();
    assert_eq!(tok.split('.').count(), 3);
}

#[test]
fn adapter_url_prefixes_wss() {
    let url = build_adapter_url("livekit.example.com", "tok");
    assert!(url.starts_with("livekit:wss://livekit.example.com?access_token=tok"));
}

#[test]
fn room_names_match_upstream() {
    assert_eq!(scene_room_name("abc"), "scene:abc");

    assert_eq!(world_scene_room_name("foo.eth", "xyz"), "world-foo.eth-xyz");
    assert_eq!(world_room_name("foo.eth"), "world-foo.eth");
}

#[test]
fn community_room_name_round_trips() {
    let name = community_voice_chat_room_name("abc-123");
    assert_eq!(name, "voice-chat-community-abc-123");
    assert!(is_community_voice_chat_room(&name));

    assert_eq!(community_id_from_room_name(&name), "abc-123");
}

#[test]
fn address_from_identity_truncates_and_validates() {
    let addr = "0x1234567890abcdef1234567890abcdef12345678";
    assert_eq!(address_from_identity(addr).as_deref(), Some(addr));

    assert_eq!(
        address_from_identity(&format!("{addr}:session")).as_deref(),
        Some(addr)
    );

    assert_eq!(address_from_identity("authoritative-server"), None);
}

#[test]
fn room_service_base_maps_scheme_like_livekit_sdk() {
    assert_eq!(
        room_service_base("wss://livekit.example.com"),
        "https://livekit.example.com"
    );
    assert_eq!(
        room_service_base("livekit.example.com/"),
        "https://livekit.example.com"
    );
    assert_eq!(
        room_service_base("https://livekit.example.com"),
        "https://livekit.example.com"
    );

    assert_eq!(
        room_service_base("ws://127.0.0.1:7880"),
        "http://127.0.0.1:7880"
    );
    assert_eq!(
        room_service_base("http://127.0.0.1:7880"),
        "http://127.0.0.1:7880"
    );
}

fn decode_jwt_payload(jwt: &str) -> serde_json::Value {
    let payload_b64 = jwt.split('.').nth(1).expect("jwt payload segment");
    let bytes = URL_SAFE_NO_PAD.decode(payload_b64).expect("base64url");
    serde_json::from_slice(&bytes).expect("payload json")
}

#[test]
fn private_voice_grants_match_upstream_generate_credentials() {
    let mut grants = VideoGrants::join("voice-chat-private-call1");
    grants.can_publish = true;
    grants.can_subscribe = true;
    grants.can_update_own_metadata = false;
    grants.can_publish_sources = Some(vec![TRACK_SOURCE_MICROPHONE.to_string()]);

    let jwt = AccessToken::new("devkey", "devsecret", "0xabc", grants)
        .to_jwt()
        .unwrap();
    let p = decode_jwt_payload(&jwt);
    assert_eq!(p["iss"], "devkey");
    assert_eq!(p["sub"], "0xabc");
    let v = &p["video"];
    assert_eq!(v["roomJoin"], true);
    assert_eq!(v["room"], "voice-chat-private-call1");
    assert_eq!(v["canPublish"], true);
    assert_eq!(v["canSubscribe"], true);
    assert_eq!(v["canPublishData"], true);
    assert_eq!(v["canUpdateOwnMetadata"], false);
    assert_eq!(v["canPublishSources"], serde_json::json!(["MICROPHONE"]));

    assert!(p["exp"].as_i64().unwrap() > p["nbf"].as_i64().unwrap());
}

#[test]
fn community_speaker_grant_omits_publish_sources_restriction() {
    let mut grants = VideoGrants::join("voice-chat-community-c1");
    grants.can_publish = true;
    grants.can_update_own_metadata = false;
    let jwt = AccessToken::new("devkey", "devsecret", "0xabc", grants)
        .with_metadata(r#"{"role":"owner","isSpeaker":true,"muted":false}"#)
        .to_jwt()
        .unwrap();
    let p = decode_jwt_payload(&jwt);
    let v = &p["video"];
    assert_eq!(v["canPublish"], true);
    assert!(v.get("canPublishSources").is_none());
    assert_eq!(
        p["metadata"],
        r#"{"role":"owner","isSpeaker":true,"muted":false}"#
    );
}

#[test]
fn community_listener_cannot_publish() {
    let mut grants = VideoGrants::join("voice-chat-community-c1");
    grants.can_publish = false;
    grants.can_subscribe = true;
    let jwt = AccessToken::new("devkey", "devsecret", "0xabc", grants)
        .to_jwt()
        .unwrap();
    let v = decode_jwt_payload(&jwt)["video"].clone();
    assert_eq!(v["canPublish"], false);
    assert_eq!(v["canSubscribe"], true);
}

#[test]
fn room_admin_token_grants_room_admin_and_list() {
    let jwt = room_admin_token("devkey", "devsecret", "some-room").unwrap();
    let p = decode_jwt_payload(&jwt);
    assert_eq!(p["iss"], "devkey");
    assert_eq!(p["sub"], "devkey");
    let v = &p["video"];
    assert_eq!(v["roomAdmin"], true);
    assert_eq!(v["roomList"], true);
    assert_eq!(v["room"], "some-room");

    assert_eq!(p["exp"].as_i64().unwrap() - p["nbf"].as_i64().unwrap(), 60);
}

struct Captured {
    line: String,
    auth: String,
    body: serde_json::Value,
}

async fn capture_once(
    resp_status: &'static str,
    resp_body: &'static str,
) -> (String, tokio::sync::oneshot::Receiver<Captured>) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let (tx, rx) = tokio::sync::oneshot::channel();
    tokio::spawn(async move {
        let (mut sock, _) = listener.accept().await.unwrap();
        let mut buf = vec![0u8; 8192];
        let mut total = 0;

        loop {
            let n = sock.read(&mut buf[total..]).await.unwrap();
            if n == 0 {
                break;
            }
            total += n;
            let text = String::from_utf8_lossy(&buf[..total]);
            if let Some(hdr_end) = text.find("\r\n\r\n") {
                let header_part = &text[..hdr_end];
                let content_len = header_part
                    .lines()
                    .find_map(|l| {
                        let l = l.to_ascii_lowercase();
                        l.strip_prefix("content-length:")
                            .map(|v| v.trim().parse::<usize>().unwrap_or(0))
                    })
                    .unwrap_or(0);
                if total >= hdr_end + 4 + content_len {
                    break;
                }
            }
        }
        let text = String::from_utf8_lossy(&buf[..total]).to_string();
        let (head, body) = text.split_once("\r\n\r\n").unwrap_or((&text, ""));
        let line = head.lines().next().unwrap_or("").to_string();
        let auth = head
            .lines()
            .find_map(|l| {
                let ll = l.to_ascii_lowercase();
                ll.strip_prefix("authorization:").map(|_| {
                    l.split_once(':')
                        .map(|x| x.1)
                        .unwrap_or("")
                        .trim()
                        .to_string()
                })
            })
            .unwrap_or_default();
        let body_json: serde_json::Value =
            serde_json::from_str(body).unwrap_or(serde_json::Value::Null);
        let resp = format!(
            "HTTP/1.1 {resp_status}\r\nContent-Type: application/json\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{resp_body}",
            resp_body.len()
        );
        sock.write_all(resp.as_bytes()).await.unwrap();
        sock.flush().await.unwrap();
        let _ = tx.send(Captured {
            line,
            auth,
            body: body_json,
        });
    });
    (format!("http://{addr}"), rx)
}

#[tokio::test]
async fn list_participant_identities_twirp_request_and_grant_shape() {
    let (host, rx) = capture_once(
        "200 OK",
        r#"{"participants":[{"identity":"0x1234567890ABCDEF1234567890abcdef12345678:wss"},{"identity":"authoritative-server"},{"identity":"0xffff567890abcdef1234567890abcdef12345678"}]}"#,
    )
    .await;
    let http = reqwest::Client::new();
    let identities = list_room_participant_identities(
        &http,
        &host,
        "devkey",
        "devsecret",
        "world-foo.eth-bafkreiabc",
    )
    .await;
    assert_eq!(
        identities,
        vec![
            "0x1234567890ABCDEF1234567890abcdef12345678:wss".to_string(),
            "authoritative-server".to_string(),
            "0xffff567890abcdef1234567890abcdef12345678".to_string(),
        ]
    );
    let cap = rx.await.unwrap();
    assert_eq!(
        cap.line,
        "POST /twirp/livekit.RoomService/ListParticipants HTTP/1.1"
    );
    assert_eq!(
        cap.body,
        serde_json::json!({ "room": "world-foo.eth-bafkreiabc" })
    );
    let bearer = cap.auth.strip_prefix("Bearer ").expect("bearer prefix");
    let claims = decode_jwt_payload(bearer);
    assert_eq!(claims["iss"], "devkey");
    assert_eq!(claims["sub"], "devkey");
    assert_eq!(claims["video"]["roomAdmin"], true);
    assert_eq!(claims["video"]["roomList"], true);
    assert_eq!(claims["video"]["room"], "world-foo.eth-bafkreiabc");
}

#[tokio::test]
async fn list_participant_identities_swallows_livekit_failures() {
    let (host, _rx) = capture_once("500 Internal Server Error", "boom").await;
    let http = reqwest::Client::new();
    let identities =
        list_room_participant_identities(&http, &host, "devkey", "devsecret", "scene:abc").await;
    assert!(identities.is_empty());
}

#[tokio::test]
async fn delete_room_posts_twirp_deleteroom_with_admin_token() {
    let (host, rx) = capture_once("200 OK", "{}").await;
    let http = reqwest::Client::new();
    let client = RoomServiceClient::new(&http, &host, "devkey", "devsecret");
    client.delete_room("voice-chat-private-c1").await.unwrap();
    let cap = rx.await.unwrap();
    assert_eq!(
        cap.line,
        "POST /twirp/livekit.RoomService/DeleteRoom HTTP/1.1"
    );
    assert_eq!(
        cap.body,
        serde_json::json!({ "room": "voice-chat-private-c1" })
    );

    let bearer = cap.auth.strip_prefix("Bearer ").expect("bearer prefix");
    let claims = decode_jwt_payload(bearer);
    assert_eq!(claims["video"]["roomAdmin"], true);
    assert_eq!(claims["video"]["room"], "voice-chat-private-c1");
}

#[tokio::test]
async fn delete_room_treats_404_as_success() {
    let (host, _rx) = capture_once("404 Not Found", "not_found").await;
    let http = reqwest::Client::new();
    let client = RoomServiceClient::new(&http, &host, "devkey", "devsecret");

    client.delete_room("gone").await.unwrap();
}

#[tokio::test]
async fn remove_participant_posts_room_and_identity() {
    let (host, rx) = capture_once("200 OK", "{}").await;
    let http = reqwest::Client::new();
    let client = RoomServiceClient::new(&http, &host, "devkey", "devsecret");
    client
        .remove_participant("voice-chat-community-c1", "0xabc")
        .await
        .unwrap();
    let cap = rx.await.unwrap();
    assert_eq!(
        cap.line,
        "POST /twirp/livekit.RoomService/RemoveParticipant HTTP/1.1"
    );
    assert_eq!(
        cap.body,
        serde_json::json!({ "room": "voice-chat-community-c1", "identity": "0xabc" })
    );
}

#[tokio::test]
async fn update_participant_sends_permission_block() {
    let (host, rx) = capture_once("200 OK", "{}").await;
    let http = reqwest::Client::new();
    let client = RoomServiceClient::new(&http, &host, "devkey", "devsecret");
    client
        .update_participant(
            "voice-chat-community-c1",
            "0xabc",
            None,
            Some(serde_json::json!({
                "canPublish": true,
                "canSubscribe": true,
                "canPublishData": true,
            })),
        )
        .await
        .unwrap();
    let cap = rx.await.unwrap();
    assert_eq!(
        cap.line,
        "POST /twirp/livekit.RoomService/UpdateParticipant HTTP/1.1"
    );
    assert_eq!(cap.body["room"], "voice-chat-community-c1");
    assert_eq!(cap.body["identity"], "0xabc");
    assert_eq!(cap.body["permission"]["canPublish"], true);

    assert!(cap.body.get("metadata").is_none());
}

async fn read_one_request<S>(sock: &mut S) -> Option<(String, serde_json::Value)>
where
    S: tokio::io::AsyncRead + Unpin,
{
    use tokio::io::AsyncReadExt;
    let mut acc: Vec<u8> = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        let n = sock.read(&mut chunk).await.ok()?;
        if n == 0 {
            if acc.is_empty() {
                return None;
            }
            break;
        }
        acc.extend_from_slice(&chunk[..n]);
        let text = String::from_utf8_lossy(&acc);
        if let Some(hdr_end) = text.find("\r\n\r\n") {
            let content_len = text[..hdr_end]
                .lines()
                .find_map(|l| {
                    l.to_ascii_lowercase()
                        .strip_prefix("content-length:")
                        .map(|v| v.trim().parse::<usize>().unwrap_or(0))
                })
                .unwrap_or(0);
            if acc.len() >= hdr_end + 4 + content_len {
                break;
            }
        }
    }
    let text = String::from_utf8_lossy(&acc).to_string();
    let (head, body) = text.split_once("\r\n\r\n").unwrap_or((&text, ""));
    let line = head.lines().next().unwrap_or("").to_string();
    let body_json: serde_json::Value =
        serde_json::from_str(body).unwrap_or(serde_json::Value::Null);
    Some((line, body_json))
}

async fn capture_seq(
    responses: Vec<&'static str>,
) -> (String, tokio::sync::oneshot::Receiver<Vec<Captured>>) {
    use tokio::io::AsyncWriteExt;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let (tx, rx) = tokio::sync::oneshot::channel();
    tokio::spawn(async move {
        let mut captured = Vec::new();
        let (mut sock, _) = listener.accept().await.unwrap();
        for resp_body in responses {
            let (line, body_json) = loop {
                match read_one_request(&mut sock).await {
                    Some(req) => break req,

                    None => {
                        let (s, _) = listener.accept().await.unwrap();
                        sock = s;
                    }
                }
            };
            captured.push(Captured {
                line,
                auth: String::new(),
                body: body_json,
            });
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{resp_body}",
                resp_body.len()
            );
            sock.write_all(resp.as_bytes()).await.unwrap();
            sock.flush().await.unwrap();
        }
        let _ = tx.send(captured);
    });
    (format!("http://{addr}"), rx)
}

#[tokio::test]
async fn merge_metadata_read_modify_writes_merged_blob() {
    let (host, rx) = capture_seq(vec![
        r#"{"participants":[{"identity":"0xabc","metadata":"{\"role\":\"owner\",\"muted\":false}"}]}"#,
        "{}",
    ])
    .await;
    let http = reqwest::Client::new();
    let client = RoomServiceClient::new(&http, &host, "devkey", "devsecret");
    let mut patch = serde_json::Map::new();
    patch.insert("muted".into(), serde_json::json!(true));
    client
        .merge_participant_metadata("voice-chat-community-c1", "0xabc", patch)
        .await
        .unwrap();
    let caps = rx.await.unwrap();
    assert_eq!(caps.len(), 2);
    assert!(caps[0].line.contains("ListParticipants"));
    assert!(caps[1].line.contains("UpdateParticipant"));

    let written: serde_json::Value =
        serde_json::from_str(caps[1].body["metadata"].as_str().unwrap()).unwrap();
    assert_eq!(written["role"], "owner");
    assert_eq!(written["muted"], true);
}

#[test]
fn merge_metadata_math_shallow_merges_over_existing() {
    let existing = r#"{"role":"owner","muted":false,"isSpeaker":true}"#;
    let mut merged: serde_json::Map<String, serde_json::Value> =
        serde_json::from_str(existing).unwrap();
    let mut patch = serde_json::Map::new();
    patch.insert("muted".into(), serde_json::json!(true));
    for (k, v) in patch {
        merged.insert(k, v);
    }
    assert_eq!(merged["muted"], true);
    assert_eq!(merged["role"], "owner");
    assert_eq!(merged["isSpeaker"], true);
}

#[tokio::test]
async fn list_rooms_parses_names() {
    let (host, _rx) = capture_once("200 OK", r#"{"rooms":[{"name":"r1"},{"name":"r2"}]}"#).await;
    let http = reqwest::Client::new();
    let client = RoomServiceClient::new(&http, &host, "devkey", "devsecret");
    let rooms = client.list_rooms().await.unwrap();
    assert_eq!(rooms, vec!["r1".to_string(), "r2".to_string()]);
}

#[tokio::test]
async fn server_500_maps_to_status_error() {
    let (host, _rx) = capture_once("500 Internal Server Error", "boom").await;
    let http = reqwest::Client::new();
    let client = RoomServiceClient::new(&http, &host, "devkey", "devsecret");
    let err = client.list_rooms().await.unwrap_err();
    assert!(matches!(err, RoomServiceError::Status(500)));
}

#[test]
fn parse_room_metadata_handles_absent_and_garbage() {
    assert!(parse_room_metadata(None).is_empty());
    assert!(parse_room_metadata(Some("not json")).is_empty());
    assert!(parse_room_metadata(Some("[1,2,3]")).is_empty());
    let m = parse_room_metadata(Some(r#"{"bannedAddresses":["0xa"]}"#));
    assert_eq!(m["bannedAddresses"], serde_json::json!(["0xa"]));
}

#[test]
fn metadata_append_dedups_and_creates_missing_array() {
    let out = metadata_with_appended(serde_json::Map::new(), SCENE_ADMINS_FIELD, "0xadmin")
        .expect("a missing field must be created");
    assert_eq!(out[SCENE_ADMINS_FIELD], serde_json::json!(["0xadmin"]));
    let existing = parse_room_metadata(Some(r#"{"sceneAdmins":["0xadmin"]}"#));
    assert!(metadata_with_appended(existing, SCENE_ADMINS_FIELD, "0xadmin").is_none());
}

#[test]
fn metadata_remove_is_noop_when_absent_and_removes_when_present() {
    let m = parse_room_metadata(Some(r#"{"bannedAddresses":["0xa"]}"#));
    assert!(metadata_with_removed(m, BANNED_ADDRESSES_FIELD, "0xzzz").is_none());
    let m = parse_room_metadata(Some(r#"{"bannedAddresses":["0xa","0xb"]}"#));
    let out = metadata_with_removed(m, BANNED_ADDRESSES_FIELD, "0xa").unwrap();
    assert_eq!(out[BANNED_ADDRESSES_FIELD], serde_json::json!(["0xb"]));
    assert!(metadata_with_removed(serde_json::Map::new(), BANNED_ADDRESSES_FIELD, "0xa").is_none());
}

#[tokio::test]
async fn append_to_room_metadata_array_reads_then_writes_back() {
    let (host, rx) = capture_seq(vec![
        r#"{"rooms":[{"name":"scene:abc","metadata":"{\"bannedAddresses\":[\"0xold\"]}"}]}"#,
        "{}",
    ])
    .await;
    let http = reqwest::Client::new();
    let client = RoomServiceClient::new(&http, &host, "devkey", "devsecret");
    client
        .append_to_room_metadata_array("scene:abc", BANNED_ADDRESSES_FIELD, "0xnew")
        .await
        .unwrap();
    let caps = rx.await.unwrap();
    assert_eq!(caps.len(), 2);
    assert!(caps[0].line.contains("ListRooms"));
    assert!(caps[1].line.contains("UpdateRoomMetadata"));
    assert_eq!(caps[1].body["room"], "scene:abc");
    let written: serde_json::Value =
        serde_json::from_str(caps[1].body["metadata"].as_str().unwrap()).unwrap();
    assert_eq!(
        written["bannedAddresses"],
        serde_json::json!(["0xold", "0xnew"])
    );
}

#[tokio::test]
async fn append_is_noop_when_value_already_present() {
    let (host, rx) = capture_seq(vec![
        r#"{"rooms":[{"name":"scene:abc","metadata":"{\"bannedAddresses\":[\"0xhere\"]}"}]}"#,
    ])
    .await;
    let http = reqwest::Client::new();
    let client = RoomServiceClient::new(&http, &host, "devkey", "devsecret");
    client
        .append_to_room_metadata_array("scene:abc", BANNED_ADDRESSES_FIELD, "0xhere")
        .await
        .unwrap();
    let caps = rx.await.unwrap();
    assert_eq!(
        caps.len(),
        1,
        "an already-present value must not trigger a write"
    );
    assert!(caps[0].line.contains("ListRooms"));
}

#[tokio::test]
async fn metadata_write_is_noop_for_missing_room() {
    let (host, rx) = capture_seq(vec![r#"{"rooms":[]}"#]).await;
    let http = reqwest::Client::new();
    let client = RoomServiceClient::new(&http, &host, "devkey", "devsecret");
    client
        .remove_from_room_metadata_array("scene:gone", BANNED_ADDRESSES_FIELD, "0xa")
        .await
        .unwrap();
    let caps = rx.await.unwrap();
    assert_eq!(caps.len(), 1);
    assert!(caps[0].line.contains("ListRooms"));
}
