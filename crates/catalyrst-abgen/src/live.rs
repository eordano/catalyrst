use crate::builder::{build_bundle, BuildOpts};
use crate::catalyst::{CatalystClient, Scene};
use crate::glbscan::{scan_entity, EntityScan, UriCache};
use crate::local_store::LocalContentStore;
use crate::naming;
use crate::space::Space;
use anyhow::{anyhow, bail, Context, Result};
use rayon::prelude::*;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

const CONVERTIBLE_EXTS: [&str; 5] = [".glb", ".gltf", ".png", ".jpg", ".jpeg"];

const DEPENDENCY_EXTS: [&str; 1] = [".bin"];

struct BuildTelemetry<'a> {
    entity: &'a str,
    entity_type: &'a str,
    file: &'a str,
    platform: &'a str,
    hash: &'a str,
    ms: u64,
    out_bytes: usize,

    result: &'a str,
}

fn emit_build_telemetry(t: &BuildTelemetry) {
    let rec = serde_json::json!({
        "entity": t.entity,
        "entity_type": t.entity_type,
        "file": t.file,
        "platform": t.platform,
        "hash": t.hash,
        "build_ms": t.ms,
        "out_bytes": t.out_bytes,
        "result": t.result,
    });
    eprintln!("ABGEN_BUILD {rec}");
}

fn is_convertible(file: &str) -> (bool, bool) {
    let fl = file.to_lowercase();
    let is_glb = fl.ends_with(".glb") || fl.ends_with(".gltf");
    let is_image = fl.ends_with(".png") || fl.ends_with(".jpg") || fl.ends_with(".jpeg");
    (is_glb, is_image)
}

#[derive(Default)]
struct KeyedLocks {
    map: Mutex<HashMap<String, Arc<Mutex<()>>>>,
}

impl KeyedLocks {
    fn get(&self, key: &str) -> Arc<Mutex<()>> {
        let mut g = self.map.lock().unwrap();
        g.entry(key.to_string()).or_default().clone()
    }
}

struct EntityCtx {
    scene: Scene,
    content_by_file: HashMap<String, String>,
    scan: EntityScan,
}

pub struct Proxy {
    catalyst: CatalystClient,
    local: Option<LocalContentStore>,
    content: LocalContentStore,
    bundle_dir: PathBuf,
    version: String,
    date: String,
    uri_cache: UriCache,

    space: Option<Arc<Space>>,
    fallback_version: String,
    timeout: Duration,
    cache_cap: u64,

    entities: Mutex<HashMap<String, Arc<EntityCtx>>>,
    hash_index: Mutex<HashMap<String, String>>,
    entity_locks: KeyedLocks,
    bundle_locks: KeyedLocks,
}

impl Proxy {
    fn ensure_content(&self, hash: &str) -> Result<()> {
        if self.content.exists(hash) {
            return Ok(());
        }
        if let Some(local) = &self.local {
            if let Ok(b) = local.fetch(hash) {
                return self.content.write(hash, &b);
            }
        }
        let bytes = self
            .catalyst
            .fetch_content(hash)
            .with_context(|| format!("fetch content {hash}"))?;
        self.content.write(hash, &bytes)
    }

    fn entity_ctx(&self, cid: &str) -> Result<Arc<EntityCtx>> {
        if let Some(c) = self.entities.lock().unwrap().get(cid) {
            return Ok(c.clone());
        }
        let lock = self.entity_locks.get(cid);
        let _g = lock.lock().unwrap();
        if let Some(c) = self.entities.lock().unwrap().get(cid) {
            return Ok(c.clone());
        }

        let scene = self
            .catalyst
            .resolve_scene(cid)
            .with_context(|| format!("resolve entity {cid}"))?;

        for c in &scene.content {
            if CONVERTIBLE_EXTS
                .iter()
                .chain(DEPENDENCY_EXTS.iter())
                .any(|e| c.file.to_lowercase().ends_with(*e))
            {
                if let Err(e) = self.ensure_content(&c.hash) {
                    eprintln!("warn: {cid}: content {} ({}): {e}", c.hash, c.file);
                }
            }
        }

        let content_by_file = scene.content_by_file();
        let scan = scan_entity(&self.content, &content_by_file, &self.uri_cache);

        {
            let mut idx = self.hash_index.lock().unwrap();
            for c in &scene.content {
                idx.entry(c.hash.to_lowercase())
                    .or_insert_with(|| cid.to_string());
            }
        }

        let ctx = Arc::new(EntityCtx {
            scene,
            content_by_file,
            scan,
        });
        self.entities
            .lock()
            .unwrap()
            .insert(cid.to_string(), ctx.clone());
        Ok(ctx)
    }

    fn bundle(&self, cid: &str, bundle_name: &str) -> Result<Vec<u8>> {
        let entity_dir = self.bundle_dir.join(cid);
        let cache_path = entity_dir.join(bundle_name);
        if let Ok(b) = std::fs::read(&cache_path) {
            return Ok(b);
        }
        let lock = self.bundle_locks.get(&format!("{cid}/{bundle_name}"));
        let _g = lock.lock().unwrap();
        if let Ok(b) = std::fs::read(&cache_path) {
            return Ok(b);
        }

        let ctx = self.entity_ctx(cid)?;
        let data = self.build(&ctx, bundle_name)?;

        std::fs::create_dir_all(&entity_dir).ok();
        let tmp = cache_path.with_extension(format!("tmp.{}", std::process::id()));
        std::fs::write(&tmp, &data).with_context(|| format!("write {}", tmp.display()))?;
        std::fs::rename(&tmp, &cache_path).ok();
        Ok(data)
    }

    fn build(&self, ctx: &EntityCtx, bundle_name: &str) -> Result<Vec<u8>> {
        let (hash, platform) = bundle_name
            .rsplit_once('_')
            .ok_or_else(|| anyhow!("bundle name {bundle_name:?} has no _<platform> suffix"))?;

        let item = match ctx
            .scene
            .content
            .iter()
            .find(|c| {
                if !c.hash.eq_ignore_ascii_case(hash) {
                    return false;
                }
                let (g, i) = is_convertible(&c.file);
                g || i
            })
            .or_else(|| {
                ctx.scene
                    .content
                    .iter()
                    .find(|c| c.hash.eq_ignore_ascii_case(hash))
            }) {
            Some(it) => it,
            None => {
                if let Some(owner) = self.entity_for_hash(hash) {
                    if !owner.eq_ignore_ascii_case(&ctx.scene.entity_id) {
                        let owner_ctx = self.entity_ctx(&owner)?;
                        return self.build(&owner_ctx, bundle_name);
                    }
                }
                bail!(
                    "hash {hash} not in entity {} (no owning entity indexed)",
                    ctx.scene.entity_id
                );
            }
        };
        let hash: &str = &item.hash;
        let file = item.file.clone();
        let (is_glb, is_image) = is_convertible(&file);
        if !is_glb && !is_image {
            bail!("content {file} (hash {hash}) is not a convertible glb/image");
        }

        self.ensure_content(hash)?;
        let glb = self.content.fetch(hash)?;

        let m_deps = if is_glb {
            ctx.scan
                .metadata_deps(&self.content, &file, hash, &ctx.content_by_file, platform)
        } else {
            Vec::new()
        };
        let model_ref = is_image && ctx.scan.model_refs.contains(hash);
        let standalone_color_space = if is_image {
            Some(if ctx.scan.linear_refs.contains(hash) {
                0
            } else {
                1
            })
        } else {
            None
        };
        let standalone_normal = is_image && ctx.scan.normal_refs.contains(hash);

        let content_by_file = &ctx.content_by_file;
        let sf_bytes = file.clone();
        let resolve_fn = |uri: &str| -> Option<Vec<u8>> {
            let key = naming::resolve_uri_to_content_file(uri, &sf_bytes)
                .ok()?
                .to_lowercase();
            let h = content_by_file.get(&key)?;
            if let Err(e) = self.ensure_content(h) {
                eprintln!("warn: resolve {uri} (hash {h}): {e:#}");
            }
            self.content.fetch(h).ok()
        };
        let resolve: crate::gltf::Resolve = if !content_by_file.is_empty() {
            Some(&resolve_fn)
        } else {
            None
        };
        let sf_hash = file.clone();
        let resolve_hash_fn = |uri: &str| -> Option<String> {
            let key = naming::resolve_uri_to_content_file(uri, &sf_hash)
                .ok()?
                .to_lowercase();
            content_by_file.get(&key).cloned()
        };
        type HashResolver<'a> = &'a dyn Fn(&str) -> Option<String>;
        let resolve_hash: Option<HashResolver<'_>> = if !content_by_file.is_empty() {
            Some(&resolve_hash_fn)
        } else {
            None
        };

        let entity_type = ctx.scene.entity_type.clone();
        let opts = BuildOpts {
            keep_forward_plus: true,
            source_file: Some(&file),
            entity_type: if entity_type.is_empty() {
                None
            } else {
                Some(entity_type.as_str())
            },
            resolve,
            resolve_hash,
            model_referenced: model_ref,
            metadata_dependencies: &m_deps,
            expect_hash: None,
            standalone_color_space,
            standalone_normal,
            force_default_material: false,
            magenta_missing: std::env::var("ABGEN_MAGENTA_MISSING").is_ok(),
        };

        let started = std::time::Instant::now();
        let outcome = crate::regen::guard(|| build_bundle(&glb, bundle_name, hash, &opts));
        let ms = started.elapsed().as_millis() as u64;

        let (result_label, out_bytes) = match &outcome {
            Ok(a) => ("ok", a.data.len()),
            Err(e) => {
                if e.to_string().starts_with("panic:") {
                    ("panic-recovered", 0usize)
                } else {
                    ("error", 0usize)
                }
            }
        };
        emit_build_telemetry(&BuildTelemetry {
            entity: &ctx.scene.entity_id,
            entity_type: &entity_type,
            file: &file,
            platform,
            hash,
            ms,
            out_bytes,
            result: result_label,
        });

        let artifact = outcome?;
        Ok(artifact.data)
    }

    fn entity_for_hash(&self, hash: &str) -> Option<String> {
        self.hash_index
            .lock()
            .unwrap()
            .get(&hash.to_lowercase())
            .cloned()
    }

    fn bundle_key(version: &str, cid: &str, file: &str) -> String {
        format!("{version}/{cid}/{file}")
    }

    fn fetch_fallback(&self, cid: &str, file: &str) -> Option<Vec<u8>> {
        let space = self.space.as_ref()?;
        let key = Self::bundle_key(&self.fallback_version, cid, file);
        match space.get(&key) {
            Ok(Some(b)) => Some(b),
            Ok(None) => None,
            Err(e) => {
                eprintln!("fallback {key}: {e}");
                None
            }
        }
    }

    fn put_generated(&self, cid: &str, file: &str, bytes: &[u8]) {
        let Some(space) = self.space.as_ref() else {
            return;
        };
        let key = Self::bundle_key(&self.version, cid, file);
        match space.put(&key, bytes, "application/octet-stream") {
            Ok(()) => eprintln!("space put {key} ({} bytes)", bytes.len()),
            Err(e) => eprintln!("put {key}: {e}"),
        }
        if let Some((_, platform)) = file.rsplit_once('_') {
            let mkey = format!("manifest/{cid}_{platform}.json");
            let body = serde_json::json!({"version": self.version, "date": self.date}).to_string();
            if let Err(e) = space.put(&mkey, body.as_bytes(), "application/json") {
                eprintln!("put {mkey}: {e}");
            }
        }
    }

    fn enforce_lru(&self) {
        if self.cache_cap == 0 {
            return;
        }

        let Ok(entities) = std::fs::read_dir(&self.bundle_dir) else {
            return;
        };
        let mut files: Vec<(std::time::SystemTime, u64, PathBuf)> = Vec::new();
        for ent in entities.filter_map(|e| e.ok()) {
            let Ok(rd) = std::fs::read_dir(ent.path()) else {
                continue;
            };
            for e in rd.filter_map(|e| e.ok()) {
                let Ok(m) = e.metadata() else { continue };
                if !m.is_file() {
                    continue;
                }
                files.push((
                    m.modified().unwrap_or(std::time::UNIX_EPOCH),
                    m.len(),
                    e.path(),
                ));
            }
        }
        let mut total: u64 = files.iter().map(|(_, l, _)| *l).sum();
        if total <= self.cache_cap {
            return;
        }
        files.sort_by_key(|(t, _, _)| *t);
        for (_, len, path) in files {
            if total <= self.cache_cap {
                break;
            }
            if std::fs::remove_file(&path).is_ok() {
                total = total.saturating_sub(len);
            }
        }
    }

    fn serve_or_fallback(self: &Arc<Self>, cid: &str, file: &str) -> (u16, Vec<u8>, &'static str) {
        if let Ok(b) = std::fs::read(self.bundle_dir.join(cid).join(file)) {
            return (200, b, "cache");
        }
        let (tx, rx) = mpsc::channel();
        let me = self.clone();
        let cid_t = cid.to_string();
        let file_t = file.to_string();
        std::thread::spawn(move || {
            let r = me.bundle(&cid_t, &file_t);
            if let Ok(bytes) = &r {
                me.put_generated(&cid_t, &file_t, bytes);
                me.enforce_lru();
            }
            let _ = tx.send(r.map_err(|e| format!("{e:#}")));
        });
        match rx.recv_timeout(self.timeout) {
            Ok(Ok(bytes)) => (200, bytes, "fresh"),
            Ok(Err(e)) => match self.fetch_fallback(cid, file) {
                Some(b) => (200, b, "fallback(build-failed)"),
                None => (500, format!("build failed: {e}").into_bytes(), "error"),
            },
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if let Some(b) = self.fetch_fallback(cid, file) {
                    (200, b, "fallback")
                } else {
                    match rx.recv() {
                        Ok(Ok(b)) => (200, b, "fresh-slow"),
                        Ok(Err(e)) => (500, format!("build failed: {e}").into_bytes(), "error"),
                        Err(_) => (500, b"build channel closed".to_vec(), "error"),
                    }
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                (500, b"build worker died".to_vec(), "error")
            }
        }
    }

    pub fn date(&self) -> &str {
        &self.date
    }

    pub fn build_entity_into_corpus(
        self: &Arc<Self>,
        out_root: &std::path::Path,
        cid: &str,
        platform: &str,
        content_server_url: &str,
    ) -> Result<Vec<String>> {
        let ctx = self.entity_ctx(cid)?;
        let pdir = out_root.join(cid).join(platform);
        std::fs::create_dir_all(&pdir).with_context(|| format!("mkdir {}", pdir.display()))?;
        let mut built: Vec<String> = Vec::new();
        let mut failed: Vec<String> = Vec::new();
        let mut tolerated: usize = 0;
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
        for c in &ctx.scene.content {
            let lf = c.file.to_lowercase();
            if !CONVERTIBLE_EXTS.iter().any(|e| lf.ends_with(e)) {
                continue;
            }
            let bundle_name = format!("{}_{}", c.hash, platform);
            if !seen.insert(bundle_name.clone()) {
                continue;
            }
            match self.bundle(cid, &bundle_name) {
                Ok(bytes) => {
                    let dst = pdir.join(&bundle_name);
                    let tmp = dst.with_extension(format!("tmp.{}", std::process::id()));
                    std::fs::write(&tmp, &bytes)
                        .with_context(|| format!("write {}", tmp.display()))?;
                    std::fs::rename(&tmp, &dst).ok();
                    built.push(bundle_name);
                    let (_, is_image) = is_convertible(&c.file);
                    if is_image {
                        self.ensure_content(&c.hash).ok();
                        if let Ok(raw) = self.content.fetch(&c.hash) {
                            if !crate::builder::source_image_decodes(&raw) {
                                tolerated += 1;
                            }
                        }
                    }
                }
                Err(e) => {
                    tracing::error!(
                        entity = %cid,
                        bundle = %bundle_name,
                        file = %c.file,
                        error = %format!("{e:#}"),
                        "jit build failed — omitted from manifest, exitCode will be non-zero"
                    );
                    failed.push(bundle_name);
                }
            }
        }
        crate::manifest::write_corpus_manifest(
            out_root,
            cid,
            platform,
            &built,
            &self.version,
            content_server_url,
            crate::manifest::exit_code_for_failures(failed.len() + tolerated),
            &self.date,
        )?;
        Ok(built)
    }
}

fn build_id() -> String {
    let mut buf: Vec<u8> = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Ok(b) = std::fs::read(&exe) {
            buf.extend_from_slice(&b);
        }
    }
    if let Ok(rd) = std::fs::read_dir(crate::builder::template_dir()) {
        let mut files: Vec<PathBuf> = rd.filter_map(|e| e.ok().map(|e| e.path())).collect();
        files.sort();
        for f in files {
            if let Ok(b) = std::fs::read(&f) {
                buf.extend_from_slice(f.to_string_lossy().as_bytes());
                buf.extend_from_slice(&b);
            }
        }
    }
    crate::hashes::sha256_hex(&buf)
}

fn iso_from_build_id(id: &str) -> String {
    let n = u64::from_str_radix(id.get(..8).unwrap_or("0"), 16).unwrap_or(0);
    let base = 1_577_836_800u64;
    iso8601_utc(base + (n % 946_080_000))
}

pub fn build_scoped_date() -> String {
    use std::sync::OnceLock;
    static CACHE: OnceLock<String> = OnceLock::new();
    CACHE.get_or_init(|| iso_from_build_id(&build_id())).clone()
}

fn iso8601_utc(total_secs: u64) -> String {
    let days = (total_secs / 86_400) as i64;
    let sod = (total_secs % 86_400) as i64;
    let (h, mi, s) = (sod / 3600, (sod % 3600) / 60, sod % 60);
    let (y, mo, d) = civil_from_days(days);
    format!("{y:04}-{mo:02}-{d:02}T{h:02}:{mi:02}:{s:02}.000Z")
}

const fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

pub struct RouteResponse {
    pub code: u16,
    pub content_type: &'static str,
    pub body: Vec<u8>,
}

impl RouteResponse {
    fn new(code: u16, content_type: &'static str, body: Vec<u8>) -> Self {
        Self {
            code,
            content_type,
            body,
        }
    }
}

pub struct ProxyConfig {
    pub catalyst_url: String,

    pub local_root: Option<String>,

    pub cache_dir: String,
    pub version: String,
    pub date: Option<String>,
    pub parity: bool,
    pub magenta_missing: bool,
    pub fallback_version: String,
    pub use_space: bool,
    pub timeout_ms: u64,
    pub cache_cap_gb: f64,

    pub template_root: Option<String>,
}

impl Default for ProxyConfig {
    fn default() -> Self {
        Self {
            catalyst_url: crate::catalyst::DEFAULT_CATALYST.to_string(),
            local_root: None,
            cache_dir: "./abgen-serve-cache".to_string(),
            version: "v41".to_string(),
            date: None,
            parity: false,
            magenta_missing: false,
            fallback_version: "v41".to_string(),
            use_space: false,
            timeout_ms: 1000,
            cache_cap_gb: 0.0,
            template_root: None,
        }
    }
}

impl Proxy {
    pub fn new(cfg: ProxyConfig) -> Arc<Self> {
        if !cfg.parity {
            std::env::set_var(BuildOpts::REAL_TEXTURES_ENV, "1");
            std::env::set_var(BuildOpts::V38_COMPAT_ENV, "1");
        }
        if cfg.magenta_missing {
            std::env::set_var("ABGEN_MAGENTA_MISSING", "1");
        }
        if let Some(root) = cfg.template_root.as_deref().filter(|s| !s.is_empty()) {
            std::env::set_var("ABGEN_ROOT", root);
        }
        let bid = build_id();
        let date = cfg.date.unwrap_or_else(|| iso_from_build_id(&bid));
        let cache_root = PathBuf::from(&cfg.cache_dir);
        let content = LocalContentStore::new(cache_root.join("content"));
        let bundle_dir = cache_root.join("bundles").join(&bid[..16.min(bid.len())]);
        let _ = std::fs::create_dir_all(&bundle_dir);
        let space = if cfg.use_space {
            Space::from_env().map(Arc::new)
        } else {
            None
        };
        Arc::new(Proxy {
            catalyst: CatalystClient::from_args(&cfg.catalyst_url, cfg.local_root.as_deref()),
            local: cfg.local_root.map(LocalContentStore::new),
            content,
            bundle_dir,
            version: cfg.version,
            date,
            uri_cache: UriCache::new(),
            space,
            fallback_version: cfg.fallback_version,
            timeout: Duration::from_millis(cfg.timeout_ms),
            cache_cap: (cfg.cache_cap_gb * 1e9) as u64,
            entities: Mutex::new(HashMap::new()),
            hash_index: Mutex::new(HashMap::new()),
            entity_locks: KeyedLocks::default(),
            bundle_locks: KeyedLocks::default(),
        })
    }

    pub fn turbojpeg_available() -> bool {
        crate::ffi::turbojpeg_available()
    }

    pub fn serve_route(self: &Arc<Self>, method: &str, path: &str, body: &[u8]) -> RouteResponse {
        let raw_path = path.split('?').next().unwrap_or("/");
        let trimmed = raw_path.trim_matches('/');
        let parts: Vec<&str> = trimmed.split('/').collect();

        if method == "OPTIONS" {
            return RouteResponse::new(204, "text/plain", Vec::new());
        }

        if method == "POST" && trimmed.ends_with("entities/versions") {
            let pointers: Vec<String> = serde_json::from_slice::<serde_json::Value>(body)
                .ok()
                .and_then(|v| {
                    v.get("pointers").and_then(|p| p.as_array()).map(|a| {
                        a.iter()
                            .filter_map(|x| x.as_str().map(String::from))
                            .collect()
                    })
                })
                .unwrap_or_default();
            let pairs: Vec<(String, String)> = pointers
                .par_iter()
                .flat_map(|p| {
                    self.catalyst
                        .resolve_scene(p)
                        .map(|s| {
                            s.content
                                .iter()
                                .map(|c| (c.hash.to_lowercase(), s.entity_id.clone()))
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default()
                })
                .collect();
            {
                let mut idx = self.hash_index.lock().unwrap();
                for (h, e) in pairs {
                    idx.entry(h).or_insert(e);
                }
            }
            let ver = serde_json::json!({"version": self.version, "buildDate": self.date});
            let mut assets = serde_json::Map::new();
            for p in &pointers {
                assets.insert(p.clone(), ver.clone());
            }
            let resp = serde_json::json!({
                "pointers": pointers,
                "versions": {"assets": {"windows": ver, "mac": ver, "linux": ver, "webgl": ver}},
                "bundles": {"assets": serde_json::Value::Object(assets)},
            });
            return RouteResponse::new(200, "application/json", resp.to_string().into_bytes());
        }

        if method != "GET" && method != "HEAD" {
            return RouteResponse::new(404, "text/plain", b"not found".to_vec());
        }

        if parts.len() == 2 && parts[0] == "manifest" && parts[1].ends_with(".json") {
            let stem = &parts[1][..parts[1].len() - 5];
            let entity = stem.rsplit_once('_').map(|(e, _)| e).unwrap_or(stem);
            match self.entity_ctx(entity) {
                Ok(_) => {
                    let body = serde_json::json!({"version": self.version, "date": self.date});
                    if let Some(space) = self.space.as_ref() {
                        let key = format!("manifest/{stem}.json");
                        let _ = space.put(&key, body.to_string().as_bytes(), "application/json");
                    }
                    return RouteResponse::new(
                        200,
                        "application/json",
                        body.to_string().into_bytes(),
                    );
                }
                Err(_) => {
                    if let Some(space) = self.space.as_ref() {
                        let key = format!("manifest/{stem}.json");
                        if let Ok(Some(b)) = space.get(&key) {
                            return RouteResponse::new(200, "application/json", b);
                        }
                    }
                    return RouteResponse::new(
                        404,
                        "application/json",
                        b"{\"error\":\"unknown entity\"}".to_vec(),
                    );
                }
            }
        }

        let (cid, file): (Option<String>, &str) = match parts.as_slice() {
            [_v, entity, f] => (Some((*entity).to_string()), *f),
            [_v, f] => {
                let hash = f.rsplit_once('_').map(|(h, _)| h).unwrap_or(f);
                (self.entity_for_hash(hash), f)
            }
            _ => return RouteResponse::new(404, "text/plain", b"not found".to_vec()),
        };
        let Some(cid) = cid else {
            return RouteResponse::new(404, "text/plain", b"unknown asset".to_vec());
        };
        let (code, data, _src) = self.serve_or_fallback(&cid, file);
        let ctype = if code == 200 {
            "application/octet-stream"
        } else {
            "text/plain"
        };
        RouteResponse::new(code, ctype, data)
    }
}
