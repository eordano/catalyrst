#[path = "../../kernel-ptx/src/core/mod.rs"]
pub mod corelib;

pub(crate) mod cuda;
mod qualify;
#[cfg(feature = "gpu-wgpu")]
pub(crate) mod wgpu;
#[cfg(feature = "gpu-wgpu")]
pub(crate) mod wgpu_bc7;

use anyhow::{anyhow, Result};
use std::sync::OnceLock;

pub use crate::gpu::corelib::bc7::Bc7Profile;
pub use cuda::{
    cmd_probe, encode_blocks_gpu, tex_geometry, BlockifyStats, BlockifyTex, SlabEngine,
};

use qualify::QualStatus;

#[derive(Debug, PartialEq)]
enum BackendSel {
    Auto,
    Cuda,
    Wgpu,
    Off,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Backend {
    Cuda,
    #[cfg(feature = "gpu-wgpu")]
    Wgpu,
}

struct Resolution {
    active: Result<Backend, String>,
    status: Option<QualStatus>,
}

static RESOLVED: OnceLock<Resolution> = OnceLock::new();

fn parse_sel(raw: &str) -> Result<BackendSel, String> {
    match raw {
        "" | "auto" => Ok(BackendSel::Auto),
        "cuda" => Ok(BackendSel::Cuda),
        "wgpu" => Ok(BackendSel::Wgpu),
        "off" => Ok(BackendSel::Off),
        other => Err(format!(
            "unknown ABGEN_GPU_BACKEND value {other:?} (expected auto|cuda|wgpu|off)"
        )),
    }
}

fn parse_backend_sel() -> Result<BackendSel, String> {
    match std::env::var("ABGEN_GPU_BACKEND") {
        Ok(v) => parse_sel(&v),
        Err(std::env::VarError::NotPresent) => Ok(BackendSel::Auto),
        Err(std::env::VarError::NotUnicode(v)) => Err(format!(
            "unknown ABGEN_GPU_BACKEND value {v:?} (expected auto|cuda|wgpu|off)"
        )),
    }
}

type TryBackend<'a> = &'a dyn Fn() -> (Result<Backend, String>, QualStatus);

fn try_cuda() -> (Result<Backend, String>, QualStatus) {
    if let Err(e) = cuda::gpu_ready() {
        let reason = format!("init failed: {e}");
        return (
            Err(format!("cuda backend disabled: {reason}")),
            QualStatus {
                backend: "cuda",
                qualified: false,
                reason: Some(reason),
            },
        );
    }
    let st = qualify::qualify_backend("cuda", &|rgba, w, h, mc, flip, srgb, perc, prof| {
        cuda::encode_bc7_mip_chain_gpu(rgba, w, h, mc, flip, srgb, perc, prof)
    });
    if st.qualified {
        (Ok(Backend::Cuda), st)
    } else {
        let reason = st
            .reason
            .clone()
            .unwrap_or_else(|| String::from("unqualified"));
        (Err(format!("cuda backend disqualified: {reason}")), st)
    }
}

#[cfg(feature = "gpu-wgpu")]
fn try_wgpu() -> (Result<Backend, String>, QualStatus) {
    match wgpu::adapter_summary() {
        Err(e) => {
            let reason = format!("init failed: {e}");
            return (
                Err(format!("wgpu backend disabled: {reason}")),
                QualStatus {
                    backend: "wgpu",
                    qualified: false,
                    reason: Some(reason),
                },
            );
        }
        Ok(summary) => eprintln!("abgen-gpu: wgpu adapter: {summary}"),
    }
    let st = qualify::qualify_backend("wgpu", &|rgba, w, h, mc, flip, srgb, perc, prof| {
        wgpu_bc7::encode_bc7_mip_chain(rgba, w, h, mc, flip, srgb, perc, prof)
    });
    if st.qualified {
        (Ok(Backend::Wgpu), st)
    } else {
        let reason = st
            .reason
            .clone()
            .unwrap_or_else(|| String::from("unqualified"));
        (Err(format!("wgpu backend disqualified: {reason}")), st)
    }
}

fn log_status(st: &QualStatus) {
    eprintln!(
        "abgen-gpu: qualification backend={} qualified={} reason={}",
        st.backend,
        st.qualified,
        st.reason.as_deref().unwrap_or("-")
    );
}

fn status_resolution(active: Result<Backend, String>, status: QualStatus) -> Resolution {
    Resolution {
        active,
        status: Some(status),
    }
}

fn resolve_from(sel: BackendSel, cuda: TryBackend, wgpu_try: Option<TryBackend>) -> Resolution {
    match sel {
        BackendSel::Off => status_resolution(
            Err(String::from("gpu disabled by ABGEN_GPU_BACKEND=off")),
            QualStatus {
                backend: "off",
                qualified: false,
                reason: Some(String::from("disabled by ABGEN_GPU_BACKEND=off")),
            },
        ),
        BackendSel::Cuda => {
            let (active, status) = cuda();
            status_resolution(active, status)
        }
        BackendSel::Wgpu => match wgpu_try {
            Some(f) => {
                let (active, status) = f();
                status_resolution(active, status)
            }
            None => status_resolution(
                Err(String::from("wgpu backend not built (feature gpu-wgpu)")),
                QualStatus {
                    backend: "wgpu",
                    qualified: false,
                    reason: Some(String::from("wgpu backend not built (feature gpu-wgpu)")),
                },
            ),
        },
        BackendSel::Auto => {
            let (active, status) = cuda();
            match (active, wgpu_try) {
                (Ok(b), _) => status_resolution(Ok(b), status),
                (Err(ce), None) => status_resolution(Err(ce), status),
                (Err(ce), Some(f)) => {
                    log_status(&status);
                    let (wactive, wstatus) = f();
                    match wactive {
                        Ok(b) => status_resolution(Ok(b), wstatus),
                        Err(we) => {
                            log_status(&wstatus);
                            let combined = format!("auto: {ce}; {we}");
                            status_resolution(
                                Err(combined.clone()),
                                QualStatus {
                                    backend: "auto",
                                    qualified: false,
                                    reason: Some(combined),
                                },
                            )
                        }
                    }
                }
            }
        }
    }
}

fn resolve() -> Resolution {
    let sel = match parse_backend_sel() {
        Ok(s) => s,
        Err(e) => {
            return Resolution {
                active: Err(e.clone()),
                status: Some(QualStatus {
                    backend: "invalid",
                    qualified: false,
                    reason: Some(e),
                }),
            }
        }
    };
    #[cfg(feature = "gpu-wgpu")]
    let wgpu_try: Option<TryBackend> = Some(&try_wgpu);
    #[cfg(not(feature = "gpu-wgpu"))]
    let wgpu_try: Option<TryBackend> = None;
    let res = resolve_from(sel, &try_cuda, wgpu_try);
    if let Some(st) = &res.status {
        log_status(st);
    }
    res
}

fn resolution() -> &'static Resolution {
    RESOLVED.get_or_init(resolve)
}

pub fn gpu_ready() -> Result<(), String> {
    match &resolution().active {
        Ok(_) => Ok(()),
        Err(e) => Err(e.clone()),
    }
}

pub fn gpu_status() -> Option<(&'static str, bool, Option<String>)> {
    RESOLVED
        .get()
        .and_then(|r| r.status.as_ref())
        .map(|s| (s.backend, s.qualified, s.reason.clone()))
}

#[allow(clippy::too_many_arguments)]
pub fn encode_bc7_mip_chain_gpu(
    rgba: &[u8],
    width: u32,
    height: u32,
    mip_count: Option<i32>,
    flip: bool,
    srgb: bool,
    perceptual: bool,
    profile: Bc7Profile,
) -> Result<(Vec<u8>, i32)> {
    match &resolution().active {
        Ok(Backend::Cuda) => cuda::encode_bc7_mip_chain_gpu(
            rgba, width, height, mip_count, flip, srgb, perceptual, profile,
        ),
        #[cfg(feature = "gpu-wgpu")]
        Ok(Backend::Wgpu) => wgpu_bc7::encode_bc7_mip_chain(
            rgba, width, height, mip_count, flip, srgb, perceptual, profile,
        ),
        Err(e) => Err(anyhow!("{e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn qs(backend: &'static str, qualified: bool, reason: Option<&str>) -> QualStatus {
        QualStatus {
            backend,
            qualified,
            reason: reason.map(String::from),
        }
    }

    fn cuda_ok() -> (Result<Backend, String>, QualStatus) {
        (Ok(Backend::Cuda), qs("cuda", true, None))
    }

    fn cuda_fail() -> (Result<Backend, String>, QualStatus) {
        (
            Err(String::from(
                "cuda backend disabled: init failed: no device",
            )),
            qs("cuda", false, Some("init failed: no device")),
        )
    }

    fn wgpu_never() -> (Result<Backend, String>, QualStatus) {
        panic!("wgpu backend must not be tried");
    }

    #[test]
    fn parse_sel_values() {
        assert_eq!(parse_sel("").unwrap(), BackendSel::Auto);
        assert_eq!(parse_sel("auto").unwrap(), BackendSel::Auto);
        assert_eq!(parse_sel("cuda").unwrap(), BackendSel::Cuda);
        assert_eq!(parse_sel("wgpu").unwrap(), BackendSel::Wgpu);
        assert_eq!(parse_sel("off").unwrap(), BackendSel::Off);
        let e = parse_sel("metal").unwrap_err();
        assert!(e.contains("expected auto|cuda|wgpu|off"), "{e}");
    }

    #[test]
    fn resolve_off_disables() {
        let r = resolve_from(BackendSel::Off, &cuda_ok, Some(&wgpu_never));
        assert_eq!(
            r.active.unwrap_err(),
            "gpu disabled by ABGEN_GPU_BACKEND=off"
        );
        assert_eq!(r.status.unwrap().backend, "off");
    }

    #[test]
    fn resolve_wgpu_without_feature_fails_closed_as_not_built() {
        let r = resolve_from(BackendSel::Wgpu, &cuda_ok, None);
        assert_eq!(
            r.active.unwrap_err(),
            "wgpu backend not built (feature gpu-wgpu)"
        );
        let st = r.status.unwrap();
        assert_eq!(st.backend, "wgpu");
        assert!(!st.qualified);
        assert_eq!(
            st.reason.as_deref(),
            Some("wgpu backend not built (feature gpu-wgpu)")
        );
    }

    #[test]
    fn resolve_auto_without_feature_is_cuda_only() {
        let r = resolve_from(BackendSel::Auto, &cuda_fail, None);
        let e = r.active.unwrap_err();
        assert!(e.contains("cuda backend disabled"), "{e}");
        assert!(!e.contains("wgpu"), "{e}");
        let r = resolve_from(BackendSel::Auto, &cuda_ok, None);
        assert_eq!(r.active.unwrap(), Backend::Cuda);
        assert_eq!(r.status.unwrap().backend, "cuda");
    }

    #[test]
    fn resolve_cuda_sel_never_tries_wgpu() {
        let r = resolve_from(BackendSel::Cuda, &cuda_ok, Some(&wgpu_never));
        assert_eq!(r.active.unwrap(), Backend::Cuda);
        let r = resolve_from(BackendSel::Cuda, &cuda_fail, Some(&wgpu_never));
        assert!(r.active.unwrap_err().contains("cuda backend disabled"));
    }

    #[test]
    fn resolve_auto_prefers_cuda() {
        let r = resolve_from(BackendSel::Auto, &cuda_ok, Some(&wgpu_never));
        assert_eq!(r.active.unwrap(), Backend::Cuda);
    }

    #[cfg(feature = "gpu-wgpu")]
    mod wgpu_arms {
        use super::*;

        fn wgpu_ok() -> (Result<Backend, String>, QualStatus) {
            (Ok(Backend::Wgpu), qs("wgpu", true, None))
        }

        fn wgpu_fail() -> (Result<Backend, String>, QualStatus) {
            (
                Err(String::from(
                    "wgpu backend disqualified: divergent blocks 1 of 4",
                )),
                qs("wgpu", false, Some("divergent blocks 1 of 4")),
            )
        }

        #[test]
        fn resolve_wgpu_sel_uses_wgpu() {
            let r = resolve_from(BackendSel::Wgpu, &cuda_ok, Some(&wgpu_ok));
            assert_eq!(r.active.unwrap(), Backend::Wgpu);
            assert_eq!(r.status.unwrap().backend, "wgpu");
        }

        #[test]
        fn resolve_wgpu_sel_fails_closed_on_disqualification() {
            let r = resolve_from(BackendSel::Wgpu, &cuda_ok, Some(&wgpu_fail));
            let e = r.active.unwrap_err();
            assert!(e.contains("wgpu backend disqualified"), "{e}");
            assert!(!e.contains("not built"), "{e}");
        }

        #[test]
        fn resolve_auto_falls_back_to_wgpu() {
            let r = resolve_from(BackendSel::Auto, &cuda_fail, Some(&wgpu_ok));
            assert_eq!(r.active.unwrap(), Backend::Wgpu);
            assert_eq!(r.status.unwrap().backend, "wgpu");
        }

        #[test]
        fn resolve_auto_combines_both_failures() {
            let r = resolve_from(BackendSel::Auto, &cuda_fail, Some(&wgpu_fail));
            let e = r.active.unwrap_err();
            assert!(e.starts_with("auto: "), "{e}");
            assert!(e.contains("cuda backend disabled"), "{e}");
            assert!(e.contains("wgpu backend disqualified"), "{e}");
            let st = r.status.unwrap();
            assert_eq!(st.backend, "auto");
            assert!(!st.qualified);
        }
    }
}
