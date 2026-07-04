use abgen::unity::bundle_file::{Bundle, FileContent};
use rayon::prelude::*;
use std::io::Write as _;

#[derive(Clone)]
struct Tex {
    name: String,
    fmt: i64,
    w: usize,
    h: usize,
    mips: i64,
    payload: Vec<u8>,
}

enum Decoded {
    Rgba {
        rgba: Vec<u8>,
        w: usize,
        h: usize,
        crn_levels: Option<u32>,
    },
    Unhandled,
    Failed(String),
}

fn mip0_len(fmt: i64, w: usize, h: usize) -> Option<usize> {
    let blocks = |bs: usize| w.div_ceil(4) * h.div_ceil(4) * bs;
    Some(match fmt {
        10 => blocks(8),
        12 => blocks(16),
        25 => blocks(16),
        26 => blocks(8),
        27 => blocks(16),
        1 => w * h,
        3 => w * h * 3,
        4 => w * h * 4,
        5 => w * h * 4,
        _ => return None,
    })
}

fn decode_mip0(fmt: i64, payload: &[u8], w: usize, h: usize) -> Decoded {
    if payload.is_empty() || w == 0 || h == 0 {
        return Decoded::Failed("empty payload or zero dims".into());
    }
    match fmt {
        28 | 29 => match crunch_ffi::crn_decompress_level0(payload) {
            Some(d) => Decoded::Rgba {
                rgba: d.rgba,
                w: d.width as usize,
                h: d.height as usize,
                crn_levels: Some(d.levels),
            },
            None => Decoded::Failed(format!("crn decode failed (fmt={fmt})")),
        },
        10 | 12 | 25 | 26 | 27 => {
            let need = mip0_len(fmt, w, h).unwrap();
            if payload.len() < need {
                return Decoded::Failed(format!(
                    "payload too short: {} < {} (fmt={fmt} {w}x{h})",
                    payload.len(),
                    need
                ));
            }
            let slice = &payload[..need];
            let mut out = vec![0u32; w * h];
            let ok = match fmt {
                25 => texture2ddecoder::decode_bc7(slice, w, h, &mut out).is_ok(),
                12 => texture2ddecoder::decode_bc3(slice, w, h, &mut out).is_ok(),
                10 => texture2ddecoder::decode_bc1(slice, w, h, &mut out).is_ok(),
                26 => texture2ddecoder::decode_bc4(slice, w, h, &mut out).is_ok(),
                27 => texture2ddecoder::decode_bc5(slice, w, h, &mut out).is_ok(),
                _ => false,
            };
            if !ok {
                return Decoded::Failed(format!("bc decode failed (fmt={fmt})"));
            }
            let mut rgba = vec![0u8; w * h * 4];
            for (i, px) in out.iter().enumerate() {
                rgba[i * 4] = ((px >> 16) & 0xff) as u8;
                rgba[i * 4 + 1] = ((px >> 8) & 0xff) as u8;
                rgba[i * 4 + 2] = (px & 0xff) as u8;
                rgba[i * 4 + 3] = ((px >> 24) & 0xff) as u8;
            }
            Decoded::Rgba {
                rgba,
                w,
                h,
                crn_levels: None,
            }
        }
        1 | 3 | 4 | 5 => {
            let need = mip0_len(fmt, w, h).unwrap();
            if payload.len() < need {
                return Decoded::Failed(format!(
                    "payload too short: {} < {need} (fmt={fmt})",
                    payload.len()
                ));
            }
            let mut rgba = vec![0u8; w * h * 4];
            match fmt {
                1 => {
                    for i in 0..w * h {
                        rgba[i * 4..i * 4 + 4].copy_from_slice(&[255, 255, 255, payload[i]]);
                    }
                }
                3 => {
                    for i in 0..w * h {
                        rgba[i * 4] = payload[i * 3];
                        rgba[i * 4 + 1] = payload[i * 3 + 1];
                        rgba[i * 4 + 2] = payload[i * 3 + 2];
                        rgba[i * 4 + 3] = 255;
                    }
                }
                4 => rgba.copy_from_slice(&payload[..need]),
                5 => {
                    for i in 0..w * h {
                        rgba[i * 4] = payload[i * 4 + 1];
                        rgba[i * 4 + 1] = payload[i * 4 + 2];
                        rgba[i * 4 + 2] = payload[i * 4 + 3];
                        rgba[i * 4 + 3] = payload[i * 4];
                    }
                }
                _ => unreachable!(),
            }
            Decoded::Rgba {
                rgba,
                w,
                h,
                crn_levels: None,
            }
        }
        _ => Decoded::Unhandled,
    }
}

fn extract_textures(path: &str) -> Result<Vec<Tex>, String> {
    let data = std::fs::read(path).map_err(|e| format!("read error: {e}"))?;
    let bundle = Bundle::load_bytes(&data).map_err(|e| format!("parse error: {e:#}"))?;
    let mut raws: std::collections::HashMap<String, Vec<u8>> = std::collections::HashMap::new();
    for f in &bundle.files {
        if let FileContent::Raw(bytes) = &f.content {
            raws.insert(f.name.clone(), bytes.clone());
        }
    }
    let mut out = Vec::new();
    for f in &bundle.files {
        let FileContent::Serialized(sf) = &f.content else {
            continue;
        };
        for obj in &sf.objects {
            if obj.class_id != 28 {
                continue;
            }
            let v = sf
                .read_typetree(obj)
                .map_err(|e| format!("typetree error on Texture2D pid={}: {e:#}", obj.path_id))?;
            let name = v
                .get("m_Name")
                .and_then(|x| x.as_str())
                .unwrap_or("?")
                .to_string();
            let fmt = v
                .get("m_TextureFormat")
                .and_then(|x| x.as_i64())
                .unwrap_or(-1);
            let w = v.get("m_Width").and_then(|x| x.as_i64()).unwrap_or(0) as usize;
            let h = v.get("m_Height").and_then(|x| x.as_i64()).unwrap_or(0) as usize;
            let mips = v.get("m_MipCount").and_then(|x| x.as_i64()).unwrap_or(-1);
            let mut bytes = v
                .get("image data")
                .and_then(|x| x.as_bytes())
                .map(|b| b.to_vec())
                .unwrap_or_default();
            if bytes.is_empty() {
                if let Some(sd) = v.get("m_StreamData") {
                    let off = sd.get("offset").and_then(|x| x.as_i64()).unwrap_or(0) as usize;
                    let size = sd.get("size").and_then(|x| x.as_i64()).unwrap_or(0) as usize;
                    let p = sd.get("path").and_then(|x| x.as_str()).unwrap_or("");
                    let base = p.rsplit('/').next().unwrap_or(p);
                    if size > 0 {
                        if let Some(raw) = raws.get(base).or_else(|| {
                            raws.iter()
                                .find(|(k, _)| k.ends_with(".resS"))
                                .map(|(_, v)| v)
                        }) {
                            if off + size <= raw.len() {
                                bytes = raw[off..off + size].to_vec();
                            }
                        }
                    }
                }
            }
            out.push(Tex {
                name,
                fmt,
                w,
                h,
                mips,
                payload: bytes,
            });
        }
    }
    Ok(out)
}

fn rank(class: &str) -> i32 {
    match class {
        "identical" => 0,
        "identical-decode" => 1,
        "imperceptible" => 2,
        "visible" => 3,
        "loadFailUpstream-texture" => 4,
        "loadFailOurs-texture" => 5,
        _ => 6,
    }
}

fn rank_task(class: &str) -> i32 {
    match class {
        "identical" => 0,
        "imperceptible" => 1,
        "visible" => 2,
        "formatDiff" => 3,
        "decodeFailUpstream" => 4,
        "decodeFailOurs" => 5,
        _ => 6,
    }
}

fn process(task: &serde_json::Value) -> serde_json::Value {
    let ours_path = task["ours"].as_str().unwrap();
    let up_path = task["upstream"].as_str().unwrap();
    let platform = task
        .get("platform")
        .and_then(|v| v.as_str())
        .unwrap_or("mac");
    let mut row = serde_json::json!({
        "pair": task["pair"], "set": task["set"], "entity": task["entity"],
        "bundle": task["bundle"], "platform": platform, "kind": "texture",
        "chunk": task["chunk"], "oursSource": task["oursSource"],
        "upstreamVersion": task["upstreamVersion"],
    });
    let mut notes: Vec<String> = Vec::new();

    let ours_bytes = std::fs::read(ours_path);
    let up_bytes = std::fs::read(up_path);
    if let (Ok(a), Ok(b)) = (&ours_bytes, &up_bytes) {
        if a == b {
            row["class"] = "identical".into();
            row["classTask"] = "identical".into();
            row["formatDiff"] = false.into();
            row["px"] = 0.into();
            row["ppm"] = 0.0.into();
            row["px8"] = 0.into();
            row["ppm8"] = 0.0.into();
            row["maxChannelDelta"] = 0.into();
            row["meanDelta"] = 0.0.into();
            row["meanAbsDelta"] = 0.0.into();
            row["notes"] = serde_json::json!(["bundle byte-identical"]);
            return row;
        }
    }

    let ours = match extract_textures(ours_path) {
        Ok(t) => t,
        Err(e) => {
            row["class"] = "loadFailOurs-texture".into();
            row["classTask"] = "decodeFailOurs".into();
            row["notes"] = serde_json::json!([format!("ours: {e}")]);
            return row;
        }
    };
    let ups = match extract_textures(up_path) {
        Ok(t) => t,
        Err(e) => {
            row["class"] = "loadFailUpstream-texture".into();
            row["classTask"] = "decodeFailUpstream".into();
            row["notes"] = serde_json::json!([format!("upstream: {e}")]);
            return row;
        }
    };
    if ours.is_empty() || ups.is_empty() {
        if ours.is_empty() {
            row["class"] = "loadFailOurs-texture".into();
            row["classTask"] = "decodeFailOurs".into();
            notes.push("ours: bundle has no Texture2D objects".into());
        } else {
            row["class"] = "loadFailUpstream-texture".into();
            row["classTask"] = "decodeFailUpstream".into();
            notes.push("upstream: bundle has no Texture2D objects".into());
        }
        row["notes"] = serde_json::json!(notes);
        return row;
    }
    if ours.len() != ups.len() {
        notes.push(format!(
            "texCount ours={} upstream={}",
            ours.len(),
            ups.len()
        ));
    }

    let mut up_used = vec![false; ups.len()];
    let mut matched: Vec<(usize, usize)> = Vec::new();
    for (i, t) in ours.iter().enumerate() {
        if let Some(j) = ups
            .iter()
            .enumerate()
            .position(|(j, u)| !up_used[j] && u.name == t.name)
        {
            up_used[j] = true;
            matched.push((i, j));
        } else {
            notes.push(format!("unmatched-ours:{}", t.name));
        }
    }
    for (j, u) in ups.iter().enumerate() {
        if !up_used[j] {
            notes.push(format!("unmatched-upstream:{}", u.name));
        }
    }
    if matched.is_empty() {
        notes.push("name-match failed; positional fallback".into());
        for k in 0..ours.len().min(ups.len()) {
            matched.push((k, k));
        }
    }

    let mut worst = "identical-decode".to_string();
    let mut worst_task = "identical".to_string();
    let mut any_format_diff = false;
    let mut agg_px: i64 = 0;
    let mut agg_ppm: f64 = 0.0;
    let mut agg_px8: i64 = 0;
    let mut agg_ppm8: f64 = 0.0;
    let mut agg_mcd: i64 = 0;
    let mut agg_mean: f64 = 0.0;
    let mut agg_mean_abs: f64 = 0.0;
    let mut texes = Vec::new();

    for (i, j) in matched {
        let t = &ours[i];
        let u = &ups[j];
        let tclass: String;
        let mut tclass_task = "identical".to_string();
        let mut tex = serde_json::json!({
            "name": t.name, "fmtOurs": t.fmt, "fmtUp": u.fmt,
            "dimsOurs": format!("{}x{}", t.w, t.h), "dimsUp": format!("{}x{}", u.w, u.h),
            "mipsOurs": t.mips, "mipsUp": u.mips,
        });
        let fmt_diff = t.fmt != u.fmt || t.w != u.w || t.h != u.h || t.mips != u.mips;
        if t.fmt != u.fmt {
            notes.push(format!("fmt:{}:ours={},upstream={}", t.name, t.fmt, u.fmt));
        }
        if t.w != u.w || t.h != u.h {
            notes.push(format!(
                "dims:{}:ours={}x{},upstream={}x{}",
                t.name, t.w, t.h, u.w, u.h
            ));
        }
        if t.mips != u.mips {
            notes.push(format!(
                "mips:{}:ours={},upstream={}",
                t.name, t.mips, u.mips
            ));
        }
        if fmt_diff {
            any_format_diff = true;
            tclass_task = "formatDiff".into();
        }

        let d_ours = decode_mip0(t.fmt, &t.payload, t.w, t.h);
        let d_up = decode_mip0(u.fmt, &u.payload, u.w, u.h);
        match (&d_ours, &d_up) {
            (Decoded::Failed(e), _) => {
                tclass = "loadFailOurs-texture".into();
                tclass_task = "decodeFailOurs".into();
                notes.push(format!("decodeFailOurs:{}: {e}", t.name));
            }
            (_, Decoded::Failed(e)) => {
                tclass = "loadFailUpstream-texture".into();
                tclass_task = "decodeFailUpstream".into();
                notes.push(format!("decodeFailUpstream:{}: {e}", t.name));
            }
            (Decoded::Unhandled, Decoded::Unhandled) => {
                if t.payload == u.payload {
                    tclass = "identical-decode".into();
                    if tclass_task != "formatDiff" {
                        tclass_task = "identical".into();
                    }
                    notes.push(format!(
                        "undecoded:{}:fmt={} payload byte-identical",
                        t.name, t.fmt
                    ));
                } else {
                    tclass = "visible".into();
                    if tclass_task != "formatDiff" {
                        tclass_task = "visible".into();
                    }
                    notes.push(format!(
                        "undecoded:{}:fmt ours={} upstream={} payloads differ",
                        t.name, t.fmt, u.fmt
                    ));
                }
            }
            (Decoded::Unhandled, Decoded::Rgba { .. }) => {
                tclass = "loadFailOurs-texture".into();
                tclass_task = "decodeFailOurs".into();
                notes.push(format!(
                    "decodeFailOurs:{}: unhandled fmt={}",
                    t.name, t.fmt
                ));
            }
            (Decoded::Rgba { .. }, Decoded::Unhandled) => {
                tclass = "loadFailUpstream-texture".into();
                tclass_task = "decodeFailUpstream".into();
                notes.push(format!(
                    "decodeFailUpstream:{}: unhandled fmt={}",
                    u.name, u.fmt
                ));
            }
            (
                Decoded::Rgba {
                    rgba: ra,
                    w: wa,
                    h: ha,
                    crn_levels: la,
                },
                Decoded::Rgba {
                    rgba: rb,
                    w: wb,
                    h: hb,
                    crn_levels: lb,
                },
            ) => {
                if let (Some(la), Some(lb)) = (la, lb) {
                    if la != lb {
                        notes.push(format!("crnLevels:{}:ours={la},upstream={lb}", t.name));
                    }
                }
                if wa != wb || ha != hb {
                    tclass = "visible".into();
                    if tclass_task != "formatDiff" {
                        tclass_task = "formatDiff".into();
                        any_format_diff = true;
                    }
                    notes.push(format!(
                        "decodedDims:{}:ours={wa}x{ha},upstream={wb}x{hb} (no pixel compare)",
                        t.name
                    ));

                    let two_x_ours = *wa == *wb * 2 && *ha == *hb * 2;
                    let two_x_up = *wb == *wa * 2 && *hb == *ha * 2;
                    if two_x_ours || two_x_up {
                        let (big, small_rgba, sw, sh) = if two_x_ours {
                            (t, rb, *wb, *hb)
                        } else {
                            (u, ra, *wa, *ha)
                        };
                        if let Some(m0) = mip0_len(big.fmt, big.w, big.h) {
                            if big.payload.len() > m0 {
                                if let Decoded::Rgba {
                                    rgba: rm1,
                                    w: w1,
                                    h: h1,
                                    ..
                                } =
                                    decode_mip0(big.fmt, &big.payload[m0..], big.w / 2, big.h / 2)
                                {
                                    if w1 == sw && h1 == sh {
                                        let n = sw * sh;
                                        let mut px8: i64 = 0;
                                        let mut mcd: i64 = 0;
                                        let mut sum = 0f64;
                                        for (pa, pb) in
                                            rm1.chunks_exact(4).zip(small_rgba.chunks_exact(4))
                                        {
                                            let mut diff8 = false;
                                            for c in 0..4 {
                                                let d = (pa[c] as i64 - pb[c] as i64).abs();
                                                if d > 8 {
                                                    diff8 = true;
                                                }
                                                if d > mcd {
                                                    mcd = d;
                                                }
                                                sum += d as f64;
                                            }
                                            if diff8 {
                                                px8 += 1;
                                            }
                                        }
                                        let ppm8 = px8 as f64 * 1e6 / n as f64;
                                        tex["halfresPpm8"] =
                                            (((ppm8) * 100.0).round() / 100.0).into();
                                        tex["halfresMcd"] = mcd.into();
                                        tex["halfresMeanAbs"] =
                                            ((sum / (n as f64 * 4.0) * 10000.0).round() / 10000.0)
                                                .into();
                                        notes.push(format!(
                                            "halfres:{}:mip1-vs-mip0 mcd={mcd} ppm8={:.0}",
                                            t.name, ppm8
                                        ));
                                    }
                                }
                            }
                        }
                    }
                } else {
                    let n = wa * ha;
                    let mut px: i64 = 0;
                    let mut px8: i64 = 0;
                    let mut mcd: i64 = 0;
                    let mut sums = [0f64; 4];
                    for (pa, pb) in ra.chunks_exact(4).zip(rb.chunks_exact(4)) {
                        let mut diff = false;
                        let mut diff8 = false;
                        for c in 0..4 {
                            let d = (pa[c] as i64 - pb[c] as i64).abs();
                            if d > 0 {
                                diff = true;
                                if d > 8 {
                                    diff8 = true;
                                }
                                if d > mcd {
                                    mcd = d;
                                }
                            }
                            sums[c] += d as f64;
                        }
                        if diff {
                            px += 1;
                        }
                        if diff8 {
                            px8 += 1;
                        }
                    }
                    let ppm = px as f64 * 1e6 / n as f64;
                    let ppm8 = px8 as f64 * 1e6 / n as f64;
                    let pct8 = px8 as f64 * 100.0 / n as f64;
                    let means: Vec<f64> = sums
                        .iter()
                        .map(|s| (s / n as f64 * 10000.0).round() / 10000.0)
                        .collect();
                    let mean_max = means.iter().cloned().fold(0.0, f64::max);
                    let mean_abs = sums.iter().sum::<f64>() / (n as f64 * 4.0);
                    tclass = if px == 0 {
                        "identical-decode".into()
                    } else if ppm <= 200.0 {
                        notes.push(format!("imperceptible-by=ppm:{}:{:.1}ppm", t.name, ppm));
                        "imperceptible".into()
                    } else if mcd <= 2 {
                        notes.push(format!(
                            "imperceptible-by=maxChannelDelta:{}:mcd={mcd} ppm={:.0}",
                            t.name, ppm
                        ));
                        "imperceptible".into()
                    } else {
                        "visible".into()
                    };
                    if tclass_task != "formatDiff" {
                        tclass_task = if px == 0 {
                            "identical".into()
                        } else if mcd <= 16 && pct8 < 2.0 {
                            "imperceptible".into()
                        } else {
                            "visible".into()
                        };
                    }
                    tex["px"] = px.into();
                    tex["ppm"] = ((ppm * 100.0).round() / 100.0).into();
                    tex["px8"] = px8.into();
                    tex["ppm8"] = ((ppm8 * 100.0).round() / 100.0).into();
                    tex["maxChannelDelta"] = mcd.into();
                    tex["meanDeltaCh"] = serde_json::json!(means);
                    tex["meanAbsDelta"] = ((mean_abs * 10000.0).round() / 10000.0).into();
                    if px > agg_px {
                        agg_px = px;
                    }
                    if ppm > agg_ppm {
                        agg_ppm = ppm;
                    }
                    if px8 > agg_px8 {
                        agg_px8 = px8;
                    }
                    if ppm8 > agg_ppm8 {
                        agg_ppm8 = ppm8;
                    }
                    if mcd > agg_mcd {
                        agg_mcd = mcd;
                    }
                    if mean_max > agg_mean {
                        agg_mean = mean_max;
                    }
                    if mean_abs > agg_mean_abs {
                        agg_mean_abs = mean_abs;
                    }
                }
            }
        }
        tex["class"] = tclass.clone().into();
        tex["classTask"] = tclass_task.clone().into();
        texes.push(tex);
        if rank(&tclass) > rank(&worst) {
            worst = tclass;
        }
        if rank_task(&tclass_task) > rank_task(&worst_task) {
            worst_task = tclass_task;
        }
    }

    row["class"] = worst.into();
    row["classTask"] = worst_task.into();
    row["formatDiff"] = any_format_diff.into();
    row["px"] = agg_px.into();
    row["ppm"] = ((agg_ppm * 100.0).round() / 100.0).into();
    row["px8"] = agg_px8.into();
    row["ppm8"] = ((agg_ppm8 * 100.0).round() / 100.0).into();
    row["maxChannelDelta"] = agg_mcd.into();
    row["meanDelta"] = ((agg_mean * 10000.0).round() / 10000.0).into();
    row["meanAbsDelta"] = ((agg_mean_abs * 10000.0).round() / 10000.0).into();
    row["texCount"] = texes.len().into();
    row["textures"] = serde_json::json!(texes);
    row["notes"] = serde_json::json!(notes);
    row
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 2 {
        eprintln!("usage: texcmp2 <tasks.jsonl> <out.jsonl> [threads]");
        std::process::exit(2);
    }
    let threads: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(48);
    rayon::ThreadPoolBuilder::new()
        .num_threads(threads)
        .build_global()
        .unwrap();
    let tasks: Vec<serde_json::Value> = std::fs::read_to_string(&args[0])
        .expect("read tasks")
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("parse task"))
        .collect();
    let out = std::sync::Mutex::new(std::io::BufWriter::new(
        std::fs::File::create(&args[1]).expect("create out"),
    ));
    let done = std::sync::atomic::AtomicUsize::new(0);
    let total = tasks.len();
    tasks.par_iter().for_each(|task| {
        let row = match std::panic::catch_unwind(|| process(task)) {
            Ok(r) => r,
            Err(_) => serde_json::json!({
                "pair": task["pair"], "set": task["set"], "entity": task["entity"],
                "bundle": task["bundle"],
                "platform": task.get("platform").and_then(|v| v.as_str()).unwrap_or("mac"),
                "kind": "texture",
                "chunk": task["chunk"], "oursSource": task["oursSource"],
                "upstreamVersion": task["upstreamVersion"],
                "class": "loadFailOurs-texture",
                "classTask": "decodeFailOurs",
                "notes": ["panic during processing"],
            }),
        };
        {
            let mut g = out.lock().unwrap();
            serde_json::to_writer(&mut *g, &row).unwrap();
            g.write_all(b"\n").unwrap();
        }
        let n = done.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
        if n.is_multiple_of(500) || n == total {
            eprintln!("progress {n}/{total}");
        }
    });
    out.lock().unwrap().flush().unwrap();
}
