use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// Every writer under `contents_dir` stages bytes as `.<name>.part` before renaming onto the
/// content-addressed destination, so a surviving `.part` is by definition unreachable.
const TEMP_SUFFIX: &str = ".part";

pub const REAP_INTERVAL: Duration = Duration::from_secs(300);

const MIN_REAP_GRACE: Duration = Duration::from_secs(60);

pub fn is_temp_name(name: &str) -> bool {
    name.len() > TEMP_SUFFIX.len() + 1 && name.starts_with('.') && name.ends_with(TEMP_SUFFIX)
}

/// A temp file only exists between the staging write and the rename, both of which sit inside the
/// deployment deadlines; anything older than both deadlines together cannot belong to a live request.
pub fn reap_grace(
    multipart_upload_timeout_ms: u64,
    deployment_processing_timeout_ms: u64,
) -> Duration {
    Duration::from_millis(
        multipart_upload_timeout_ms.saturating_add(deployment_processing_timeout_ms),
    )
    .max(MIN_REAP_GRACE)
}

pub async fn reap_stale_temp_files(dir: &Path, grace: Duration) -> std::io::Result<usize> {
    let now = SystemTime::now();
    let mut entries = match tokio::fs::read_dir(dir).await {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(e) => return Err(e),
    };

    let mut reaped = 0usize;
    loop {
        let entry = match entries.next_entry().await {
            Ok(Some(entry)) => entry,
            Ok(None) => break,
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    dir = %dir.display(),
                    reaped,
                    "contents reaper: directory read ended early"
                );
                break;
            }
        };
        let file_name = entry.file_name();
        let Some(name) = file_name.to_str() else {
            continue;
        };
        if !is_temp_name(name) {
            continue;
        }
        let Ok(meta) = entry.metadata().await else {
            continue;
        };
        if !meta.is_file() {
            continue;
        }
        let stale = meta
            .modified()
            .ok()
            .and_then(|modified| now.duration_since(modified).ok())
            .is_some_and(|age| age >= grace);
        if !stale {
            continue;
        }
        match tokio::fs::remove_file(entry.path()).await {
            Ok(()) => reaped += 1,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    path = %entry.path().display(),
                    "contents reaper: could not remove stale temp file"
                );
            }
        }
    }
    Ok(reaped)
}

/// Dropping a deployment future (processing deadline, or the client disconnecting) cannot cancel the
/// blocking write already handed to the runtime, so the straggler is reaped here instead.
pub fn spawn_reaper(dir: PathBuf, grace: Duration) {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(REAP_INTERVAL);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            ticker.tick().await;
            match reap_stale_temp_files(&dir, grace).await {
                Ok(0) => {}
                Ok(reaped) => tracing::info!(
                    reaped,
                    dir = %dir.display(),
                    "contents reaper: removed stale temp files"
                ),
                Err(e) => tracing::warn!(
                    error = %e,
                    dir = %dir.display(),
                    "contents reaper: sweep failed"
                ),
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch_dir(tag: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!(
            "worlds-reaper-{tag}-{}-{nonce}",
            std::process::id()
        ))
    }

    #[test]
    fn temp_name_predicate_covers_every_writer_convention() {
        assert!(is_temp_name(".bafkreiabc.4242.17.part"));
        assert!(is_temp_name(".bafkreiabc.auth.4242.17.part"));
        assert!(is_temp_name(".bafkreiabc.part"));

        assert!(!is_temp_name("bafkreiabc"));
        assert!(!is_temp_name("bafkreiabc.auth"));
        assert!(!is_temp_name("bafkreiabc.part"));
        assert!(!is_temp_name(".keep"));
        assert!(!is_temp_name(".part"));
    }

    #[test]
    fn grace_spans_both_deployment_deadlines_and_never_drops_below_the_floor() {
        assert_eq!(reap_grace(300_000, 300_000), Duration::from_secs(600));
        assert_eq!(reap_grace(1, 1), MIN_REAP_GRACE);
    }

    #[tokio::test]
    async fn reaper_removes_only_stale_temp_files() {
        let dir = scratch_dir("stale");
        tokio::fs::create_dir_all(&dir).await.unwrap();
        for name in [
            "bafkreiabc",
            "bafkreiabc.auth",
            ".keep",
            ".bafkreiabc.4242.17.part",
            ".bafkreiabc.auth.4242.17.part",
            ".bafkreidef.part",
        ] {
            tokio::fs::write(dir.join(name), b"x").await.unwrap();
        }
        tokio::fs::create_dir_all(dir.join(".nested.part"))
            .await
            .unwrap();

        assert_eq!(
            reap_stale_temp_files(&dir, Duration::from_secs(3600))
                .await
                .unwrap(),
            0
        );
        assert_eq!(
            reap_stale_temp_files(&dir, Duration::ZERO).await.unwrap(),
            3
        );

        for name in ["bafkreiabc", "bafkreiabc.auth", ".keep", ".nested.part"] {
            assert!(
                tokio::fs::try_exists(dir.join(name)).await.unwrap(),
                "reaper deleted {name}"
            );
        }
        for name in [
            ".bafkreiabc.4242.17.part",
            ".bafkreiabc.auth.4242.17.part",
            ".bafkreidef.part",
        ] {
            assert!(
                !tokio::fs::try_exists(dir.join(name)).await.unwrap(),
                "reaper kept {name}"
            );
        }

        tokio::fs::remove_dir_all(&dir).await.unwrap();
    }

    #[tokio::test]
    async fn reaper_tolerates_a_contents_dir_that_does_not_exist_yet() {
        let dir = scratch_dir("missing");
        assert_eq!(
            reap_stale_temp_files(&dir, Duration::ZERO).await.unwrap(),
            0
        );
    }

    #[tokio::test]
    async fn reaper_still_reports_a_contents_dir_it_cannot_open() {
        let path = scratch_dir("notadir");
        tokio::fs::write(&path, b"x").await.unwrap();
        assert!(reap_stale_temp_files(&path, Duration::ZERO).await.is_err());
        tokio::fs::remove_file(&path).await.unwrap();
    }
}
