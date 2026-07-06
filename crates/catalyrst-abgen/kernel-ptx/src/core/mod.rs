pub mod bc5;
pub mod bc7;
pub mod dxt1;
pub mod mips;
pub mod mode_tree;

#[inline]
pub fn sqrtf(x: f32) -> f32 {
    #[cfg(not(target_arch = "nvptx64"))]
    {
        x.sqrt()
    }
    #[cfg(target_arch = "nvptx64")]
    {
        libm::sqrtf(x)
    }
}
