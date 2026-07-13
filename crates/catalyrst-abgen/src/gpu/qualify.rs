use crate::gpu::Bc7Profile;
use anyhow::Result;

pub(crate) struct QualStatus {
    pub backend: &'static str,
    pub qualified: bool,
    pub reason: Option<String>,
}

pub(crate) type EncodeFn<'a> = &'a dyn Fn(
    &[u8],
    u32,
    u32,
    Option<i32>,
    bool,
    bool,
    bool,
    Bc7Profile,
) -> Result<(Vec<u8>, i32)>;

pub(crate) fn qualify_backend(backend: &'static str, encode: EncodeFn) -> QualStatus {
    let skip = !crate::clihelp::env_bool("ABGEN_GPU_QUALIFY", true);
    qualify_backend_with(backend, encode, skip)
}

fn oracle_profile(profile: Bc7Profile) -> crate::bc7_pure::Bc7Profile {
    match profile {
        Bc7Profile::Slow => crate::bc7_pure::Bc7Profile::Slow,
        Bc7Profile::Basic => crate::bc7_pure::Bc7Profile::Basic,
    }
}

pub(crate) fn qualify_backend_with(
    backend: &'static str,
    encode: EncodeFn,
    skip: bool,
) -> QualStatus {
    if skip {
        return QualStatus {
            backend,
            qualified: true,
            reason: Some(String::from("qualification skipped by env")),
        };
    }
    let sizes: [(u32, u32); 3] = [(64, 64), (128, 32), (37, 53)];
    for &(w, h) in &sizes {
        let tex = crate::gpuhost::oracle::gen_texture(1, w, h);
        for srgb in [false, true] {
            for perceptual in [false, true] {
                for profile in [Bc7Profile::Slow, Bc7Profile::Basic] {
                    let case = format!(
                        "bc7-mip {w}x{h} srgb={srgb} perceptual={perceptual} profile={profile:?}"
                    );
                    let (want, want_mips) = crate::bc7_pure::encode_bc7_mip_chain_with_profile(
                        &tex,
                        w,
                        h,
                        None,
                        true,
                        srgb,
                        perceptual,
                        oracle_profile(profile),
                    );
                    let (got, got_mips) =
                        match encode(&tex, w, h, None, true, srgb, perceptual, profile) {
                            Ok(r) => r,
                            Err(e) => {
                                return QualStatus {
                                    backend,
                                    qualified: false,
                                    reason: Some(format!("{case}: encode failed: {e:#}")),
                                }
                            }
                        };
                    if got_mips != want_mips {
                        return QualStatus {
                            backend,
                            qualified: false,
                            reason: Some(format!(
                                "{case}: mip_count {got_mips} != oracle {want_mips}"
                            )),
                        };
                    }
                    if got != want {
                        let n = got.len().min(want.len()) / 16;
                        let mut divergent = 0usize;
                        for i in 0..n {
                            if got[i * 16..(i + 1) * 16] != want[i * 16..(i + 1) * 16] {
                                divergent += 1;
                            }
                        }
                        let mut reason = format!("{case}: divergent blocks {divergent} of {n}");
                        if got.len() != want.len() {
                            reason.push_str(&format!(
                                " (length {} != oracle {})",
                                got.len(),
                                want.len()
                            ));
                        }
                        return QualStatus {
                            backend,
                            qualified: false,
                            reason: Some(reason),
                        };
                    }
                }
            }
        }
    }
    QualStatus {
        backend,
        qualified: true,
        reason: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn delegate(
        rgba: &[u8],
        w: u32,
        h: u32,
        mip_count: Option<i32>,
        flip: bool,
        srgb: bool,
        perceptual: bool,
        profile: Bc7Profile,
    ) -> Result<(Vec<u8>, i32)> {
        Ok(crate::bc7_pure::encode_bc7_mip_chain_with_profile(
            rgba,
            w,
            h,
            mip_count,
            flip,
            srgb,
            perceptual,
            oracle_profile(profile),
        ))
    }

    fn corrupt(
        rgba: &[u8],
        w: u32,
        h: u32,
        mip_count: Option<i32>,
        flip: bool,
        srgb: bool,
        perceptual: bool,
        profile: Bc7Profile,
    ) -> Result<(Vec<u8>, i32)> {
        let (mut v, m) = delegate(rgba, w, h, mip_count, flip, srgb, perceptual, profile)?;
        v[0] ^= 0xff;
        Ok((v, m))
    }

    fn broken(
        _rgba: &[u8],
        _w: u32,
        _h: u32,
        _mip_count: Option<i32>,
        _flip: bool,
        _srgb: bool,
        _perceptual: bool,
        _profile: Bc7Profile,
    ) -> Result<(Vec<u8>, i32)> {
        anyhow::bail!("device exploded")
    }

    #[test]
    fn corrupted_backend_disqualifies_with_context() {
        let st = qualify_backend_with("fake", &corrupt, false);
        assert_eq!(st.backend, "fake");
        assert!(!st.qualified);
        let reason = st.reason.expect("reason populated");
        assert!(reason.contains("divergent blocks 1 of"), "{reason}");
        assert!(
            reason.contains("bc7-mip 64x64 srgb=false perceptual=false profile=Slow"),
            "{reason}"
        );
    }

    #[test]
    fn faithful_backend_qualifies() {
        let st = qualify_backend_with("fake", &delegate, false);
        assert!(st.qualified);
        assert!(st.reason.is_none());
    }

    #[test]
    fn erroring_backend_disqualifies() {
        let st = qualify_backend_with("fake", &broken, false);
        assert!(!st.qualified);
        let reason = st.reason.expect("reason populated");
        assert!(reason.contains("encode failed"), "{reason}");
        assert!(reason.contains("device exploded"), "{reason}");
    }

    #[test]
    fn skip_by_env_qualifies_without_running() {
        let st = qualify_backend_with("fake", &broken, true);
        assert!(st.qualified);
        assert_eq!(st.reason.as_deref(), Some("qualification skipped by env"));
    }
}
