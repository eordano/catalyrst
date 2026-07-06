use crate::builder::{build_bundle, BuildOpts};
use crate::catalyst::{CatalystClient, Scene};
use crate::glbscan::{scan_entity, EntityScan, UriCache};
use crate::local_store::LocalContentStore;
use crate::naming;
use crate::space::Space;
use anyhow::{anyhow, bail, Context, Result};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

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

fn bounded_reserve<V>(map: &mut HashMap<String, V>, cap: usize, key: &str) {
    if map.len() >= cap && !map.contains_key(key) {
        if let Some(k) = map.keys().next().cloned() {
            map.remove(&k);
        }
    }
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

    entities: Mutex<HashMap<String, Arc<EntityCtx>>>,
    hash_index: Mutex<HashMap<String, String>>,
    entity_cap: usize,
    hash_index_cap: usize,
    entity_locks: KeyedLocks,
    bundle_locks: KeyedLocks,
    collection_mode: bool,
    real_textures: bool,
    v38_compat: bool,
    v38_timestamp: i64,
    magenta_missing: bool,
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
                let key = c.hash.to_lowercase();
                bounded_reserve(&mut idx, self.hash_index_cap, &key);
                idx.entry(key).or_insert_with(|| cid.to_string());
            }
        }

        let ctx = Arc::new(EntityCtx {
            scene,
            content_by_file,
            scan,
        });
        let mut g = self.entities.lock().unwrap();
        bounded_reserve(&mut g, self.entity_cap, cid);
        g.insert(cid.to_string(), ctx.clone());
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
            magenta_missing: self.magenta_missing,
            collection_mode: self.collection_mode,
            real_textures: self.real_textures,
            v38_compat: self.v38_compat,
            v38_timestamp: self.v38_timestamp,
            lod: None,
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

    pub fn entity_for_hash(&self, hash: &str) -> Option<String> {
        self.hash_index
            .lock()
            .unwrap()
            .get(&hash.to_lowercase())
            .cloned()
    }

    pub fn index_content_hashes<I: IntoIterator<Item = (String, String)>>(&self, pairs: I) {
        let mut idx = self.hash_index.lock().unwrap();
        for (hash, entity) in pairs {
            let key = hash.to_lowercase();
            bounded_reserve(&mut idx, self.hash_index_cap, &key);
            idx.entry(key).or_insert(entity);
        }
    }

    fn bundle_key(version: &str, cid: &str, file: &str) -> String {
        format!("{version}/{cid}/{file}")
    }

    pub fn space_configured(&self) -> bool {
        self.space.is_some()
    }

    fn space_get_timed(space: &crate::space::Space, key: &str) -> crate::Result<Option<Vec<u8>>> {
        let t = std::time::Instant::now();
        let r = space.get(key);
        let result = match &r {
            Ok(Some(_)) => "hit",
            Ok(None) => "miss",
            Err(_) => "error",
        };
        metrics::histogram!("abgen_space_request_duration_seconds", "op" => "get", "result" => result)
            .record(t.elapsed().as_secs_f64());
        if let Ok(Some(b)) = &r {
            metrics::counter!("abgen_space_transfer_bytes_total", "direction" => "download")
                .increment(b.len() as u64);
            tracing::info!(key = %key, bytes = b.len(), ms = t.elapsed().as_millis() as u64, "space hit");
        }
        if r.is_err() {
            metrics::counter!("abgen_space_errors_total", "op" => "get").increment(1);
        }
        r
    }

    fn space_put_timed(
        space: &crate::space::Space,
        key: &str,
        bytes: &[u8],
        content_type: &str,
    ) -> crate::Result<()> {
        let t = std::time::Instant::now();
        let r = space.put(key, bytes, content_type);
        let result = if r.is_ok() { "ok" } else { "error" };
        metrics::histogram!("abgen_space_request_duration_seconds", "op" => "put", "result" => result)
            .record(t.elapsed().as_secs_f64());
        if r.is_ok() {
            metrics::counter!("abgen_space_transfer_bytes_total", "direction" => "upload")
                .increment(bytes.len() as u64);
            metrics::histogram!("abgen_space_object_bytes").record(bytes.len() as f64);
        } else {
            metrics::counter!("abgen_space_errors_total", "op" => "put").increment(1);
        }
        r
    }

    pub fn space_get_bundle(&self, cid: &str, file: &str) -> Option<Vec<u8>> {
        let space = self.space.as_ref()?;
        let mut versions = vec![self.version.as_str()];
        if self.fallback_version != self.version {
            versions.push(self.fallback_version.as_str());
        }
        for ver in versions {
            let key = Self::bundle_key(ver, cid, file);
            match Self::space_get_timed(space, &key) {
                Ok(Some(b)) => return Some(b),
                Ok(None) => {}
                Err(e) => {
                    tracing::warn!(key = %key, error = %format!("{e:#}"), "space get failed; trying next version");
                }
            }
        }
        None
    }

    pub fn space_get_key(&self, key: &str) -> Option<Vec<u8>> {
        let space = self.space.as_ref()?;
        match Self::space_get_timed(space, key) {
            Ok(Some(b)) => Some(b),
            Ok(None) => None,
            Err(e) => {
                tracing::warn!(key = %key, error = %format!("{e:#}"), "space get failed");
                None
            }
        }
    }

    pub fn space_put_key(&self, key: &str, bytes: &[u8], content_type: &str) {
        let Some(space) = self.space.as_ref() else {
            return;
        };
        if space.read_only {
            return;
        }
        match Self::space_put_timed(space, key, bytes, content_type) {
            Ok(()) => tracing::info!(key = %key, bytes = bytes.len(), "space put"),
            Err(e) => tracing::warn!(key = %key, error = %format!("{e:#}"), "space put failed"),
        }
    }

    pub fn space_probe_versions(&self, first: &str) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for v in [first, self.version.as_str(), self.fallback_version.as_str()] {
            if !v.is_empty() && !out.iter().any(|o| o == v) {
                out.push(v.to_string());
            }
        }
        out
    }

    pub fn space_get_manifest(&self, stem: &str) -> Option<Vec<u8>> {
        self.space_get_key(&format!("manifest/{stem}.json"))
    }

    pub fn space_put_bundle(&self, cid: &str, file: &str, bytes: &[u8]) {
        self.space_put_key(
            &Self::bundle_key(&self.version, cid, file),
            bytes,
            "application/octet-stream",
        );
    }

    pub fn space_put_manifest(&self, stem: &str, bytes: &[u8]) {
        self.space_put_key(&format!("manifest/{stem}.json"), bytes, "application/json");
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
            let dst = pdir.join(&bundle_name);
            let existed = dst.is_file();
            match self.bundle(cid, &bundle_name) {
                Ok(bytes) => {
                    let tmp = dst.with_extension(format!("tmp.{}", std::process::id()));
                    std::fs::write(&tmp, &bytes)
                        .with_context(|| format!("write {}", tmp.display()))?;
                    std::fs::rename(&tmp, &dst).ok();
                    if !existed {
                        self.space_put_bundle(cid, &bundle_name, &bytes);
                    }
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
        let manifest_path = crate::manifest::write_corpus_manifest(
            out_root,
            cid,
            platform,
            &built,
            &self.version,
            content_server_url,
            crate::manifest::exit_code_for_failures(failed.len() + tolerated),
            &self.date,
        )?;
        if self.space_configured() {
            match std::fs::read(&manifest_path) {
                Ok(mbytes) => self.space_put_manifest(&format!("{cid}_{platform}"), &mbytes),
                Err(e) => tracing::warn!(
                    path = %manifest_path.display(),
                    error = %e,
                    "manifest read for space put failed"
                ),
            }
        }
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
    crate::dates::iso8601_utc(base + (n % 946_080_000))
}

pub fn build_scoped_date() -> String {
    use std::sync::OnceLock;
    static CACHE: OnceLock<String> = OnceLock::new();
    CACHE.get_or_init(|| iso_from_build_id(&build_id())).clone()
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
            template_root: None,
        }
    }
}

impl Proxy {
    pub fn new(cfg: ProxyConfig) -> Arc<Self> {
        Self::new_with_space(cfg, None)
    }

    pub fn new_with_space(cfg: ProxyConfig, injected_space: Option<Arc<Space>>) -> Arc<Self> {
        let collection_mode = BuildOpts::env_collection_mode();
        let real_textures = !cfg.parity || BuildOpts::env_real_textures();
        let v38_compat = !cfg.parity || BuildOpts::env_v38_compat();
        let v38_timestamp = BuildOpts::env_v38_timestamp();
        let magenta_missing = cfg.magenta_missing || BuildOpts::env_magenta_missing();
        if let Some(root) = cfg.template_root.as_deref().filter(|s| !s.is_empty()) {
            let env_root = std::env::var("ABGEN_ROOT").unwrap_or_default();
            if env_root.trim() != root {
                tracing::warn!(
                    template_root = %root,
                    abgen_root_env = %env_root,
                    "template_root differs from the ABGEN_ROOT env — builder templates \
                     resolve from ABGEN_ROOT (or the crate dir); set ABGEN_ROOT at \
                     process start"
                );
            }
        }
        let bid = build_id();
        let date = cfg.date.unwrap_or_else(|| iso_from_build_id(&bid));
        let cache_root = PathBuf::from(&cfg.cache_dir);
        let content = LocalContentStore::new(cache_root.join("content"));
        let bundle_dir = cache_root.join("bundles").join(&bid[..16.min(bid.len())]);
        let _ = std::fs::create_dir_all(&bundle_dir);
        let space = match injected_space {
            Some(s) => Some(s),
            None if cfg.use_space => {
                let s = Space::from_env().map(Arc::new);
                if s.is_none() {
                    tracing::warn!(
                        "S3 space cache requested (use_space) but disabled: endpoint/credentials \
                         missing (set ABGEN_S3_ENDPOINT and credentials)"
                    );
                }
                s
            }
            None => None,
        };
        Arc::new(Proxy {
            catalyst: {
                let mut c = CatalystClient::from_args(&cfg.catalyst_url, cfg.local_root.as_deref());
                if let Some(wurl) = crate::worlds::content_fallback_from_env() {
                    tracing::info!(url = %wurl, "worlds content fallback ENABLED");
                    c = c.with_fallback_base(wurl);
                }
                c
            },
            local: cfg.local_root.map(LocalContentStore::new),
            content,
            bundle_dir,
            version: cfg.version,
            date,
            uri_cache: UriCache::new(),
            space,
            fallback_version: cfg.fallback_version,
            entities: Mutex::new(HashMap::new()),
            hash_index: Mutex::new(HashMap::new()),
            entity_cap: std::env::var("ABGEN_ENTITY_CACHE_CAP")
                .ok()
                .and_then(|s| s.parse::<usize>().ok())
                .filter(|n| *n > 0)
                .unwrap_or(4096),
            hash_index_cap: std::env::var("ABGEN_HASH_INDEX_CAP")
                .ok()
                .and_then(|s| s.parse::<usize>().ok())
                .filter(|n| *n > 0)
                .unwrap_or(65536),
            entity_locks: KeyedLocks::default(),
            bundle_locks: KeyedLocks::default(),
            collection_mode,
            real_textures,
            v38_compat,
            v38_timestamp,
            magenta_missing,
        })
    }

    pub fn turbojpeg_available() -> bool {
        crate::ffi::turbojpeg_available()
    }
}

#[cfg(test)]
pub mod stub {
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};

    pub type Routes = Vec<(String, u16, Vec<u8>)>;

    pub fn stub_proxy_at(
        host: &str,
        catalyst_url: &str,
        read_only: bool,
        cache_dir: &std::path::Path,
    ) -> Arc<super::Proxy> {
        let space = crate::space::Space::with_static_creds(
            "http",
            host,
            "us-east-1",
            None,
            false,
            read_only,
            "AKIATEST",
            "secret",
        );
        let cfg = super::ProxyConfig {
            catalyst_url: catalyst_url.to_string(),
            cache_dir: cache_dir.to_string_lossy().into_owned(),
            version: "v41".to_string(),
            fallback_version: "v40".to_string(),
            ..Default::default()
        };
        super::Proxy::new_with_space(cfg, Some(Arc::new(space)))
    }

    pub fn serve(routes: Routes) -> (String, Arc<Mutex<Vec<String>>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let seen: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let seen2 = seen.clone();
        std::thread::spawn(move || {
            for conn in listener.incoming() {
                let Ok(mut stream) = conn else { break };
                let mut reader = BufReader::new(match stream.try_clone() {
                    Ok(r) => r,
                    Err(_) => continue,
                });
                let mut line = String::new();
                if reader.read_line(&mut line).is_err() {
                    continue;
                }
                let mut parts = line.split_whitespace();
                let method = parts.next().unwrap_or("").to_string();
                let path = parts.next().unwrap_or("").to_string();
                let mut content_len = 0usize;
                loop {
                    let mut h = String::new();
                    if reader.read_line(&mut h).is_err() {
                        break;
                    }
                    let ht = h.trim();
                    if ht.is_empty() {
                        break;
                    }
                    if let Some(v) = ht.to_ascii_lowercase().strip_prefix("content-length:") {
                        content_len = v.trim().parse().unwrap_or(0);
                    }
                }
                if content_len > 0 {
                    let mut body = vec![0u8; content_len];
                    let _ = reader.read_exact(&mut body);
                }
                seen2.lock().unwrap().push(format!("{method} {path}"));
                let (code, body) = routes
                    .iter()
                    .find(|(p, _, _)| path == *p)
                    .map(|(_, c, b)| (*c, b.clone()))
                    .unwrap_or((404, Vec::new()));
                let reason = match code {
                    200 => "OK",
                    404 => "Not Found",
                    _ => "Error",
                };
                let _ = write!(
                    stream,
                    "HTTP/1.1 {code} {reason}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = stream.write_all(&body);
                let _ = stream.flush();
            }
        });
        (format!("127.0.0.1:{}", addr.port()), seen)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_cache(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("abgen-live-test-{tag}-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        dir
    }

    fn stub_proxy(host: &str, read_only: bool, tag: &str) -> Arc<Proxy> {
        super::stub::stub_proxy_at(host, "http://127.0.0.1:9", read_only, &temp_cache(tag))
    }

    #[test]
    fn space_get_bundle_continues_to_fallback_after_transport_error() {
        let (host, seen) = super::stub::serve(vec![
            ("/v41/bafkcid/Qmhash_windows".to_string(), 500, Vec::new()),
            (
                "/v40/bafkcid/Qmhash_windows".to_string(),
                200,
                b"FALLBACK".to_vec(),
            ),
        ]);
        let proxy = stub_proxy(&host, false, "bug5");
        let got = proxy.space_get_bundle("bafkcid", "Qmhash_windows");
        assert_eq!(got.as_deref(), Some(&b"FALLBACK"[..]));
        let log = seen.lock().unwrap().clone();
        assert_eq!(
            log,
            vec![
                "GET /v41/bafkcid/Qmhash_windows".to_string(),
                "GET /v40/bafkcid/Qmhash_windows".to_string(),
            ]
        );
    }

    #[test]
    fn space_key_helpers_roundtrip_and_respect_read_only() {
        let (host, seen) = super::stub::serve(vec![
            (
                "/LOD/1/bafk_1_windows".to_string(),
                200,
                b"LODBYTES".to_vec(),
            ),
            ("/v41/flatalias_windows".to_string(), 200, Vec::new()),
        ]);
        let proxy = stub_proxy(&host, false, "keys");
        assert_eq!(
            proxy.space_get_key("LOD/1/bafk_1_windows").as_deref(),
            Some(&b"LODBYTES"[..])
        );
        assert_eq!(proxy.space_get_key("LOD/1/other_1_windows"), None);
        proxy.space_put_key("v41/flatalias_windows", b"X", "application/octet-stream");
        let log = seen.lock().unwrap().clone();
        assert!(
            log.contains(&"PUT /v41/flatalias_windows".to_string()),
            "{log:?}"
        );

        let (host_ro, seen_ro) = super::stub::serve(vec![]);
        let ro = stub_proxy(&host_ro, true, "keys-ro");
        ro.space_put_key("v41/never_windows", b"X", "application/octet-stream");
        assert!(seen_ro.lock().unwrap().is_empty());
    }

    #[test]
    fn bounded_reserve_evicts_only_past_cap() {
        let mut m: HashMap<String, u32> = HashMap::new();
        m.insert("a".to_string(), 1);
        m.insert("b".to_string(), 2);
        bounded_reserve(&mut m, 2, "c");
        assert_eq!(m.len(), 1);
        m.insert("c".to_string(), 3);
        assert!(m.contains_key("c"));
        bounded_reserve(&mut m, 2, "c");
        assert_eq!(m.len(), 2);
        assert!(m.contains_key("c"));
    }

    #[test]
    fn probe_versions_dedup_and_hash_index() {
        let (host, _seen) = super::stub::serve(vec![]);
        let proxy = stub_proxy(&host, false, "probe");
        assert_eq!(
            proxy.space_probe_versions("v39"),
            vec!["v39".to_string(), "v41".to_string(), "v40".to_string()]
        );
        assert_eq!(
            proxy.space_probe_versions("v41"),
            vec!["v41".to_string(), "v40".to_string()]
        );
        assert_eq!(proxy.entity_for_hash("QmAbC"), None);
        proxy.index_content_hashes(vec![("QmAbC".to_string(), "bafkowner".to_string())]);
        assert_eq!(proxy.entity_for_hash("qmabc").as_deref(), Some("bafkowner"));
        assert_eq!(proxy.entity_for_hash("QmAbC").as_deref(), Some("bafkowner"));
    }
}
