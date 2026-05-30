use anyhow::{anyhow, Context, Result};
use std::env;

/// How the service obtains the body/face PNGs it serves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendKind {
    /// Render locally with the headless Godot avatar renderer (the real,
    /// self-hosted path). Resolves the profile from the local content core,
    /// renders the equipped wearables to PNGs, caches them. Optionally falls
    /// back to the proxy origin (if configured) when a render fails.
    Render,
    /// Origin-pull from a prod profile-images deployment. No local render.
    Proxy,
    /// No upstream; serve 404 for cache misses. Useful for tests / offline.
    Disabled,
}

impl BackendKind {
    pub fn label(self) -> &'static str {
        match self {
            BackendKind::Render => "render",
            BackendKind::Proxy => "proxy",
            BackendKind::Disabled => "disabled",
        }
    }
}

/// Everything the headless Godot avatar renderer needs to run.
#[derive(Debug, Clone)]
pub struct RenderConfig {
    /// Path to the exported Godot client binary
    /// (`decentraland.godot.client.x86_64`).
    pub godot_bin: String,
    /// Working dir to spawn the client from (the godot-explorer project root),
    /// so the gdextension's relative `libdclgodot.so` path resolves.
    pub work_root: String,
    /// `--rendering-method` (default `gl_compatibility`).
    pub rendering_method: String,
    /// `--rendering-driver` (default `opengl3`).
    pub rendering_driver: String,
    /// `--dclenv` (env for wearable lookups: `org` | `zone` | `today` | …).
    /// `None` leaves the client's default (`org`).
    pub dclenv: Option<String>,
    /// Pass `--headless` (needs Xvfb / EGL surfaceless to actually draw).
    pub headless: bool,
    /// Value for the child's `DISPLAY` env var (X11). `None` inherits.
    pub display: Option<String>,
    /// Extra raw args appended to the godot invocation.
    pub extra_args: Vec<String>,
    /// Per-render wall-clock timeout (seconds).
    pub timeout_seconds: u64,
    /// Max concurrent Godot processes.
    pub max_concurrent: usize,
    /// Scratch dir root for per-render `avatars.json` + output PNGs.
    pub workdir_root: String,
}

pub struct Config {
    pub http_host: String,
    pub http_port: u16,

    pub backend_kind: BackendKind,

    /// Local catalyst content base (with `/content` suffix), used to resolve
    /// profile entities and as the renderer's wearable lookup `baseUrl`.
    /// Required for the `render` backend.
    pub content_base: Option<String>,
    /// Render settings (present iff backend is `render`).
    pub render: Option<RenderConfig>,
    /// When the `render` backend fails, fall back to the proxy origin instead
    /// of returning 502. Off by default — the proxy is an *explicit* last
    /// resort, not the primary path.
    pub render_fallback_proxy: bool,

    /// Origin base, e.g. `https://profile-images.decentraland.org`. Used by the
    /// `proxy` backend and by `render` when `render_fallback_proxy` is on.
    pub origin_url: Option<String>,

    /// Filesystem cache root. One subtree per entity:
    /// `<root>/<hex-prefix>/<entity>/{face,body}.png`.
    pub cache_dir: String,
    /// Cache entry freshness in seconds; stale entries trigger a re-render /
    /// origin re-pull. 0 disables expiry (serve cached forever once present).
    pub cache_ttl_seconds: u64,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let origin_url = env::var("PROFILE_IMAGES_ORIGIN_URL")
            .ok()
            .filter(|s| !s.is_empty())
            .map(|s| s.trim_end_matches('/').to_string());

        let content_base = env::var("PROFILE_IMAGES_CONTENT_URL")
            .ok()
            .filter(|s| !s.is_empty())
            .map(|s| s.trim_end_matches('/').to_string());

        let backend_kind = match env::var("PROFILE_IMAGES_BACKEND").ok().as_deref() {
            Some("render") => BackendKind::Render,
            Some("proxy") => BackendKind::Proxy,
            Some("disabled") => BackendKind::Disabled,
            // Back-compat default selection when unset: prefer render if a
            // content base is configured, else proxy if an origin is set.
            None if content_base.is_some() => BackendKind::Render,
            None if origin_url.is_some() => BackendKind::Proxy,
            None => BackendKind::Disabled,
            Some(other) => return Err(anyhow!("unknown PROFILE_IMAGES_BACKEND={other}")),
        };

        if backend_kind == BackendKind::Proxy && origin_url.is_none() {
            return Err(anyhow!(
                "PROFILE_IMAGES_BACKEND=proxy requires PROFILE_IMAGES_ORIGIN_URL"
            ));
        }

        let cache_dir = env::var("PROFILE_IMAGES_CACHE_DIR")
            .unwrap_or_else(|_| "./data/profile-images".to_string());

        let render_fallback_proxy = env_bool("PROFILE_IMAGES_RENDER_FALLBACK_PROXY", false)?;

        let render = if backend_kind == BackendKind::Render {
            if content_base.is_none() {
                return Err(anyhow!(
                    "PROFILE_IMAGES_BACKEND=render requires PROFILE_IMAGES_CONTENT_URL \
                     (e.g. http://127.0.0.1:5141/content)"
                ));
            }
            if render_fallback_proxy && origin_url.is_none() {
                return Err(anyhow!(
                    "PROFILE_IMAGES_RENDER_FALLBACK_PROXY=true requires PROFILE_IMAGES_ORIGIN_URL"
                ));
            }
            let godot_bin = env::var("PROFILE_IMAGES_GODOT_BIN").map_err(|_| {
                anyhow!(
                    "PROFILE_IMAGES_BACKEND=render requires PROFILE_IMAGES_GODOT_BIN \
                     (path to decentraland.godot.client.x86_64)"
                )
            })?;
            let work_root = match env::var("PROFILE_IMAGES_GODOT_PROJECT") {
                Ok(p) if !p.is_empty() => p,
                _ => {
                    // Default: the binary's parent dir's parent (exports/.. = repo root).
                    std::path::Path::new(&godot_bin)
                        .parent()
                        .and_then(|p| p.parent())
                        .map(|p| p.to_string_lossy().into_owned())
                        .ok_or_else(|| {
                            anyhow!("could not derive PROFILE_IMAGES_GODOT_PROJECT from godot bin path")
                        })?
                }
            };
            Some(RenderConfig {
                godot_bin,
                work_root,
                rendering_method: env::var("PROFILE_IMAGES_RENDERING_METHOD")
                    .unwrap_or_else(|_| "gl_compatibility".to_string()),
                rendering_driver: env::var("PROFILE_IMAGES_RENDERING_DRIVER")
                    .unwrap_or_else(|_| "opengl3".to_string()),
                dclenv: env::var("PROFILE_IMAGES_DCLENV")
                    .ok()
                    .filter(|s| !s.is_empty()),
                headless: env_bool("PROFILE_IMAGES_GODOT_HEADLESS", false)?,
                display: env::var("PROFILE_IMAGES_GODOT_DISPLAY")
                    .ok()
                    .filter(|s| !s.is_empty()),
                extra_args: env::var("PROFILE_IMAGES_GODOT_EXTRA_ARGS")
                    .ok()
                    .filter(|s| !s.is_empty())
                    .map(|s| s.split_whitespace().map(String::from).collect())
                    .unwrap_or_default(),
                timeout_seconds: get_u64("PROFILE_IMAGES_RENDER_TIMEOUT_SECONDS", 120)?,
                max_concurrent: get_u64("PROFILE_IMAGES_RENDER_MAX_CONCURRENT", 1)? as usize,
                workdir_root: env::var("PROFILE_IMAGES_RENDER_WORKDIR")
                    .unwrap_or_else(|_| format!("{cache_dir}/.render-tmp")),
            })
        } else {
            None
        };

        Ok(Self {
            http_host: env::var("HTTP_SERVER_HOST").unwrap_or_else(|_| "127.0.0.1".to_string()),
            http_port: get_port("HTTP_SERVER_PORT", 5152)?,
            backend_kind,
            content_base,
            render,
            render_fallback_proxy,
            origin_url,
            cache_dir,
            cache_ttl_seconds: get_u64("PROFILE_IMAGES_CACHE_TTL_SECONDS", 86_400)?,
        })
    }
}

fn get_port(key: &str, default: u16) -> Result<u16> {
    match env::var(key) {
        Ok(s) => s.parse::<u16>().with_context(|| format!("invalid {}", key)),
        Err(_) => Ok(default),
    }
}

fn get_u64(key: &str, default: u64) -> Result<u64> {
    match env::var(key) {
        Ok(s) => s.parse::<u64>().with_context(|| format!("invalid {}", key)),
        Err(_) => Ok(default),
    }
}

fn env_bool(key: &str, default: bool) -> Result<bool> {
    match env::var(key) {
        Ok(s) => match s.trim().to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" | "on" => Ok(true),
            "0" | "false" | "no" | "off" | "" => Ok(false),
            other => Err(anyhow!("invalid bool {key}={other}")),
        },
        Err(_) => Ok(default),
    }
}
