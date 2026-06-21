use std::{
    collections::HashMap,
    net::SocketAddr,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::SystemTime,
};

use alloy::signers::{local::PrivateKeySigner, Signer};
use anyhow::{bail, Context, Result};
use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{Html, IntoResponse},
    routing::{get, post},
    Json, Router,
};
use base64::Engine as _;
use catalyrst_hashing::hash_bytes_v1;
use clap::Parser;
use serde::Deserialize;
use serde_json::{json, Value};

fn load_or_create_key(path: &std::path::Path) -> Result<PrivateKeySigner> {
    if path.exists() {
        let raw = std::fs::read_to_string(path)
            .with_context(|| format!("reading key {}", path.display()))?;
        let hexs = raw.trim().trim_start_matches("0x");
        let wallet: PrivateKeySigner = hexs.parse().context("parsing private key hex")?;
        Ok(wallet)
    } else {
        let wallet = PrivateKeySigner::random();
        let hexs = format!("0x{}", hex::encode(wallet.to_bytes()));
        std::fs::write(path, format!("{hexs}\n"))
            .with_context(|| format!("writing key {}", path.display()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
        }
        tracing::warn!(
            "generated NEW deploy key at {} — address {} — keep the file secret",
            path.display(),
            addr_str(&wallet)
        );
        Ok(wallet)
    }
}

fn addr_str(w: &PrivateKeySigner) -> String {
    format!("{:#x}", w.address())
}

async fn eip191_sign(w: &PrivateKeySigner, msg: &str) -> Result<String> {
    let sig = w
        .sign_message(msg.as_bytes())
        .await
        .context("EIP-191 sign")?;
    Ok(sig.to_string())
}

fn read_world(args: &Args) -> Result<String> {
    if let Some(w) = &args.world {
        return Ok(w.to_lowercase());
    }
    let scene_json = std::fs::read(args.scene_dir.join("scene.json"))
        .context("reading scene.json for world name")?;
    let meta: Value = serde_json::from_slice(&scene_json).context("parsing scene.json")?;
    meta.get("worldConfiguration")
        .and_then(|w| w.get("name"))
        .and_then(|n| n.as_str())
        .map(|s| s.to_lowercase())
        .context("no worldConfiguration.name in scene.json and no --world given")
}

#[derive(Parser, Debug)]
#[command(about = "One-shot wallet-signature page + Decentraland scene/World deployer")]
struct Args {
    #[arg(long, default_value = ".")]
    scene_dir: PathBuf,
    #[arg(long, default_value = "https://worlds-content-server.decentraland.org")]
    content_server: String,
    #[arg(long, default_value = "0.0.0.0")]
    bind: String,
    #[arg(long, default_value_t = 8099)]
    port: u16,
    #[arg(long)]
    world: Option<String>,
    #[arg(long)]
    file: Vec<String>,
    #[arg(long)]
    sign_key: Option<PathBuf>,
    #[arg(long, default_value_t = false)]
    grant: bool,
    #[arg(long, default_value = "deployment")]
    permission: String,
    #[arg(long, default_value_t = false)]
    serve_delegations: bool,
    #[arg(long, default_value_t = 3600)]
    delegation_ttl_secs: u64,
    #[arg(long)]
    delegation_token: Option<String>,
}

fn is_ignored_dir(name: &str) -> bool {
    name.starts_with('.') || matches!(name, "node_modules" | "src" | "dist" | "export")
}

fn is_ignored_file(name: &str) -> bool {
    if name.starts_with('.') {
        return true;
    }
    if matches!(
        name,
        "package.json"
            | "package-lock.json"
            | "yarn.lock"
            | "yarn-lock.json"
            | "tsconfig.json"
            | "tslint.json"
            | "build.json"
            | "builder.json"
            | "Dockerfile"
            | "README.md"
    ) {
        return true;
    }
    let ext = name.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    matches!(
        ext.as_str(),
        "ts" | "tsx" | "map" | "blend" | "fbx" | "zip" | "rar" | "md"
    )
}

fn collect_files(dir: &std::path::Path, base: &std::path::Path, out: &mut Vec<String>) {
    let rd = match std::fs::read_dir(dir) {
        Ok(x) => x,
        Err(_) => return,
    };
    for entry in rd.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if path.is_dir() {
            if !is_ignored_dir(&name) {
                collect_files(&path, base, out);
            }
        } else if !is_ignored_file(&name) {
            if let Ok(rel) = path.strip_prefix(base) {
                out.push(rel.to_string_lossy().replace('\\', "/"));
            }
        }
    }
}

struct Prepared {
    world: String,
    files: Vec<(String, String, Vec<u8>)>,
    content_server: String,
    pointers: Vec<String>,
    metadata: Value,
}

fn build_entity(p: &Prepared, timestamp: i64) -> (String, Vec<u8>) {
    let content: Vec<Value> = p
        .files
        .iter()
        .map(|(f, h, _)| json!({ "file": f, "hash": h }))
        .collect();
    let entity = json!({
        "version": "v3",
        "type": "scene",
        "pointers": p.pointers,
        "timestamp": timestamp,
        "content": content,
        "metadata": p.metadata,
    });
    let entity_bytes = serde_json::to_vec(&entity).expect("serializing entity");
    let entity_id = hash_bytes_v1(&entity_bytes);
    (entity_id, entity_bytes)
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn prepare(args: &Args) -> Result<Prepared> {
    let scene_json_path = args.scene_dir.join("scene.json");
    let scene_json_bytes = std::fs::read(&scene_json_path)
        .with_context(|| format!("reading {}", scene_json_path.display()))?;
    let scene_meta: Value =
        serde_json::from_slice(&scene_json_bytes).context("parsing scene.json")?;

    let world = args
        .world
        .clone()
        .or_else(|| {
            scene_meta
                .get("worldConfiguration")
                .and_then(|w| w.get("name"))
                .and_then(|n| n.as_str())
                .map(str::to_string)
        })
        .context("no worldConfiguration.name in scene.json and no --world given")?
        .to_lowercase();

    let mut rel_paths: Vec<String> = Vec::new();
    collect_files(&args.scene_dir, &args.scene_dir, &mut rel_paths);
    for extra in &args.file {
        if !rel_paths.iter().any(|r| r == extra) {
            rel_paths.push(extra.clone());
        }
    }
    if !rel_paths.iter().any(|r| r == "scene.json") {
        rel_paths.push("scene.json".to_string());
    }
    if let Some(main) = scene_meta.get("main").and_then(|m| m.as_str()) {
        if !rel_paths.iter().any(|r| r == main) {
            bail!("scene main '{main}' not found among collected content files");
        }
    }
    rel_paths.sort();
    rel_paths.dedup();

    let mut files = Vec::new();
    for rel in &rel_paths {
        let bytes = if rel == "scene.json" {
            scene_json_bytes.clone()
        } else {
            let p = args.scene_dir.join(rel);
            std::fs::read(&p).with_context(|| format!("reading content file {}", p.display()))?
        };
        let hash = hash_bytes_v1(&bytes);
        files.push((rel.clone(), hash, bytes));
    }

    let pointers: Vec<String> = scene_meta
        .get("scene")
        .and_then(|s| s.get("parcels"))
        .and_then(|p| p.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    if pointers.is_empty() {
        bail!("scene.parcels is missing/empty in scene.json — cannot form entity pointers");
    }

    Ok(Prepared {
        world,
        files,
        content_server: args.content_server.trim_end_matches('/').to_string(),
        pointers,
        metadata: scene_meta,
    })
}

#[derive(Clone)]
struct AppState {
    prepared: Arc<Prepared>,
    pending: Arc<Mutex<HashMap<String, Vec<u8>>>>,
}

async fn index() -> Html<&'static str> {
    Html(PAGE)
}

async fn info(State(st): State<AppState>) -> Json<Value> {
    let p = &st.prepared;
    let ts = now_ms();
    let (entity_id, entity_bytes) = build_entity(p, ts);
    {
        let mut pending = st.pending.lock().unwrap();
        if pending.len() > 32 {
            pending.clear();
        }
        pending.insert(entity_id.clone(), entity_bytes);
    }
    Json(json!({
        "world": p.world,
        "entityId": entity_id,
        "contentServer": p.content_server,
        "timestamp": ts,
        "playUrl": format!("https://decentraland.org/play/?realm={}", p.world),
        "files": p.files.iter().map(|(f, h, b)| json!({"file": f, "hash": h, "size": b.len()})).collect::<Vec<_>>(),
    }))
}

#[derive(Deserialize)]
struct SignReq {
    address: String,
    signature: String,
    #[serde(rename = "entityId")]
    entity_id: String,
}

async fn sign(State(st): State<AppState>, Json(req): Json<SignReq>) -> Json<Value> {
    let entity_bytes = st.pending.lock().unwrap().get(&req.entity_id).cloned();
    let Some(entity_bytes) = entity_bytes else {
        return Json(json!({
            "ok": false,
            "error": "unknown/stale entity id — reload the page and sign again (the deployment expires after ~5 min)"
        }));
    };
    match deploy(
        &st.prepared,
        &req.entity_id,
        &entity_bytes,
        &req.address,
        &req.signature,
    )
    .await
    {
        Ok(msg) => {
            tracing::info!("deploy succeeded: {msg}");
            Json(json!({ "ok": true, "message": msg }))
        }
        Err(e) => {
            tracing::error!("deploy failed: {e:#}");
            Json(json!({ "ok": false, "error": format!("{e:#}") }))
        }
    }
}

async fn deploy(
    p: &Prepared,
    entity_id: &str,
    entity_bytes: &[u8],
    address: &str,
    signature: &str,
) -> Result<String> {
    let auth_chain = json!([
        { "type": "SIGNER", "payload": address, "signature": "" },
        { "type": "ECDSA_SIGNED_ENTITY", "payload": entity_id, "signature": signature },
    ]);

    let mut form = reqwest::multipart::Form::new()
        .text("entityId", entity_id.to_string())
        .text("authChain", serde_json::to_string(&auth_chain)?)
        .text("authChain[0][type]", "SIGNER")
        .text("authChain[0][payload]", address.to_string())
        .text("authChain[0][signature]", "")
        .text("authChain[1][type]", "ECDSA_SIGNED_ENTITY")
        .text("authChain[1][payload]", entity_id.to_string())
        .text("authChain[1][signature]", signature.to_string());

    form = form.part(
        entity_id.to_string(),
        reqwest::multipart::Part::bytes(entity_bytes.to_vec())
            .file_name(entity_id.to_string())
            .mime_str("application/json")?,
    );
    for (_rel, hash, bytes) in &p.files {
        form = form.part(
            hash.clone(),
            reqwest::multipart::Part::bytes(bytes.clone()).file_name(hash.clone()),
        );
    }

    let url = format!("{}/entities", p.content_server);
    tracing::info!(
        "uploading deployment to {url} (world={}, entity={})",
        p.world,
        entity_id
    );
    let resp = reqwest::Client::new()
        .post(&url)
        .multipart(form)
        .send()
        .await
        .context("posting to content server")?;
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    if status.is_success() {
        Ok(format!(
            "Deployed to {} \u{2713} (HTTP {}). Live at https://decentraland.org/play/?realm={} — server: {}",
            p.world, status.as_u16(), p.world, body
        ))
    } else {
        bail!(
            "content server rejected the deployment (HTTP {}): {}",
            status.as_u16(),
            body
        )
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let args = Args::parse();
    let content_server = args.content_server.trim_end_matches('/').to_string();

    if args.serve_delegations {
        let keypath = args.sign_key.clone().context(
            "--serve-delegations requires --sign-key <path>: the authoritative key that signs scope claims",
        )?;
        let wallet = load_or_create_key(&keypath)?;
        return run_delegation_minter(&args, wallet).await;
    }

    if args.grant {
        let keypath = args.sign_key.clone().context(
            "--grant requires --sign-key <path>: the key whose address gets the permission",
        )?;
        let wallet = load_or_create_key(&keypath)?;
        let world = read_world(&args)?;
        return run_grant(&args, &content_server, &world, &wallet).await;
    }

    let prepared = Arc::new(prepare(&args).context("preparing deployment")?);

    let (sample_id, _) = build_entity(&prepared, now_ms());
    tracing::info!("world       = {}", prepared.world);
    tracing::info!(
        "entity id   = {} (sample; re-minted at sign time)",
        sample_id
    );
    tracing::info!("content     = {} files", prepared.files.len());
    for (f, h, b) in &prepared.files {
        tracing::info!("   {:<24} {} ({} bytes)", f, h, b.len());
    }
    tracing::info!("target      = {}/entities", prepared.content_server);

    if let Some(keypath) = args.sign_key.clone() {
        let wallet = load_or_create_key(&keypath)?;
        let address = addr_str(&wallet);
        let ts = now_ms();
        let (entity_id, entity_bytes) = build_entity(&prepared, ts);
        let signature = eip191_sign(&wallet, &entity_id).await?;
        tracing::info!("headless deploy signed by {address} (entity {entity_id})");
        match deploy(&prepared, &entity_id, &entity_bytes, &address, &signature).await {
            Ok(msg) => {
                tracing::info!("{msg}");
                return Ok(());
            }
            Err(e) => {
                tracing::error!("{e:#}");
                bail!(
                    "headless deploy failed. If this is a permission error, grant {address} \
                     the 'deployment' permission once:  --grant --sign-key {}",
                    keypath.display()
                );
            }
        }
    }

    let world_disp = prepared.world.clone();
    let state = AppState {
        prepared,
        pending: Arc::new(Mutex::new(HashMap::new())),
    };
    let app = Router::new()
        .route("/", get(index))
        .route("/api/info", get(info))
        .route("/sign", post(sign))
        .with_state(state);

    let addr: SocketAddr = format!("{}:{}", args.bind, args.port)
        .parse()
        .context("parsing bind address")?;
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .with_context(|| format!("binding {addr}"))?;
    tracing::info!("");
    tracing::info!("  ┌───────────────────────────────────────────────────────────");
    tracing::info!("  │  Open the signing page in a browser with your wallet:");
    tracing::info!("  │    http://{}:{}/", args.bind, args.port);
    tracing::info!("  │  Connect the wallet that controls  {}", world_disp);
    tracing::info!("  │  then Sign → it deploys and this process exits on success.");
    tracing::info!("  └───────────────────────────────────────────────────────────");
    tracing::info!("");

    axum::serve(listener, app).await.context("serving")?;
    Ok(())
}

#[derive(Clone)]
struct GrantState {
    content_server: String,
    world: String,
    deploy_address: String,
    permission: String,
    path: String,
    shutdown: Arc<Mutex<Option<tokio::sync::oneshot::Sender<()>>>>,
}

async fn grant_index() -> Html<&'static str> {
    Html(GRANT_PAGE)
}

async fn grant_info(State(st): State<GrantState>) -> Json<Value> {
    Json(json!({
        "world": st.world,
        "deployAddress": st.deploy_address,
        "contentServer": st.content_server,
        "permission": st.permission,
        "path": st.path,
    }))
}

#[derive(Deserialize)]
struct GrantReq {
    #[serde(rename = "ownerAddress")]
    owner_address: String,
    signature: String,
    timestamp: i64,
}

async fn grant_submit(State(st): State<GrantState>, Json(req): Json<GrantReq>) -> Json<Value> {
    match do_grant(&st, &req).await {
        Ok(msg) => {
            tracing::info!("{msg}");
            if let Some(tx) = st.shutdown.lock().unwrap().take() {
                let _ = tx.send(());
            }
            Json(json!({ "ok": true, "message": msg }))
        }
        Err(e) => {
            tracing::error!("grant failed: {e:#}");
            Json(json!({ "ok": false, "error": format!("{e:#}") }))
        }
    }
}

async fn do_grant(st: &GrantState, req: &GrantReq) -> Result<String> {
    let payload = format!("put:{}:{}:{{}}", st.path, req.timestamp).to_lowercase();
    let link0 = json!({ "type": "SIGNER", "payload": req.owner_address, "signature": "" });
    let link1 =
        json!({ "type": "ECDSA_SIGNED_ENTITY", "payload": payload, "signature": req.signature });
    let url = format!("{}{}", st.content_server, st.path);
    tracing::info!(
        "granting '{}' on {} to {} (owner {})",
        st.permission,
        st.world,
        st.deploy_address,
        req.owner_address
    );
    let resp = reqwest::Client::new()
        .put(&url)
        .header("x-identity-auth-chain-0", link0.to_string())
        .header("x-identity-auth-chain-1", link1.to_string())
        .header("x-identity-timestamp", req.timestamp.to_string())
        .header("x-identity-metadata", "{}")
        .send()
        .await
        .context("PUT permission to content server")?;
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    if status.is_success() {
        Ok(format!(
            "Granted '{}' on {} to {} \u{2713} (HTTP {}). Future deploys are unattended.",
            st.permission,
            st.world,
            st.deploy_address,
            status.as_u16()
        ))
    } else {
        bail!(
            "permission server rejected the grant (HTTP {}): {}",
            status.as_u16(),
            body
        )
    }
}

async fn run_grant(
    args: &Args,
    content_server: &str,
    world: &str,
    wallet: &PrivateKeySigner,
) -> Result<()> {
    let deploy_address = addr_str(wallet);
    let permission = args.permission.to_lowercase();
    if permission != "deployment" && permission != "streaming" {
        bail!("--permission must be 'deployment' or 'streaming' (got '{permission}')");
    }
    let path = format!(
        "/world/{}/permissions/{}/{}",
        world, permission, deploy_address
    );
    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    let st = GrantState {
        content_server: content_server.to_string(),
        world: world.to_string(),
        deploy_address: deploy_address.clone(),
        permission: permission.clone(),
        path,
        shutdown: Arc::new(Mutex::new(Some(tx))),
    };
    let app = Router::new()
        .route("/", get(grant_index))
        .route("/api/grant-info", get(grant_info))
        .route("/grant", post(grant_submit))
        .with_state(st);
    let addr: SocketAddr = format!("{}:{}", args.bind, args.port)
        .parse()
        .context("parsing bind address")?;
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .with_context(|| format!("binding {addr}"))?;
    tracing::info!("");
    tracing::info!("  ┌───────────────────────────────────────────────────────────");
    tracing::info!("  │  GRANT '{}' on  {}", permission, world);
    tracing::info!("  │  to deploy-key address:  {}", deploy_address);
    tracing::info!("  │  Open in a browser with the OWNER wallet of the World:");
    tracing::info!("  │    http://{}:{}/", args.bind, args.port);
    tracing::info!("  │  Sign once → permission set → future deploys need no wallet.");
    tracing::info!("  └───────────────────────────────────────────────────────────");
    tracing::info!("");
    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            let _ = rx.await;
        })
        .await
        .context("serving grant page")?;
    tracing::info!("grant complete — exiting.");
    Ok(())
}

const STORAGE_DELEGATION_PREFIX: &str = "Decentraland Authoritative Storage Delegation";

fn is_valid_parcel(parcel: &str) -> bool {
    fn is_coord(s: &str) -> bool {
        let digits = s.strip_prefix('-').unwrap_or(s);
        !digits.is_empty() && digits.len() <= 10 && digits.bytes().all(|b| b.is_ascii_digit())
    }
    parcel
        .split_once(',')
        .is_some_and(|(x, y)| is_coord(x) && is_coord(y))
}

fn validate_delegation_req(
    world: &str,
    scene_id: &str,
    parcel: &str,
) -> Result<(String, String, String), &'static str> {
    let world = world.trim().to_lowercase();
    if world.is_empty() {
        return Err("world must not be empty");
    }
    let scene_id = scene_id.trim();
    if scene_id.is_empty() {
        return Err("sceneId must not be empty");
    }
    if !is_valid_parcel(parcel) {
        return Err("parcel must be two comma-separated integer coordinates");
    }
    Ok((world, scene_id.to_string(), parcel.to_string()))
}

async fn mint_delegation_envelope(
    authoritative: &PrivateKeySigner,
    world: &str,
    scene_id: &str,
    parcel: &str,
    ttl: std::time::Duration,
) -> Result<String> {
    let ephemeral = PrivateKeySigner::random();
    let expiration = chrono::Utc::now() + chrono::Duration::seconds(ttl.as_secs() as i64);
    let payload = format!(
        "{STORAGE_DELEGATION_PREFIX}\nEphemeral: {}\nWorld: {}\nSceneId: {}\nParcel: {}\nExpiration: {}",
        addr_str(&ephemeral),
        world,
        scene_id,
        parcel,
        expiration.to_rfc3339()
    );
    let signature = eip191_sign(authoritative, &payload).await?;
    let envelope = json!({
        "v": 1,
        "ephemeral": {
            "privateKey": format!("0x{}", hex::encode(ephemeral.to_bytes())),
            "publicKey": format!(
                "0x{}",
                hex::encode(ephemeral.credential().verifying_key().to_encoded_point(false).as_bytes())
            ),
            "address": addr_str(&ephemeral),
        },
        "scope": { "payload": payload, "signature": signature },
    });
    Ok(base64::engine::general_purpose::STANDARD.encode(envelope.to_string()))
}

#[derive(Clone)]
struct MinterState {
    wallet: Arc<PrivateKeySigner>,
    ttl: std::time::Duration,
    token: Option<String>,
}

fn timing_safe_eq(a: &[u8], b: &[u8]) -> bool {
    let mut diff: u8 = (a.len() ^ b.len()) as u8 | ((a.len() ^ b.len()) >> 8) as u8;
    let n = a.len().max(b.len());
    for i in 0..n {
        let x = a.get(i).copied().unwrap_or(0);
        let y = b.get(i).copied().unwrap_or(0);
        diff |= x ^ y;
    }
    diff == 0 && a.len() == b.len()
}

#[derive(Deserialize)]
struct DelegationReq {
    world: String,
    #[serde(rename = "sceneId")]
    scene_id: String,
    parcel: String,
}

async fn mint_delegation(
    State(st): State<MinterState>,
    headers: HeaderMap,
    Json(req): Json<DelegationReq>,
) -> axum::response::Response {
    if let Some(expected) = &st.token {
        let presented = headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.strip_prefix("Bearer "));
        let authorized = presented
            .map(|p| timing_safe_eq(p.as_bytes(), expected.as_bytes()))
            .unwrap_or(false);
        if !authorized {
            return (StatusCode::UNAUTHORIZED, "unauthorized").into_response();
        }
    }
    let (world, scene_id, parcel) =
        match validate_delegation_req(&req.world, &req.scene_id, &req.parcel) {
            Ok(v) => v,
            Err(msg) => return (StatusCode::BAD_REQUEST, msg).into_response(),
        };
    match mint_delegation_envelope(&st.wallet, &world, &scene_id, &parcel, st.ttl).await {
        Ok(delegation) => {
            tracing::info!(world, scene_id, parcel, "minted storage delegation");
            Json(json!({ "delegation": delegation })).into_response()
        }
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "mint failed").into_response(),
    }
}

fn effective_delegation_token(raw: &Option<String>) -> Option<String> {
    raw.clone().filter(|s| !s.is_empty())
}

async fn run_delegation_minter(args: &Args, wallet: PrivateKeySigner) -> Result<()> {
    let token = effective_delegation_token(&args.delegation_token);
    let bind = if token.is_some() {
        args.bind.clone()
    } else {
        if args.bind != "127.0.0.1" {
            tracing::warn!(
                "--serve-delegations without a non-empty --delegation-token binds 127.0.0.1 (ignoring --bind {})",
                args.bind
            );
        }
        "127.0.0.1".to_string()
    };
    let address = addr_str(&wallet);
    let st = MinterState {
        wallet: Arc::new(wallet),
        ttl: std::time::Duration::from_secs(args.delegation_ttl_secs.max(1)),
        token,
    };
    let app = Router::new()
        .route("/delegations", post(mint_delegation))
        .with_state(st);
    let addr: SocketAddr = format!("{}:{}", bind, args.port)
        .parse()
        .context("parsing bind address")?;
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .with_context(|| format!("binding {addr}"))?;
    tracing::info!(
        "delegation minter listening on {addr} — authoritative address {address} (configure it as AUTHORITATIVE_SERVER_ADDRESS in world-storage)"
    );
    axum::serve(listener, app)
        .await
        .context("serving delegations")?;
    Ok(())
}

const GRANT_PAGE: &str = r##"<!doctype html>
<html lang="en"><head>
<meta charset="utf-8"/>
<meta name="viewport" content="width=device-width, initial-scale=1"/>
<title>Grant deploy permission</title>
<style>
  :root{color-scheme:dark}
  body{margin:0;background:#0b0e16;color:#e6ebf2;font:15px/1.5 system-ui,sans-serif;
       display:flex;min-height:100vh;align-items:center;justify-content:center}
  .card{width:min(680px,92vw);background:#12151e;border:1px solid #24314d;border-radius:16px;
        padding:28px 30px;box-shadow:0 20px 60px #0008}
  h1{margin:0 0 4px;font-size:20px} h1 .t{color:#22e6d0}
  .sub{color:#8ea1c0;margin:0 0 20px;font-size:13px}
  .kv{display:grid;grid-template-columns:130px 1fr;gap:6px 12px;font-size:13px;margin:14px 0}
  .kv b{color:#8ea1c0;font-weight:500}
  code{font-family:ui-monospace,monospace;word-break:break-all;color:#cfe}
  button{margin-top:18px;width:100%;padding:13px;border:0;border-radius:10px;cursor:pointer;
         font-size:15px;font-weight:600;background:#22e6d0;color:#04121a}
  button:disabled{opacity:.5;cursor:default}
  #status{margin-top:16px;padding:12px 14px;border-radius:10px;font-size:13px;white-space:pre-wrap;display:none}
  .ok{background:#0e2a1e;border:1px solid #1f6b46;color:#8ff0bf}
  .err{background:#2a1414;border:1px solid #6b2626;color:#ff9a9a}
  .info{background:#101a2c;border:1px solid #24314d;color:#a9c0e6}
</style></head>
<body><div class="card">
  <h1>Grant deploy <span class="t">·</span> owner signature</h1>
  <p class="sub">Connect the wallet that OWNS this World and authorize the deploy key once. After this, deployments run unattended (no wallet).</p>
  <div class="kv">
    <b>World</b><code id="world">…</code>
    <b>Permission</b><code id="perm">…</code>
    <b>Deploy key</b><code id="addr">…</code>
    <b>Content server</b><code id="cs">…</code>
  </div>
  <button id="go" disabled>Connect owner wallet &amp; grant</button>
  <div id="status"></div>
</div>
<script>
let INFO=null;
const $=id=>document.getElementById(id);
function show(cls,msg){const s=$("status");s.className=cls;s.style.display="block";s.textContent=msg;}
async function load(){
  try{
    INFO=await (await fetch("api/grant-info")).json();
    $("world").textContent=INFO.world;
    $("perm").textContent=INFO.permission;
    $("addr").textContent=INFO.deployAddress;
    $("cs").textContent=INFO.contentServer;
    $("go").disabled=false;
  }catch(e){show("err","Could not load grant info: "+e);}
}
$("go").onclick=async()=>{
  const btn=$("go");
  try{
    if(!window.ethereum){show("err","No wallet found. Open in a browser with MetaMask (or another EIP-1193 wallet).");return;}
    btn.disabled=true;
    show("info","Requesting wallet…");
    const accounts=await window.ethereum.request({method:"eth_requestAccounts"});
    const owner=accounts[0];
    const ts=Date.now();
    const payload=("put:"+INFO.path+":"+ts+":{}").toLowerCase();
    show("info","Signing authorization with "+owner+" …");
    const signature=await window.ethereum.request({method:"personal_sign",params:[payload,owner]});
    show("info","Submitting grant…");
    const r=await (await fetch("grant",{method:"POST",headers:{"content-type":"application/json"},
      body:JSON.stringify({ownerAddress:owner,signature,timestamp:ts})})).json();
    if(r.ok){show("ok","✓ "+r.message+"\n\nYou can close this tab.");}
    else{show("err","✗ "+r.error);btn.disabled=false;}
  }catch(e){show("err","✗ "+(e&&e.message?e.message:e));btn.disabled=false;}
};
load();
</script></body></html>
"##;

const PAGE: &str = r##"<!doctype html>
<html lang="en"><head>
<meta charset="utf-8"/>
<meta name="viewport" content="width=device-width, initial-scale=1"/>
<title>Deploy signer</title>
<style>
  :root{color-scheme:dark}
  body{margin:0;background:#0b0e16;color:#e6ebf2;font:15px/1.5 system-ui,sans-serif;
       display:flex;min-height:100vh;align-items:center;justify-content:center}
  .card{width:min(680px,92vw);background:#12151e;border:1px solid #24314d;border-radius:16px;
        padding:28px 30px;box-shadow:0 20px 60px #0008}
  h1{margin:0 0 4px;font-size:20px}
  h1 .t{color:#22e6d0}
  .sub{color:#8ea1c0;margin:0 0 20px;font-size:13px}
  .kv{display:grid;grid-template-columns:120px 1fr;gap:6px 12px;font-size:13px;margin:14px 0}
  .kv b{color:#8ea1c0;font-weight:500}
  code{font-family:ui-monospace,monospace;word-break:break-all;color:#cfe}
  ul{margin:6px 0 0;padding-left:18px;color:#b9c6dd;font-size:12px}
  button{margin-top:18px;width:100%;padding:13px;border:0;border-radius:10px;cursor:pointer;
         font-size:15px;font-weight:600;background:#22e6d0;color:#04121a}
  button:disabled{opacity:.5;cursor:default}
  #status{margin-top:16px;padding:12px 14px;border-radius:10px;font-size:13px;white-space:pre-wrap;display:none}
  .ok{background:#0e2a1e;border:1px solid #1f6b46;color:#8ff0bf}
  .err{background:#2a1414;border:1px solid #6b2626;color:#ff9a9a}
  .info{background:#101a2c;border:1px solid #24314d;color:#a9c0e6}
  a{color:#22e6d0}
</style></head>
<body><div class="card">
  <h1>Deploy <span class="t">·</span> signature required</h1>
  <p class="sub">Connect the wallet that controls this World and sign the deployment. One-time; the signer exits on success.</p>
  <div class="kv">
    <b>World</b><code id="world">…</code>
    <b>Content server</b><code id="cs">…</code>
    <b>Entity id</b><code id="eid">…</code>
    <b>Files</b><div><ul id="files"></ul></div>
  </div>
  <button id="go" disabled>Connect wallet &amp; sign &amp; deploy</button>
  <div id="status"></div>
</div>
<script>
let INFO=null;
const $=id=>document.getElementById(id);
function show(cls,msg){const s=$("status");s.className=cls;s.style.display="block";s.textContent=msg;}
async function load(){
  try{
    INFO=await (await fetch("api/info")).json();
    $("world").textContent=INFO.world;
    $("cs").textContent=INFO.contentServer;
    $("eid").textContent=INFO.entityId;
    $("files").innerHTML=INFO.files.map(f=>`<li>${f.file} <span style="color:#5d6c8c">(${f.size} B)</span></li>`).join("");
    $("go").disabled=false;
  }catch(e){show("err","Could not load deployment info: "+e);}
}
$("go").onclick=async()=>{
  const btn=$("go");
  try{
    if(!window.ethereum){show("err","No wallet found. Open this in a browser with MetaMask (or another EIP-1193 wallet).");return;}
    btn.disabled=true;
    show("info","Requesting wallet…");
    const accounts=await window.ethereum.request({method:"eth_requestAccounts"});
    const address=accounts[0];
    // Re-mint a fresh entity NOW: the content server rejects deployments whose
    // timestamp is older than ~5 min, so we sign one that's only seconds old.
    show("info","Preparing a fresh deployment…");
    INFO=await (await fetch("api/info")).json();
    $("eid").textContent=INFO.entityId;
    show("info","Signing entity id with "+address+" …");
    const signature=await window.ethereum.request({method:"personal_sign",params:[INFO.entityId,address]});
    show("info","Uploading deployment to "+INFO.contentServer+" …");
    const r=await (await fetch("sign",{method:"POST",headers:{"content-type":"application/json"},
      body:JSON.stringify({address,signature,entityId:INFO.entityId})})).json();
    if(r.ok){show("ok","✓ "+r.message+"\n\nOpen: "+INFO.playUrl);}
    else{show("err","✗ "+r.error);btn.disabled=false;}
  }catch(e){show("err","✗ "+(e&&e.message?e.message:e));btn.disabled=false;}
};
load();
</script></body></html>
"##;

#[cfg(test)]
mod delegation_tests {
    use super::*;

    const KEY: &str = "0x4c0883a69102937d6231471b5dbb6204fe5129617082792ae468d01a3f362318";

    fn authoritative() -> PrivateKeySigner {
        KEY.trim_start_matches("0x").parse().unwrap()
    }

    fn decode(envelope: &str) -> Value {
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(envelope)
            .expect("envelope must be standard base64");
        serde_json::from_slice(&bytes).expect("envelope must be JSON")
    }

    fn strict_parse_claim(payload: &str) -> Option<[String; 5]> {
        const FIELDS: [&str; 5] = ["Ephemeral:", "World:", "SceneId:", "Parcel:", "Expiration:"];
        let mut lines = payload.split('\n');
        if lines.next() != Some(STORAGE_DELEGATION_PREFIX) {
            return None;
        }
        let mut values: [Option<String>; 5] = Default::default();
        for line in lines {
            let idx = FIELDS.iter().position(|p| line.starts_with(p))?;
            if values[idx].is_some() {
                return None;
            }
            values[idx] = Some(line[FIELDS[idx].len()..].trim().to_string());
        }
        let values = values.map(|v| v.filter(|s| !s.is_empty()));
        if values.iter().any(|v| v.is_none()) {
            return None;
        }
        Some(values.map(|v| v.unwrap()))
    }

    #[tokio::test]
    async fn envelope_round_trips_and_the_claim_matches_the_verifier_format() {
        let ttl = std::time::Duration::from_secs(3600);
        let envelope = mint_delegation_envelope(
            &authoritative(),
            "myworld.dcl.eth",
            "bafkreigcene",
            "10,-25",
            ttl,
        )
        .await
        .unwrap();

        let v = decode(&envelope);
        assert_eq!(v["v"], 1);
        for field in ["privateKey", "publicKey", "address"] {
            assert!(
                v["ephemeral"][field].is_string(),
                "{field} must be a string"
            );
        }
        assert!(v["scope"]["payload"].is_string());
        assert!(v["scope"]["signature"].is_string());

        let payload = v["scope"]["payload"].as_str().unwrap();
        let [ephemeral, world, scene_id, parcel, expiration] = strict_parse_claim(payload)
            .expect("claim must strict-parse with the verifier's field set");
        assert_eq!(ephemeral, v["ephemeral"]["address"].as_str().unwrap());
        assert_eq!(world, "myworld.dcl.eth");
        assert_eq!(scene_id, "bafkreigcene");
        assert_eq!(parcel, "10,-25");

        let expiration =
            chrono::DateTime::parse_from_rfc3339(&expiration).expect("expiration must be RFC3339");
        let delta = expiration.with_timezone(&chrono::Utc) - chrono::Utc::now();
        assert!(
            delta > chrono::Duration::seconds(3590) && delta <= chrono::Duration::seconds(3600),
            "ttl must be ~1h, got {delta}"
        );
    }

    #[tokio::test]
    async fn scope_signature_recovers_to_the_authoritative_address() {
        let wallet = authoritative();
        let envelope = mint_delegation_envelope(
            &wallet,
            "myworld.dcl.eth",
            "bafkreigcene",
            "0,0",
            std::time::Duration::from_secs(60),
        )
        .await
        .unwrap();
        let v = decode(&envelope);
        let payload = v["scope"]["payload"].as_str().unwrap().to_string();
        let sig: alloy::primitives::Signature = v["scope"]["signature"]
            .as_str()
            .unwrap()
            .trim_start_matches("0x")
            .parse()
            .unwrap();
        let recovered = sig.recover_address_from_msg(payload).unwrap();
        assert_eq!(format!("{recovered:#x}"), addr_str(&wallet));

        let eph = v["ephemeral"]["address"].as_str().unwrap();
        assert_ne!(eph, addr_str(&wallet));
        let envelope2 = mint_delegation_envelope(
            &wallet,
            "myworld.dcl.eth",
            "bafkreigcene",
            "0,0",
            std::time::Duration::from_secs(60),
        )
        .await
        .unwrap();
        assert_ne!(
            decode(&envelope2)["ephemeral"]["address"].as_str().unwrap(),
            eph
        );
    }

    #[test]
    fn timing_safe_eq_matches_semantics() {
        assert!(timing_safe_eq(b"tok", b"tok"));
        assert!(!timing_safe_eq(b"tok", b"toK"));
        assert!(!timing_safe_eq(b"tok", b"token"));
        assert!(timing_safe_eq(b"", b""));
    }

    #[test]
    fn empty_delegation_token_is_treated_as_absent() {
        assert_eq!(effective_delegation_token(&Some(String::new())), None);
        assert_eq!(effective_delegation_token(&None), None);
        assert_eq!(
            effective_delegation_token(&Some("s3cr3t".into())),
            Some("s3cr3t".into())
        );
    }

    #[test]
    fn request_validation_rejects_bad_input_and_lowercases_the_world() {
        assert_eq!(
            validate_delegation_req("MyWorld.DCL.eth", "bafkreigcene", "10,-25").unwrap(),
            (
                "myworld.dcl.eth".to_string(),
                "bafkreigcene".to_string(),
                "10,-25".to_string()
            )
        );
        assert!(validate_delegation_req("", "scene", "0,0").is_err());
        assert!(validate_delegation_req("   ", "scene", "0,0").is_err());
        assert!(validate_delegation_req("w.dcl.eth", "", "0,0").is_err());
        for bad in ["10", "10,", ",25", "a,b", "10.5,2", "10, 25", "1,2/../x"] {
            assert!(
                validate_delegation_req("w.dcl.eth", "scene", bad).is_err(),
                "parcel {bad:?} must be rejected"
            );
        }
    }
}
