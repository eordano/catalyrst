//! Headless Godot avatar renderer driver.
//!
//! This is the Rust analogue of upstream `decentraland/profile-images`'s
//! `src/adapters/godot.ts`: it writes an `avatars.json` payload, invokes the
//! exported Godot client in `--avatar-renderer` mode, and lets the client
//! rasterize the equipped 3D wearables to two PNGs (body 256x512, face
//! 256x256). The exact invocation upstream uses is:
//!
//! ```text
//! <client> --rendering-method gl_compatibility --rendering-driver opengl3 \
//!          --avatar-renderer --avatars <avatars.json> [--dclenv <env>]
//! ```
//!
//! Unlike upstream (which is an SQS producer/consumer rig), we render
//! synchronously on a cache miss, one entity at a time, behind a single-flight
//! lock owned by `RenderQueue`. One render produces *both* face and body, so a
//! concurrent face+body request pair collapses to a single Godot invocation.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use serde_json::{json, Value};
use tokio::process::Command;

use crate::cache::ImageKind;
use crate::config::RenderConfig;

/// Body image dimensions (must match the upstream contract).
pub const BODY_W: u32 = 256;
pub const BODY_H: u32 = 512;
/// Face image dimensions.
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

/// Where a render writes its two PNGs (absolute paths under a temp workdir).
pub struct RenderOutputs {
    pub body_path: PathBuf,
    pub face_path: PathBuf,
}

/// Drives a single headless Godot avatar render for one entity.
pub struct GodotRenderer {
    cfg: RenderConfig,
}

impl GodotRenderer {
    pub fn new(cfg: RenderConfig) -> Self {
        Self { cfg }
    }

    /// Render `entity`'s `avatar` payload against `content_base`, producing a
    /// body and face PNG in `workdir`. The caller is responsible for moving
    /// the results into the content-addressed cache and cleaning `workdir`.
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

        // Mirror the avatars.json schema from PROFILE_IMAGE.md / godot.ts.
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

        // Build the argv. Upstream's godot.ts uses:
        //   --rendering-driver opengl3 --avatar-renderer --avatars <json>
        // and the local snapshot scripts add --rendering-method gl_compatibility
        // (Compatibility renderer) which is what works without a Vulkan device.
        let mut args: Vec<String> = vec![
            "--rendering-method".into(),
            self.cfg.rendering_method.clone(),
            "--rendering-driver".into(),
            self.cfg.rendering_driver.clone(),
        ];
        if self.cfg.headless {
            // Off-screen: needs Xvfb / EGL surfaceless to actually draw. Off by
            // default for the same reason the local script keeps it off.
            args.push("--headless".into());
        }
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
            // Run from the godot project root so the gdextension's relative
            // libdclgodot.so path resolves (see local_profile_snapshot.sh).
            .current_dir(&self.cfg.work_root)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        if let Some(display) = &self.cfg.display {
            cmd.env("DISPLAY", display);
        }

        tracing::debug!(
            entity = %entity,
            bin = %bin.display(),
            args = ?args,
            "spawning godot avatar renderer"
        );

        let child = cmd
            .spawn()
            .map_err(|e| RenderError::Spawn(e.to_string()))?;

        let timeout = Duration::from_secs(self.cfg.timeout_seconds);
        let output = match tokio::time::timeout(timeout, child.wait_with_output()).await {
            Ok(Ok(o)) => o,
            Ok(Err(e)) => return Err(RenderError::Spawn(e.to_string())),
            Err(_) => return Err(RenderError::Timeout(timeout)),
        };

        if !output.status.success() {
            let tail = tail_of(&output.stderr, &output.stdout);
            return Err(RenderError::NonZero {
                status: output.status.to_string(),
                tail,
            });
        }

        // Godot can exit 0 yet fail to draw (e.g. blank avatar). Verify both
        // PNGs exist and are non-trivial before declaring success.
        verify_output(&body_path, ImageKind::Body).await?;
        verify_output(&face_path, ImageKind::Face).await?;

        Ok(RenderOutputs {
            body_path,
            face_path,
        })
    }
}

/// A render that exits 0 but writes a missing/blank PNG should be treated as a
/// failure so we don't cache garbage. We reuse the same blank threshold the
/// upstream classifier (`local_entity_snapshot.sh`) uses (~3 KB).
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

/// Last ~2 KB of stderr (falling back to stdout) for error reporting.
fn tail_of(stderr: &[u8], stdout: &[u8]) -> String {
    let src = if stderr.is_empty() { stdout } else { stderr };
    let s = String::from_utf8_lossy(src);
    let n = s.len().saturating_sub(2048);
    s[n..].trim().to_string()
}
