use catalyrst_crypto::signed_fetch::build_payload;
use catalyrst_crypto::Wallet;
use catalyrst_drpc::client::{RpcClient, RpcClientModule, ServiceClient};
use catalyrst_drpc::transports::web_sockets::tungstenite::WebSocketClient;
use catalyrst_drpc::transports::web_sockets::{
    Message as WsMessage, WebSocket, WebSocketTransport,
};
use catalyrst_drpc::transports::Transport;
use catalyrst_social_service::rpc::proto::v2::*;

#[derive(Clone, PartialEq, prost::Message)]
struct EmptyPayload {}
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::time::Duration;

const CALL_TIMEOUT: Duration = Duration::from_secs(15);

struct Identity {
    root: Wallet,
    ephemeral: Wallet,
    ephemeral_link: Value,
}

impl Identity {
    fn load(key_path: &str) -> anyhow::Result<Self> {
        let raw = std::fs::read_to_string(key_path)?;
        let raw = raw.trim();
        let root = Wallet::from_hex(raw).map_err(|e| anyhow::anyhow!("root key: {e:?}"))?;
        let mut h = Sha256::new();
        h.update(raw.as_bytes());
        h.update(b"dcl-one social-probe ephemeral v1");
        let eph_hex = hex::encode(h.finalize());
        let ephemeral =
            Wallet::from_hex(&eph_hex).map_err(|e| anyhow::anyhow!("ephemeral key: {e:?}"))?;
        let expiration = (chrono::Utc::now() + chrono::Duration::days(10))
            .format("%Y-%m-%dT%H:%M:%S%.3fZ")
            .to_string();
        let payload = format!(
            "Decentraland Login\nEphemeral address: {}\nExpiration: {}",
            ephemeral.address(),
            expiration
        );
        let signature = root
            .sign_message(payload.as_bytes())
            .map_err(|e| anyhow::anyhow!("ephemeral link: {e:?}"))?;
        let ephemeral_link = json!({
            "type": "ECDSA_EPHEMERAL",
            "payload": payload,
            "signature": signature,
        });
        Ok(Self {
            root,
            ephemeral,
            ephemeral_link,
        })
    }

    fn address(&self) -> String {
        self.root.address().to_lowercase()
    }

    fn chain_headers(
        &self,
        method: &str,
        path: &str,
        metadata: &str,
    ) -> anyhow::Result<Vec<(String, String)>> {
        let ts = chrono::Utc::now().timestamp_millis().to_string();
        let payload = build_payload(method, path, &ts, metadata);
        let entity_sig = self
            .ephemeral
            .sign_message(payload.as_bytes())
            .map_err(|e| anyhow::anyhow!("entity sig: {e:?}"))?;
        let signer_link = json!({
            "type": "SIGNER",
            "payload": self.root.address(),
            "signature": "",
        });
        let entity_link = json!({
            "type": "ECDSA_SIGNED_ENTITY",
            "payload": payload,
            "signature": entity_sig,
        });
        Ok(vec![
            ("x-identity-auth-chain-0".into(), signer_link.to_string()),
            (
                "x-identity-auth-chain-1".into(),
                self.ephemeral_link.to_string(),
            ),
            ("x-identity-auth-chain-2".into(), entity_link.to_string()),
            ("x-identity-timestamp".into(), ts),
            ("x-identity-metadata".into(), metadata.into()),
        ])
    }
}

struct Social<T: Transport + 'static>(RpcClientModule<T>);

impl<T: Transport + 'static> ServiceClient<T> for Social<T> {
    fn set_client_module(client_module: RpcClientModule<T>) -> Self {
        Self(client_module)
    }
}

fn profile_json(f: &FriendProfile) -> Value {
    json!({
        "address": f.address,
        "name": f.name,
        "has_claimed_name": f.has_claimed_name,
        "profile_picture_url": f.profile_picture_url,
        "name_color": f.name_color.as_ref().map(|c| format!("{:?}", c)),
    })
}

fn requests_json(resp: &PaginatedFriendshipRequestsResponse) -> Value {
    match &resp.response {
        Some(paginated_friendship_requests_response::Response::Requests(reqs)) => json!({
            "requests": reqs
                .requests
                .iter()
                .map(|r| json!({
                    "id": r.id,
                    "friend": r.friend.as_ref().map(profile_json),
                    "created_at": r.created_at,
                    "message": r.message,
                }))
                .collect::<Vec<_>>(),
            "pagination": resp.pagination_data.as_ref().map(|p| json!({"total": p.total, "page": p.page})),
        }),
        Some(other) => json!({ "error": format!("{:?}", other) }),
        None => json!({ "error": "empty response" }),
    }
}

fn status_json(resp: &GetFriendshipStatusResponse) -> Value {
    match &resp.response {
        Some(get_friendship_status_response::Response::Accepted(ok)) => {
            json!({ "status": ok.status().as_str_name(), "message": ok.message })
        }
        Some(other) => json!({ "error": format!("{:?}", other) }),
        None => json!({ "error": "empty response" }),
    }
}

fn upsert_json(resp: &UpsertFriendshipResponse) -> Value {
    match &resp.response {
        Some(upsert_friendship_response::Response::Accepted(ok)) => json!({
            "accepted": {
                "id": ok.id,
                "created_at": ok.created_at,
                "friend": ok.friend.as_ref().map(profile_json),
                "message": ok.message,
            }
        }),
        Some(other) => json!({ "error": format!("{:?}", other) }),
        None => json!({ "error": "empty response" }),
    }
}

async fn call<T, Req, Resp>(
    module: &Social<T>,
    name: &str,
    payload: Req,
    render: impl Fn(&Resp) -> Value,
) -> Value
where
    T: Transport + 'static,
    Req: prost::Message + Default,
    Resp: prost::Message + Default,
{
    match tokio::time::timeout(
        CALL_TIMEOUT,
        module.0.call_unary_procedure::<Resp, Req>(name, payload),
    )
    .await
    {
        Ok(Ok(resp)) => json!({ "ok": render(&resp) }),
        Ok(Err(e)) => json!({ "err": format!("{:?}", e) }),
        Err(_) => json!({ "err": "timeout" }),
    }
}

fn pagination() -> Option<Pagination> {
    Some(Pagination {
        limit: 100,
        offset: 0,
    })
}

struct WsArgs {
    url: String,
    key: String,
    sign_path: Option<String>,
    peer: Option<String>,
    action: Option<String>,
    message: Option<String>,
    handshake_only: bool,
}

async fn ws_probe(args: WsArgs) -> anyhow::Result<Value> {
    let identity = Identity::load(&args.key)?;
    let uri: axum::http::Uri = args.url.parse()?;
    let sign_path = args
        .sign_path
        .clone()
        .unwrap_or_else(|| uri.path().to_string());

    let ws = WebSocketClient::connect(&args.url)
        .await
        .map_err(|e| anyhow::anyhow!("connect: {e:?}"))?;
    let headers = identity.chain_headers("get", &sign_path, "{}")?;
    let frame: serde_json::Map<String, Value> = headers
        .into_iter()
        .map(|(k, v)| (k, Value::String(v)))
        .collect();
    ws.send(WsMessage::Text(Value::Object(frame).to_string()))
        .await
        .map_err(|e| anyhow::anyhow!("auth frame send: {e:?}"))?;

    let mut out = serde_json::Map::new();
    out.insert("target".into(), json!(args.url));
    out.insert("wallet".into(), json!(identity.address()));
    out.insert("signed_path".into(), json!(sign_path));

    let transport = WebSocketTransport::new(ws);
    let mut client = match tokio::time::timeout(CALL_TIMEOUT, RpcClient::new(transport)).await {
        Ok(Ok(c)) => c,
        Ok(Err(e)) => {
            out.insert("handshake".into(), json!({ "err": format!("{:?}", e) }));
            return Ok(Value::Object(out));
        }
        Err(_) => {
            out.insert(
                "handshake".into(),
                json!({ "err": "timeout waiting for rpc server ready (auth likely rejected)" }),
            );
            return Ok(Value::Object(out));
        }
    };
    out.insert("handshake".into(), json!({ "ok": "rpc server ready" }));
    if args.handshake_only {
        return Ok(Value::Object(out));
    }

    let port = client
        .create_port("social")
        .await
        .map_err(|e| anyhow::anyhow!("create_port: {e:?}"))?;
    let module = port
        .load_module::<Social<_>>("SocialService")
        .await
        .map_err(|e| anyhow::anyhow!("load_module: {e:?}"))?;

    if let (Some(action), Some(peer)) = (args.action.as_deref(), args.peer.as_deref()) {
        let user = Some(User {
            address: peer.to_string(),
        });
        let act = match action {
            "request" => upsert_friendship_payload::Action::Request(
                upsert_friendship_payload::RequestPayload {
                    user,
                    message: args.message.clone(),
                },
            ),
            "accept" => upsert_friendship_payload::Action::Accept(
                upsert_friendship_payload::AcceptPayload { user },
            ),
            "reject" => upsert_friendship_payload::Action::Reject(
                upsert_friendship_payload::RejectPayload { user },
            ),
            "delete" => upsert_friendship_payload::Action::Delete(
                upsert_friendship_payload::DeletePayload { user },
            ),
            "cancel" => upsert_friendship_payload::Action::Cancel(
                upsert_friendship_payload::CancelPayload { user },
            ),
            other => anyhow::bail!("unknown action {other}"),
        };
        let result = call(
            &module,
            "UpsertFriendship",
            UpsertFriendshipPayload { action: Some(act) },
            upsert_json,
        )
        .await;
        out.insert(format!("upsert_{action}"), result);
    }

    let friends = call(
        &module,
        "GetFriends",
        GetFriendsPayload {
            pagination: pagination(),
        },
        |r: &PaginatedFriendsProfilesResponse| {
            json!({
                "friends": r.friends.iter().map(profile_json).collect::<Vec<_>>(),
                "pagination": r.pagination_data.as_ref().map(|p| json!({"total": p.total, "page": p.page})),
            })
        },
    )
    .await;
    out.insert("get_friends".into(), friends);

    let pending = call(
        &module,
        "GetPendingFriendshipRequests",
        GetFriendshipRequestsPayload {
            pagination: pagination(),
        },
        requests_json,
    )
    .await;
    out.insert("get_pending_friendship_requests".into(), pending);

    let sent = call(
        &module,
        "GetSentFriendshipRequests",
        GetFriendshipRequestsPayload {
            pagination: pagination(),
        },
        requests_json,
    )
    .await;
    out.insert("get_sent_friendship_requests".into(), sent);

    let blocked = call(
        &module,
        "GetBlockedUsers",
        GetBlockedUsersPayload {
            pagination: pagination(),
        },
        |r: &GetBlockedUsersResponse| {
            json!({
                "profiles": r.profiles.len(),
                "pagination": r.pagination_data.as_ref().map(|p| json!({"total": p.total})),
            })
        },
    )
    .await;
    out.insert("get_blocked_users".into(), blocked);

    let settings = call(
        &module,
        "GetSocialSettings",
        EmptyPayload {},
        |r: &GetSocialSettingsResponse| match &r.response {
            Some(get_social_settings_response::Response::Ok(ok)) => {
                json!({ "settings": format!("{:?}", ok.settings) })
            }
            Some(other) => json!({ "error": format!("{:?}", other) }),
            None => json!({ "error": "empty response" }),
        },
    )
    .await;
    out.insert("get_social_settings".into(), settings);

    if let Some(peer) = args.peer.as_deref() {
        let status = call(
            &module,
            "GetFriendshipStatus",
            GetFriendshipStatusPayload {
                user: Some(User {
                    address: peer.to_string(),
                }),
            },
            status_json,
        )
        .await;
        out.insert("get_friendship_status".into(), status);
    }

    Ok(Value::Object(out))
}

fn shape(v: &Value) -> Value {
    match v {
        Value::Null => json!("null"),
        Value::Bool(_) => json!("bool"),
        Value::Number(_) => json!("number"),
        Value::String(_) => json!("string"),
        Value::Array(items) => match items.first() {
            Some(first) => json!([shape(first)]),
            None => json!([]),
        },
        Value::Object(map) => {
            let mut out = serde_json::Map::new();
            for (k, val) in map {
                out.insert(k.clone(), shape(val));
            }
            Value::Object(out)
        }
    }
}

struct RestArgs {
    base: String,
    key: String,
    paths: Vec<String>,
    raw: bool,
}

async fn rest_probe(args: RestArgs) -> anyhow::Result<Value> {
    let identity = Identity::load(&args.key)?;
    let client = reqwest::Client::builder().timeout(CALL_TIMEOUT).build()?;
    let mut out = serde_json::Map::new();
    out.insert("target".into(), json!(args.base));
    out.insert("wallet".into(), json!(identity.address()));
    let mut results = serde_json::Map::new();
    for path in &args.paths {
        let sign_path = path.split('?').next().unwrap_or(path);
        let headers = identity.chain_headers("get", sign_path, "{}")?;
        let mut req = client.get(format!("{}{}", args.base, path));
        for (k, v) in headers {
            req = req.header(k, v);
        }
        let entry = match req.send().await {
            Ok(resp) => {
                let status = resp.status().as_u16();
                match resp.json::<Value>().await {
                    Ok(body) => {
                        if args.raw {
                            json!({ "status": status, "body": body })
                        } else {
                            json!({ "status": status, "shape": shape(&body) })
                        }
                    }
                    Err(e) => json!({ "status": status, "body_err": e.to_string() }),
                }
            }
            Err(e) => json!({ "err": e.to_string() }),
        };
        results.insert(path.clone(), entry);
    }
    out.insert("results".into(), Value::Object(results));
    Ok(Value::Object(out))
}

fn arg_value(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

fn usage() -> ! {
    eprintln!(
        "usage:\n  social-probe ws --url <ws(s)://host[/path]> --key <keyfile> \
         [--sign-path <path>] [--peer <0xaddr>] [--action request|accept|reject|cancel|delete] \
         [--message <text>] [--handshake-only]\n  social-probe rest --base <http(s)://host> \
         --key <keyfile> --paths <p1,p2,...> [--raw]"
    );
    std::process::exit(2)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mode = args.first().cloned().unwrap_or_default();
    let result = match mode.as_str() {
        "ws" => {
            let (Some(url), Some(key)) = (arg_value(&args, "--url"), arg_value(&args, "--key"))
            else {
                usage()
            };
            ws_probe(WsArgs {
                url,
                key,
                sign_path: arg_value(&args, "--sign-path"),
                peer: arg_value(&args, "--peer"),
                action: arg_value(&args, "--action"),
                message: arg_value(&args, "--message"),
                handshake_only: args.iter().any(|a| a == "--handshake-only"),
            })
            .await?
        }
        "rest" => {
            let (Some(base), Some(key)) = (arg_value(&args, "--base"), arg_value(&args, "--key"))
            else {
                usage()
            };
            let paths = arg_value(&args, "--paths")
                .map(|p| p.split(',').map(str::to_string).collect())
                .unwrap_or_else(|| {
                    vec![
                        "/v1/communities?limit=5".into(),
                        "/v1/mutes".into(),
                        "/v1/community-voice-chats/active".into(),
                    ]
                });
            rest_probe(RestArgs {
                base,
                key,
                paths,
                raw: args.iter().any(|a| a == "--raw"),
            })
            .await?
        }
        _ => usage(),
    };
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}
