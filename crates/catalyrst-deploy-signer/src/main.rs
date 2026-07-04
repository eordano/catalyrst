use std::{
    collections::HashMap,
    net::SocketAddr,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::SystemTime,
};

use anyhow::{bail, Context, Result};
use axum::{
    extract::State,
    response::Html,
    routing::{get, post},
    Json, Router,
};
use catalyrst_hashing::hash_bytes_v1;
use clap::Parser;
use ethers_signers::{LocalWallet, Signer};
use serde::Deserialize;
use serde_json::{json, Value};

fn load_or_create_key(path: &std::path::Path) -> Result<LocalWallet> {
    if path.exists() {
        let raw = std::fs::read_to_string(path)
            .with_context(|| format!("reading key {}", path.display()))?;
        let hexs = raw.trim().trim_start_matches("0x");
        let wallet: LocalWallet = hexs.parse().context("parsing private key hex")?;
        Ok(wallet)
    } else {
        let wallet = LocalWallet::new(&mut ethers_core::rand::thread_rng());
        let hexs = format!("0x{}", hex::encode(wallet.signer().to_bytes()));
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

fn addr_str(w: &LocalWallet) -> String {
    format!("{:#x}", w.address())
}

async fn eip191_sign(w: &LocalWallet, msg: &str) -> Result<String> {
    let sig = w
        .sign_message(msg.as_bytes())
        .await
        .context("EIP-191 sign")?;
    Ok(format!("0x{sig}"))
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
    wallet: &LocalWallet,
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
