use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use serde_json::{json, Value};
use tokio::process::Command;

use crate::cache::ImageKind;
use crate::config::RenderConfig;

pub const BODY_W: u32 = 256;
pub const BODY_H: u32 = 512;

pub const FACE_W: u32 = 256;
pub const FACE_H: u32 = 256;

#[derive(Debug, thiserror::Error)]
pub enum RenderError {
    #[error("godot client binary not found at {0}")]
    BinaryMissing(PathBuf),
    #[error("failed to spawn godot: {0}")]
    Spawn(String),
    #[error("godot render timed out after {0:?}")]
    Timeout(Duration),
    #[error("godot exited with status {status}: {tail}")]
    NonZero { status: String, tail: String },
    #[error("render produced no {kind} png (godot ran but output missing)")]
    OutputMissing { kind: &'static str },
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("payload serialize error: {0}")]
    Serde(#[from] serde_json::Error),
}

pub struct RenderOutputs {
    pub body_path: PathBuf,
    pub face_path: PathBuf,
}

pub struct GodotRenderer {
    cfg: RenderConfig,
}

impl GodotRenderer {
    pub fn new(cfg: RenderConfig) -> Self {
        Self { cfg }
    }

    pub async fn render(
        &self,
        entity: &str,
        avatar: &Value,
        content_base: &str,
        workdir: &Path,
    ) -> Result<RenderOutputs, RenderError> {
        let bin = PathBuf::from(&self.cfg.godot_bin);
        if !bin.is_file() {
            return Err(RenderError::BinaryMissing(bin));
        }

        tokio::fs::create_dir_all(workdir).await?;
        let body_path = workdir.join(format!("{entity}.png"));
        let face_path = workdir.join(format!("{entity}_face.png"));
        let payload_path = workdir.join("avatars.json");

        let payload = json!({
            "baseUrl": content_base,
            "payload": [{
                "entity": entity,
                "destPath": body_path.to_string_lossy(),
                "width": BODY_W,
                "height": BODY_H,
                "faceDestPath": face_path.to_string_lossy(),
                "faceWidth": FACE_W,
                "faceHeight": FACE_H,
                "avatar": avatar,
            }]
        });
        tokio::fs::write(&payload_path, serde_json::to_vec(&payload)?).await?;

        let mut args: Vec<String> = vec![
            "--rendering-method".into(),
            self.cfg.rendering_method.clone(),
            "--rendering-driver".into(),
            self.cfg.rendering_driver.clone(),
        ];
        args.push("--avatar-renderer".into());
        args.push("--avatars".into());
        args.push(payload_path.to_string_lossy().into_owned());
        if let Some(env) = &self.cfg.dclenv {
            args.push("--dclenv".into());
            args.push(env.clone());
        }
        args.extend(self.cfg.extra_args.iter().cloned());

        let mut cmd = Command::new(&bin);
        cmd.args(&args)
            .current_dir(&self.cfg.work_root)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            // Its own process group, so a timeout can reap the whole tree. The
            // binary is a wrapper that starts an X server beside godot, and
            // kill_on_drop's SIGKILL reaches only the wrapper -- the server and
            // godot would outlive it, once per timeout, forever.
            .process_group(0);
        if let Some(display) = &self.cfg.display {
            cmd.env("DISPLAY", display);
        }
        // The payload's baseUrl only steers the profile fetch. Wearable and
        // emote lookups go through the engine's own peer_base(), which upstream
        // pins to peer.decentraland.org -- so without this a self-hosted node
        // resolves its avatars against Decentraland's catalyst. The patched
        // build reads DCL_PEER_BASE; on an unpatched binary it is ignored and
        // behaviour is unchanged.
        cmd.env("DCL_PEER_BASE", peer_base_of(content_base));

        tracing::debug!(
            entity = %entity,
            bin = %bin.display(),
            args = ?args,
            "spawning godot avatar renderer"
        );

        let child = cmd.spawn().map_err(|e| RenderError::Spawn(e.to_string()))?;
        let pgid = child.id();

        let timeout = Duration::from_secs(self.cfg.timeout_seconds);
        let output = match tokio::time::timeout(timeout, child.wait_with_output()).await {
            Ok(Ok(o)) => o,
            Ok(Err(e)) => return Err(RenderError::Spawn(e.to_string())),
            Err(_) => {
                if let Some(pgid) = pgid {
                    kill_process_group(pgid);
                }
                return Err(RenderError::Timeout(timeout));
            }
        };

        // The images decide the outcome, not the exit code. Godot writes both
        // PNGs and only then tears down its GL context, where the NVIDIA driver
        // aborts on a double free -- so a run that produced perfectly good
        // output can still exit non-zero. Verify first, and report the exit
        // status only when there is nothing usable on disk to serve.
        let body_ok = verify_output(&body_path, ImageKind::Body).await;
        let face_ok = verify_output(&face_path, ImageKind::Face).await;
        if let (Ok(()), Ok(())) = (&body_ok, &face_ok) {
            if !output.status.success() {
                tracing::warn!(
                    status = %output.status,
                    "godot exited non-zero after writing both images; serving them"
                );
            }
        } else if !output.status.success() {
            let tail = tail_of(&output.stderr, &output.stdout);
            return Err(RenderError::NonZero {
                status: output.status.to_string(),
                tail,
            });
        }
        body_ok?;
        face_ok?;

        Ok(RenderOutputs {
            body_path,
            face_path,
        })
    }
}

/// The catalyst root a content base belongs to, e.g.
/// `http://127.0.0.1:5141/content` -> `http://127.0.0.1:5141`.
///
/// The engine appends its own `/content/` and `/lambdas/`, so it wants the root
/// rather than the content endpoint the payload carries.
fn peer_base_of(content_base: &str) -> String {
    let trimmed = content_base.trim_end_matches('/');
    trimmed
        .strip_suffix("/content")
        .unwrap_or(trimmed)
        .to_string()
}

/// SIGKILL every process in `pgid`, which spawn() made a group of its own.
///
/// Runs after kill_on_drop has already SIGKILLed the group leader, so this is
/// what actually reaches the X server and godot beneath it. Best-effort:
/// the group is simply gone when a render exits between the timeout firing and
/// this call, and ESRCH is the ordinary result rather than a fault to report.
fn kill_process_group(pgid: u32) {
    let pgid = pgid as i32;
    if pgid <= 0 {
        return;
    }
    unsafe {
        libc::kill(-pgid, libc::SIGKILL);
    }
}

const BLANK_BYTES_THRESHOLD: u64 = 3000;

async fn verify_output(path: &Path, kind: ImageKind) -> Result<(), RenderError> {
    let label = match kind {
        ImageKind::Body => "body",
        ImageKind::Face => "face",
    };
    match tokio::fs::metadata(path).await {
        Ok(m) if m.is_file() && m.len() >= BLANK_BYTES_THRESHOLD => Ok(()),
        _ => Err(RenderError::OutputMissing { kind: label }),
    }
}

fn tail_of(stderr: &[u8], stdout: &[u8]) -> String {
    let src = if stderr.is_empty() { stdout } else { stderr };
    let start = src.len().saturating_sub(2048);
    String::from_utf8_lossy(&src[start..]).trim().to_string()
}

#[cfg(test)]
mod peer_base_tests {
    use super::peer_base_of;

    #[test]
    fn the_content_endpoint_is_reduced_to_its_catalyst_root() {
        assert_eq!(
            peer_base_of("http://127.0.0.1:5141/content"),
            "http://127.0.0.1:5141"
        );
        assert_eq!(
            peer_base_of("http://127.0.0.1:5141/content/"),
            "http://127.0.0.1:5141"
        );
    }

    #[test]
    fn a_base_that_is_already_a_root_is_left_alone() {
        assert_eq!(
            peer_base_of("https://peer.example.org"),
            "https://peer.example.org"
        );
        assert_eq!(
            peer_base_of("https://peer.example.org/"),
            "https://peer.example.org"
        );
    }

    /// A path merely ending in the word "content" is not the content endpoint;
    /// stripping it would point the engine at a parent that serves nothing.
    #[test]
    fn only_a_trailing_content_segment_is_stripped() {
        assert_eq!(
            peer_base_of("https://host.example/my-content"),
            "https://host.example/my-content"
        );
        assert_eq!(
            peer_base_of("https://host.example/catalyst/content"),
            "https://host.example/catalyst"
        );
    }
}
