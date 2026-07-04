use crate::builder::{build_bundle, BuildOpts};
use crate::catalyst::CatalystClient;
use crate::compress;
use crate::naming;
use anyhow::{anyhow, bail, Context, Result};
use rayon::prelude::*;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

pub const DEFAULT_CONTENT_ENV: &str = "./content.env";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntityKind {
    Scene,
    Wearable,
    Emote,
    All,
}

impl EntityKind {
    pub fn parse(s: &str) -> Result<EntityKind> {
        Ok(match s.to_lowercase().as_str() {
            "scene" => EntityKind::Scene,
            "wearable" => EntityKind::Wearable,
            "emote" => EntityKind::Emote,
            "all" => EntityKind::All,
            other => bail!("unknown --entity-type {other:?} (scene|wearable|emote|all)"),
        })
    }

    const fn db_types(self) -> &'static [&'static str] {
        match self {
            EntityKind::Scene => &["scene"],
            EntityKind::Wearable => &["wearable"],
            EntityKind::Emote => &["emote"],
            EntityKind::All => &["scene", "wearable", "emote"],
        }
    }
}

#[derive(Clone, Debug)]
pub struct RegenConfig {
    pub output_dir: String,
    pub catalyst: String,
    pub platform: String,
    pub ab_version: String,
    pub jobs: usize,
    pub limit: Option<usize>,
    pub offset: usize,
    pub entity_kind: EntityKind,
    pub dry_run: bool,
    pub compress: bool,
    pub keep_forward_plus: bool,
    pub content_env: String,
    pub progress_every: usize,
    pub local: Option<String>,

    pub force: bool,

    pub magenta_missing: bool,
}

impl Default for RegenConfig {
    fn default() -> Self {
        RegenConfig {
            output_dir: "./out".to_string(),
            catalyst: crate::catalyst::DEFAULT_CATALYST.to_string(),
            platform: "windows".to_string(),
            ab_version: crate::manifest::DEFAULT_AB_VERSION.to_string(),
            jobs: num_cpus::get(),
            limit: None,
            offset: 0,
            entity_kind: EntityKind::All,
            dry_run: false,
            compress: true,
            keep_forward_plus: true,
            content_env: DEFAULT_CONTENT_ENV.to_string(),
            progress_every: 25,
            local: None,
            force: false,
            magenta_missing: false,
        }
    }
}

#[derive(Clone, Debug)]
pub struct EntityRow {
    pub entity_id: String,

    pub entity_type: String,

    pub content_by_file: HashMap<String, String>,

    pub glbs: Vec<(String, String)>,
}

fn parse_env_file(path: &str) -> Result<HashMap<String, String>> {
    let text = std::fs::read_to_string(path).with_context(|| format!("reading env file {path}"))?;
    let mut map = HashMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            let v = v.trim().trim_matches('"').trim_matches('\'');
            map.insert(k.trim().to_string(), v.to_string());
        }
    }
    Ok(map)
}

fn connection_string(env: &HashMap<String, String>) -> Result<String> {
    let user = env
        .get("POSTGRES_CONTENT_USER")
        .ok_or_else(|| anyhow!("POSTGRES_CONTENT_USER missing from env file"))?;
    let password = env
        .get("POSTGRES_CONTENT_PASSWORD")
        .ok_or_else(|| anyhow!("POSTGRES_CONTENT_PASSWORD missing from env file"))?;
    let db = env
        .get("POSTGRES_CONTENT_DB")
        .map(String::as_str)
        .unwrap_or("content");
    let host = env
        .get("POSTGRES_HOST")
        .map(String::as_str)
        .unwrap_or("127.0.0.1");
    let port = env
        .get("POSTGRES_PORT")
        .map(String::as_str)
        .unwrap_or("5433");

    let esc = |s: &str| s.replace('\\', "\\\\").replace('\'', "\\'");
    Ok(format!(
        "host='{}' port={} user='{}' password='{}' dbname='{}' connect_timeout=30",
        esc(host),
        port,
        esc(user),
        esc(password),
        esc(db),
    ))
}

pub fn enumerate_entities(cfg: &RegenConfig) -> Result<Vec<EntityRow>> {
    use postgres::{Client, NoTls};

    let env = parse_env_file(&cfg.content_env)?;
    let conn = connection_string(&env)?;
    let mut client = Client::connect(&conn, NoTls).context("connecting to content postgres DB")?;

    let types: Vec<&str> = cfg.entity_kind.db_types().to_vec();

    let limit_sql = match cfg.limit {
        Some(n) => format!("LIMIT {n}"),
        None => "LIMIT ALL".to_string(),
    };
    let query = format!(
        "WITH ents AS (
            SELECT id, entity_id, entity_type
            FROM deployments
            WHERE deleter_deployment IS NULL
              AND entity_type = ANY($1)
            ORDER BY id
            {limit_sql} OFFSET {offset}
         )
         SELECT e.entity_id, e.entity_type, cf.key, cf.content_hash
         FROM ents e
         JOIN content_files cf ON cf.deployment = e.id
         ORDER BY e.id, cf.key",
        offset = cfg.offset,
    );

    let mut order: Vec<String> = Vec::new();
    let mut by_entity: HashMap<String, EntityRow> = HashMap::new();

    let rows = client
        .query(query.as_str(), &[&types])
        .context("querying active entities + content files")?;
    for row in rows {
        let entity_id: String = row.get(0);
        let entity_type: String = row.get(1);
        let key: String = row.get(2);
        let hash: String = row.get(3);
        let row_entry = by_entity.entry(entity_id.clone()).or_insert_with(|| {
            order.push(entity_id.clone());
            EntityRow {
                entity_id: entity_id.clone(),
                entity_type: entity_type.clone(),
                content_by_file: HashMap::new(),
                glbs: Vec::new(),
            }
        });
        row_entry
            .content_by_file
            .insert(key.to_lowercase(), hash.clone());
        let ext = naming::file_extension(&key);
        if naming::GLTF_EXTENSIONS.contains(&ext.as_str()) {
            row_entry.glbs.push((key, hash));
        }
    }

    Ok(order
        .into_iter()
        .filter_map(|id| by_entity.remove(&id))
        .collect())
}

#[derive(Debug, Default)]
pub struct RegenReport {
    pub entities: usize,
    pub glb_refs: usize,
    pub unique_assets: usize,
    pub converted: usize,
    pub already_present: usize,
    pub failed: usize,
    pub output_bytes: u64,
    pub elapsed_secs: f64,
    pub failures: Vec<(String, String)>,
}

impl RegenReport {
    pub fn dedup_ratio(&self) -> f64 {
        if self.unique_assets == 0 {
            0.0
        } else {
            self.glb_refs as f64 / self.unique_assets as f64
        }
    }
}

fn assets_dir(cfg: &RegenConfig) -> PathBuf {
    PathBuf::from(&cfg.output_dir)
        .join(&cfg.ab_version)
        .join("assets")
}

fn manifest_path(cfg: &RegenConfig, entity_id: &str) -> PathBuf {
    let dir = PathBuf::from(&cfg.output_dir).join("manifest");
    let fname = if cfg.platform.is_empty() || cfg.platform == "webgl" {
        format!("{entity_id}.json")
    } else {
        format!("{entity_id}_{}.json", cfg.platform)
    };
    dir.join(fname)
}

pub fn guard<T>(f: impl FnOnce() -> Result<T>) -> Result<T> {
    use std::panic::{catch_unwind, AssertUnwindSafe};
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(r) => r,
        Err(payload) => {
            let msg = payload
                .downcast_ref::<&str>()
                .map(|s| s.to_string())
                .or_else(|| payload.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "panic in build_bundle".to_string());
            Err(anyhow!("panic: {msg}"))
        }
    }
}

fn plan_asset(
    cli: &CatalystClient,
    cfg: &RegenConfig,
    glb_file: &str,
    glb_hash: &str,
    content_by_file: &HashMap<String, String>,
) -> Result<(String, Vec<u8>)> {
    let ext = naming::file_extension(glb_file);
    let glb = cli.fetch_content(glb_hash)?;
    let digest = naming::deps_digest_for_glb(&glb, glb_file, content_by_file, cfg.magenta_missing)?;
    let name = naming::canonical_filename(glb_hash, &ext, &cfg.platform, Some(&digest))?;
    Ok((name, glb))
}

fn convert_and_write(
    cli: &CatalystClient,
    cfg: &RegenConfig,
    glb_file: &str,
    glb_hash: &str,
    canonical_name: &str,
    glb_bytes: &[u8],
    content_by_file: &HashMap<String, String>,
    entity_type: &str,
) -> Result<u64> {
    let glb_file_owned = glb_file.to_string();
    let content_clone = content_by_file.clone();
    let resolve_fn = move |uri: &str| -> Option<Vec<u8>> {
        let key = naming::resolve_uri_to_content_file(uri, &glb_file_owned)
            .ok()?
            .to_lowercase();
        let content_hash = content_clone.get(&key)?;
        cli.fetch_content(content_hash).ok()
    };
    let resolve: crate::gltf::Resolve = Some(&resolve_fn);

    let data = guard(|| {
        let opts = BuildOpts {
            keep_forward_plus: cfg.keep_forward_plus,
            source_file: Some(glb_file),
            entity_type: Some(entity_type),
            resolve,
            magenta_missing: cfg.magenta_missing,
            ..Default::default()
        };
        build_bundle(glb_bytes, canonical_name, glb_hash, &opts).map(|a| a.data)
    })?;

    let dir = assets_dir(cfg);
    std::fs::create_dir_all(&dir)?;

    let final_path = dir.join(canonical_name);
    let tmp_path = dir.join(format!(".{canonical_name}.tmp{}", std::process::id()));
    std::fs::write(&tmp_path, &data)?;
    std::fs::rename(&tmp_path, &final_path)?;

    if cfg.compress {
        let mut br = final_path.into_os_string();
        br.push(".br");
        let compressed = compress::brotli(&data)?;
        std::fs::write(PathBuf::from(br), compressed)?;
    }
    Ok(data.len() as u64)
}

fn write_entity_manifest(
    cfg: &RegenConfig,
    entity_id: &str,
    canonical_names: &[String],
) -> Result<()> {
    let mut files: Vec<String> = canonical_names.to_vec();
    files.sort();
    files.dedup();
    let manifest = serde_json::json!({
        "version": cfg.ab_version,
        "files": files,
        "exitCode": 0,
        "contentServerUrl": cli_base(cfg),
        "date": crate::live::build_scoped_date(),
    });
    let path = manifest_path(cfg, entity_id);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let text = serde_json::to_string_pretty(&manifest)?;
    std::fs::write(&path, &text)?;
    if cfg.compress {
        let mut br = path.into_os_string();
        br.push(".br");
        std::fs::write(PathBuf::from(br), compress::brotli(text.as_bytes())?)?;
    }
    Ok(())
}

fn cli_base(cfg: &RegenConfig) -> String {
    cfg.catalyst.trim_end_matches('/').to_string()
}

pub fn regenerate(cfg: &RegenConfig) -> Result<RegenReport> {
    let start = Instant::now();
    eprintln!(
        "enumerating {:?} entities from content DB (offset {}, limit {:?}) …",
        cfg.entity_kind, cfg.offset, cfg.limit
    );
    let entities = enumerate_entities(cfg)?;
    let glb_refs: usize = entities.iter().map(|e| e.glbs.len()).sum();
    eprintln!(
        "{} entities, {} glb/gltf references",
        entities.len(),
        glb_refs
    );

    let assets = assets_dir(cfg);

    if cfg.dry_run {
        return dry_run(cfg, &entities, glb_refs, start);
    }

    std::panic::set_hook(Box::new(|_| {}));

    rayon::ThreadPoolBuilder::new()
        .num_threads(cfg.jobs.max(1))
        .build_global()
        .ok();

    let cli = CatalystClient::from_args(&cfg.catalyst, cfg.local.as_deref());

    let converted = AtomicUsize::new(0);
    let already = AtomicUsize::new(0);
    let failed = AtomicUsize::new(0);
    let bytes = AtomicU64::new(0);
    let processed = AtomicUsize::new(0);
    let failures: Mutex<Vec<(String, String)>> = Mutex::new(Vec::new());

    let claimed: Mutex<HashSet<String>> = Mutex::new(HashSet::new());

    let manifest_results: Vec<()> = entities
        .par_iter()
        .map(|ent| {
            let mut entity_names: Vec<String> = Vec::new();
            for (glb_file, glb_hash) in &ent.glbs {
                let plan = plan_asset(&cli, cfg, glb_file, glb_hash, &ent.content_by_file);
                let (name, glb_bytes) = match plan {
                    Ok(v) => v,
                    Err(e) => {
                        failed.fetch_add(1, Ordering::Relaxed);
                        failures
                            .lock()
                            .unwrap()
                            .push((format!("{}::{}", ent.entity_id, glb_file), format!("{e:#}")));
                        continue;
                    }
                };
                entity_names.push(name.clone());

                let final_path = assets.join(&name);
                if !cfg.force && final_path.exists() {
                    already.fetch_add(1, Ordering::Relaxed);
                    continue;
                }
                {
                    let mut c = claimed.lock().unwrap();
                    if c.contains(&name) {
                        already.fetch_add(1, Ordering::Relaxed);
                        continue;
                    }
                    c.insert(name.clone());
                }

                match convert_and_write(
                    &cli,
                    cfg,
                    glb_file,
                    glb_hash,
                    &name,
                    &glb_bytes,
                    &ent.content_by_file,
                    &ent.entity_type,
                ) {
                    Ok(n) => {
                        converted.fetch_add(1, Ordering::Relaxed);
                        bytes.fetch_add(n, Ordering::Relaxed);
                    }
                    Err(e) => {
                        failed.fetch_add(1, Ordering::Relaxed);

                        claimed.lock().unwrap().remove(&name);
                        failures
                            .lock()
                            .unwrap()
                            .push((format!("{}::{}", ent.entity_id, glb_file), format!("{e:#}")));
                    }
                }
            }

            if !entity_names.is_empty() {
                if let Err(e) = write_entity_manifest(cfg, &ent.entity_id, &entity_names) {
                    failures
                        .lock()
                        .unwrap()
                        .push((format!("manifest::{}", ent.entity_id), format!("{e:#}")));
                }
            }

            let done = processed.fetch_add(1, Ordering::Relaxed) + 1;
            if cfg.progress_every > 0 && done.is_multiple_of(cfg.progress_every) {
                let conv = converted.load(Ordering::Relaxed);
                let skip = already.load(Ordering::Relaxed);
                let fail = failed.load(Ordering::Relaxed);
                let secs = start.elapsed().as_secs_f64().max(1e-6);
                eprintln!(
                    "  [{done}/{}] converted={conv} present={skip} failed={fail}  {:.1} ent/s",
                    entities.len(),
                    done as f64 / secs
                );
            }
        })
        .collect();
    let _ = manifest_results;

    let report = RegenReport {
        entities: entities.len(),
        glb_refs,
        unique_assets: converted.load(Ordering::Relaxed) + already.load(Ordering::Relaxed),
        converted: converted.load(Ordering::Relaxed),
        already_present: already.load(Ordering::Relaxed),
        failed: failed.load(Ordering::Relaxed),
        output_bytes: bytes.load(Ordering::Relaxed),
        elapsed_secs: start.elapsed().as_secs_f64(),
        failures: failures.into_inner().unwrap(),
    };
    Ok(report)
}

fn dry_run(
    cfg: &RegenConfig,
    entities: &[EntityRow],
    glb_refs: usize,
    start: Instant,
) -> Result<RegenReport> {
    rayon::ThreadPoolBuilder::new()
        .num_threads(cfg.jobs.max(1))
        .build_global()
        .ok();
    let cli = CatalystClient::from_args(&cfg.catalyst, cfg.local.as_deref());
    let assets = assets_dir(cfg);

    let unique: Mutex<HashSet<String>> = Mutex::new(HashSet::new());
    let on_disk = AtomicUsize::new(0);
    let failed = AtomicUsize::new(0);

    let glb_byte_total = AtomicU64::new(0);
    let glb_byte_count = AtomicUsize::new(0);
    let failures: Mutex<Vec<(String, String)>> = Mutex::new(Vec::new());

    entities.par_iter().for_each(|ent| {
        for (glb_file, glb_hash) in &ent.glbs {
            match plan_asset(&cli, cfg, glb_file, glb_hash, &ent.content_by_file) {
                Ok((name, glb)) => {
                    let mut u = unique.lock().unwrap();
                    if u.insert(name.clone()) {
                        if !cfg.force && assets.join(&name).exists() {
                            on_disk.fetch_add(1, Ordering::Relaxed);
                        }
                        glb_byte_total.fetch_add(glb.len() as u64, Ordering::Relaxed);
                        glb_byte_count.fetch_add(1, Ordering::Relaxed);
                    }
                }
                Err(e) => {
                    failed.fetch_add(1, Ordering::Relaxed);
                    failures
                        .lock()
                        .unwrap()
                        .push((format!("{}::{}", ent.entity_id, glb_file), format!("{e:#}")));
                }
            }
        }
    });

    let unique_assets = unique.into_inner().unwrap().len();
    let already = on_disk.load(Ordering::Relaxed);

    let glb_total = glb_byte_total.load(Ordering::Relaxed);
    let est_output = (glb_total as f64 * 1.2) as u64;

    let report = RegenReport {
        entities: entities.len(),
        glb_refs,
        unique_assets,
        converted: unique_assets.saturating_sub(already),
        already_present: already,
        failed: failed.load(Ordering::Relaxed),
        output_bytes: est_output,
        elapsed_secs: start.elapsed().as_secs_f64(),
        failures: failures.into_inner().unwrap(),
    };
    Ok(report)
}

#[allow(dead_code)]
fn iso8601_utc_now() -> String {
    let dur = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let total_secs = dur.as_secs();
    let millis = dur.subsec_millis();
    let days = (total_secs / 86_400) as i64;
    let secs_of_day = (total_secs % 86_400) as i64;
    let hour = secs_of_day / 3600;
    let minute = (secs_of_day % 3600) / 60;
    let second = secs_of_day % 60;
    let (year, month, day) = civil_from_days(days);
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.{millis:03}Z")
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entity_kind_parse() {
        assert_eq!(EntityKind::parse("scene").unwrap(), EntityKind::Scene);
        assert_eq!(EntityKind::parse("ALL").unwrap(), EntityKind::All);
        assert!(EntityKind::parse("profile").is_err());
        assert_eq!(EntityKind::All.db_types().len(), 3);
        assert_eq!(EntityKind::Emote.db_types(), &["emote"]);
    }

    #[test]
    fn parse_env_basic() {
        let tmp = std::env::temp_dir().join(format!("abgen_env_test_{}", std::process::id()));
        std::fs::write(
            &tmp,
            "# comment\nPOSTGRES_CONTENT_USER=cs_abc\nPOSTGRES_CONTENT_PASSWORD=\"sec\"\nPOSTGRES_PORT=5433\n\nNO_EQUALS_LINE\n",
        )
        .unwrap();
        let env = parse_env_file(tmp.to_str().unwrap()).unwrap();
        assert_eq!(env.get("POSTGRES_CONTENT_USER").unwrap(), "cs_abc");
        assert_eq!(env.get("POSTGRES_CONTENT_PASSWORD").unwrap(), "sec");
        assert!(!env.contains_key("NO_EQUALS_LINE"));
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn connstr_uses_socket_host() {
        let mut env = HashMap::new();
        env.insert("POSTGRES_CONTENT_USER".into(), "u".into());
        env.insert("POSTGRES_CONTENT_PASSWORD".into(), "p'q".into());
        env.insert("POSTGRES_HOST".into(), "/var/run/pg".into());
        env.insert("POSTGRES_PORT".into(), "5433".into());
        let s = connection_string(&env).unwrap();
        assert!(s.contains("host='/var/run/pg'"));
        assert!(s.contains("port=5433"));
        assert!(s.contains("dbname='content'"));

        assert!(s.contains("password='p\\'q'"));
    }

    #[test]
    fn manifest_path_webgl_vs_target() {
        let mut cfg = RegenConfig {
            output_dir: "/tmp/x".into(),
            platform: "windows".into(),
            ..RegenConfig::default()
        };
        assert!(manifest_path(&cfg, "Qm123")
            .to_string_lossy()
            .ends_with("manifest/Qm123_windows.json"));
        cfg.platform = "webgl".into();
        assert!(manifest_path(&cfg, "Qm123")
            .to_string_lossy()
            .ends_with("manifest/Qm123.json"));
    }

    #[test]
    fn iso8601_shape() {
        let s = iso8601_utc_now();
        assert!(s.ends_with('Z'));
        assert_eq!(s.len(), "2026-05-21T12:00:00.000Z".len());
        assert_eq!(&s[10..11], "T");
    }

    #[test]
    fn dedup_ratio_calc() {
        let r = RegenReport {
            glb_refs: 10,
            unique_assets: 4,
            ..RegenReport::default()
        };
        assert!((r.dedup_ratio() - 2.5).abs() < 1e-9);
    }
}
