use super::*;

fn layout_slab(texs: &[BlockifyTex]) -> Result<SlabLayout> {
    let mut level_dims: Vec<Vec<(usize, usize)>> = Vec::with_capacity(texs.len());
    let mut bucket_nb = [0u64; 4];
    let mut max_levels = 0usize;
    for t in texs {
        ensure!(t.bucket < 4, "bad bucket {}", t.bucket);
        ensure!(t.mip_count > 0, "bad mip_count {}", t.mip_count);
        ensure!(
            t.rgba.len() == t.w as usize * t.h as usize * 4,
            "texture bytes {} != {}x{}x4",
            t.rgba.len(),
            t.w,
            t.h
        );
        let mut dims = Vec::with_capacity(t.mip_count as usize);
        let (mut w, mut h) = (t.w as usize, t.h as usize);
        for _ in 0..t.mip_count {
            dims.push((w, h));
            let (bw, bh) = level_block_dims(w, h);
            bucket_nb[t.bucket] += (bw * bh) as u64;
            let (nw, nh) = box_halve_dims(w, h);
            w = nw;
            h = nh;
        }
        max_levels = max_levels.max(dims.len());
        level_dims.push(dims);
    }
    let mut bucket_base = [0u64; 4];
    let mut acc = 0u64;
    for b in 0..4 {
        bucket_base[b] = acc;
        acc += bucket_nb[b];
    }

    let mut lay = SlabLayout {
        lin_items: Vec::with_capacity(texs.len()),
        lin_prefix: vec![0u64],
        pack_items: vec![Vec::new(); max_levels],
        pack_prefix: vec![vec![0u64]; max_levels],
        halve_items: vec![Vec::new(); max_levels.saturating_sub(1)],
        halve_prefix: vec![vec![0u64]; max_levels.saturating_sub(1)],
        base_px_total: 0,
        pyr_px_total: 0,
        bucket_nb,
        bucket_base,
        total_blocks: acc,
    };
    let mut bucket_cursor = bucket_base;
    for (ti, t) in texs.iter().enumerate() {
        let dims = &level_dims[ti];
        let base_px = t.w as u64 * t.h as u64;
        lay.lin_items.push(LinItem {
            base_px: lay.base_px_total,
            pyr_px: lay.pyr_px_total,
            srgb: t.srgb as u32,
            pad: 0,
        });
        lay.lin_prefix
            .push(lay.lin_prefix.last().unwrap() + base_px);
        let mut lvl_px_off: Vec<u64> = Vec::with_capacity(dims.len());
        for (li, &(w, h)) in dims.iter().enumerate() {
            lvl_px_off.push(lay.pyr_px_total);
            let (bw, bh) = level_block_dims(w, h);
            let nb = (bw * bh) as u64;
            lay.pack_items[li].push(PackItem {
                lvl_px: lay.pyr_px_total,
                blk_off: bucket_cursor[t.bucket],
                w: w as u32,
                h: h as u32,
                srgb: t.srgb as u32,
                pad: 0,
            });
            let last = *lay.pack_prefix[li].last().unwrap();
            lay.pack_prefix[li].push(last + nb);
            bucket_cursor[t.bucket] += nb;
            lay.pyr_px_total += (w * h) as u64;
        }
        for li in 0..dims.len().saturating_sub(1) {
            let (w, h) = dims[li];
            lay.halve_items[li].push(HalveItem {
                src_px: lvl_px_off[li],
                dst_px: lvl_px_off[li + 1],
                w: w as u32,
                h: h as u32,
            });
            let (nw, nh) = dims[li + 1];
            let last = *lay.halve_prefix[li].last().unwrap();
            lay.halve_prefix[li].push(last + (nw * nh) as u64);
        }
        lay.base_px_total += base_px;
    }
    Ok(lay)
}

impl SlabEngine {
    pub fn new(params: &[Params; 4], tables: &OptTables) -> Result<SlabEngine> {
        let g = gpu()?;
        ensure!(
            g.has_blockify,
            "PTX at ABGEN_GPU_PTX has no blockify kernels (rebuild kernel-ptx)"
        );
        unsafe {
            let mut compute: *mut c_void = std::ptr::null_mut();
            let mut copy: *mut c_void = std::ptr::null_mut();
            let mut upstream: *mut c_void = std::ptr::null_mut();
            g.check((g.stream_create)(&mut compute, 0))?;
            g.check((g.stream_create)(&mut copy, 0))?;
            g.check((g.stream_create)(&mut upstream, 0))?;
            let mut evs = [std::ptr::null_mut::<c_void>(); 22];
            for e in evs.iter_mut() {
                g.check((g.event_create)(e, 0))?;
            }
            let params4_bytes = std::slice::from_raw_parts(
                params.as_ptr().cast::<u8>(),
                std::mem::size_of::<Params>() * 4,
            );
            let d_params4 = g.alloc_upload(params4_bytes)?;
            let mut d_params = [0u64; 4];
            for (i, p) in params.iter().enumerate() {
                let bytes = std::slice::from_raw_parts(
                    (p as *const Params).cast::<u8>(),
                    std::mem::size_of::<Params>(),
                );
                d_params[i] = g.alloc_upload(bytes)?;
            }
            let tables_bytes = std::slice::from_raw_parts(
                (tables as *const OptTables).cast::<u8>(),
                std::mem::size_of::<OptTables>(),
            );
            let d_tables = g.alloc_upload(tables_bytes)?;
            Ok(SlabEngine {
                g,
                compute,
                copy,
                upstream,
                d_all: DevArena::new(),
                d_out: [DevArena::new(), DevArena::new()],
                d_sig: [DevArena::new(), DevArena::new()],
                d_perm: [DevArena::new(), DevArena::new()],
                d_desc: [DevArena::new(), DevArena::new()],
                d_out_all: [DevArena::new(), DevArena::new()],
                h_base: [PinnedBuf::new(), PinnedBuf::new()],
                h_meta: [PinnedBuf::new(), PinnedBuf::new()],
                h_out: [PinnedBuf::new(), PinnedBuf::new()],
                h_sig: [PinnedBuf::new(), PinnedBuf::new()],
                h_perm: [PinnedBuf::new(), PinnedBuf::new()],
                h_out_all: [PinnedBuf::new(), PinnedBuf::new()],
                h_desc: [PinnedBuf::new(), PinnedBuf::new()],
                ev_kdone: [evs[0], evs[1]],
                ev_out: [evs[2], evs[3]],
                ev_b0: evs[4],
                ev_b1: evs[5],
                ev_e0: evs[6],
                ev_e1: evs[7],
                ev_u0: [evs[8], evs[9]],
                ev_u1: [evs[10], evs[11]],
                ev_sig: [evs[12], evs[21]],
                ev_d0: [evs[13], evs[14]],
                ev_d1: [evs[15], evs[16]],
                ev_db0: [evs[17], evs[18]],
                ev_db1: [evs[19], evs[20]],
                d_params,
                d_params4,
                d_tables,
                slab_ctr: 0,
                parity: 0,
                uploaded: [false, false],
                inflight: None,
                collect: false,
                collected: Vec::new(),
                launch_plan: None,
                desc_pending: VecDeque::new(),
                desc_done: VecDeque::new(),
                desc_texmeta: None,
            })
        }
    }

    unsafe fn drain_inflight(&mut self) -> Result<Option<BlockifyStats>> {
        let mut inf = match self.inflight.take() {
            None => return Ok(None),
            Some(i) => i,
        };
        let g = self.g;
        let t_drain = std::time::Instant::now();
        for k in 0..2 {
            let slot = (inf.launch_idx + k) % 2;
            if inf.pending[slot] {
                inf.pending[slot] = false;
                g.check((g.event_synchronize)(self.ev_out[slot]))?;
                let first = std::slice::from_raw_parts(self.h_out[slot].ptr, 8);
                inf.stats.fingerprint ^= u64::from_le_bytes(first.try_into().unwrap());
                inf.stats.launches += 1;
                if self.collect {
                    let full =
                        std::slice::from_raw_parts(self.h_out[slot].ptr, inf.pending_len[slot]);
                    self.collected.extend_from_slice(full);
                }
            }
        }
        g.check((g.stream_synchronize)(self.compute))?;
        g.check((g.stream_synchronize)(self.copy))?;
        let drain_ms = t_drain.elapsed().as_secs_f64() * 1e3;
        if inf.lin_total > 0 {
            let mut ms: f32 = 0.0;
            g.check((g.event_elapsed)(&mut ms, self.ev_b0, self.ev_b1))?;
            inf.stats.blockify_ns = (ms as f64 * 1e6) as u64;
        }
        if inf.launch_idx > 0 {
            let mut ms: f32 = 0.0;
            g.check((g.event_elapsed)(&mut ms, self.ev_e0, self.ev_e1))?;
            inf.stats.encode_ns = (ms as f64 * 1e6) as u64;
        }
        let mut upload_ms: f32 = 0.0;
        g.check((g.event_elapsed)(
            &mut upload_ms,
            self.ev_u0[inf.parity],
            self.ev_u1[inf.parity],
        ))?;
        eprintln!(
            "{{\"slab\":{},\"layout_ms\":{:.1},\"stage_ms\":{:.1},\"upload_ms\":{:.1},\"blockify_ms\":{:.1},\"encode_span_ms\":{:.1},\"drain_ms\":{:.1},\"span_ms\":{:.1},\"overlap\":{},\"base_mb\":{:.1},\"blocks\":{}}}",
            self.slab_ctr,
            inf.layout_ms,
            inf.stage_ms,
            upload_ms,
            inf.stats.blockify_ns as f64 / 1e6,
            inf.stats.encode_ns as f64 / 1e6,
            drain_ms,
            inf.t0.elapsed().as_secs_f64() * 1e3,
            inf.overlapped as u32,
            inf.base_bytes as f64 / 1e6,
            inf.stats.blocks_by_bucket.iter().sum::<u64>()
        );
        self.slab_ctr += 1;
        Ok(Some(inf.stats))
    }

    pub fn finish(&mut self) -> Result<Option<BlockifyStats>> {
        unsafe { self.drain_inflight() }
    }

    pub fn submit_slab(
        &mut self,
        texs: &[BlockifyTex],
        max_blocks_per_launch: usize,
    ) -> Result<Option<BlockifyStats>> {
        if texs.is_empty() {
            return Ok(None);
        }
        let g = self.g;
        let t0 = std::time::Instant::now();
        let t_layout = std::time::Instant::now();
        let lay = layout_slab(texs)?;
        let layout_ms = t_layout.elapsed().as_secs_f64() * 1e3;
        let mut stats = BlockifyStats {
            blocks_by_bucket: lay.bucket_nb,
            ..Default::default()
        };
        let block_dim: u32 = 256;

        unsafe {
            let mut secs: Vec<&[u8]> = Vec::new();
            secs.push(struct_slice_bytes(&lay.lin_items));
            secs.push(struct_slice_bytes(&lay.lin_prefix));
            for li in 0..lay.pack_items.len() {
                secs.push(struct_slice_bytes(&lay.pack_items[li]));
                secs.push(struct_slice_bytes(&lay.pack_prefix[li]));
            }
            for li in 0..lay.halve_items.len() {
                secs.push(struct_slice_bytes(&lay.halve_items[li]));
                secs.push(struct_slice_bytes(&lay.halve_prefix[li]));
            }
            let mut offs = Vec::with_capacity(secs.len());
            let mut meta_len = 0usize;
            for s in &secs {
                offs.push(meta_len);
                meta_len = (meta_len + s.len() + 7) & !7;
            }
            let meta_len = meta_len.max(8);
            let base_bytes = (lay.base_px_total * 4) as usize;
            let algn = |x: usize| (x + 255) & !255;
            let pyr_bytes = ((lay.pyr_px_total * 16) as usize).max(1);
            let blocks_bytes = ((lay.total_blocks * 64) as usize).max(1);
            let o_base = algn(meta_len);
            let o_pyr = algn(o_base + base_bytes);
            let o_blocks = algn(o_pyr + pyr_bytes);
            let need = o_blocks + blocks_bytes;

            let overlap_ok = match &self.inflight {
                Some(inf) => need <= self.d_all.cap && o_base + base_bytes <= inf.o_blocks,
                None => false,
            };
            let mut prev: Option<BlockifyStats> = None;
            if self.inflight.is_some() && !overlap_ok {
                prev = self.drain_inflight()?;
            }
            let desc_mode = self.desc_texmeta.is_some();
            while self.desc_pending.len() >= 2
                || self
                    .desc_pending
                    .iter()
                    .any(|pend| pend.slot == self.parity)
            {
                if let Some(outs) = self.complete_desc_pending()? {
                    self.desc_done.push_back(outs);
                }
            }
            let desc_overlap = need <= self.d_all.cap
                && self
                    .desc_pending
                    .iter()
                    .all(|pend| o_base + base_bytes <= pend.o_blocks);
            if !self.desc_pending.is_empty() && !desc_overlap {
                while let Some(outs) = self.complete_desc_pending()? {
                    self.desc_done.push_back(outs);
                }
            }

            let p = self.parity;
            let t_stage = std::time::Instant::now();
            let mut uwait_ms = 0.0f64;
            if self.uploaded[p] {
                let t_uw = std::time::Instant::now();
                g.check((g.event_synchronize)(self.ev_u1[p]))?;
                uwait_ms = t_uw.elapsed().as_secs_f64() * 1e3;
            }
            let hm = self.h_meta[p].ensure(g, meta_len, "h_meta")?;
            for (i, s) in secs.iter().enumerate() {
                std::ptr::copy_nonoverlapping(s.as_ptr(), hm.add(offs[i]), s.len());
            }
            let hb = self.h_base[p].ensure(g, base_bytes, "h_base")?;
            let mut off = 0usize;
            for t in texs {
                std::ptr::copy_nonoverlapping(t.rgba.as_ptr(), hb.add(off), t.rgba.len());
                off += t.rgba.len();
            }
            let stage_ms = t_stage.elapsed().as_secs_f64() * 1e3;

            let d_arena = self.d_all.ensure(g, need, "d_arena")?;
            let d_meta = d_arena;
            let d_base = d_arena + o_base as u64;
            let d_pyr = d_arena + o_pyr as u64;
            let d_blocks = d_arena + o_blocks as u64;

            let overlapped = self.inflight.is_some();
            if overlapped {
                g.check((g.stream_wait_event)(self.upstream, self.ev_b1, 0))?;
            }
            if let Some(pend) = self.desc_pending.back() {
                let e = self.ev_db1[pend.slot];
                g.check((g.stream_wait_event)(self.upstream, e, 0))?;
            }
            g.check((g.event_record)(self.ev_u0[p], self.upstream))?;
            g.check((g.memcpy_htod_async)(
                d_base,
                hb.cast(),
                base_bytes,
                self.upstream,
            ))?;
            g.check((g.memcpy_htod_async)(
                d_meta,
                hm.cast(),
                meta_len,
                self.upstream,
            ))?;
            g.check((g.event_record)(self.ev_u1[p], self.upstream))?;
            self.uploaded[p] = true;

            if self.inflight.is_some() {
                let d = self.drain_inflight()?;
                prev = prev.or(d);
            }

            if let Some(mut pend) = self.desc_pending.pop_back_if(|x| !x.finalized) {
                let r = self.finalize_desc(&mut pend);
                self.desc_pending.push_back(pend);
                r?;
            }
            ensure!(
                self.desc_pending.iter().all(|x| x.finalized),
                "unfinalized pending before compute enqueue"
            );

            g.check((g.stream_wait_event)(self.compute, self.ev_u1[p], 0))?;
            if desc_mode {
                g.check((g.event_record)(self.ev_db0[p], self.compute))?;
            } else {
                g.check((g.event_record)(self.ev_b0, self.compute))?;
            }
            let lin_total = *lay.lin_prefix.last().unwrap();
            if lin_total > 0 {
                let mut args = [
                    d_meta + offs[0] as u64,
                    d_meta + offs[1] as u64,
                    lay.lin_items.len() as u64,
                    lin_total,
                    d_base,
                    d_pyr,
                ];
                g.launch_u64s_on(
                    g.func_linearize,
                    (lin_total as usize).div_ceil(block_dim as usize) as u32,
                    block_dim,
                    self.compute,
                    &mut args,
                )?;
            }
            let npack = lay.pack_items.len();
            for li in 0..npack {
                let total = *lay.pack_prefix[li].last().unwrap();
                if total > 0 {
                    let mut args = [
                        d_meta + offs[2 + 2 * li] as u64,
                        d_meta + offs[3 + 2 * li] as u64,
                        lay.pack_items[li].len() as u64,
                        total,
                        d_pyr,
                        d_blocks,
                    ];
                    g.launch_u64s_on(
                        g.func_quantize_pack,
                        (total as usize).div_ceil(block_dim as usize) as u32,
                        block_dim,
                        self.compute,
                        &mut args,
                    )?;
                }
                if li < lay.halve_items.len() {
                    let total = *lay.halve_prefix[li].last().unwrap();
                    if total > 0 {
                        let mut args = [
                            d_meta + offs[2 + 2 * npack + 2 * li] as u64,
                            d_meta + offs[3 + 2 * npack + 2 * li] as u64,
                            lay.halve_items[li].len() as u64,
                            total,
                            d_pyr,
                        ];
                        g.launch_u64s_on(
                            g.func_halve,
                            (total as usize).div_ceil(block_dim as usize) as u32,
                            block_dim,
                            self.compute,
                            &mut args,
                        )?;
                    }
                }
            }
            if desc_mode {
                g.check((g.event_record)(self.ev_db1[p], self.compute))?;
            } else {
                g.check((g.event_record)(self.ev_b1, self.compute))?;
            }

            let bdim: u32 = encode_bdim();
            let binning =
                encode_binning() && !g.func_sigs.is_null() && !g.func_encode_perm.is_null();
            let mut launches: Vec<(u64, usize, usize)> = Vec::new();
            match &self.launch_plan {
                Some(plan) => {
                    launches.extend_from_slice(plan);
                }
                None => {
                    for b in 0..4 {
                        let nt = lay.bucket_nb[b] as usize;
                        let mut off_b = 0usize;
                        while off_b < nt {
                            let n = (nt - off_b).min(max_blocks_per_launch);
                            launches.push((lay.bucket_base[b] + off_b as u64, n, b));
                            off_b += n;
                        }
                    }
                }
            }
            if desc_mode {
                let (tex_bytes, total_bytes) = self.desc_texmeta.take().unwrap();
                let t_db = std::time::Instant::now();
                let mut ndescs = 0usize;
                for &(_, n, _) in &launches {
                    ndescs += n.div_ceil(GROUP_WIDTH);
                }
                let hd = self.h_desc[p].ensure(g, ndescs * 8, "h_desc")? as *mut u64;
                let mut di = 0usize;
                for &(start, n, bucket) in &launches {
                    let mut gs = 0usize;
                    while gs < n {
                        let nl = (n - gs).min(GROUP_WIDTH);
                        *hd.add(di) =
                            ((start + gs as u64) << 8) | ((bucket as u64) << 4) | nl as u64;
                        di += 1;
                        gs += nl;
                    }
                }
                let descbuild_ms = t_db.elapsed().as_secs_f64() * 1e3;
                let t_sub = std::time::Instant::now();
                let out_len = (lay.total_blocks as usize) * 16;
                self.d_out_all[p].ensure(g, out_len.max(1), "d_out_all")?;
                self.h_out_all[p].ensure(g, out_len.max(1), "h_out_all")?;
                let desc_binning = encode_binning() && !g.func_sigs_desc.is_null();
                let deferred = desc_binning && ndescs > 0;
                let mut sig_ms = 0.0f64;
                if ndescs > 0 {
                    let d_desc = self.d_desc[p].ensure(g, ndescs * 8, "d_desc")?;
                    g.check((g.memcpy_htod_async)(
                        d_desc,
                        hd.cast(),
                        ndescs * 8,
                        self.compute,
                    ))?;
                    if desc_binning {
                        let t_sig = std::time::Instant::now();
                        let d_sig = self.d_sig[p].ensure(g, ndescs, "d_sig")?;
                        let h_sig = self.h_sig[p].ensure(g, ndescs, "h_sig")?;
                        let mut args = [d_blocks, d_desc, ndescs as u64, d_sig];
                        g.launch_u64s_on(
                            g.func_sigs_desc,
                            (ndescs as u32).div_ceil(bdim),
                            bdim,
                            self.compute,
                            &mut args,
                        )?;
                        g.check((g.memcpy_dtoh_async)(
                            h_sig.cast(),
                            d_sig,
                            ndescs,
                            self.compute,
                        ))?;
                        g.check((g.event_record)(self.ev_sig[p], self.compute))?;
                        sig_ms = t_sig.elapsed().as_secs_f64() * 1e3;
                    }
                }
                if !deferred {
                    self.enqueue_encode_out(p, ndescs, o_blocks, out_len, bdim)?;
                }
                let submit_ms = t_sub.elapsed().as_secs_f64() * 1e3;
                ensure!(
                    self.desc_pending.iter().all(|x| x.finalized),
                    "unfinalized pending at desc push"
                );
                self.desc_pending.push_back(DescPending {
                    slot: p,
                    plan: launches,
                    tex_bytes,
                    total_bytes,
                    out_len,
                    o_blocks,
                    ntexs: texs.len(),
                    ndescs,
                    nblocks: lay.total_blocks,
                    base_bytes,
                    layout_ms,
                    stage_ms,
                    uwait_ms,
                    descbuild_ms,
                    sig_ms,
                    submit_ms,
                    t0,
                    finalized: !deferred,
                    bdim,
                });
                self.parity ^= 1;
                return Ok(prev);
            }
            let mut sig_offs: Vec<usize> = Vec::new();
            let mut h_sig_ptr: *mut u8 = std::ptr::null_mut();
            if binning {
                let mut so = 0usize;
                for &(_, n, _) in &launches {
                    sig_offs.push(so);
                    so += n.div_ceil(GROUP_WIDTH);
                }
                if so > 0 {
                    let d_sig = self.d_sig[p].ensure(g, so, "d_sig")?;
                    h_sig_ptr = self.h_sig[p].ensure(g, so, "h_sig")?;
                    for (ri, &(start, n, _)) in launches.iter().enumerate() {
                        let ng = n.div_ceil(GROUP_WIDTH) as u32;
                        let mut args =
                            [d_blocks + start * 64, n as u64, d_sig + sig_offs[ri] as u64];
                        g.launch_u64s_on(
                            g.func_sigs,
                            ng.div_ceil(bdim),
                            bdim,
                            self.compute,
                            &mut args,
                        )?;
                    }
                    g.check((g.memcpy_dtoh_async)(
                        h_sig_ptr.cast(),
                        d_sig,
                        so,
                        self.compute,
                    ))?;
                    g.check((g.event_record)(self.ev_sig[p], self.compute))?;
                }
            }

            let mut pending = [false; 2];
            let mut pending_len = [0usize; 2];
            let mut launch_idx = 0usize;
            let mut sig_synced = false;
            g.check((g.event_record)(self.ev_e0, self.compute))?;
            for &(start, n, b) in &launches {
                {
                    let slot = launch_idx % 2;
                    if pending[slot] {
                        pending[slot] = false;
                        g.check((g.event_synchronize)(self.ev_out[slot]))?;
                        let first = std::slice::from_raw_parts(self.h_out[slot].ptr, 8);
                        stats.fingerprint ^= u64::from_le_bytes(first.try_into().unwrap());
                        stats.launches += 1;
                        if self.collect {
                            let full =
                                std::slice::from_raw_parts(self.h_out[slot].ptr, pending_len[slot]);
                            self.collected.extend_from_slice(full);
                        }
                    }
                    let out_len = n * 16;
                    let d_o = self.d_out[slot].ensure(g, out_len, "d_out")?;
                    let h_o = self.h_out[slot].ensure(g, out_len, "h_out")?;
                    let d_ptr = d_blocks + start * 64;
                    let num_groups = n.div_ceil(GROUP_WIDTH) as u32;
                    if binning {
                        if !sig_synced {
                            g.check((g.event_synchronize)(self.ev_sig[p]))?;
                            sig_synced = true;
                        }
                        let ng = num_groups as usize;
                        let sigs =
                            std::slice::from_raw_parts(h_sig_ptr.add(sig_offs[launch_idx]), ng);
                        let hp = self.h_perm[slot].ensure(g, ng * 4, "h_perm")? as *mut u32;
                        sort_perm_into(sigs, std::slice::from_raw_parts_mut(hp, ng));
                        let d_pm = self.d_perm[slot].ensure(g, ng * 4, "d_perm")?;
                        g.check((g.memcpy_htod_async)(d_pm, hp.cast(), ng * 4, self.compute))?;
                        let mut args =
                            [d_ptr, n as u64, d_pm, self.d_params[b], self.d_tables, d_o];
                        g.launch_u64s_on(
                            g.func_encode_perm,
                            num_groups.div_ceil(bdim),
                            bdim,
                            self.compute,
                            &mut args,
                        )?;
                    } else {
                        let mut args = [d_ptr, n as u64, self.d_params[b], self.d_tables, d_o];
                        g.launch_u64s_on(
                            g.func_encode,
                            num_groups.div_ceil(bdim),
                            bdim,
                            self.compute,
                            &mut args,
                        )?;
                    }
                    g.check((g.event_record)(self.ev_kdone[slot], self.compute))?;
                    g.check((g.stream_wait_event)(self.copy, self.ev_kdone[slot], 0))?;
                    g.check((g.memcpy_dtoh_async)(h_o.cast(), d_o, out_len, self.copy))?;
                    g.check((g.event_record)(self.ev_out[slot], self.copy))?;
                    pending[slot] = true;
                    pending_len[slot] = out_len;
                    launch_idx += 1;
                }
            }
            g.check((g.event_record)(self.ev_e1, self.compute))?;

            self.inflight = Some(Inflight {
                stats,
                pending,
                pending_len,
                launch_idx,
                lin_total,
                parity: p,
                o_blocks,
                layout_ms,
                stage_ms,
                overlapped,
                base_bytes,
                t0,
            });
            self.parity ^= 1;
            Ok(prev)
        }
    }
}

fn tex_level_nbs(t: &BlockifyTex) -> Vec<usize> {
    let mut cw = t.w as usize;
    let mut ch = t.h as usize;
    let mut nbs = Vec::with_capacity(t.mip_count as usize);
    for _ in 0..t.mip_count {
        let (bw, bh) = level_block_dims(cw, ch);
        nbs.push(bw * bh);
        let (nw, nh) = box_halve_dims(cw, ch);
        cw = nw;
        ch = nh;
    }
    nbs
}

impl SlabEngine {
    pub fn submit_texs(
        &mut self,
        texs: Vec<BlockifyTex>,
        max_blocks_per_launch: usize,
    ) -> Result<()> {
        if texs.is_empty() {
            return Ok(());
        }
        if self.inflight.is_some() {
            unsafe { self.drain_inflight()? };
        }
        let levels: Vec<Vec<usize>> = texs.iter().map(tex_level_nbs).collect();
        let mut bucket_nb = [0u64; 4];
        for (t, nbs) in texs.iter().zip(&levels) {
            ensure!(t.bucket < 4, "bad bucket {}", t.bucket);
            for &nb in nbs {
                bucket_nb[t.bucket] += nb as u64;
            }
        }
        let mut bucket_base = [0u64; 4];
        let mut acc = 0u64;
        for b in 0..4 {
            bucket_base[b] = acc;
            acc += bucket_nb[b];
        }
        let mut cursor = bucket_base;
        let mut plan: Vec<(u64, usize, usize)> = Vec::new();
        let mut tex_bytes: Vec<usize> = Vec::with_capacity(texs.len());
        for (t, nbs) in texs.iter().zip(&levels) {
            let mut tb = 0usize;
            for &nb in nbs {
                let mut o = 0usize;
                while o < nb {
                    let ln = (nb - o).min(max_blocks_per_launch);
                    plan.push((cursor[t.bucket] + o as u64, ln, t.bucket));
                    o += ln;
                }
                cursor[t.bucket] += nb as u64;
                tb += nb * 16;
            }
            tex_bytes.push(tb);
        }
        let total_bytes: usize = tex_bytes.iter().sum();
        if !self.g.func_encode_desc.is_null() {
            self.launch_plan = Some(plan);
            self.desc_texmeta = Some((tex_bytes, total_bytes));
            let r = self.submit_slab(&texs, max_blocks_per_launch);
            self.launch_plan = None;
            self.desc_texmeta = None;
            if r.is_err() {
                self.desc_pending.clear();
                self.desc_done.clear();
                unsafe {
                    let _ = (self.g.ctx_synchronize)();
                }
            }
            r.map(|_| ())
        } else {
            self.launch_plan = Some(plan);
            self.collect = true;
            self.collected = Vec::with_capacity(total_bytes);
            let r = self
                .submit_slab(&texs, max_blocks_per_launch)
                .and_then(|_| self.finish());
            self.collect = false;
            self.launch_plan = None;
            let out = std::mem::take(&mut self.collected);
            r?;
            ensure!(
                out.len() == total_bytes,
                "collected {} bytes, expected {}",
                out.len(),
                total_bytes
            );
            let mut outs = Vec::with_capacity(tex_bytes.len());
            let mut off = 0usize;
            for tb in tex_bytes {
                outs.push(out[off..off + tb].to_vec());
                off += tb;
            }
            self.desc_done.push_back(outs);
            Ok(())
        }
    }

    pub fn complete_texs(&mut self) -> Result<Option<Vec<Vec<u8>>>> {
        if let Some(outs) = self.desc_done.pop_front() {
            return Ok(Some(outs));
        }
        unsafe { self.complete_desc_pending() }
    }

    unsafe fn enqueue_encode_out(
        &mut self,
        slot: usize,
        ndescs: usize,
        o_blocks: usize,
        out_len: usize,
        bdim: u32,
    ) -> Result<()> {
        let g = self.g;
        g.check((g.event_record)(self.ev_d0[slot], self.compute))?;
        if ndescs > 0 {
            let mut args = [
                self.d_all.ptr + o_blocks as u64,
                self.d_desc[slot].ptr,
                ndescs as u64,
                self.d_params4,
                self.d_tables,
                self.d_out_all[slot].ptr,
            ];
            g.launch_u64s_on(
                g.func_encode_desc,
                (ndescs as u32).div_ceil(bdim),
                bdim,
                self.compute,
                &mut args,
            )?;
            g.check((g.event_record)(self.ev_d1[slot], self.compute))?;
            g.check((g.memcpy_dtoh_async)(
                self.h_out_all[slot].ptr.cast(),
                self.d_out_all[slot].ptr,
                out_len,
                self.compute,
            ))?;
        } else {
            g.check((g.event_record)(self.ev_d1[slot], self.compute))?;
        }
        g.check((g.event_record)(self.ev_out[slot], self.compute))?;
        Ok(())
    }

    unsafe fn finalize_desc(&mut self, pend: &mut DescPending) -> Result<()> {
        if pend.finalized {
            return Ok(());
        }
        let g = self.g;
        let t = std::time::Instant::now();
        g.check((g.event_synchronize)(self.ev_sig[pend.slot]))?;
        let hd = self.h_desc[pend.slot].ptr as *mut u64;
        let sigs = std::slice::from_raw_parts(self.h_sig[pend.slot].ptr as *const u8, pend.ndescs);
        let unsorted = std::slice::from_raw_parts(hd as *const u64, pend.ndescs);
        let mut sorted = vec![0u64; pend.ndescs];
        sort_descs_by_sig(unsorted, sigs, &mut sorted);
        std::ptr::copy_nonoverlapping(sorted.as_ptr(), hd, pend.ndescs);
        g.check((g.memcpy_htod_async)(
            self.d_desc[pend.slot].ptr,
            hd.cast(),
            pend.ndescs * 8,
            self.compute,
        ))?;
        self.enqueue_encode_out(
            pend.slot,
            pend.ndescs,
            pend.o_blocks,
            pend.out_len,
            pend.bdim,
        )?;
        pend.sig_ms += t.elapsed().as_secs_f64() * 1e3;
        pend.finalized = true;
        Ok(())
    }

    pub fn finalize_pending(&mut self) -> Result<bool> {
        if self.desc_pending.back().is_none_or(|x| x.finalized) {
            return Ok(false);
        }
        let mut pend = self.desc_pending.pop_back().unwrap();
        let r = unsafe { self.finalize_desc(&mut pend) };
        self.desc_pending.push_back(pend);
        match r {
            Ok(()) => Ok(true),
            Err(e) => {
                self.desc_pending.clear();
                self.desc_done.clear();
                unsafe {
                    let _ = (self.g.ctx_synchronize)();
                }
                Err(e)
            }
        }
    }

    unsafe fn complete_desc_pending(&mut self) -> Result<Option<Vec<Vec<u8>>>> {
        let mut pend = match self.desc_pending.pop_front() {
            None => return Ok(None),
            Some(x) => x,
        };
        let g = self.g;
        if !pend.finalized {
            self.finalize_desc(&mut pend)?;
        }
        let t_wait = std::time::Instant::now();
        g.check((g.event_synchronize)(self.ev_out[pend.slot]))?;
        let wait_ms = t_wait.elapsed().as_secs_f64() * 1e3;
        let t_slice = std::time::Instant::now();
        let all =
            std::slice::from_raw_parts(self.h_out_all[pend.slot].ptr as *const u8, pend.out_len);
        let mut outs: Vec<Vec<u8>> = Vec::with_capacity(pend.tex_bytes.len());
        let mut pi = 0usize;
        for &tb in &pend.tex_bytes {
            let mut v = Vec::with_capacity(tb);
            while v.len() < tb {
                ensure!(
                    pi < pend.plan.len(),
                    "desc plan exhausted at tex {}",
                    outs.len()
                );
                let (start, n, _) = pend.plan[pi];
                let o = start as usize * 16;
                v.extend_from_slice(&all[o..o + n * 16]);
                pi += 1;
            }
            ensure!(v.len() == tb, "desc tex bytes {} != {}", v.len(), tb);
            outs.push(v);
        }
        ensure!(
            pi == pend.plan.len(),
            "desc plan entries {} consumed {}",
            pend.plan.len(),
            pi
        );
        ensure!(
            outs.iter().map(|v| v.len()).sum::<usize>() == pend.total_bytes,
            "desc total bytes mismatch"
        );
        let slice_ms = t_slice.elapsed().as_secs_f64() * 1e3;
        if std::env::var("ABGEN_GPU_LOG").is_ok() {
            let mut gpu_upload_ms: f32 = 0.0;
            g.check((g.event_elapsed)(
                &mut gpu_upload_ms,
                self.ev_u0[pend.slot],
                self.ev_u1[pend.slot],
            ))?;
            let mut gpu_blockify_ms: f32 = 0.0;
            g.check((g.event_elapsed)(
                &mut gpu_blockify_ms,
                self.ev_db0[pend.slot],
                self.ev_db1[pend.slot],
            ))?;
            let mut gpu_encode_ms: f32 = 0.0;
            g.check((g.event_elapsed)(
                &mut gpu_encode_ms,
                self.ev_d0[pend.slot],
                self.ev_d1[pend.slot],
            ))?;
            let mut gpu_dtoh_ms: f32 = 0.0;
            g.check((g.event_elapsed)(
                &mut gpu_dtoh_ms,
                self.ev_d1[pend.slot],
                self.ev_out[pend.slot],
            ))?;
            eprintln!(
                "gpu-desc: texs={} descs={} blocks={} out_mb={:.1} base_mb={:.1} layout={:.2} uwait={:.2} stage={:.2} descbuild={:.2} sig={:.2} submit={:.2} wait={:.2} slice={:.2} gpu_upload={:.2} gpu_blockify={:.2} gpu_encode={:.2} gpu_dtoh={:.2} span={:.2}",
                pend.ntexs,
                pend.ndescs,
                pend.nblocks,
                pend.out_len as f64 / 1e6,
                pend.base_bytes as f64 / 1e6,
                pend.layout_ms,
                pend.uwait_ms,
                pend.stage_ms,
                pend.descbuild_ms,
                pend.sig_ms,
                pend.submit_ms,
                wait_ms,
                slice_ms,
                gpu_upload_ms,
                gpu_blockify_ms,
                gpu_encode_ms,
                gpu_dtoh_ms,
                pend.t0.elapsed().as_secs_f64() * 1e3
            );
        }
        Ok(Some(outs))
    }
}
