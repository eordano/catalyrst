use super::*;

pub fn tex_geometry(width: u32, height: u32, mip_count: i32) -> (u64, u64) {
    let mut cw = width as usize;
    let mut ch = height as usize;
    let mut pyr_px = 0u64;
    let mut nb = 0u64;
    for _ in 0..mip_count {
        pyr_px += (cw * ch) as u64;
        let (bw, bh) = level_block_dims(cw, ch);
        nb += (bw * bh) as u64;
        let (nw, nh) = box_halve_dims(cw, ch);
        cw = nw;
        ch = nh;
    }
    let base_bytes = (width as u64) * (height as u64) * 4;
    (base_bytes + pyr_px * 16 + nb * 64 + nb * 16 + 256, nb)
}

fn flip_rgba(rgba: &[u8], width: u32, height: u32) -> Vec<u8> {
    let w = width as usize;
    let h = height as usize;
    let mut flipped = vec![0u8; w * h * 4];
    for y in 0..h {
        let src = &rgba[(h - 1 - y) * w * 4..(h - 1 - y) * w * 4 + w * 4];
        flipped[y * w * 4..y * w * 4 + w * 4].copy_from_slice(src);
    }
    flipped
}

type MipJob = (
    BlockifyTex,
    std::sync::mpsc::SyncSender<Result<Vec<u8>, String>>,
);

type PendingReplies = (
    Vec<std::sync::mpsc::SyncSender<Result<Vec<u8>, String>>>,
    usize,
    u64,
    std::time::Instant,
);

fn mip_worker() -> Result<&'static std::sync::mpsc::SyncSender<MipJob>, String> {
    static W: OnceLock<Result<std::sync::mpsc::SyncSender<MipJob>, String>> = OnceLock::new();
    W.get_or_init(|| {
        let (tx, rx) = std::sync::mpsc::sync_channel::<MipJob>(64);
        let (itx, irx) = std::sync::mpsc::sync_channel::<Result<(), String>>(1);
        let spawned = std::thread::Builder::new()
            .name("abgen-gpu-gpu".into())
            .spawn(move || {
                match gpu() {
                    Ok(g) => {
                        if let Err(e) = unsafe { g.check((g.ctx_set_current)(g.ctx)) } {
                            let _ = itx.send(Err(format!("cuCtxSetCurrent: {e:#}")));
                            return;
                        }
                    }
                    Err(e) => {
                        let _ = itx.send(Err(format!("{e:#}")));
                        return;
                    }
                }
                let params = [
                    Params::slow(false),
                    Params::slow(true),
                    Params::basic(false),
                    Params::basic(true),
                ];
                let tables = build_opt_tables();
                let mut engine = match SlabEngine::new(&params, &tables) {
                    Ok(e) => {
                        let _ = itx.send(Ok(()));
                        e
                    }
                    Err(e) => {
                        let _ = itx.send(Err(format!("{e:#}")));
                        return;
                    }
                };
                let log = std::env::var("ABGEN_GPU_LOG").is_ok();
                let send_outs =
                    |reply: Vec<std::sync::mpsc::SyncSender<Result<Vec<u8>, String>>>,
                     res: Result<Option<Vec<Vec<u8>>>>,
                     n: usize,
                     nb: u64,
                     t0: std::time::Instant| {
                        match res {
                            Ok(Some(outs)) if outs.len() == reply.len() => {
                                for (r, o) in reply.into_iter().zip(outs) {
                                    let _ = r.send(Ok(o));
                                }
                                if log {
                                    eprintln!(
                                        "gpu-batch: n={} blocks={} ms={:.1} ok=true",
                                        n,
                                        nb,
                                        t0.elapsed().as_secs_f64() * 1e3
                                    );
                                }
                            }
                            other => {
                                let msg = match other {
                                    Err(e) => format!("{e:#}"),
                                    Ok(o) => format!(
                                        "gpu pipeline returned {} outputs for {} texs",
                                        o.map(|v| v.len()).unwrap_or(0),
                                        reply.len()
                                    ),
                                };
                                for r in reply {
                                    let _ = r.send(Err(msg.clone()));
                                }
                                if log {
                                    eprintln!(
                                        "gpu-batch: n={} blocks={} ms={:.1} ok=false",
                                        n,
                                        nb,
                                        t0.elapsed().as_secs_f64() * 1e3
                                    );
                                }
                            }
                        }
                    };
                let mut last_done: Option<std::time::Instant> = None;
                let mut pending: Option<PendingReplies> = None;
                loop {
                    let first = if pending.is_some() {
                        rx.try_recv().ok()
                    } else {
                        match rx.recv() {
                            Ok(j) => Some(j),
                            Err(_) => break,
                        }
                    };
                    let (first_tex, first_reply) = match first {
                        Some(j) => j,
                        None => {
                            if let Some((prep, pn, pnb, pt0)) = pending.take() {
                                let res = engine.complete_texs();
                                send_outs(prep, res, pn, pnb, pt0);
                                last_done = Some(std::time::Instant::now());
                            }
                            continue;
                        }
                    };
                    let gap_ms = last_done.map(|t| t.elapsed().as_secs_f64() * 1e3);
                    let t_drain = std::time::Instant::now();
                    let mut texs = vec![first_tex];
                    let mut reply = vec![first_reply];
                    let mut dev = tex_geometry(texs[0].w, texs[0].h, texs[0].mip_count).0;
                    let mut drain_err: Option<anyhow::Error> = None;
                    while texs.len() < 128 && dev < batch_dev_cap() {
                        match rx.try_recv() {
                            Ok((t, r)) => {
                                dev += tex_geometry(t.w, t.h, t.mip_count).0;
                                texs.push(t);
                                reply.push(r);
                            }
                            Err(_) => {
                                let linger_rem = pending.as_ref().map(|(_, _, pnb, pt0)| {
                                    (8.0 + *pnb as f64 / 25_000.0).min(120.0)
                                        - pt0.elapsed().as_secs_f64() * 1e3
                                });
                                match linger_rem {
                                    Some(rem) if rem > 0.0 && texs.len() < 32 => {
                                        if let Err(e) = engine.finalize_pending() {
                                            drain_err = Some(e);
                                            break;
                                        }
                                        match rx.recv_timeout(
                                            std::time::Duration::from_millis(1),
                                        ) {
                                            Ok((t, r)) => {
                                                dev +=
                                                    tex_geometry(t.w, t.h, t.mip_count).0;
                                                texs.push(t);
                                                reply.push(r);
                                            }
                                            Err(
                                                std::sync::mpsc::RecvTimeoutError::Timeout,
                                            ) => {}
                                            Err(_) => break,
                                        }
                                    }
                                    _ => match engine.finalize_pending() {
                                        Ok(true) => continue,
                                        Ok(false) => break,
                                        Err(e) => {
                                            drain_err = Some(e);
                                            break;
                                        }
                                    },
                                }
                            }
                        }
                    }
                    let drain_ms = t_drain.elapsed().as_secs_f64() * 1e3;
                    let t0 = std::time::Instant::now();
                    let nb: u64 = texs.iter().map(|t| tex_geometry(t.w, t.h, t.mip_count).1).sum();
                    let n = texs.len();
                    let sr = match drain_err {
                        Some(e) => Err(e),
                        None => engine.submit_texs(texs, 64_000_000),
                    };
                    if log {
                        eprintln!(
                            "gpu-submit: n={} blocks={} ms={:.2} gap={:.1} drain={:.2} pipelined={} ok={}",
                            n,
                            nb,
                            t0.elapsed().as_secs_f64() * 1e3,
                            gap_ms.unwrap_or(0.0),
                            drain_ms,
                            pending.is_some() as u32,
                            sr.is_ok()
                        );
                    }
                    match sr {
                        Ok(()) => {
                            if let Some((prep, pn, pnb, pt0)) = pending.take() {
                                let res = engine.complete_texs();
                                send_outs(prep, res, pn, pnb, pt0);
                                last_done = Some(std::time::Instant::now());
                            }
                            pending = Some((reply, n, nb, t0));
                        }
                        Err(e) => {
                            let msg = format!("{e:#}");
                            if let Some((prep, _, _, _)) = pending.take() {
                                for r in prep {
                                    let _ = r.send(Err(msg.clone()));
                                }
                            }
                            for r in reply {
                                let _ = r.send(Err(msg.clone()));
                            }
                            last_done = Some(std::time::Instant::now());
                        }
                    }
                }
                if let Some((prep, pn, pnb, pt0)) = pending.take() {
                    let res = engine.complete_texs();
                    send_outs(prep, res, pn, pnb, pt0);
                }
            });
        if spawned.is_err() {
            return Err("failed to spawn abgen-gpu-gpu worker thread".to_string());
        }
        match irx.recv() {
            Ok(Ok(())) => Ok(tx),
            Ok(Err(e)) => Err(e),
            Err(_) => Err("abgen-gpu-gpu worker exited during init".to_string()),
        }
    });
    match W.get().unwrap() {
        Ok(tx) => Ok(tx),
        Err(e) => Err(e.clone()),
    }
}

pub(crate) fn gpu_ready() -> Result<(), String> {
    mip_worker().map(|_| ())
}

pub(crate) fn encode_bc7_mip_chain_gpu(
    rgba: &[u8],
    width: u32,
    height: u32,
    mip_count: Option<i32>,
    flip: bool,
    srgb: bool,
    perceptual: bool,
    profile: Bc7Profile,
) -> Result<(Vec<u8>, i32)> {
    let w = width as usize;
    let h = height as usize;
    ensure!(
        rgba.len() == w * h * 4,
        "rgba len {} != {}x{}x4",
        rgba.len(),
        w,
        h
    );
    let tx = mip_worker().map_err(|e| anyhow!("gpu unavailable: {e}"))?;
    let mips = mip_count.unwrap_or_else(|| compute_default_mip_count(width, height));
    let data = if flip {
        flip_rgba(rgba, width, height)
    } else {
        rgba.to_vec()
    };
    let bucket = match (profile, perceptual) {
        (Bc7Profile::Slow, false) => 0,
        (Bc7Profile::Slow, true) => 1,
        (Bc7Profile::Basic, false) => 2,
        (Bc7Profile::Basic, true) => 3,
    };
    let t_rt = std::time::Instant::now();
    let (rtx, rrx) = std::sync::mpsc::sync_channel(1);
    tx.send((
        BlockifyTex {
            rgba: data,
            w: width,
            h: height,
            mip_count: mips,
            srgb,
            bucket,
        },
        rtx,
    ))
    .map_err(|_| anyhow!("abgen-gpu-gpu worker is gone"))?;
    let out = rrx
        .recv()
        .map_err(|_| anyhow!("abgen-gpu-gpu worker died mid-request"))?
        .map_err(|e| anyhow!("{e}"))?;
    if std::env::var("ABGEN_GPU_LOG").is_ok() {
        static T0: OnceLock<std::time::Instant> = OnceLock::new();
        let t0 = *T0.get_or_init(std::time::Instant::now);
        eprintln!(
            "gpu-rt: px={} ms={:.1} at={:.2}",
            w * h,
            t_rt.elapsed().as_secs_f64() * 1e3,
            t0.elapsed().as_secs_f64()
        );
    }
    Ok((out, mips))
}
