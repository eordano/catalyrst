use catalyrst_archipelago::config::{
    AuthConfig, ClusterConfig, Config, GossipConfig, LivekitConfig, ServerConfig,
};
use catalyrst_archipelago::{api_router, build_state};
use reqwest::header::ORIGIN;
use reqwest::Method;

fn test_config(require_signed_challenge: bool) -> Config {
    Config {
        http_host: "127.0.0.1".into(),
        http_port: 0,
        cluster: ClusterConfig::default(),
        server: ServerConfig::default(),
        auth: AuthConfig {
            require_signed_challenge,
            challenge_ttl_secs: 120,
            signature_max_age_secs: 300,
            deny_list_url: None,
        },
        livekit: LivekitConfig::default(),
        gossip: GossipConfig::default(),
        content_database_url: None,
        content_base_url: String::new(),
        commit_hash: "deadbeef".into(),
    }
}

async fn start_server(require_signed_challenge: bool) -> u16 {
    let state = build_state(&test_config(require_signed_challenge))
        .await
        .expect("state");
    let app = api_router().with_state(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    port
}

const ORIGIN_VALUE: &str = "https://play.decentraland.org";

#[tokio::test]
async fn cors_headers_on_every_route() {
    let port = start_server(false).await;
    let client = reqwest::Client::new();

    let routes = [
        "/status",
        "/ping",
        "/peers",
        "/comms/peers",
        "/peers/0xnope",
        "/islands",
        "/islands/nope",
        "/parcels",
        "/hot-scenes",
        "/core-status",
        "/stats/health",
        "/gossip/info",
    ];
    for path in routes {
        let resp = client
            .get(format!("http://127.0.0.1:{port}{path}"))
            .header(ORIGIN, ORIGIN_VALUE)
            .send()
            .await
            .expect("request");
        assert_eq!(
            resp.headers()
                .get("access-control-allow-origin")
                .and_then(|v| v.to_str().ok()),
            Some("*"),
            "{path} allow-origin"
        );
        assert_eq!(
            resp.headers()
                .get("access-control-expose-headers")
                .and_then(|v| v.to_str().ok()),
            Some("*"),
            "{path} expose-headers"
        );
    }

    let preflight = client
        .request(Method::OPTIONS, format!("http://127.0.0.1:{port}/peers"))
        .header(ORIGIN, ORIGIN_VALUE)
        .header("access-control-request-method", "POST")
        .send()
        .await
        .expect("preflight");
    assert!(
        preflight.status().is_success(),
        "preflight status {}",
        preflight.status()
    );
    assert_eq!(
        preflight
            .headers()
            .get("access-control-allow-origin")
            .and_then(|v| v.to_str().ok()),
        Some("*"),
        "preflight allow-origin"
    );
    assert_eq!(
        preflight
            .headers()
            .get("access-control-allow-methods")
            .and_then(|v| v.to_str().ok()),
        Some("*"),
        "preflight allow-methods"
    );
    assert_eq!(
        preflight
            .headers()
            .get("access-control-allow-headers")
            .and_then(|v| v.to_str().ok()),
        Some("*"),
        "preflight allow-headers"
    );

    let err_resp = client
        .post(format!("http://127.0.0.1:{port}/auth/challenge"))
        .header(ORIGIN, ORIGIN_VALUE)
        .header("content-type", "application/json")
        .body("{}")
        .send()
        .await
        .expect("error request");
    assert_eq!(err_resp.status().as_u16(), 400);
    assert_eq!(
        err_resp
            .headers()
            .get("access-control-allow-origin")
            .and_then(|v| v.to_str().ok()),
        Some("*"),
        "error path allow-origin"
    );
}

#[tokio::test]
async fn error_bodies_byte_exact_vs_upstream() {
    let port = start_server(false).await;
    let client = reqwest::Client::new();

    async fn post(client: &reqwest::Client, port: u16, path: &str, body: &str) -> (u16, String) {
        let resp = client
            .post(format!("http://127.0.0.1:{port}{path}"))
            .header("content-type", "application/json")
            .body(body.to_string())
            .send()
            .await
            .expect("request");
        let status = resp.status().as_u16();
        let text = resp.text().await.expect("text");
        (status, text)
    }

    let (status, text) = post(&client, port, "/auth/challenge", "{}").await;
    assert_eq!(status, 400);
    assert_eq!(
        text,
        "{\"error\":\"invalid json body: missing field `address` at line 1 column 2\"}"
    );

    let (status, text) = post(&client, port, "/heartbeat", "{}").await;
    assert_eq!(status, 400);
    assert_eq!(
        text,
        "{\"error\":\"invalid json body: missing field `address` at line 1 column 2\"}"
    );

    let (status, text) = post(
        &client,
        port,
        "/heartbeat",
        "{\"address\":\"\",\"position\":[0,0,0],\"parcel\":[0,0]}",
    )
    .await;
    assert_eq!(status, 400);
    assert_eq!(text, "{\"ok\":false,\"error\":\"missing address\"}");

    let (status, text) = post(&client, port, "/auth/challenge", "{\"address\":\"\"}").await;
    assert_eq!(status, 400);
    assert_eq!(text, "{\"error\":\"missing address\"}");

    let (status, text) = post(&client, port, "/gossip/heartbeat", "{}").await;
    assert_eq!(status, 400);
    assert_eq!(text, "{\"error\":\"missing X-Archipelago-Node\"}");

    let (status, text) = post(
        &client,
        port,
        "/auth/livekit-token",
        "{\"address\":\"0x0000000000000000000000000000000000000001\",\"challenge\":\"deadbeef\",\"room\":\"\",\"auth_chain\":[{\"type\":\"SIGNER\",\"payload\":\"0x0000000000000000000000000000000000000001\",\"signature\":\"\"}]}",
    )
    .await;
    assert_eq!(status, 401);
    let body: serde_json::Value = serde_json::from_str(&text).expect("json");
    let obj = body.as_object().expect("object");
    assert_eq!(obj.len(), 1, "single-key error object");
    assert!(obj.contains_key("error"), "error key present");
}

#[tokio::test]
async fn heartbeat_auth_required_body_byte_exact() {
    let port = start_server(true).await;
    let client = reqwest::Client::new();

    let resp = client
        .post(format!("http://127.0.0.1:{port}/heartbeat"))
        .header("content-type", "application/json")
        .body("{\"address\":\"0xabc\",\"position\":[0,0,0],\"parcel\":[0,0]}")
        .send()
        .await
        .expect("request");
    assert_eq!(resp.status().as_u16(), 401);
    assert_eq!(
        resp.headers()
            .get("access-control-allow-origin")
            .and_then(|v| v.to_str().ok()),
        Some("*"),
        "auth-required path allow-origin"
    );
    let text = resp.text().await.expect("text");
    assert_eq!(
        text,
        "{\"ok\":false,\"error\":\"auth required; use /ws after /auth/challenge\"}"
    );
}
