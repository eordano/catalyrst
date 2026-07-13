use crate::gpu;
use crate::gpuhost::corpus;
use crate::gpuhost::oracle;

use anyhow::{anyhow, bail, Result};
use std::fmt::Write as _;

const USAGE: &str = "usage: abgen-verify gpu <command> [flags]\n  diff   [--blocks N] [--seed S] [--scalar-oracle] [--gpu]   (defaults: N=8192 S=1)\n  bench  [--blocks N] [--seed S] [--gpu]                     (defaults: N=65536 S=1)\n  corpus --entities <file> [--limit N] [--slab-gb G] [--jobs J] [--cpu] [--store <dir>]\n         (defaults: G=20.0 J=available-parallelism store=./contents)";

pub fn run(args: &[String]) -> Result<i32> {
    let cmd = args.first().ok_or_else(|| {
        eprintln!("{USAGE}");
        anyhow!("missing command")
    })?;
    match cmd.as_str() {
        "diff" => {
            let (blocks, seed, scalar, use_gpu) = parse_flags(&args[1..], 8192, true)?;
            Ok(cmd_diff(blocks, seed, scalar, use_gpu))
        }
        "bench" => {
            let (blocks, seed, _, use_gpu) = parse_flags(&args[1..], 65536, false)?;
            cmd_bench(blocks, seed, use_gpu);
            Ok(0)
        }
        "corpus" => corpus::cmd_corpus(&args[1..]),
        "probe" => {
            gpu::cmd_probe();
            Ok(0)
        }
        other => bail!("unknown command: {other}"),
    }
}

fn parse_flags(
    args: &[String],
    default_blocks: usize,
    allow_scalar: bool,
) -> Result<(usize, u64, bool, bool)> {
    let mut blocks = default_blocks;
    let mut seed = 1u64;
    let mut scalar = false;
    let mut use_gpu = false;
    let mut i = 0usize;
    while i < args.len() {
        match args[i].as_str() {
            "--blocks" => {
                i += 1;
                let v = args
                    .get(i)
                    .ok_or_else(|| anyhow!("--blocks needs a value"))?;
                blocks = v.parse().map_err(|_| anyhow!("bad --blocks value: {v}"))?;
            }
            "--seed" => {
                i += 1;
                let v = args.get(i).ok_or_else(|| anyhow!("--seed needs a value"))?;
                seed = v.parse().map_err(|_| anyhow!("bad --seed value: {v}"))?;
            }
            "--scalar-oracle" if allow_scalar => {
                scalar = true;
            }
            "--gpu" => {
                use_gpu = true;
            }
            other => bail!("unknown flag: {other}"),
        }
        i += 1;
    }
    Ok((blocks, seed, scalar, use_gpu))
}

fn cmd_diff(num_blocks: usize, seed: u64, scalar_oracle: bool, use_gpu: bool) -> i32 {
    if scalar_oracle {
        oracle::set_oracle_scalar(true);
    }
    println!("diff: blocks={num_blocks} seed={seed} scalar_oracle={scalar_oracle}");
    let tables = crate::gpu::corelib::bc7::build_opt_tables();
    let mut results: Vec<(String, bool)> = Vec::new();

    let blocks = oracle::gen_blocks(seed, num_blocks);
    for profile in [
        crate::gpu::corelib::bc7::Bc7Profile::Slow,
        crate::gpu::corelib::bc7::Bc7Profile::Basic,
    ] {
        for perceptual in [false, true] {
            let name = format!("bc7-blocks profile={profile:?} perceptual={perceptual}");
            let params = match profile {
                crate::gpu::corelib::bc7::Bc7Profile::Slow => {
                    crate::gpu::corelib::bc7::Params::slow(perceptual)
                }
                crate::gpu::corelib::bc7::Bc7Profile::Basic => {
                    crate::gpu::corelib::bc7::Params::basic(perceptual)
                }
            };
            let ours =
                crate::gpu::corelib::bc7::encode_blocks(&blocks, num_blocks, &params, &tables);
            let oracle_profile = match profile {
                crate::gpu::corelib::bc7::Bc7Profile::Slow => crate::bc7_pure::Bc7Profile::Slow,
                crate::gpu::corelib::bc7::Bc7Profile::Basic => crate::bc7_pure::Bc7Profile::Basic,
            };
            let theirs = oracle::oracle_bc7(&blocks, num_blocks, oracle_profile, perceptual);
            let pass = compare(&name, &ours, &theirs, 16, &|i| {
                let s = i * 64;
                blocks.get(s..s + 64).map(hex)
            });
            results.push((name, pass));
            if use_gpu {
                let name = format!("bc7-blocks-GPU profile={profile:?} perceptual={perceptual}");
                let pass = match gpu::encode_blocks_gpu(&blocks, num_blocks, &params, &tables) {
                    Ok(gout) => compare(&name, &gout, &theirs, 16, &|i| {
                        let s = i * 64;
                        blocks.get(s..s + 64).map(hex)
                    }),
                    Err(e) => {
                        println!("MISMATCH {name}: {e:#}");
                        false
                    }
                };
                results.push((name, pass));
            }
        }
    }

    let sizes: [(u32, u32); 3] = [(64, 64), (128, 32), (37, 53)];

    for &(w, h) in &sizes {
        let tex = oracle::gen_texture(seed, w, h);
        for srgb in [false, true] {
            let name = format!("bc7-mip {w}x{h} srgb={srgb}");
            let (ours, our_mips) = crate::gpu::corelib::mips::encode_rgba32_mip_chain(
                &tex, w, h, None, false, srgb, &tables,
            );
            let (theirs, their_mips) = oracle::oracle_bc7_mip_chain(&tex, w, h, None, false, srgb);
            let mut pass = compare(&name, &ours, &theirs, 4, &|i| {
                if i < (w as usize) * (h as usize) {
                    tex.get(i * 4..i * 4 + 4).map(hex)
                } else {
                    None
                }
            });
            if our_mips != their_mips {
                println!("MISMATCH {name} mip_count ours={our_mips} oracle={their_mips}");
                pass = false;
            }
            results.push((name, pass));
        }
    }

    if use_gpu {
        for &(w, h) in &sizes {
            let tex = oracle::gen_texture(seed, w, h);
            for srgb in [false, true] {
                for perceptual in [false, true] {
                    for profile in [
                        crate::gpu::corelib::bc7::Bc7Profile::Slow,
                        crate::gpu::corelib::bc7::Bc7Profile::Basic,
                    ] {
                        let name = format!(
                            "bc7-mip-GPU {w}x{h} srgb={srgb} perceptual={perceptual} profile={profile:?}"
                        );
                        let oprof = match profile {
                            crate::gpu::corelib::bc7::Bc7Profile::Slow => {
                                crate::bc7_pure::Bc7Profile::Slow
                            }
                            crate::gpu::corelib::bc7::Bc7Profile::Basic => {
                                crate::bc7_pure::Bc7Profile::Basic
                            }
                        };
                        let (theirs, their_mips) =
                            crate::bc7_pure::encode_bc7_mip_chain_with_profile(
                                &tex, w, h, None, true, srgb, perceptual, oprof,
                            );
                        let pass = match gpu::encode_bc7_mip_chain_gpu(
                            &tex, w, h, None, true, srgb, perceptual, profile,
                        ) {
                            Ok((ours, our_mips)) => {
                                let mut p = compare(&name, &ours, &theirs, 16, &|_| None);
                                if our_mips != their_mips {
                                    println!(
                                        "MISMATCH {name} mip_count ours={our_mips} oracle={their_mips}"
                                    );
                                    p = false;
                                }
                                p
                            }
                            Err(e) => {
                                println!("MISMATCH {name}: {e:#}");
                                false
                            }
                        };
                        results.push((name, pass));
                    }
                }
            }
        }
    }

    for &(w, h) in &sizes {
        let tex = oracle::gen_texture(seed, w, h);
        for srgb in [false, true] {
            let name = format!("dxt1-mip {w}x{h} srgb={srgb}");
            let (ours, our_mips) =
                crate::gpu::corelib::dxt1::encode_dxt1_mip_chain(&tex, w, h, None, false, srgb);
            let (theirs, their_mips) = oracle::oracle_dxt1_mip_chain(&tex, w, h, None, false, srgb);
            let mut pass = compare(&name, &ours, &theirs, 8, &|i| {
                mip0_block_input(&tex, w, h, i)
            });
            if our_mips != their_mips {
                println!("MISMATCH {name} mip_count ours={our_mips} oracle={their_mips}");
                pass = false;
            }
            results.push((name, pass));
        }
    }

    for &(w, h) in &sizes {
        let tex = oracle::gen_texture(seed, w, h);
        let name = format!("bc5-mip {w}x{h}");
        let (ours, our_mips) =
            crate::gpu::corelib::bc5::encode_bc5_mip_chain(&tex, w, h, None, false);
        let (theirs, their_mips) = oracle::oracle_bc5_mip_chain(&tex, w, h, None, false);
        let mut pass = compare(&name, &ours, &theirs, 16, &|i| {
            mip0_block_input(&tex, w, h, i)
        });
        if our_mips != their_mips {
            println!("MISMATCH {name} mip_count ours={our_mips} oracle={their_mips}");
            pass = false;
        }
        results.push((name, pass));
    }

    println!();
    println!("== results ==");
    let mut all = true;
    for (name, pass) in &results {
        println!("{}  {}", if *pass { "PASS" } else { "FAIL" }, name);
        all &= *pass;
    }
    if all {
        0
    } else {
        1
    }
}

fn cmd_bench(num_blocks: usize, seed: u64, use_gpu: bool) {
    let tables = crate::gpu::corelib::bc7::build_opt_tables();
    let params = crate::gpu::corelib::bc7::Params::slow(true);
    let blocks = oracle::gen_blocks(seed, num_blocks);
    if use_gpu {
        let warm = oracle::gen_blocks(seed, 1024);
        if let Err(e) = gpu::encode_blocks_gpu(&warm, 1024, &params, &tables) {
            println!("bench: gpu unavailable: {e:#}");
            return;
        }
        let start = std::time::Instant::now();
        let out = gpu::encode_blocks_gpu(&blocks, num_blocks, &params, &tables).unwrap();
        let elapsed = start.elapsed();
        std::hint::black_box(&out);
        let secs = elapsed.as_secs_f64();
        let rate = if secs > 0.0 {
            num_blocks as f64 / secs
        } else {
            0.0
        };
        println!(
            "bench: bc7-GPU profile=Slow perceptual=true blocks={num_blocks} out_bytes={} time_s={secs:.3} blocks/s={rate:.0}",
            out.len()
        );
        return;
    }
    let start = std::time::Instant::now();
    let out = crate::gpu::corelib::bc7::encode_blocks(&blocks, num_blocks, &params, &tables);
    let elapsed = start.elapsed();
    std::hint::black_box(&out);
    let secs = elapsed.as_secs_f64();
    let rate = if secs > 0.0 {
        num_blocks as f64 / secs
    } else {
        0.0
    };
    let per = if num_blocks > 0 {
        elapsed.as_nanos() as f64 / num_blocks as f64
    } else {
        0.0
    };
    println!(
        "bench: bc7 profile=Slow perceptual=true blocks={num_blocks} out_bytes={} time_s={secs:.3} blocks/s={rate:.0} ns/block={per:.1}",
        out.len()
    );
}

fn compare(
    name: &str,
    ours: &[u8],
    theirs: &[u8],
    chunk: usize,
    input_hex: &dyn Fn(usize) -> Option<String>,
) -> bool {
    if ours == theirs {
        return true;
    }
    println!("MISMATCH {name}");
    if ours.len() != theirs.len() {
        println!("  length ours={} oracle={}", ours.len(), theirs.len());
    }
    let n = ours.len().min(theirs.len()) / chunk;
    let mut shown = 0usize;
    let mut total = 0usize;
    for i in 0..n {
        let a = &ours[i * chunk..(i + 1) * chunk];
        let b = &theirs[i * chunk..(i + 1) * chunk];
        if a != b {
            total += 1;
            if shown < 3 {
                println!("  block {i}");
                match input_hex(i) {
                    Some(s) => println!("    input:  {s}"),
                    None => println!("    input:  (derived mip data)"),
                }
                println!("    ours:   {}", hex(a));
                println!("    oracle: {}", hex(b));
                shown += 1;
            }
        }
    }
    println!("  divergent blocks: {total} of {n}");
    false
}

fn mip0_block_input(tex: &[u8], w: u32, h: u32, i: usize) -> Option<String> {
    let w = w as usize;
    let h = h as usize;
    let bw = w.div_ceil(4);
    let bh = h.div_ceil(4);
    if i >= bw * bh {
        return None;
    }
    let bx = i % bw;
    let by = i / bw;
    let mut buf = [0u8; 64];
    for dy in 0..4 {
        for dx in 0..4 {
            let sx = (bx * 4 + dx) % w;
            let sy = (by * 4 + dy) % h;
            let s = (sy * w + sx) * 4;
            let d = (dy * 4 + dx) * 4;
            buf[d..d + 4].copy_from_slice(&tex[s..s + 4]);
        }
    }
    Some(hex(&buf))
}

fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(s, "{b:02x}");
    }
    s
}
