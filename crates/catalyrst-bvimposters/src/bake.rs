use std::collections::VecDeque;
use std::path::Path;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use tokio::sync::Notify;

use crate::key::TileKey;
use crate::quarantine::Quarantine;
use crate::store::Store;

#[derive(Clone)]
pub struct BakeConfig {
    pub wrapper: String,
    pub bin: String,
    pub server: String,
    pub content_server: String,
    pub timeout: Duration,
    pub queue_depth: usize,
}

pub struct BakeQueue {
    cfg: BakeConfig,
    store: Arc<Store>,
    quarantine: Arc<Quarantine>,
    queue: Mutex<VecDeque<TileKey>>,
    inflight: Mutex<Option<TileKey>>,
    notify: Notify,
}

impl BakeQueue {
    pub fn new(cfg: BakeConfig, store: Arc<Store>, quarantine: Arc<Quarantine>) -> Arc<Self> {
        Arc::new(Self {
            cfg,
            store,
            quarantine,
            queue: Mutex::new(VecDeque::new()),
            inflight: Mutex::new(None),
            notify: Notify::new(),
        })
    }

    pub fn spawn_worker(self: &Arc<Self>) {
        let this = self.clone();
        tokio::spawn(async move { this.worker().await });
    }

    pub fn enqueue(&self, tile: TileKey) -> bool {
        let mut queue = self.queue.lock().unwrap();
        if queue.contains(&tile) || *self.inflight.lock().unwrap() == Some(tile) {
            return false;
        }
        if queue.len() >= self.cfg.queue_depth {
            return false;
        }
        queue.push_back(tile);
        drop(queue);
        self.notify.notify_one();
        true
    }

    pub fn snapshot(&self) -> (Vec<TileKey>, Option<TileKey>) {
        let queue = self.queue.lock().unwrap().iter().copied().collect();
        let inflight = *self.inflight.lock().unwrap();
        (queue, inflight)
    }

    async fn worker(self: Arc<Self>) {
        loop {
            let next = {
                let mut queue = self.queue.lock().unwrap();
                let next = queue.pop_front();
                *self.inflight.lock().unwrap() = next;
                next
            };
            let Some(tile) = next else {
                self.notify.notified().await;
                continue;
            };
            match run_bake(&self.cfg, self.store.clone(), tile).await {
                Ok(count) => {
                    tracing::info!(tile = %tile.label(), count, "bake harvested");
                    self.quarantine.record_success(&tile);
                }
                Err(e) => {
                    tracing::warn!(tile = %tile.label(), error = %e, "bake failed");
                    self.quarantine.record_failure(&tile);
                }
            }
            *self.inflight.lock().unwrap() = None;
        }
    }
}

pub fn bake_args(tile: TileKey, cfg: &BakeConfig, staging: &Path) -> Vec<String> {
    let extent = 1i32 << tile.level;
    let half = extent / 2;
    let mut args = vec!["--server".to_string(), cfg.server.clone()];
    if !cfg.content_server.is_empty() {
        args.push("--content-server".to_string());
        args.push(cfg.content_server.clone());
    }
    args.extend([
        "--location".to_string(),
        format!("{},{}", tile.x + half, tile.y + half),
        "--range".to_string(),
        half.max(1).to_string(),
        "--levels".to_string(),
        tile.level.to_string(),
        "--threads".to_string(),
        "4".to_string(),
        "--no-download".to_string(),
        "--zip-output".to_string(),
        staging.display().to_string(),
    ]);
    args
}

pub async fn run_bake(cfg: &BakeConfig, store: Arc<Store>, tile: TileKey) -> Result<usize> {
    let job_id = uuid::Uuid::new_v4().to_string();
    let staging = store.staging_root().join(&job_id);
    tokio::fs::create_dir_all(&staging)
        .await
        .with_context(|| format!("creating {}", staging.display()))?;

    let mut argv: Vec<String> = cfg.wrapper.split_whitespace().map(String::from).collect();
    argv.push(cfg.bin.clone());
    argv.extend(bake_args(tile, cfg, &staging));
    let program = argv.remove(0);

    tracing::info!(tile = %tile.label(), %program, args = ?argv, "bake starting");
    let spawned = tokio::process::Command::new(&program)
        .args(&argv)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .kill_on_drop(true)
        .spawn();
    let mut child = match spawned {
        Ok(child) => child,
        Err(e) => {
            let _ = tokio::fs::remove_dir_all(&staging).await;
            return Err(e).with_context(|| format!("spawning {program}"));
        }
    };
    let pid = child.id();

    let status = match tokio::time::timeout(cfg.timeout, child.wait()).await {
        Ok(waited) => waited.context("waiting for bake process")?,
        Err(_) => {
            if let Some(pid) = pid {
                unsafe {
                    libc::killpg(pid as i32, libc::SIGKILL);
                }
            }
            let _ = child.wait().await;
            let _ = tokio::fs::remove_dir_all(&staging).await;
            return Err(anyhow!("bake timed out after {:?}", cfg.timeout));
        }
    };
    if !status.success() {
        let _ = tokio::fs::remove_dir_all(&staging).await;
        return Err(anyhow!("bake exited with {status}"));
    }

    let harvest_store = store.clone();
    let harvest_staging = staging.clone();
    let harvested =
        tokio::task::spawn_blocking(move || harvest(&harvest_store, &harvest_staging, tile.level))
            .await
            .context("harvest task")??;
    let _ = tokio::fs::remove_dir_all(&staging).await;
    if harvested == 0 {
        return Err(anyhow!("bake produced no harvestable zips"));
    }

    let evict_store = store.clone();
    let _ = tokio::task::spawn_blocking(move || evict_store.evict_pass()).await;
    Ok(harvested)
}

fn harvest(store: &Store, staging: &Path, max_level: u8) -> Result<usize> {
    let mut count = 0;
    let realms_root = staging.join("imposters").join("realms");
    let Ok(realms) = std::fs::read_dir(&realms_root) else {
        return Ok(0);
    };
    for realm in realms.flatten() {
        if !realm.path().is_dir() {
            continue;
        }
        for level in 0..=max_level {
            let level_dir = realm.path().join(level.to_string());
            let Ok(rd) = std::fs::read_dir(&level_dir) else {
                continue;
            };
            for entry in rd.flatten() {
                let name = entry.file_name().to_string_lossy().into_owned();
                let Some(key) = crate::key::parse_zip_request(&level.to_string(), &name) else {
                    continue;
                };
                let Ok(bytes) = std::fs::read(entry.path()) else {
                    continue;
                };
                if let Err(e) = crate::zips::verify_zip(&bytes, &key) {
                    tracing::warn!(file = %name, error = %e, "harvest zip failed verification");
                    continue;
                }
                std::fs::create_dir_all(store.level_dir(key.tile.level))?;
                std::fs::rename(entry.path(), store.zip_path(&key))?;
                count += 1;
            }
        }
    }
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn test_cfg(bin: &str) -> BakeConfig {
        BakeConfig {
            wrapper: String::new(),
            bin: bin.to_string(),
            server: "https://catalyst.example".to_string(),
            content_server: "http://localhost:5141".to_string(),
            timeout: Duration::from_secs(30),
            queue_depth: 1,
        }
    }

    fn write_script(dir: &Path, name: &str, body: &str) -> String {
        let path = dir.join(name);
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path.display().to_string()
    }

    #[test]
    fn args_for_level_zero() {
        let cfg = test_cfg("/bin/impost");
        let tile = TileKey::new(0, 7, -3).unwrap();
        let args = bake_args(tile, &cfg, Path::new("/s/j1"));
        assert_eq!(
            args,
            vec![
                "--server",
                "https://catalyst.example",
                "--content-server",
                "http://localhost:5141",
                "--location",
                "7,-3",
                "--range",
                "1",
                "--levels",
                "0",
                "--threads",
                "4",
                "--no-download",
                "--zip-output",
                "/s/j1"
            ]
        );
    }

    #[test]
    fn empty_content_server_is_omitted() {
        let mut cfg = test_cfg("/bin/impost");
        cfg.content_server = String::new();
        let tile = TileKey::new(0, 7, -3).unwrap();
        let args = bake_args(tile, &cfg, Path::new("/s/j1"));
        assert!(!args.contains(&"--content-server".to_string()));
        assert_eq!(args[2], "--location");
    }

    #[test]
    fn args_for_level_two() {
        let cfg = test_cfg("/bin/impost");
        let tile = TileKey::new(2, -64, -128).unwrap();
        let args = bake_args(tile, &cfg, Path::new("/s/j2"));
        assert_eq!(args[5], "-62,-126");
        assert_eq!(args[7], "2");
        assert_eq!(args[9], "2");
    }

    #[tokio::test]
    async fn fake_bake_harvests_all_levels() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::new(dir.path().join("root"), u64::MAX));
        store.init().unwrap();
        let fixtures = dir.path().join("fixtures");
        std::fs::create_dir_all(&fixtures).unwrap();
        std::fs::write(
            fixtures.join("2,4.777.zip"),
            crate::zips::test_zip_bytes(2, 4, 777),
        )
        .unwrap();
        std::fs::write(
            fixtures.join("2,4.888.zip"),
            crate::zips::test_zip_bytes(2, 4, 888),
        )
        .unwrap();
        std::fs::write(
            fixtures.join("3,5.999.zip"),
            crate::zips::test_zip_bytes(3, 5, 999),
        )
        .unwrap();
        let body = format!(
            "out=\"\"\nwhile [ $# -gt 0 ]; do\n  if [ \"$1\" = \"--zip-output\" ]; then out=\"$2\"; fi\n  shift\ndone\nmkdir -p \"$out/imposters/realms/enc/0\" \"$out/imposters/realms/enc/1\"\ncp '{f}/2,4.777.zip' \"$out/imposters/realms/enc/1/\"\ncp '{f}/2,4.888.zip' \"$out/imposters/realms/enc/0/\"\ncp '{f}/3,5.999.zip' \"$out/imposters/realms/enc/0/\"",
            f = fixtures.display()
        );
        let script = write_script(dir.path(), "fake-impost.sh", &body);

        let cfg = test_cfg(&script);
        let tile = TileKey::new(1, 2, 4).unwrap();
        let harvested = run_bake(&cfg, store.clone(), tile).await.unwrap();
        assert_eq!(harvested, 3);
        assert!(store.level_dir(1).join("2,4.777.zip").exists());
        assert!(store.level_dir(0).join("2,4.888.zip").exists());
        assert!(store.level_dir(0).join("3,5.999.zip").exists());
        assert_eq!(std::fs::read_dir(store.staging_root()).unwrap().count(), 0);
    }

    #[tokio::test]
    async fn failing_bin_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::new(dir.path().join("root"), u64::MAX));
        store.init().unwrap();
        let script = write_script(dir.path(), "fail.sh", "exit 1");
        let cfg = test_cfg(&script);
        let tile = TileKey::new(0, 0, 0).unwrap();
        assert!(run_bake(&cfg, store.clone(), tile).await.is_err());
        assert_eq!(std::fs::read_dir(store.staging_root()).unwrap().count(), 0);
    }

    #[tokio::test]
    async fn missing_bin_is_an_error_and_cleans_staging() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::new(dir.path().join("root"), u64::MAX));
        store.init().unwrap();
        let cfg = test_cfg(&dir.path().join("no-such-bin").display().to_string());
        let tile = TileKey::new(0, 0, 0).unwrap();
        assert!(run_bake(&cfg, store.clone(), tile).await.is_err());
        assert_eq!(std::fs::read_dir(store.staging_root()).unwrap().count(), 0);
    }

    #[tokio::test]
    async fn timeout_kills_the_bake_and_cleans_staging() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::new(dir.path().join("root"), u64::MAX));
        store.init().unwrap();
        let script = write_script(dir.path(), "hang.sh", "sleep 30 &\nsleep 30");
        let mut cfg = test_cfg(&script);
        cfg.timeout = Duration::from_millis(200);
        let tile = TileKey::new(0, 0, 0).unwrap();
        let err = run_bake(&cfg, store.clone(), tile).await.unwrap_err();
        assert!(err.to_string().contains("timed out"));
        assert_eq!(std::fs::read_dir(store.staging_root()).unwrap().count(), 0);
    }

    #[tokio::test]
    async fn empty_harvest_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::new(dir.path().join("root"), u64::MAX));
        store.init().unwrap();
        let script = write_script(dir.path(), "noop.sh", "exit 0");
        let cfg = test_cfg(&script);
        let tile = TileKey::new(0, 0, 0).unwrap();
        assert!(run_bake(&cfg, store.clone(), tile).await.is_err());
        assert_eq!(std::fs::read_dir(store.staging_root()).unwrap().count(), 0);
    }

    #[tokio::test]
    async fn queue_coalesces_and_bounds() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::new(dir.path().join("root"), u64::MAX));
        store.init().unwrap();
        let quarantine = Arc::new(Quarantine::load(store.quarantine_path(), 3, 86400));
        let queue = BakeQueue::new(test_cfg("/bin/true"), store, quarantine);
        let a = TileKey::new(0, 0, 0).unwrap();
        let b = TileKey::new(0, 1, 0).unwrap();
        assert!(queue.enqueue(a));
        assert!(!queue.enqueue(a));
        assert!(!queue.enqueue(b));
        let (pending, inflight) = queue.snapshot();
        assert_eq!(pending, vec![a]);
        assert_eq!(inflight, None);
    }
}
