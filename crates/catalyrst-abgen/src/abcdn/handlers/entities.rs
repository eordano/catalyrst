use super::*;

const MAX_POINTERS: usize = 200;

fn parse_pointers(body: &serde_json::Value) -> Result<Vec<String>, (StatusCode, &'static str)> {
    let pointers: Vec<String> = body
        .get("pointers")
        .and_then(|p| p.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    if pointers.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            "pointers must be a non-empty array",
        ));
    }
    if pointers.len() > MAX_POINTERS {
        return Err((StatusCode::BAD_REQUEST, "too many pointers"));
    }
    Ok(pointers)
}

struct ResolvedEntity {
    entity_id: String,
    entity_type: String,
    timestamp: i64,
    pointers: Vec<String>,
    content: Vec<(String, String)>,
    metadata: serde_json::Value,
    deployer: String,
}

impl ResolvedEntity {
    fn from_scene(s: crate::catalyst::Scene, timestamp: i64) -> Self {
        Self {
            entity_id: s.entity_id,
            entity_type: s.entity_type,
            timestamp,
            pointers: s.pointers,
            content: s.content.into_iter().map(|c| (c.file, c.hash)).collect(),
            metadata: s.metadata,
            deployer: String::new(),
        }
    }

    fn from_active(e: dcl_contents::types::ActiveEntity) -> Self {
        Self {
            entity_id: e.entity_id,
            entity_type: e.entity_type,
            timestamp: e.timestamp,
            pointers: e.pointers,
            content: e.content.into_iter().map(|c| (c.file, c.hash)).collect(),
            metadata: e.metadata,
            deployer: e
                .deployer_address
                .map(|d| d.to_lowercase())
                .unwrap_or_default(),
        }
    }
}

fn feed_hash_index(state: &AppState, ents: &[ResolvedEntity]) {
    if let Some(proxy) = &state.live_proxy {
        proxy.index_content_hashes(ents.iter().flat_map(|e| {
            e.content
                .iter()
                .map(|(_, h)| (h.clone(), e.entity_id.clone()))
        }));
    }
}

async fn resolve_entities(state: &AppState, pointers: Vec<String>) -> Vec<ResolvedEntity> {
    #[cfg(feature = "content-db")]
    if let Some(cdb) = &state.content_db {
        let ents = match cdb.resolve_pointers(&pointers).await {
            Ok(ents) => ents
                .into_iter()
                .map(|e| ResolvedEntity {
                    entity_id: e.entity_id,
                    entity_type: e.entity_type,
                    timestamp: e.timestamp,
                    pointers: e.pointers,
                    content: e.content.into_iter().map(|c| (c.file, c.hash)).collect(),
                    metadata: e.metadata,
                    deployer: e
                        .deployer_address
                        .map(|d| d.to_lowercase())
                        .unwrap_or_default(),
                })
                .collect(),
            Err(e) => {
                tracing::warn!(error = %e, "folded index: content-db resolve_pointers failed");
                Vec::new()
            }
        };
        feed_hash_index(state, &ents);
        return ents;
    }

    if let Some(registry) = &state.contents_registry {
        let ents: Vec<ResolvedEntity> = match registry.content.resolve_pointers(&pointers).await {
            Ok(actives) => actives
                .into_iter()
                .map(ResolvedEntity::from_active)
                .collect(),
            Err(e) => {
                tracing::warn!(error = %e, "registry proxy resolve_pointers failed");
                Vec::new()
            }
        };
        feed_hash_index(state, &ents);
        return ents;
    }

    let st = state.clone();
    let ents: Vec<ResolvedEntity> = tokio::task::spawn_blocking(move || {
        pointers
            .iter()
            .filter_map(|p| {
                let s = st.content.resolve_scene(p).ok()?;
                Some(ResolvedEntity::from_scene(s, 0))
            })
            .collect()
    })
    .await
    .unwrap_or_default();
    feed_hash_index(state, &ents);
    ents
}

pub(super) fn valid_world_name(name: &str) -> bool {
    resolver::is_safe_component(name)
        && name.len() <= 253
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_'))
}

async fn resolve_world_entities(
    state: &AppState,
    name: &str,
    pointers: &[String],
) -> Vec<ResolvedEntity> {
    let Some(url) = state.worlds_content_url.clone() else {
        tracing::warn!(
            world = %name,
            "world_name given but the worlds content lane is disabled — falling back to pointer resolution"
        );
        return resolve_entities(state, pointers.to_vec()).await;
    };
    if !valid_world_name(name) {
        return Vec::new();
    }
    let name_q = name.to_string();
    let secs = crate::worlds::SERVE_FETCH_TIMEOUT_SECS;
    let fetched = tokio::task::spawn_blocking(move || -> anyhow::Result<Vec<serde_json::Value>> {
        let scenes = crate::worlds::resolve_world_bounded(&url, &name_q, secs)?;
        Ok(scenes
            .iter()
            .filter_map(|s| match crate::worlds::fetch_scene_entity(s, secs) {
                Ok(v) => Some(v),
                Err(e) => {
                    tracing::warn!(
                        entity = %s.entity_id,
                        error = %format!("{e:#}"),
                        "world scene entity fetch failed"
                    );
                    None
                }
            })
            .collect())
    })
    .await;
    let raw_entities = match fetched {
        Ok(Ok(v)) => v,
        Ok(Err(e)) => {
            tracing::warn!(world = %name, error = %format!("{e:#}"), "world resolution failed");
            return Vec::new();
        }
        Err(e) => {
            tracing::error!(error = %e, "world resolution worker panicked");
            return Vec::new();
        }
    };
    let wanted: std::collections::HashSet<String> =
        pointers.iter().map(|p| p.trim().to_lowercase()).collect();
    let mut out = Vec::new();
    for v in raw_entities {
        let Ok(scene) = crate::catalyst::CatalystClient::parse_entity(&v) else {
            continue;
        };
        if !scene
            .pointers
            .iter()
            .any(|p| wanted.contains(&p.trim().to_lowercase()))
        {
            continue;
        }
        let timestamp = v.get("timestamp").and_then(|t| t.as_i64()).unwrap_or(0);
        out.push(ResolvedEntity::from_scene(scene, timestamp));
    }
    feed_hash_index(state, &out);
    out
}

fn entity_buildable(content: &[(String, String)]) -> bool {
    content.iter().any(|(f, _)| super::index::is_convertible(f))
}

pub(super) struct PendingGuard(pub(super) std::sync::Arc<std::sync::atomic::AtomicUsize>);

impl Drop for PendingGuard {
    fn drop(&mut self) {
        self.0.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
    }
}

fn eager_build_index(state: &AppState, entities: &[ResolvedEntity]) {
    let ib = &state.index_build;
    if !ib.eager {
        return;
    }
    let Some(proxy) = state.live_proxy.clone() else {
        return;
    };
    let mut targets: Vec<(String, String)> = Vec::new();
    for e in entities {
        if !entity_buildable(&e.content) {
            continue;
        }
        for platform in &ib.platforms {
            let manifest = state
                .out_root
                .join(&e.entity_id)
                .join(format!("{platform}.manifest.json"));
            if !manifest.exists() {
                targets.push((e.entity_id.clone(), platform.clone()));
            }
        }
    }
    if targets.is_empty() {
        return;
    }

    let out = state.out_root.clone();
    let csu = state.manifest_content_server_url.clone();
    let sem = ib.sem.clone();
    let pending = ib.pending.clone();
    let max_queue = ib.max_queue;
    let deadline = ib.deadline;

    tokio::spawn(async move {
        let mut handles = Vec::new();
        for (ent, plat) in targets {
            if max_queue > 0 && pending.load(std::sync::atomic::Ordering::Relaxed) >= max_queue {
                metrics::counter!("abgen_index_jit_skipped_total").increment(1);
                continue;
            }
            pending.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let guard = PendingGuard(pending.clone());
            let sem = sem.clone();
            let px = proxy.clone();
            let out = out.clone();
            let csu = csu.clone();
            handles.push(tokio::spawn(async move {
                let _guard = guard;
                let Ok(_permit) = sem.acquire_owned().await else {
                    return;
                };
                let _ = timed_corpus_build(
                    px,
                    out,
                    ent,
                    plat,
                    csu,
                    "abgen_index_jit_builds_total",
                    "abgen_index_jit_build_duration_seconds",
                    "index",
                )
                .await;
            }));
        }
        let deadline = tokio::time::Instant::now() + deadline;
        for h in handles {
            if tokio::time::timeout_at(deadline, h).await.is_err() {
                break;
            }
        }
    });
}

pub async fn post_entities_versions(
    State(state): State<AppState>,
    Json(body): Json<serde_json::Value>,
) -> Response {
    let pointers = match parse_pointers(&body) {
        Ok(p) => p,
        Err(e) => return e.into_response(),
    };
    let ents = resolve_entities(&state, pointers).await;
    eager_build_index(&state, &ents);

    let st = state.clone();
    let out = tokio::task::spawn_blocking(move || {
        ents.into_iter()
            .filter_map(|e| {
                let (versions, bundles, status) = super::index::entity_ab_record(
                    &st.out_root,
                    &st.bundle_index,
                    &e.entity_id,
                    entity_buildable(&e.content),
                    &st.ab_version,
                    &st.ab_date,
                )?;
                Some(serde_json::json!({
                    "pointers": e.pointers,
                    "versions": versions,
                    "bundles": bundles,
                    "status": status,
                }))
            })
            .collect::<Vec<_>>()
    })
    .await
    .unwrap_or_default();

    Json(out).into_response()
}

pub async fn post_entities_active(
    State(state): State<AppState>,
    Query(query): Query<std::collections::HashMap<String, String>>,
    Json(body): Json<serde_json::Value>,
) -> Response {
    let pointers = match parse_pointers(&body) {
        Ok(p) => p,
        Err(e) => return e.into_response(),
    };
    let world = query
        .get("world_name")
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let ents = match world {
        Some(name) => resolve_world_entities(&state, &name, &pointers).await,
        None => resolve_entities(&state, pointers).await,
    };
    eager_build_index(&state, &ents);
    entities_active_records(&state, ents).await
}

async fn entities_active_records(state: &AppState, ents: Vec<ResolvedEntity>) -> Response {
    let st = state.clone();
    let out = tokio::task::spawn_blocking(move || {
        ents.into_iter()
            .filter_map(|e| {
                let (versions, bundles, status) = super::index::entity_ab_record(
                    &st.out_root,
                    &st.bundle_index,
                    &e.entity_id,
                    entity_buildable(&e.content),
                    &st.ab_version,
                    &st.ab_date,
                )?;
                let content: Vec<serde_json::Value> = e
                    .content
                    .iter()
                    .map(|(f, h)| serde_json::json!({ "file": f, "hash": h }))
                    .collect();
                Some(serde_json::json!({
                    "id": e.entity_id,
                    "type": e.entity_type,
                    "timestamp": e.timestamp,
                    "pointers": e.pointers,
                    "content": content,
                    "metadata": e.metadata,
                    "deployer": e.deployer,
                    "status": status,
                    "bundles": bundles,
                    "versions": versions,
                }))
            })
            .collect::<Vec<_>>()
    })
    .await
    .unwrap_or_default();

    Json(out).into_response()
}
