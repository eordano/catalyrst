use std::sync::atomic::{AtomicBool, Ordering};

static ENABLED: AtomicBool = AtomicBool::new(false);
static FALLBACK_WARNED: AtomicBool = AtomicBool::new(false);

pub fn enable() -> Result<(), String> {
    crate::gpu::gpu_ready()?;
    ENABLED.store(true, Ordering::Relaxed);
    Ok(())
}

pub fn enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

#[allow(clippy::too_many_arguments)]
pub fn encode_bc7_mip_chain(
    rgba: &[u8],
    width: u32,
    height: u32,
    mip_count: Option<i32>,
    flip: bool,
    srgb: bool,
    perceptual: bool,
    profile: crate::bc7_pure::Bc7Profile,
) -> Option<(Vec<u8>, i32)> {
    let gp = match profile {
        crate::bc7_pure::Bc7Profile::Slow => crate::gpu::Bc7Profile::Slow,
        crate::bc7_pure::Bc7Profile::Basic => crate::gpu::Bc7Profile::Basic,
    };
    match crate::gpu::encode_bc7_mip_chain_gpu(
        rgba, width, height, mip_count, flip, srgb, perceptual, gp,
    ) {
        Ok(r) => Some(r),
        Err(e) => {
            if !FALLBACK_WARNED.swap(true, Ordering::Relaxed) {
                eprintln!("warn: gpu bc7 encode failed, falling back to cpu: {e:#}");
            }
            None
        }
    }
}
