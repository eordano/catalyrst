use super::*;

fn record_space_read(lane: Option<&'static str>, hit: bool) {
    let outcome = if hit { "hit" } else { "miss" };
    match lane {
        None => metrics::counter!("abgen_space_reads_total", "outcome" => outcome).increment(1),
        Some(lane) => {
            metrics::counter!("abgen_space_lane_reads_total", "lane" => lane, "outcome" => outcome)
                .increment(1)
        }
    }
}

async fn space_materialize<F>(lane: Option<&'static str>, dst: std::path::PathBuf, fetch: F) -> bool
where
    F: FnOnce() -> Option<Vec<u8>> + Send + 'static,
{
    let hit = tokio::task::spawn_blocking(move || match fetch() {
        Some(bytes) => write_materialized(&dst, &bytes),
        None => false,
    })
    .await
    .unwrap_or(false);
    record_space_read(lane, hit);
    hit
}

#[allow(clippy::too_many_arguments)]
async fn space_lane_serve<F>(
    state: &AppState,
    lane: Option<&'static str>,
    dst: std::path::PathBuf,
    fetch: F,
    invalidate_key: &str,
    path: &str,
    method: &Method,
    headers: &HeaderMap,
) -> Option<Response>
where
    F: FnOnce() -> Option<Vec<u8>> + Send + 'static,
{
    if !space_materialize(lane, dst, fetch).await {
        return None;
    }
    state.resolve_cache.invalidate(invalidate_key).await;
    let resp = dispatch_local(state, path, method, headers).await;
    (resp.status() != StatusCode::NOT_FOUND).then_some(resp)
}

pub(super) enum JitBuild {
    Built,
    Coalesced,
    Failed,
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn timed_corpus_build(
    proxy: std::sync::Arc<crate::live::Proxy>,
    out_root: std::path::PathBuf,
    entity: String,
    platform: String,
    csu: String,
    counter: &'static str,
    histogram: &'static str,
    lane: &'static str,
) -> bool {
    let t = std::time::Instant::now();
    let (e2, p2) = (entity.clone(), platform.clone());
    let built = tokio::task::spawn_blocking(move || {
        proxy.build_entity_into_corpus(&out_root, &e2, &p2, &csu)
    })
    .await;
    let secs = t.elapsed().as_secs_f64();
    let outcome = match &built {
        Ok(Ok(_)) => "ok",
        Ok(Err(_)) => "error",
        Err(_) => "panic",
    };
    metrics::counter!(counter, "outcome" => outcome).increment(1);
    metrics::histogram!(histogram, "outcome" => outcome).record(secs);
    match built {
        Ok(Ok(_)) => true,
        Ok(Err(e)) => {
            tracing::warn!(
                error = %format!("{e:#}"),
                entity = %entity,
                platform = %platform,
                lane,
                elapsed_s = secs,
                "jit write-back failed"
            );
            false
        }
        Err(e) => {
            tracing::error!(error = %e, lane, "jit build worker panicked");
            false
        }
    }
}

async fn probe_exists(probe: Option<std::path::PathBuf>) -> bool {
    let Some(p) = probe else { return false };
    tokio::task::spawn_blocking(move || p.is_file())
        .await
        .unwrap_or(false)
}

fn jit_build_permits() -> usize {
    std::env::var("ABGEN_JIT_BUILD_CONCURRENCY")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .filter(|n| *n > 0)
        .unwrap_or_else(|| {
            std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(1)
        })
        .max(1)
}

fn jit_build_sem() -> &'static tokio::sync::Semaphore {
    static SEM: std::sync::OnceLock<tokio::sync::Semaphore> = std::sync::OnceLock::new();
    SEM.get_or_init(|| tokio::sync::Semaphore::new(jit_build_permits()))
}

pub(super) async fn jit_build_entity(
    state: &AppState,
    proxy: &std::sync::Arc<crate::live::Proxy>,
    entity: &str,
    platform: &str,
    probe: Option<std::path::PathBuf>,
    lane: &'static str,
) -> JitBuild {
    let key = format!("{entity}:{platform}");
    if state.jit_fail_cache.get(&key).await.is_some() {
        metrics::counter!("abgen_jit_builds_total", "outcome" => "fail-cached").increment(1);
        return JitBuild::Failed;
    }
    let lock = {
        let mut g = state.jit_inflight.lock().await;
        g.entry(key.clone()).or_default().clone()
    };
    let guard = lock.clone().lock_owned().await;
    let outcome = if state.jit_fail_cache.get(&key).await.is_some() {
        metrics::counter!("abgen_jit_builds_total", "outcome" => "fail-cached").increment(1);
        JitBuild::Failed
    } else if probe_exists(probe).await {
        metrics::counter!("abgen_jit_coalesced_total").increment(1);
        JitBuild::Coalesced
    } else {
        let _permit = jit_build_sem().acquire().await.ok();
        if timed_corpus_build(
            proxy.clone(),
            state.out_root.clone(),
            entity.to_string(),
            platform.to_string(),
            state.manifest_content_server_url.clone(),
            "abgen_jit_builds_total",
            "abgen_jit_build_duration_seconds",
            lane,
        )
        .await
        {
            JitBuild::Built
        } else {
            state.jit_fail_cache.insert(key.clone(), ()).await;
            JitBuild::Failed
        }
    };
    drop(guard);
    {
        let mut g = state.jit_inflight.lock().await;
        if let Some(l) = g.get(&key) {
            if std::sync::Arc::ptr_eq(l, &lock) && std::sync::Arc::strong_count(l) <= 2 {
                g.remove(&key);
            }
        }
    }
    outcome
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn bundle_fallback(
    state: &AppState,
    proxy: std::sync::Arc<crate::live::Proxy>,
    path: &str,
    target: &JitTarget,
    method: &Method,
    headers: &HeaderMap,
    local: Response,
) -> Response {
    if proxy.space_configured() && target.space_eligible() {
        if let Some(dst) = target.probe_path(&state.out_root) {
            let fetch = target.space_fetch(proxy.clone());
            if let Some(resp) =
                space_lane_serve(state, None, dst, fetch, path, path, method, headers).await
            {
                return resp;
            }
        }
    }
    match jit_build_entity(
        state,
        &proxy,
        target.entity(),
        target.platform(),
        target.probe_path(&state.out_root),
        "entity",
    )
    .await
    {
        JitBuild::Built | JitBuild::Coalesced => {
            state.resolve_cache.invalidate(path).await;
            dispatch_local(state, path, method, headers).await
        }
        JitBuild::Failed => local,
    }
}

#[cfg(feature = "content-db")]
async fn resolve_lod_sid_case(state: &AppState, sid: &str) -> String {
    if !lodjit::sid_needs_case_resolution(sid) {
        return sid.to_string();
    }
    let Some(cdb) = &state.content_db else {
        tracing::warn!(
            sid = %sid,
            "lowercased Qm scene id and no content DB; the case-sensitive content fetch will fail"
        );
        return sid.to_string();
    };
    let looked = sqlx::query_scalar::<_, String>(
        "SELECT entity_id FROM deployments \
         WHERE lower(entity_id) = $1 AND deleter_deployment IS NULL \
         ORDER BY entity_timestamp DESC LIMIT 1",
    )
    .bind(sid)
    .fetch_optional(cdb.pool())
    .await;
    match looked {
        Ok(Some(exact)) => exact,
        Ok(None) => sid.to_string(),
        Err(e) => {
            tracing::warn!(sid = %sid, error = %e, "content-db entity-id case lookup failed");
            sid.to_string()
        }
    }
}

#[cfg(not(feature = "content-db"))]
async fn resolve_lod_sid_case(_state: &AppState, sid: &str) -> String {
    if lodjit::sid_needs_case_resolution(sid) {
        tracing::warn!(
            sid = %sid,
            "lowercased Qm scene id and no content DB; the case-sensitive content fetch will fail"
        );
    }
    sid.to_string()
}

pub(super) async fn lod_fallback(
    state: &AppState,
    path: &str,
    method: &Method,
    headers: &HeaderMap,
    local: Response,
) -> Response {
    if let Some(resp) = lod_space_read_through(state, path, method, headers).await {
        return resp;
    }
    let reason = match lodjit::lod_jit_target(path) {
        None => lodjit::invalid_lod_reason(path).to_string(),
        Some((sid, level, _platform)) => {
            if *method != Method::GET {
                "lod-not-built".to_string()
            } else {
                let build_sid = resolve_lod_sid_case(state, &sid).await;
                let built = lodjit::build_and_redispatch(state, path, &build_sid, level, || {
                    dispatch_local(state, path, method, headers)
                })
                .await;
                match built {
                    Ok(resp) => {
                        if resp.status() != StatusCode::NOT_FOUND {
                            spawn_lod_writeback(state, &sid);
                            return resp;
                        }
                        "lod-not-built".to_string()
                    }
                    Err(reason) => reason,
                }
            }
        }
    };
    with_reason(local, &reason)
}

async fn lod_space_read_through(
    state: &AppState,
    path: &str,
    method: &Method,
    headers: &HeaderMap,
) -> Option<Response> {
    let proxy = state.live_proxy.clone()?;
    if !proxy.space_configured() {
        return None;
    }
    let segs: Vec<&str> = path.split('/').collect();
    if segs.len() != 3 {
        return None;
    }
    let dst = resolver::lod_path(&state.out_root, segs[1], segs[2])?;
    let key = path.to_string();
    space_lane_serve(
        state,
        Some("lod"),
        dst,
        move || proxy.space_get_key(&key),
        path,
        path,
        method,
        headers,
    )
    .await
}

pub(super) fn spawn_lod_writeback(state: &AppState, sid: &str) {
    let Some(proxy) = state.live_proxy.clone() else {
        return;
    };
    if !proxy.space_configured() {
        return;
    }
    let scene_dir = state.out_root.join(sid);
    let sid = sid.to_string();
    tokio::task::spawn_blocking(move || {
        let mut puts = 0usize;
        for level in lodjit::LOD_LEVELS {
            let dir = scene_dir.join("LOD").join(level.to_string());
            let Ok(rd) = std::fs::read_dir(&dir) else {
                continue;
            };
            for ent in rd.flatten() {
                if !ent.file_type().map(|t| t.is_file()).unwrap_or(false) {
                    continue;
                }
                let name = ent.file_name();
                let Some(name) = name.to_str() else { continue };
                if name.contains(".tmp.") {
                    continue;
                }
                if let Ok(bytes) = std::fs::read(ent.path()) {
                    proxy.space_put_key(
                        &format!("LOD/{level}/{name}"),
                        &bytes,
                        "application/octet-stream",
                    );
                    puts += 1;
                }
            }
        }
        let iss = format!("{sid}{}", crate::lodgen::placements::ISS_SUFFIX);
        for cand in [iss.clone(), format!("{iss}.br")] {
            if let Ok(bytes) = std::fs::read(scene_dir.join(&cand)) {
                let ct = if cand.ends_with(".json") {
                    "application/json"
                } else {
                    "application/octet-stream"
                };
                proxy.space_put_key(&format!("lods-unity/manifests/{cand}"), &bytes, ct);
                puts += 1;
            }
        }
        tracing::info!(sid = %sid, puts, "lod space write-back finished");
    });
}

pub(super) async fn iss_fallback(
    state: &AppState,
    path: &str,
    method: &Method,
    headers: &HeaderMap,
    local: Response,
) -> Response {
    let segs: Vec<&str> = path.split('/').collect();
    let filename = segs[2];
    let Some(dst) = resolver::iss_manifest_path(&state.out_root, filename) else {
        return local;
    };
    if let Some(proxy) = state.live_proxy.clone().filter(|p| p.space_configured()) {
        let key = path.to_string();
        if let Some(resp) = space_lane_serve(
            state,
            Some("iss"),
            dst,
            move || proxy.space_get_key(&key),
            filename,
            path,
            method,
            headers,
        )
        .await
        {
            return resp;
        }
    }
    with_reason(local, "iss-not-built")
}

pub(super) fn with_reason(mut resp: Response, reason: &str) -> Response {
    if let Ok(v) = reason.parse() {
        resp.headers_mut().insert(lodjit::REASON_HEADER, v);
    }
    resp
}

pub(super) fn materialize_tmp_path(dst: &std::path::Path) -> std::path::PathBuf {
    let mut tmp_os = dst.as_os_str().to_owned();
    tmp_os.push(format!(".tmp.{}", std::process::id()));
    std::path::PathBuf::from(tmp_os)
}

fn write_materialized(dst: &std::path::Path, bytes: &[u8]) -> bool {
    let Some(parent) = dst.parent() else {
        return false;
    };
    if std::fs::create_dir_all(parent).is_err() {
        return false;
    }
    let tmp = materialize_tmp_path(dst);
    if std::fs::write(&tmp, bytes).is_err() {
        return false;
    }
    std::fs::rename(&tmp, dst).is_ok()
}

pub(super) enum JitTarget {
    Manifest {
        stem: String,
        entity: String,
        platform: String,
    },
    Bundle {
        entity: String,
        file: String,
        platform: String,
    },
}

impl JitTarget {
    pub(super) fn entity(&self) -> &str {
        match self {
            JitTarget::Manifest { entity, .. } | JitTarget::Bundle { entity, .. } => entity,
        }
    }

    pub(super) fn platform(&self) -> &str {
        match self {
            JitTarget::Manifest { platform, .. } | JitTarget::Bundle { platform, .. } => platform,
        }
    }

    fn probe_path(&self, out_root: &std::path::Path) -> Option<std::path::PathBuf> {
        match self {
            JitTarget::Manifest { stem, .. } => resolver::manifest_path(out_root, stem),
            JitTarget::Bundle {
                entity,
                file,
                platform,
            } => Some(out_root.join(entity).join(platform).join(file)),
        }
    }

    fn space_eligible(&self) -> bool {
        match self {
            JitTarget::Manifest { .. } => true,
            JitTarget::Bundle { file, platform, .. } => file
                .rsplit_once('_')
                .is_some_and(|(_, suffix)| suffix == platform),
        }
    }

    fn space_fetch(
        &self,
        proxy: std::sync::Arc<crate::live::Proxy>,
    ) -> Box<dyn FnOnce() -> Option<Vec<u8>> + Send> {
        match self {
            JitTarget::Manifest { stem, .. } => {
                let stem = stem.clone();
                Box::new(move || proxy.space_get_manifest(&stem))
            }
            JitTarget::Bundle { entity, file, .. } => {
                let (entity, file) = (entity.clone(), file.clone());
                Box::new(move || proxy.space_get_bundle(&entity, &file))
            }
        }
    }
}

pub(super) fn jit_target(path: &str) -> Option<JitTarget> {
    let segs: Vec<&str> = path.split('/').collect();

    if segs.len() == 2 && segs[0] == "manifest" {
        let stem = segs[1].strip_suffix(".json")?;
        let (entity, platform) = stem.rsplit_once('_')?;
        if !resolver::is_platform(platform) || !resolver::is_safe_component(entity) {
            return None;
        }
        return Some(JitTarget::Manifest {
            stem: stem.to_string(),
            entity: entity.to_string(),
            platform: platform.to_string(),
        });
    }

    if segs.len() == 3 && segs[0] != "manifest" && segs[0] != "LOD" {
        let entity = segs[1];
        let file = segs[2];
        if file.ends_with(".br") {
            return None;
        }
        if !is_bundle_name(file) || !resolver::is_safe_component(entity) {
            return None;
        }
        return Some(JitTarget::Bundle {
            entity: entity.to_string(),
            file: file.to_string(),
            platform: resolver::platform_of(file).to_string(),
        });
    }
    None
}

pub(super) fn br_bundle_target(path: &str) -> bool {
    path.split('/').count() == 3 && path.strip_suffix(".br").and_then(jit_target).is_some()
}

pub(super) fn flat_target(path: &str) -> Option<(String, String)> {
    let segs: Vec<&str> = path.split('/').collect();
    if segs.len() != 2 {
        return None;
    }
    if matches!(segs[0], "manifest" | "LOD" | "lods-unity" | "dcl") {
        return None;
    }
    if !resolver::is_safe_component(segs[0]) || !resolver::is_safe_component(segs[1]) {
        return None;
    }
    let raw = segs[1].strip_suffix(".br").unwrap_or(segs[1]);
    let (bare, platform) = raw.rsplit_once('_')?;
    if bare.is_empty() || !resolver::is_platform(platform) {
        return None;
    }
    Some((bare.to_string(), platform.to_string()))
}

async fn resolve_hash_owner(
    state: &AppState,
    proxy: &std::sync::Arc<crate::live::Proxy>,
    bare: &str,
) -> Result<Option<String>, ()> {
    if let Some(cid) = proxy.entity_for_hash(bare) {
        return Ok(Some(cid));
    }
    #[cfg(feature = "content-db")]
    if let Some(cdb) = &state.content_db {
        match cdb.entity_for_content_hash(bare).await {
            Ok(Some(cid)) => return Ok(Some(cid)),
            Ok(None) => {}
            Err(e) => {
                tracing::warn!(hash = %bare, error = %e, "content-db hash lookup failed")
            }
        }
    }
    let st = state.clone();
    let hash = bare.to_string();
    let looked =
        tokio::task::spawn_blocking(move || st.content.active_entities_by_hash(&hash)).await;
    match looked {
        Ok(Ok(ids)) => Ok(ids.into_iter().next()),
        Ok(Err(e)) => {
            tracing::warn!(hash = %bare, error = %format!("{e:#}"), "catalyst active-entities lookup failed; not negative-caching");
            Err(())
        }
        Err(e) => {
            tracing::error!(error = %e, "hash resolution worker panicked");
            Err(())
        }
    }
}

async fn serve_flat_rewrite(
    state: &AppState,
    path: &str,
    cid: &str,
    filename: &str,
    raw: &str,
    is_br: bool,
    method: &Method,
    headers: &HeaderMap,
) -> Option<Response> {
    let exact = resolver::binary_path(&state.out_root, cid, filename)?;
    let resp = serve::serve_binary(state, path, &exact, raw, is_br, method, headers).await;
    if resp.status() != StatusCode::NOT_FOUND {
        Some(resp)
    } else {
        None
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn flat_fallback(
    state: &AppState,
    path: &str,
    bare: &str,
    platform: &str,
    method: &Method,
    headers: &HeaderMap,
    local: Response,
) -> Response {
    let Some(proxy) = state.live_proxy.clone() else {
        return local;
    };
    let segs: Vec<&str> = path.split('/').collect();
    let (url_ver, filename) = (segs[0].to_string(), segs[1].to_string());
    let raw = filename
        .strip_suffix(".br")
        .unwrap_or(&filename)
        .to_string();
    let is_br = filename.ends_with(".br");
    if proxy.space_configured() {
        let dst = state.out_root.join(&filename);
        let (p2, key) = (proxy.clone(), path.to_string());
        if let Some(resp) = space_lane_serve(
            state,
            Some("flat"),
            dst,
            move || p2.space_get_key(&key),
            path,
            path,
            method,
            headers,
        )
        .await
        {
            return resp;
        }
    }
    let neg_key = bare.to_string();
    if state.hash_neg_cache.get(&neg_key).await.is_some() {
        return with_reason(local, "hash-unresolved");
    }
    let cid = match resolve_hash_owner(state, &proxy, bare).await {
        Ok(Some(cid)) if resolver::is_safe_component(&cid) => cid,
        Ok(_) => {
            state.hash_neg_cache.insert(neg_key, ()).await;
            return with_reason(local, "hash-unresolved");
        }
        Err(()) => return with_reason(local, "hash-unresolved"),
    };
    if proxy.space_configured() && !is_br {
        let (p2, c2, r2) = (proxy.clone(), cid.clone(), raw.clone());
        let dst = state.out_root.join(&cid).join(platform).join(&raw);
        let materialized = space_materialize(Some("flat-alias"), dst, move || {
            p2.space_get_bundle(&c2, &r2)
        })
        .await;
        if materialized {
            state.resolve_cache.invalidate(path).await;
            if let Some(resp) =
                serve_flat_rewrite(state, path, &cid, &filename, &raw, is_br, method, headers).await
            {
                return resp;
            }
        }
    }
    let probe = state.out_root.join(&cid).join(platform).join(&raw);
    let built = jit_build_entity(state, &proxy, &cid, platform, Some(probe), "flat").await;
    if matches!(built, JitBuild::Failed) {
        return local;
    }
    state.resolve_cache.invalidate(path).await;
    if matches!(built, JitBuild::Built) && proxy.space_configured() {
        let src = state.out_root.join(&cid).join(platform).join(&raw);
        let alias = format!("{url_ver}/{raw}");
        let p3 = proxy.clone();
        tokio::task::spawn_blocking(move || {
            if let Ok(bytes) = std::fs::read(&src) {
                p3.space_put_key(&alias, &bytes, "application/octet-stream");
            }
        });
    }
    if let Some(resp) =
        serve_flat_rewrite(state, path, &cid, &filename, &raw, is_br, method, headers).await
    {
        return resp;
    }
    local
}

fn shader_flat_alias(state: &AppState, basename: &str) -> Option<std::path::PathBuf> {
    if basename.starts_with("scene_ignore_") {
        Some(state.out_root.join(basename))
    } else {
        None
    }
}

fn materialize_shader(
    dst: &std::path::Path,
    flat: Option<&std::path::PathBuf>,
    bytes: &[u8],
) -> bool {
    let ok = write_materialized(dst, bytes);
    if ok {
        if let Some(f) = flat {
            let _ = write_materialized(f, bytes);
        }
    }
    ok
}

pub(super) async fn shader_fallback(
    state: &AppState,
    path: &str,
    target: &resolver::ShaderTarget,
    method: &Method,
    headers: &HeaderMap,
    local: Response,
) -> Response {
    let Some(exact) = resolver::shader_path(&state.out_root, &target.canonical) else {
        return local;
    };
    let basename = target
        .canonical
        .rsplit('/')
        .next()
        .unwrap_or(&target.canonical)
        .to_string();
    let cache_key = format!("shader:{}", target.canonical);
    let first =
        serve::serve_binary(state, &cache_key, &exact, &basename, false, method, headers).await;
    if first.status() != StatusCode::NOT_FOUND {
        return first;
    }
    let mut materialized = false;
    if let Some(proxy) = state.live_proxy.clone().filter(|p| p.space_configured()) {
        let keys: Vec<String> = proxy
            .space_probe_versions(&target.url_ver)
            .into_iter()
            .map(|v| format!("{v}/{}", target.canonical))
            .collect();
        let (p2, dst, flat) = (
            proxy.clone(),
            exact.clone(),
            shader_flat_alias(state, &basename),
        );
        materialized = tokio::task::spawn_blocking(move || {
            for key in keys {
                if let Some(bytes) = p2.space_get_key(&key) {
                    return materialize_shader(&dst, flat.as_ref(), &bytes);
                }
            }
            false
        })
        .await
        .unwrap_or(false);
        record_space_read(Some("shader"), materialized);
    }
    if !materialized && state.shader_jit && crate::shader::vendored_sha(&basename).is_some() {
        let proxy = state.live_proxy.clone().filter(|p| p.space_configured());
        let (dst, flat) = (exact.clone(), shader_flat_alias(state, &basename));
        let put_key = format!("{}/{}", target.url_ver, target.canonical);
        let name = basename.clone();
        materialized = tokio::task::spawn_blocking(move || {
            match crate::shader::bundle_bytes_verified_named(&name) {
                Ok(bytes) => {
                    let ok = materialize_shader(&dst, flat.as_ref(), &bytes);
                    if ok {
                        if let Some(p) = proxy {
                            p.space_put_key(&put_key, &bytes, "application/octet-stream");
                        }
                    }
                    ok
                }
                Err(e) => {
                    tracing::warn!(
                        error = %format!("{e:#}"),
                        "vendored shader bundle unavailable — shader JIT lane cannot materialize"
                    );
                    false
                }
            }
        })
        .await
        .unwrap_or(false);
        let outcome = if materialized { "ok" } else { "error" };
        metrics::counter!("abgen_shader_jit_total", "outcome" => outcome).increment(1);
    }
    if materialized {
        state.resolve_cache.invalidate(&cache_key).await;
        state.resolve_cache.invalidate(path).await;
        let resp =
            serve::serve_binary(state, &cache_key, &exact, &basename, false, method, headers).await;
        if resp.status() != StatusCode::NOT_FOUND {
            return resp;
        }
    }
    with_reason(local, "shader-unavailable")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jit_build_sem_uses_configured_permit_count() {
        std::env::set_var("ABGEN_JIT_BUILD_CONCURRENCY", "3");
        let permits = jit_build_permits();
        std::env::remove_var("ABGEN_JIT_BUILD_CONCURRENCY");
        assert_eq!(permits, 3);
        assert_eq!(tokio::sync::Semaphore::new(permits).available_permits(), 3);
    }
}
