use super::*;

#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub(super) struct EndpointErr {
    pub(super) error: u16,
    pub(super) lo: u8,
    pub(super) hi: u8,
}

pub(super) struct OptTables {
    pub(super) mode0: [[[EndpointErr; 2]; 2]; 256],
    pub(super) mode1: [[EndpointErr; 2]; 256],
    pub(super) mode6: [[[EndpointErr; 2]; 2]; 256],
    pub(super) mode7: [[[EndpointErr; 2]; 2]; 256],
    pub(super) mode5: [u32; 256],
    pub(super) mode4_3: [u32; 256],
    pub(super) mode4_2: [u32; 256],
}

pub(super) const MODE0_IDX: usize = 2;
pub(super) const MODE1_IDX: usize = 2;
pub(super) const MODE6_IDX: usize = 5;
pub(super) const MODE5_IDX: usize = 1;
pub(super) const MODE4_IDX3: usize = 2;
pub(super) const MODE4_IDX2: usize = 1;
pub(super) const MODE7_IDX: usize = 1;

fn best_endpoints_per_c(
    lcount: u32,
    hcount: u32,
    expand_lo: impl Fn(u32) -> u32,
    expand_hi: impl Fn(u32) -> u32,
    weight: u32,
) -> [EndpointErr; 256] {
    let mut first_idx = [u32::MAX; 256];
    let mut first_lo = [0u8; 256];
    let mut first_hi = [0u8; 256];
    let mut idx = 0u32;
    for l in 0..lcount {
        let low_part = expand_lo(l) * (64 - weight);
        for h in 0..hcount {
            let k = ((low_part + expand_hi(h) * weight + 32) >> 6) as usize;
            debug_assert!(k < 256);
            if first_idx[k] == u32::MAX {
                first_idx[k] = idx;
                first_lo[k] = l as u8;
                first_hi[k] = h as u8;
            }
            idx += 1;
        }
    }
    let mut out = [EndpointErr::default(); 256];
    for c in 0..256usize {
        let mut done = false;
        for d in 0..256usize {
            let mut best_k = usize::MAX;
            let mut best_i = u32::MAX;
            if d <= c {
                let k = c - d;
                if first_idx[k] != u32::MAX {
                    best_k = k;
                    best_i = first_idx[k];
                }
            }

            if d > 0 && c + d < 256 {
                let k = c + d;
                if first_idx[k] < best_i {
                    best_k = k;
                }
            }
            if best_k != usize::MAX {
                out[c] = EndpointErr {
                    error: (d * d) as u16,
                    lo: first_lo[best_k],
                    hi: first_hi[best_k],
                };
                done = true;
                break;
            }
        }
        debug_assert!(done, "no reachable k for c={c}");
    }
    out
}

fn build_opt_tables() -> Box<OptTables> {
    let mut t = Box::new(OptTables {
        mode0: [[[EndpointErr::default(); 2]; 2]; 256],
        mode1: [[EndpointErr::default(); 2]; 256],
        mode6: [[[EndpointErr::default(); 2]; 2]; 256],
        mode7: [[[EndpointErr::default(); 2]; 2]; 256],
        mode5: [0u32; 256],
        mode4_3: [0u32; 256],
        mode4_2: [0u32; 256],
    });

    for hp in 0..2u32 {
        for lp in 0..2usize {
            let per_c = best_endpoints_per_c(
                16,
                16,
                |l| {
                    let mut low = ((l << 1) | lp as u32) << 3;
                    low |= low >> 5;
                    low
                },
                |h| {
                    let mut high = ((h << 1) | hp) << 3;
                    high |= high >> 5;
                    high
                },
                G_WEIGHTS3[MODE0_IDX],
            );
            for c in 0..256usize {
                t.mode0[c][hp as usize][lp] = per_c[c];
            }
        }
    }

    for lp in 0..2usize {
        let per_c = best_endpoints_per_c(
            64,
            64,
            |l| {
                let mut low = ((l << 1) | lp as u32) << 1;
                low |= low >> 7;
                low
            },
            |h| {
                let mut high = ((h << 1) | lp as u32) << 1;
                high |= high >> 7;
                high
            },
            G_WEIGHTS3[MODE1_IDX],
        );
        for c in 0..256usize {
            t.mode1[c][lp] = per_c[c];
        }
    }

    for hp in 0..2u32 {
        for lp in 0..2usize {
            let per_c = best_endpoints_per_c(
                128,
                128,
                |l| (l << 1) | lp as u32,
                |h| (h << 1) | hp,
                G_WEIGHTS4[MODE6_IDX],
            );
            for c in 0..256usize {
                t.mode6[c][hp as usize][lp] = per_c[c];
            }
        }
    }

    {
        let per_c = best_endpoints_per_c(
            128,
            128,
            |l| {
                let mut low = l << 1;
                low |= low >> 7;
                low
            },
            |h| {
                let mut high = h << 1;
                high |= high >> 7;
                high
            },
            G_WEIGHTS2[MODE5_IDX],
        );
        for c in 0..256usize {
            t.mode5[c] = per_c[c].lo as u32 | ((per_c[c].hi as u32) << 8);
        }
    }

    {
        let per_c = best_endpoints_per_c(
            32,
            32,
            |l| {
                let mut low = l << 3;
                low |= low >> 5;
                low
            },
            |h| {
                let mut high = h << 3;
                high |= high >> 5;
                high
            },
            G_WEIGHTS3[MODE4_IDX3],
        );
        for c in 0..256usize {
            t.mode4_3[c] = per_c[c].lo as u32 | ((per_c[c].hi as u32) << 8);
        }
    }

    {
        let per_c = best_endpoints_per_c(
            32,
            32,
            |l| {
                let mut low = l << 3;
                low |= low >> 5;
                low
            },
            |h| {
                let mut high = h << 3;
                high |= high >> 5;
                high
            },
            G_WEIGHTS2[MODE4_IDX2],
        );
        for c in 0..256usize {
            t.mode4_2[c] = per_c[c].lo as u32 | ((per_c[c].hi as u32) << 8);
        }
    }

    for hp in 0..2u32 {
        for lp in 0..2usize {
            let per_c = best_endpoints_per_c(
                32,
                32,
                |l| {
                    let mut low = ((l << 1) | lp as u32) << 2;
                    low |= low >> 6;
                    low
                },
                |h| {
                    let mut high = ((h << 1) | hp) << 2;
                    high |= high >> 6;
                    high
                },
                G_WEIGHTS2[MODE7_IDX],
            );
            for c in 0..256usize {
                t.mode7[c][hp as usize][lp] = per_c[c];
            }
        }
    }

    t
}

#[cfg(test)]
fn build_opt_tables_reference() -> Box<OptTables> {
    let mut t = Box::new(OptTables {
        mode0: [[[EndpointErr::default(); 2]; 2]; 256],
        mode1: [[EndpointErr::default(); 2]; 256],
        mode6: [[[EndpointErr::default(); 2]; 2]; 256],
        mode7: [[[EndpointErr::default(); 2]; 2]; 256],
        mode5: [0u32; 256],
        mode4_3: [0u32; 256],
        mode4_2: [0u32; 256],
    });

    for c in 0..256i32 {
        for hp in 0..2usize {
            for lp in 0..2usize {
                let mut best = EndpointErr {
                    error: u16::MAX,
                    lo: 0,
                    hi: 0,
                };
                for l in 0..16u32 {
                    let mut low = ((l << 1) | lp as u32) << 3;
                    low |= low >> 5;
                    for h in 0..16u32 {
                        let mut high = ((h << 1) | hp as u32) << 3;
                        high |= high >> 5;
                        let k = ((low * (64 - G_WEIGHTS3[MODE0_IDX])
                            + high * G_WEIGHTS3[MODE0_IDX]
                            + 32)
                            >> 6) as i32;
                        let err = (k - c) * (k - c);
                        if err < best.error as i32 {
                            best.error = err as u16;
                            best.lo = l as u8;
                            best.hi = h as u8;
                        }
                    }
                }
                t.mode0[c as usize][hp][lp] = best;
            }
        }
    }

    for c in 0..256i32 {
        for lp in 0..2usize {
            let mut best = EndpointErr {
                error: u16::MAX,
                lo: 0,
                hi: 0,
            };
            for l in 0..64u32 {
                let mut low = ((l << 1) | lp as u32) << 1;
                low |= low >> 7;
                for h in 0..64u32 {
                    let mut high = ((h << 1) | lp as u32) << 1;
                    high |= high >> 7;
                    let k =
                        ((low * (64 - G_WEIGHTS3[MODE1_IDX]) + high * G_WEIGHTS3[MODE1_IDX] + 32)
                            >> 6) as i32;
                    let err = (k - c) * (k - c);
                    if err < best.error as i32 {
                        best.error = err as u16;
                        best.lo = l as u8;
                        best.hi = h as u8;
                    }
                }
            }
            t.mode1[c as usize][lp] = best;
        }
    }

    for c in 0..256i32 {
        for hp in 0..2usize {
            for lp in 0..2usize {
                let mut best = EndpointErr {
                    error: u16::MAX,
                    lo: 0,
                    hi: 0,
                };
                for l in 0..128u32 {
                    let low = (l << 1) | lp as u32;
                    for h in 0..128u32 {
                        let high = (h << 1) | hp as u32;
                        let k = ((low * (64 - G_WEIGHTS4[MODE6_IDX])
                            + high * G_WEIGHTS4[MODE6_IDX]
                            + 32)
                            >> 6) as i32;
                        let err = (k - c) * (k - c);
                        if err < best.error as i32 {
                            best.error = err as u16;
                            best.lo = l as u8;
                            best.hi = h as u8;
                        }
                    }
                }
                t.mode6[c as usize][hp][lp] = best;
            }
        }
    }

    for c in 0..256i32 {
        let mut best = EndpointErr {
            error: u16::MAX,
            lo: 0,
            hi: 0,
        };
        for l in 0..128u32 {
            let mut low = l << 1;
            low |= low >> 7;
            for h in 0..128u32 {
                let mut high = h << 1;
                high |= high >> 7;
                let k = ((low * (64 - G_WEIGHTS2[MODE5_IDX]) + high * G_WEIGHTS2[MODE5_IDX] + 32)
                    >> 6) as i32;
                let err = (k - c) * (k - c);
                if err < best.error as i32 {
                    best.error = err as u16;
                    best.lo = l as u8;
                    best.hi = h as u8;
                }
            }
        }
        t.mode5[c as usize] = best.lo as u32 | ((best.hi as u32) << 8);
    }

    for c in 0..256i32 {
        let mut best = EndpointErr {
            error: u16::MAX,
            lo: 0,
            hi: 0,
        };
        for l in 0..32u32 {
            let mut low = l << 3;
            low |= low >> 5;
            for h in 0..32u32 {
                let mut high = h << 3;
                high |= high >> 5;
                let k = ((low * (64 - G_WEIGHTS3[MODE4_IDX3]) + high * G_WEIGHTS3[MODE4_IDX3] + 32)
                    >> 6) as i32;
                let err = (k - c) * (k - c);
                if err < best.error as i32 {
                    best.error = err as u16;
                    best.lo = l as u8;
                    best.hi = h as u8;
                }
            }
        }
        t.mode4_3[c as usize] = best.lo as u32 | ((best.hi as u32) << 8);
    }

    for c in 0..256i32 {
        let mut best = EndpointErr {
            error: u16::MAX,
            lo: 0,
            hi: 0,
        };
        for l in 0..32u32 {
            let mut low = l << 3;
            low |= low >> 5;
            for h in 0..32u32 {
                let mut high = h << 3;
                high |= high >> 5;
                let k = ((low * (64 - G_WEIGHTS2[MODE4_IDX2]) + high * G_WEIGHTS2[MODE4_IDX2] + 32)
                    >> 6) as i32;
                let err = (k - c) * (k - c);
                if err < best.error as i32 {
                    best.error = err as u16;
                    best.lo = l as u8;
                    best.hi = h as u8;
                }
            }
        }
        t.mode4_2[c as usize] = best.lo as u32 | ((best.hi as u32) << 8);
    }

    for c in 0..256i32 {
        for hp in 0..2usize {
            for lp in 0..2usize {
                let mut best = EndpointErr {
                    error: u16::MAX,
                    lo: 0,
                    hi: 0,
                };
                for l in 0..32u32 {
                    let mut low = ((l << 1) | lp as u32) << 2;
                    low |= low >> 6;
                    for h in 0..32u32 {
                        let mut high = ((h << 1) | hp as u32) << 2;
                        high |= high >> 6;
                        let k = ((low * (64 - G_WEIGHTS2[MODE7_IDX])
                            + high * G_WEIGHTS2[MODE7_IDX]
                            + 32)
                            >> 6) as i32;
                        let err = (k - c) * (k - c);
                        if err < best.error as i32 {
                            best.error = err as u16;
                            best.lo = l as u8;
                            best.hi = h as u8;
                        }
                    }
                }
                t.mode7[c as usize][hp][lp] = best;
            }
        }
    }
    t
}

use std::sync::OnceLock;
static OPT: OnceLock<Box<OptTables>> = OnceLock::new();
pub(super) fn opt() -> &'static OptTables {
    OPT.get_or_init(build_opt_tables)
}

#[cfg(target_arch = "x86_64")]
static HAS_AVX2: OnceLock<bool> = OnceLock::new();
#[cfg(target_arch = "x86_64")]
#[inline]
pub(super) fn has_avx2() -> bool {
    *HAS_AVX2.get_or_init(|| {
        if std::env::var_os("ABGEN_BC7_SCALAR").is_some() {
            return false;
        }
        std::is_x86_feature_detected!("avx2")
    })
}
#[cfg(not(target_arch = "x86_64"))]
#[inline]
pub(super) fn has_avx2() -> bool {
    false
}

#[cfg(target_arch = "x86_64")]
static HAS_AVX512VL: OnceLock<bool> = OnceLock::new();
#[cfg(target_arch = "x86_64")]
#[inline]
pub(super) fn has_avx512vl() -> bool {
    *HAS_AVX512VL.get_or_init(|| {
        if std::env::var_os("ABGEN_BC7_SCALAR").is_some()
            || std::env::var_os("ABGEN_BC7_NO512").is_some()
        {
            return false;
        }
        std::is_x86_feature_detected!("avx512f") && std::is_x86_feature_detected!("avx512vl")
    })
}
#[cfg(not(target_arch = "x86_64"))]
#[inline]
pub(super) fn has_avx512vl() -> bool {
    false
}

#[cfg(test)]
mod opt_table_tests {
    use super::*;

    #[test]
    fn fast_opt_tables_match_reference() {
        let fast = build_opt_tables();
        let reference = build_opt_tables_reference();
        assert!(fast.mode0 == reference.mode0, "mode0 mismatch");
        assert!(fast.mode1 == reference.mode1, "mode1 mismatch");
        assert!(fast.mode6 == reference.mode6, "mode6 mismatch");
        assert!(fast.mode7 == reference.mode7, "mode7 mismatch");
        assert!(fast.mode5 == reference.mode5, "mode5 mismatch");
        assert!(fast.mode4_3 == reference.mode4_3, "mode4_3 mismatch");
        assert!(fast.mode4_2 == reference.mode4_2, "mode4_2 mismatch");
    }
}
