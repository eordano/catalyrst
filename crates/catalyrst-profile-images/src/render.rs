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
        if self.cfg.headless {
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

        let child = cmd.spawn().map_err(|e| RenderError::Spawn(e.to_string()))?;

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

        verify_output(&body_path, ImageKind::Body).await?;
        verify_output(&face_path, ImageKind::Face).await?;

        Ok(RenderOutputs {
            body_path,
            face_path,
        })
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
    let s = String::from_utf8_lossy(src);
    let n = s.len().saturating_sub(2048);
    s[n..].trim().to_string()
}
