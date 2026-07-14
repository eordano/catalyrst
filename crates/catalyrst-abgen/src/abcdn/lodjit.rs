use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use super::resolver;
use super::state::{AppState, ResolveCache};
use crate::lodgen::simplify::SimplifierBackend;

pub const REASON_HEADER: &str = "x-abgen-reason";
pub const JIT_ENV: &str = "ABGEN_LOD_JIT";
pub const MANIFEST_BUILDER_ENV: &str = "ABGEN_LOD_MANIFEST_BUILDER";
pub const CACHE_DIR_ENV: &str = "ABGEN_LOD_CACHE_DIR";
pub const TIMEOUT_ENV: &str = "ABGEN_LOD_JIT_TIMEOUT_S";
pub const FAIL_TTL_ENV: &str = "ABGEN_LOD_JIT_FAIL_TTL_S";
pub const BUILD_CONCURRENCY_ENV: &str = "ABGEN_LOD_BUILD_CONCURRENCY";

pub const LOD_PLATFORMS: [&str; 3] = ["windows", "mac", "linux"];
pub const LOD_LEVELS: [u32; 2] = [0, 1];

pub type InflightMap = Arc<tokio::sync::Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>>;

static STAGE_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

async fn remove_inflight_entry(map: &InflightMap, key: &str, lock: &Arc<tokio::sync::Mutex<()>>) {
    let mut g = map.lock().await;
    if let Some(l) = g.get(key) {
        if Arc::ptr_eq(l, lock) && Arc::strong_count(l) <= 2 {
            g.remove(key);
        }
    }
}

fn promote_tree(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for ent in std::fs::read_dir(src)? {
        let ent = ent?;
        let to = dst.join(ent.file_name());
        if ent.file_type()?.is_dir() {
            promote_tree(&ent.path(), &to)?;
        } else if std::fs::rename(ent.path(), &to).is_err() {
            let bytes = std::fs::read(ent.path())?;
            let mut tmp_os = to.as_os_str().to_owned();
            tmp_os.push(format!(".tmp.{}", std::process::id()));
            let tmp = PathBuf::from(tmp_os);
            std::fs::write(&tmp, &bytes)?;
            std::fs::rename(&tmp, &to)?;
        }
    }
    Ok(())
}

pub type LodRunner = Arc<
    dyn Fn(crate::lodgen::GenerateParams) -> anyhow::Result<crate::lodgen::GenerateOutcome>
        + Send
        + Sync,
>;

pub fn lod_jit_target(path: &str) -> Option<(String, u32, String)> {
    let segs: Vec<&str> = path.split('/').collect();
    if segs.len() != 3 || segs[0] != "LOD" {
        return None;
    }
    let level: u32 = segs[1].parse().ok()?;
    if level >= 2 || segs[1] != level.to_string() {
        return None;
    }
    let raw = segs[2].strip_suffix(".br").unwrap_or(segs[2]);
    let (platform, stem) = resolver::split_platform(raw);
    if !LOD_PLATFORMS.contains(&platform) {
        return None;
    }
    let sid = stem.strip_suffix(&format!("_{level}"))?;
    if sid.is_empty() || !resolver::is_safe_component(sid) {
        return None;
    }
    Some((sid.to_string(), level, platform.to_string()))
}

pub fn sid_needs_case_resolution(sid: &str) -> bool {
    sid.starts_with("qm") && !sid.chars().any(|c| c.is_ascii_uppercase())
}

pub fn invalid_lod_reason(path: &str) -> &'static str {
    let segs: Vec<&str> = path.split('/').collect();
    if segs.len() == 3 && segs[0] == "LOD" {
        if let Ok(level) = segs[1].parse::<u32>() {
            if level >= 2 && segs[1] == level.to_string() {
                return "lod-level-unsupported";
            }
        }
    }
    "bad-path"
}

fn env_secs(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(default)
}

fn env_permits(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|s| s.trim().parse::<usize>().ok())
        .filter(|n| *n > 0)
        .unwrap_or(default)
}

fn first_line(msg: &str) -> String {
    let line = msg.lines().next().unwrap_or("").trim();
    let mut out: String = line.chars().take(500).collect();
    if out.is_empty() {
        out = "build failed".to_string();
    }
    out
}

pub struct LodJit {
    pub enabled: bool,
    pub simplifier: SimplifierBackend,
    pub gltfpack: Option<PathBuf>,
    pub manifest_builder: Option<String>,
    pub cache_dir: PathBuf,
    pub workdir: PathBuf,
    pub timeout: Duration,
    pub neg_cache: moka::future::Cache<String, String>,
    pub inflight: InflightMap,
    pub build_sem: Arc<tokio::sync::Semaphore>,
    pub disabled_reasons: Vec<String>,
    pub runner: LodRunner,
}

impl LodJit {
    pub fn from_env(live_cache_dir: &str) -> Self {
        let want = crate::clihelp::env_bool(JIT_ENV, false);
        let simplifier = SimplifierBackend::from_env();
        let probe = if want && simplifier == SimplifierBackend::Gltfpack {
            Some(crate::lodgen::simplify::resolve_gltfpack(None))
        } else {
            None
        };
        let manifest_builder = std::env::var(MANIFEST_BUILDER_ENV)
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        let base = std::env::var(CACHE_DIR_ENV)
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| live_cache_dir.to_string());
        Self::assemble(
            want,
            simplifier,
            probe,
            manifest_builder,
            &base,
            env_secs(TIMEOUT_ENV, 600),
            env_secs(FAIL_TTL_ENV, 3600),
            env_permits(BUILD_CONCURRENCY_ENV, 1),
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn assemble(
        flag_on: bool,
        simplifier: SimplifierBackend,
        gltfpack_probe: Option<anyhow::Result<PathBuf>>,
        manifest_builder: Option<String>,
        cache_base: &str,
        timeout_s: u64,
        fail_ttl_s: u64,
        build_concurrency: usize,
    ) -> Self {
        let mut disabled_reasons: Vec<String> = Vec::new();
        let mut gltfpack: Option<PathBuf> = None;
        if !flag_on {
            disabled_reasons.push("env-off".to_string());
        } else if simplifier == SimplifierBackend::Gltfpack {
            match gltfpack_probe {
                Some(Ok(bin)) => gltfpack = Some(bin),
                Some(Err(e)) => {
                    tracing::error!(
                        error = %format!("{e:#}"),
                        "ABGEN_LOD_JIT set but gltfpack is missing — LOD JIT lane DISABLED \
                         (fail closed); set ABGEN_GLTFPACK to a meshoptimizer gltfpack binary \
                         or ABGEN_SIMPLIFIER=meshopt for the in-crate simplifier"
                    );
                    disabled_reasons.push("gltfpack".to_string());
                }
                None => disabled_reasons.push("gltfpack".to_string()),
            }
        }
        let enabled = flag_on && disabled_reasons.is_empty();
        if enabled && manifest_builder.is_none() {
            tracing::warn!(
                "ABGEN_LOD_MANIFEST_BUILDER unset — LOD JIT can only build scenes that \
                 have a published ISS descriptor; the manifest-builder fallback is unavailable"
            );
        }
        LodJit {
            enabled,
            simplifier,
            gltfpack,
            manifest_builder,
            cache_dir: PathBuf::from(cache_base).join("lod-content"),
            workdir: PathBuf::from(cache_base).join("lod-work"),
            timeout: Duration::from_secs(timeout_s.max(1)),
            neg_cache: moka::future::Cache::builder()
                .max_capacity(10_000)
                .time_to_live(Duration::from_secs(fail_ttl_s.max(1)))
                .build(),
            inflight: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            build_sem: Arc::new(tokio::sync::Semaphore::new(build_concurrency.max(1))),
            disabled_reasons,
            runner: Arc::new(|p| crate::lodgen::generate(&p)),
        }
    }

    pub fn disabled_reason(&self) -> String {
        if self.disabled_reasons.is_empty() {
            "unknown".to_string()
        } else {
            self.disabled_reasons.join(",")
        }
    }

    pub async fn run_build(
        &self,
        out_root: &Path,
        catalyst_url: &str,
        resolve_cache: &ResolveCache,
        path: &str,
        sid: &str,
        level: u32,
    ) -> Result<(), String> {
        if !self.enabled {
            return Err(format!("lod-jit-disabled:{}", self.disabled_reason()));
        }
        let sid_lower = sid.to_lowercase();
        let key = sid_lower.clone();
        if self.neg_cache.get(&key).await.is_some() {
            metrics::counter!("abgen_lod_jit_negcache_hits_total").increment(1);
            return Err("lod-build-failed-cached".to_string());
        }
        let lock = {
            let mut g = self.inflight.lock().await;
            g.entry(key.clone()).or_default().clone()
        };
        let guard = match tokio::time::timeout(self.timeout, lock.clone().lock_owned()).await {
            Ok(g) => g,
            Err(_) => return Err("lod-build-inflight".to_string()),
        };
        let result = self
            .locked_build(
                out_root,
                catalyst_url,
                resolve_cache,
                path,
                sid,
                &sid_lower,
                level,
                &key,
                guard,
            )
            .await;
        remove_inflight_entry(&self.inflight, &key, &lock).await;
        result
    }

    #[allow(clippy::too_many_arguments)]
    async fn locked_build(
        &self,
        out_root: &Path,
        catalyst_url: &str,
        resolve_cache: &ResolveCache,
        path: &str,
        sid: &str,
        sid_lower: &str,
        level: u32,
        key: &str,
        guard: tokio::sync::OwnedMutexGuard<()>,
    ) -> Result<(), String> {
        if self.neg_cache.get(key).await.is_some() {
            metrics::counter!("abgen_lod_jit_coalesced_total").increment(1);
            metrics::counter!("abgen_lod_jit_negcache_hits_total").increment(1);
            return Err("lod-build-failed-cached".to_string());
        }
        if self.on_disk(out_root, path).await {
            metrics::counter!("abgen_lod_jit_coalesced_total").increment(1);
            invalidate_paths(resolve_cache, path, sid_lower).await;
            return Ok(());
        }
        let permit = match tokio::time::timeout(
            self.timeout,
            self.build_sem.clone().acquire_owned(),
        )
        .await
        {
            Ok(Ok(p)) => p,
            Ok(Err(_)) | Err(_) => return Err("lod-build-inflight".to_string()),
        };
        if self.on_disk(out_root, path).await {
            metrics::counter!("abgen_lod_jit_coalesced_total").increment(1);
            invalidate_paths(resolve_cache, path, sid_lower).await;
            return Ok(());
        }
        let stage_root = self.workdir.join(format!(
            "stage-{sid_lower}-{level}-{}-{}",
            std::process::id(),
            STAGE_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        let params = crate::lodgen::GenerateParams {
            scene: sid.to_string(),
            out_dir: stage_root.to_string_lossy().into_owned(),
            platforms: LOD_PLATFORMS.iter().map(|s| s.to_string()).collect(),
            levels: LOD_LEVELS.to_vec(),
            tri_cap_auto: true,
            iss: "auto".to_string(),
            manifest_builder: self.manifest_builder.clone(),
            workdir: Some(self.workdir.clone()),
            cache: Some(self.cache_dir.clone()),
            simplifier: self.simplifier,
            gltfpack: self.gltfpack.clone(),
            catalyst: catalyst_url.to_string(),
            crop: true,
            ..Default::default()
        };
        let runner = self.runner.clone();
        let finish = BuildFinish {
            neg_cache: self.neg_cache.clone(),
            resolve_cache: resolve_cache.clone(),
            out_root: out_root.to_path_buf(),
            stage_root,
            path: path.to_string(),
            sid_lower: sid_lower.to_string(),
            level,
            key: key.to_string(),
            started: std::time::Instant::now(),
        };
        let mut handle = tokio::task::spawn_blocking(move || runner(params));
        match tokio::time::timeout(self.timeout, &mut handle).await {
            Ok(joined) => {
                let result = finish.settle(joined).await;
                drop(permit);
                drop(guard);
                result
            }
            Err(_) => {
                metrics::counter!("abgen_lod_jit_builds_total", "outcome" => "timeout")
                    .increment(1);
                metrics::histogram!("abgen_lod_jit_build_duration_seconds", "outcome" => "timeout")
                    .record(finish.started.elapsed().as_secs_f64());
                tracing::warn!(
                    sid = %sid_lower,
                    level,
                    timeout_s = self.timeout.as_secs(),
                    "lod jit build timed out; single-flight lock and build slot stay held until \
                     the detached worker finishes (NOT negative-cached)"
                );
                let inflight = self.inflight.clone();
                let key_owned = key.to_string();
                tokio::spawn(async move {
                    let joined = handle.await;
                    let _ = finish.settle(joined).await;
                    drop(permit);
                    let lock = tokio::sync::OwnedMutexGuard::mutex(&guard).clone();
                    drop(guard);
                    remove_inflight_entry(&inflight, &key_owned, &lock).await;
                });
                Err("lod-build-timeout".to_string())
            }
        }
    }

    async fn on_disk(&self, out_root: &Path, path: &str) -> bool {
        let segs: Vec<&str> = path.split('/').collect();
        if segs.len() != 3 {
            return false;
        }
        let Some(exact) = resolver::lod_path(out_root, segs[1], segs[2]) else {
            return false;
        };
        tokio::task::spawn_blocking(move || resolver::resolve_with_casing(&exact).is_some())
            .await
            .unwrap_or(false)
    }
}

struct BuildFinish {
    neg_cache: moka::future::Cache<String, String>,
    resolve_cache: ResolveCache,
    out_root: PathBuf,
    stage_root: PathBuf,
    path: String,
    sid_lower: String,
    level: u32,
    key: String,
    started: std::time::Instant,
}

impl BuildFinish {
    async fn discard_stage(&self) {
        let stage = self.stage_root.clone();
        let _ = tokio::task::spawn_blocking(move || std::fs::remove_dir_all(&stage)).await;
    }

    async fn promote_stage(&self, scene_id: &str) -> std::io::Result<()> {
        let src = self.stage_root.join(scene_id);
        let dst = self.out_root.join(scene_id);
        let stage = self.stage_root.clone();
        tokio::task::spawn_blocking(move || {
            let r = promote_tree(&src, &dst);
            let _ = std::fs::remove_dir_all(&stage);
            r
        })
        .await
        .map_err(std::io::Error::other)?
    }

    async fn settle(
        self,
        joined: Result<anyhow::Result<crate::lodgen::GenerateOutcome>, tokio::task::JoinError>,
    ) -> Result<(), String> {
        let secs = self.started.elapsed().as_secs_f64();
        let record = |outcome: &'static str| {
            metrics::counter!("abgen_lod_jit_builds_total", "outcome" => outcome).increment(1);
            metrics::histogram!("abgen_lod_jit_build_duration_seconds", "outcome" => outcome)
                .record(secs);
        };
        match joined {
            Err(e) => {
                record("panic");
                tracing::error!(sid = %self.sid_lower, level = self.level, error = %e, "lod jit build worker panicked");
                self.discard_stage().await;
                self.neg_cache
                    .insert(self.key.clone(), format!("build worker panicked: {e}"))
                    .await;
                Err("lod-build-failed".to_string())
            }
            Ok(Err(e)) => {
                record("error");
                let msg = first_line(&format!("{e:#}"));
                tracing::warn!(sid = %self.sid_lower, level = self.level, elapsed_s = secs, error = %format!("{e:#}"), "lod jit build failed");
                self.discard_stage().await;
                self.neg_cache.insert(self.key.clone(), msg).await;
                Err("lod-build-failed".to_string())
            }
            Ok(Ok(outcome)) => {
                let fails = crate::lodgen::gate_failures(&outcome.gate);
                if fails > 0 {
                    record("gate_fail");
                    let first_fail = outcome
                        .gate
                        .iter()
                        .find(|c| !c.ok)
                        .map(|c| format!("{}: {}", c.label, c.detail))
                        .unwrap_or_default();
                    tracing::error!(
                        sid = %self.sid_lower,
                        level = self.level,
                        failures = fails,
                        first = %first_fail,
                        "lod jit build FAILED self-gate — discarding staged bundles, never serving them"
                    );
                    self.discard_stage().await;
                    self.neg_cache
                        .insert(
                            self.key.clone(),
                            format!(
                                "self-gate failed ({fails} checks): {}",
                                first_line(&first_fail)
                            ),
                        )
                        .await;
                    Err("lod-build-failed".to_string())
                } else if let Err(e) = self.promote_stage(&outcome.scene_id).await {
                    record("error");
                    tracing::error!(
                        sid = %self.sid_lower,
                        level = self.level,
                        error = %e,
                        "lod jit build passed the gate but promoting the staged output failed"
                    );
                    Err("lod-build-failed".to_string())
                } else {
                    record("ok");
                    tracing::info!(
                        sid = %self.sid_lower,
                        level = self.level,
                        elapsed_s = secs,
                        bundle_bytes = outcome
                            .levels
                            .iter()
                            .map(|l| l.bundle_bytes)
                            .sum::<usize>(),
                        "lod jit build ok (levels 0+1, windows+mac+linux written)"
                    );
                    invalidate_paths(&self.resolve_cache, &self.path, &self.sid_lower).await;
                    Ok(())
                }
            }
        }
    }
}

async fn invalidate_paths(resolve_cache: &ResolveCache, path: &str, sid_lower: &str) {
    let mut keys: Vec<String> = vec![path.to_string()];
    match path.strip_suffix(".br") {
        Some(stripped) => keys.push(stripped.to_string()),
        None => keys.push(format!("{path}.br")),
    }
    for level in LOD_LEVELS {
        for plat in LOD_PLATFORMS {
            let p = format!("LOD/{level}/{sid_lower}_{level}_{plat}");
            keys.push(format!("{p}.br"));
            keys.push(p);
        }
    }
    keys.push(format!("{sid_lower}/LOD.manifest.json"));
    keys.push(format!("{sid_lower}_InitialSceneState.json"));
    for k in keys {
        resolve_cache.invalidate(&k).await;
    }
}

pub async fn build_and_redispatch<F, Fut>(
    state: &AppState,
    path: &str,
    sid: &str,
    level: u32,
    redispatch: F,
) -> Result<axum::response::Response, String>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = axum::response::Response>,
{
    state
        .lod_jit
        .run_build(
            &state.jit_root,
            &state.catalyst_url,
            &state.resolve_cache,
            path,
            sid,
            level,
        )
        .await?;
    Ok(redispatch().await)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

    static SEQ: AtomicU64 = AtomicU64::new(0);

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "abgen-lodjit-test-{tag}-{}-{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn ok_outcome(sid: &str) -> crate::lodgen::GenerateOutcome {
        crate::lodgen::GenerateOutcome {
            entity_id: sid.to_string(),
            scene_id: sid.to_string(),
            source_tris: 0,
            levels: Vec::new(),
            gate: Vec::new(),
            log: Vec::new(),
        }
    }

    fn test_jit(runner: LodRunner, enabled: bool, timeout: Duration, base: &Path) -> LodJit {
        LodJit {
            enabled,
            simplifier: SimplifierBackend::Gltfpack,
            gltfpack: Some(PathBuf::from("/bin/true")),
            manifest_builder: None,
            cache_dir: base.join("lod-content"),
            workdir: base.join("lod-work"),
            timeout,
            neg_cache: moka::future::Cache::builder()
                .max_capacity(100)
                .time_to_live(Duration::from_secs(3600))
                .build(),
            inflight: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            build_sem: Arc::new(tokio::sync::Semaphore::new(1)),
            disabled_reasons: if enabled {
                Vec::new()
            } else {
                vec!["env-off".to_string()]
            },
            runner,
        }
    }

    fn new_cache() -> ResolveCache {
        moka::future::Cache::builder().max_capacity(100).build()
    }

    #[test]
    fn target_accepts_valid_lod_paths() {
        assert_eq!(
            lod_jit_target("LOD/1/bafk_1_windows"),
            Some(("bafk".to_string(), 1, "windows".to_string()))
        );
        assert_eq!(
            lod_jit_target("LOD/0/bafk_0_mac.br"),
            Some(("bafk".to_string(), 0, "mac".to_string()))
        );
        assert_eq!(
            lod_jit_target("LOD/1/QmUpper_1_linux"),
            Some(("QmUpper".to_string(), 1, "linux".to_string()))
        );
    }

    #[test]
    fn target_rejects_invalid_lod_paths() {
        assert_eq!(lod_jit_target("LOD/2/x_2_windows"), None);
        assert_eq!(lod_jit_target("LOD/1/bafk_0_windows"), None);
        assert_eq!(lod_jit_target("LOD/1/bafk_1"), None);
        assert_eq!(lod_jit_target("LOD/1/bafk_1_webgl"), None);
        assert_eq!(lod_jit_target("LOD/01/bafk_1_windows"), None);
        assert_eq!(lod_jit_target("LOD/1/_1_windows"), None);
        assert_eq!(lod_jit_target("LOD/1/.._1_windows"), None);
        assert_eq!(lod_jit_target("v41/bafk/Qmhash_windows"), None);
        assert_eq!(lod_jit_target("manifest/bafk_windows.json"), None);
        assert_eq!(lod_jit_target("LOD/1"), None);
        assert_eq!(lod_jit_target("LOD/1/a/b"), None);
    }

    #[test]
    fn case_resolution_predicate() {
        assert!(sid_needs_case_resolution(
            "qmy9qldkkf4pccghggkt2p13oj7chdydtfizihz3kifgvg"
        ));
        assert!(!sid_needs_case_resolution(
            "QmY9QLDKKF4pCcGhGGKt2p13oj7CHdyDTfizihZ3Kifgvg"
        ));
        assert!(!sid_needs_case_resolution(
            "bafkreieb6izdbhadi6vyjniq3hhpb363i44rf676wpjygyjrlhzsfp7eoa"
        ));
        assert!(!sid_needs_case_resolution("QmUpper"));
    }

    #[test]
    fn invalid_reason_taxonomy() {
        assert_eq!(
            invalid_lod_reason("LOD/2/x_2_windows"),
            "lod-level-unsupported"
        );
        assert_eq!(
            invalid_lod_reason("LOD/7/x_7_mac.br"),
            "lod-level-unsupported"
        );
        assert_eq!(invalid_lod_reason("LOD/1/bafk_1"), "bad-path");
        assert_eq!(invalid_lod_reason("LOD/x/bafk_1_windows"), "bad-path");
        assert_eq!(invalid_lod_reason("LOD/1"), "bad-path");
    }

    #[test]
    fn assemble_fails_closed_without_gltfpack() {
        let jit = LodJit::assemble(
            true,
            SimplifierBackend::Gltfpack,
            Some(Err(anyhow::anyhow!("gltfpack not found"))),
            None,
            "/tmp/abgen-lodjit-assemble",
            600,
            3600,
            1,
        );
        assert!(!jit.enabled);
        assert_eq!(jit.disabled_reasons, vec!["gltfpack".to_string()]);
        assert_eq!(jit.disabled_reason(), "gltfpack");

        let jit = LodJit::assemble(
            false,
            SimplifierBackend::Gltfpack,
            None,
            None,
            "/tmp/abgen-lodjit-assemble",
            600,
            3600,
            1,
        );
        assert!(!jit.enabled);
        assert_eq!(jit.disabled_reason(), "env-off");
        assert_eq!(jit.build_sem.available_permits(), 1);

        let jit = LodJit::assemble(
            true,
            SimplifierBackend::Gltfpack,
            Some(Ok(PathBuf::from("/bin/true"))),
            Some("/mb".to_string()),
            "/tmp/abgen-lodjit-assemble",
            42,
            7,
            3,
        );
        assert!(jit.enabled);
        assert_eq!(jit.gltfpack.as_deref(), Some(Path::new("/bin/true")));
        assert_eq!(jit.timeout, Duration::from_secs(42));
        assert!(jit.cache_dir.ends_with("lod-content"));
        assert!(jit.workdir.ends_with("lod-work"));
        assert_eq!(jit.build_sem.available_permits(), 3);

        let jit = LodJit::assemble(
            false,
            SimplifierBackend::Gltfpack,
            None,
            None,
            "/tmp/abgen-lodjit-assemble",
            600,
            3600,
            0,
        );
        assert_eq!(jit.build_sem.available_permits(), 1);
    }

    #[test]
    fn meshopt_backend_enables_without_gltfpack() {
        let jit = LodJit::assemble(
            true,
            SimplifierBackend::Meshopt,
            None,
            None,
            "/tmp/abgen-lodjit-assemble",
            600,
            3600,
            1,
        );
        assert!(jit.enabled);
        assert_eq!(jit.simplifier, SimplifierBackend::Meshopt);
        assert_eq!(jit.simplifier.name(), "meshopt");
        assert!(jit.gltfpack.is_none());
        assert!(jit.disabled_reasons.is_empty());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 8)]
    async fn concurrent_requests_coalesce_to_one_build() {
        let out_root = temp_dir("coalesce");
        let sid = "bafkreicoalesce";
        let calls = Arc::new(AtomicUsize::new(0));
        let c2 = calls.clone();
        let or2 = out_root.clone();
        let runner: LodRunner = Arc::new(move |p: crate::lodgen::GenerateParams| {
            c2.fetch_add(1, Ordering::SeqCst);
            std::thread::sleep(Duration::from_millis(150));
            let dir = PathBuf::from(&p.out_dir)
                .join(&p.scene)
                .join("LOD")
                .join("1");
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join(format!("{}_1_windows", p.scene)), b"bundle").unwrap();
            assert_ne!(p.out_dir, or2.to_string_lossy());
            assert!(PathBuf::from(&p.out_dir).starts_with(or2.join("lod-work")));
            assert_eq!(p.platforms, vec!["windows", "mac", "linux"]);
            assert_eq!(p.levels, vec![0, 1]);
            assert!(p.tri_cap_auto);
            assert!(p.crop);
            assert_eq!(p.iss, "auto");
            Ok(ok_outcome(&p.scene))
        });
        let jit = Arc::new(test_jit(runner, true, Duration::from_secs(30), &out_root));
        let cache = new_cache();
        let path = format!("LOD/1/{sid}_1_windows");
        let mut handles = Vec::new();
        for _ in 0..8 {
            let jit = jit.clone();
            let cache = cache.clone();
            let out_root = out_root.clone();
            let path = path.clone();
            handles.push(tokio::spawn(async move {
                jit.run_build(&out_root, "http://127.0.0.1:9", &cache, &path, sid, 1)
                    .await
            }));
        }
        for h in handles {
            assert_eq!(h.await.unwrap(), Ok(()));
        }
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert!(out_root
            .join(sid)
            .join("LOD")
            .join("1")
            .join(format!("{sid}_1_windows"))
            .is_file());
        assert!(jit.inflight.lock().await.is_empty());
        let _ = std::fs::remove_dir_all(&out_root);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 8)]
    async fn mixed_level_requests_coalesce_to_one_build() {
        let out_root = temp_dir("mixedlevel");
        let sid = "bafkreimixed";
        let calls = Arc::new(AtomicUsize::new(0));
        let c2 = calls.clone();
        let runner: LodRunner = Arc::new(move |p: crate::lodgen::GenerateParams| {
            c2.fetch_add(1, Ordering::SeqCst);
            std::thread::sleep(Duration::from_millis(150));
            for lvl in LOD_LEVELS {
                let dir = PathBuf::from(&p.out_dir)
                    .join(&p.scene)
                    .join("LOD")
                    .join(lvl.to_string());
                std::fs::create_dir_all(&dir).unwrap();
                std::fs::write(dir.join(format!("{}_{lvl}_windows", p.scene)), b"bundle").unwrap();
            }
            Ok(ok_outcome(&p.scene))
        });
        let jit = Arc::new(test_jit(runner, true, Duration::from_secs(30), &out_root));
        let cache = new_cache();
        let mut handles = Vec::new();
        for i in 0..8u32 {
            let level = i % 2;
            let jit = jit.clone();
            let cache = cache.clone();
            let out_root = out_root.clone();
            let path = format!("LOD/{level}/{sid}_{level}_windows");
            handles.push(tokio::spawn(async move {
                jit.run_build(&out_root, "http://127.0.0.1:9", &cache, &path, sid, level)
                    .await
            }));
        }
        for h in handles {
            assert_eq!(h.await.unwrap(), Ok(()));
        }
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert!(jit.inflight.lock().await.is_empty());
        let _ = std::fs::remove_dir_all(&out_root);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn failing_runner_negcaches_and_short_circuits() {
        let out_root = temp_dir("negcache");
        let sid = "bafkreifailing";
        let calls = Arc::new(AtomicUsize::new(0));
        let c2 = calls.clone();
        let runner: LodRunner = Arc::new(move |_p| {
            c2.fetch_add(1, Ordering::SeqCst);
            Err(anyhow::anyhow!("boom: resolve scene failed"))
        });
        let jit = test_jit(runner, true, Duration::from_secs(30), &out_root);
        let cache = new_cache();
        let path = format!("LOD/1/{sid}_1_windows");
        let first = jit
            .run_build(&out_root, "http://127.0.0.1:9", &cache, &path, sid, 1)
            .await;
        assert_eq!(first, Err("lod-build-failed".to_string()));
        let second = jit
            .run_build(&out_root, "http://127.0.0.1:9", &cache, &path, sid, 1)
            .await;
        assert_eq!(second, Err("lod-build-failed-cached".to_string()));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        let stored = jit.neg_cache.get(sid).await.unwrap();
        assert!(stored.contains("boom"), "{stored}");
        let _ = std::fs::remove_dir_all(&out_root);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn disabled_lane_never_invokes_runner() {
        let out_root = temp_dir("disabled");
        let calls = Arc::new(AtomicUsize::new(0));
        let c2 = calls.clone();
        let runner: LodRunner = Arc::new(move |p| {
            c2.fetch_add(1, Ordering::SeqCst);
            Ok(ok_outcome(&p.scene))
        });
        let jit = test_jit(runner, false, Duration::from_secs(30), &out_root);
        let cache = new_cache();
        let got = jit
            .run_build(
                &out_root,
                "http://127.0.0.1:9",
                &cache,
                "LOD/1/bafk_1_windows",
                "bafk",
                1,
            )
            .await;
        assert_eq!(got, Err("lod-jit-disabled:env-off".to_string()));
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        let _ = std::fs::remove_dir_all(&out_root);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn timeout_returns_without_negcaching() {
        let out_root = temp_dir("timeout");
        let sid = "bafkreitimeout";
        let calls = Arc::new(AtomicUsize::new(0));
        let c2 = calls.clone();
        let runner: LodRunner = Arc::new(move |p| {
            c2.fetch_add(1, Ordering::SeqCst);
            std::thread::sleep(Duration::from_millis(400));
            Ok(ok_outcome(&p.scene))
        });
        let jit = test_jit(runner, true, Duration::from_millis(60), &out_root);
        let cache = new_cache();
        let path = format!("LOD/1/{sid}_1_windows");
        let got = jit
            .run_build(&out_root, "http://127.0.0.1:9", &cache, &path, sid, 1)
            .await;
        assert_eq!(got, Err("lod-build-timeout".to_string()));
        tokio::time::sleep(Duration::from_millis(600)).await;
        assert!(jit.neg_cache.get(sid).await.is_none());
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        loop {
            if jit.inflight.lock().await.is_empty() {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "detached timed-out build never removed its inflight entry"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        let _ = std::fs::remove_dir_all(&out_root);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn timed_out_build_holds_locks_until_worker_finishes() {
        let out_root = temp_dir("detach");
        let sid = "bafkreidetach";
        let calls = Arc::new(AtomicUsize::new(0));
        let release = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let c2 = calls.clone();
        let r2 = release.clone();
        let runner: LodRunner = Arc::new(move |p: crate::lodgen::GenerateParams| {
            c2.fetch_add(1, Ordering::SeqCst);
            while !r2.load(Ordering::SeqCst) {
                std::thread::sleep(Duration::from_millis(5));
            }
            let dir = PathBuf::from(&p.out_dir)
                .join(&p.scene)
                .join("LOD")
                .join("1");
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join(format!("{}_1_windows", p.scene)), b"bundle").unwrap();
            Ok(ok_outcome(&p.scene))
        });
        let jit = test_jit(runner, true, Duration::from_millis(80), &out_root);
        let cache = new_cache();
        let path = format!("LOD/1/{sid}_1_windows");
        let first = jit
            .run_build(&out_root, "http://127.0.0.1:9", &cache, &path, sid, 1)
            .await;
        assert_eq!(first, Err("lod-build-timeout".to_string()));
        let t0 = std::time::Instant::now();
        while calls.load(Ordering::SeqCst) == 0 {
            assert!(t0.elapsed() < Duration::from_secs(10));
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        let retry = jit
            .run_build(&out_root, "http://127.0.0.1:9", &cache, &path, sid, 1)
            .await;
        assert_eq!(retry, Err("lod-build-inflight".to_string()));
        let other = jit
            .run_build(
                &out_root,
                "http://127.0.0.1:9",
                &cache,
                "LOD/1/bafkreiother_1_windows",
                "bafkreiother",
                1,
            )
            .await;
        assert_eq!(other, Err("lod-build-inflight".to_string()));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        release.store(true, Ordering::SeqCst);
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        loop {
            let again = jit
                .run_build(&out_root, "http://127.0.0.1:9", &cache, &path, sid, 1)
                .await;
            if again == Ok(()) {
                break;
            }
            assert_eq!(again, Err("lod-build-inflight".to_string()));
            assert!(
                std::time::Instant::now() < deadline,
                "detached build never released the single-flight lock"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        let _ = std::fs::remove_dir_all(&out_root);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn detached_failure_is_negcached_after_completion() {
        let out_root = temp_dir("detachfail");
        let sid = "bafkreidetachfail";
        let calls = Arc::new(AtomicUsize::new(0));
        let c2 = calls.clone();
        let runner: LodRunner = Arc::new(move |_p| {
            c2.fetch_add(1, Ordering::SeqCst);
            std::thread::sleep(Duration::from_millis(250));
            Err(anyhow::anyhow!("late boom"))
        });
        let jit = test_jit(runner, true, Duration::from_millis(60), &out_root);
        let cache = new_cache();
        let path = format!("LOD/1/{sid}_1_windows");
        let first = jit
            .run_build(&out_root, "http://127.0.0.1:9", &cache, &path, sid, 1)
            .await;
        assert_eq!(first, Err("lod-build-timeout".to_string()));
        let key = sid.to_string();
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while jit.neg_cache.get(&key).await.is_none() {
            assert!(
                std::time::Instant::now() < deadline,
                "detached failure never negative-cached"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        let stored = jit.neg_cache.get(&key).await.unwrap();
        assert!(stored.contains("late boom"), "{stored}");
        let second = jit
            .run_build(&out_root, "http://127.0.0.1:9", &cache, &path, sid, 1)
            .await;
        assert_eq!(second, Err("lod-build-failed-cached".to_string()));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        let _ = std::fs::remove_dir_all(&out_root);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn gate_failed_build_never_reaches_the_serving_root() {
        let out_root = temp_dir("gatefail");
        let sid = "bafkreigatefail";
        let runner: LodRunner = Arc::new(move |p: crate::lodgen::GenerateParams| {
            let entity_dir = PathBuf::from(&p.out_dir).join(&p.scene);
            let dir = entity_dir.join("LOD").join("1");
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join(format!("{}_1_windows", p.scene)), b"bad").unwrap();
            std::fs::write(entity_dir.join("LOD.manifest.json"), b"{}").unwrap();
            std::fs::write(
                entity_dir.join(format!("{}_InitialSceneState.json", p.scene)),
                b"{}",
            )
            .unwrap();
            let mut out = ok_outcome(&p.scene);
            out.gate.push(crate::lodgen::GateCheck {
                label: "root-name".to_string(),
                ok: false,
                detail: "got x want y".to_string(),
            });
            Ok(out)
        });
        let jit = test_jit(runner, true, Duration::from_secs(30), &out_root);
        let cache = new_cache();
        let path = format!("LOD/1/{sid}_1_windows");
        let got = jit
            .run_build(&out_root, "http://127.0.0.1:9", &cache, &path, sid, 1)
            .await;
        assert_eq!(got, Err("lod-build-failed".to_string()));
        assert!(!out_root.join(sid).exists());
        let leftovers: Vec<_> = std::fs::read_dir(out_root.join("lod-work"))
            .map(|rd| rd.flatten().map(|e| e.path()).collect())
            .unwrap_or_default();
        assert!(leftovers.is_empty(), "{leftovers:?}");
        let stored = jit.neg_cache.get(sid).await.unwrap();
        assert!(stored.contains("self-gate failed"), "{stored}");
        let _ = std::fs::remove_dir_all(&out_root);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn gate_passed_build_promotes_manifest_and_iss() {
        let out_root = temp_dir("promote");
        let sid = "bafkreipromote";
        let runner: LodRunner = Arc::new(move |p: crate::lodgen::GenerateParams| {
            let entity_dir = PathBuf::from(&p.out_dir).join(&p.scene);
            let dir = entity_dir.join("LOD").join("1");
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join(format!("{}_1_windows", p.scene)), b"bundle").unwrap();
            std::fs::write(dir.join(format!("{}_1_windows.br", p.scene)), b"br").unwrap();
            std::fs::write(entity_dir.join("LOD.manifest.json"), b"{}").unwrap();
            std::fs::write(
                entity_dir.join(format!("{}_InitialSceneState.json", p.scene)),
                b"{}",
            )
            .unwrap();
            Ok(ok_outcome(&p.scene))
        });
        let jit = test_jit(runner, true, Duration::from_secs(30), &out_root);
        let cache = new_cache();
        let path = format!("LOD/1/{sid}_1_windows");
        let got = jit
            .run_build(&out_root, "http://127.0.0.1:9", &cache, &path, sid, 1)
            .await;
        assert_eq!(got, Ok(()));
        let entity_dir = out_root.join(sid);
        for rel in [
            format!("LOD/1/{sid}_1_windows"),
            format!("LOD/1/{sid}_1_windows.br"),
            "LOD.manifest.json".to_string(),
            format!("{sid}_InitialSceneState.json"),
        ] {
            assert!(entity_dir.join(&rel).is_file(), "{rel}");
        }
        let leftovers: Vec<_> = std::fs::read_dir(out_root.join("lod-work"))
            .map(|rd| rd.flatten().map(|e| e.path()).collect())
            .unwrap_or_default();
        assert!(leftovers.is_empty(), "{leftovers:?}");
        let _ = std::fs::remove_dir_all(&out_root);
    }
}
