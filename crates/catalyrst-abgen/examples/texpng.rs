use abgen::unity::bundle_file::{Bundle, FileContent};
use rayon::prelude::*;

#[derive(Clone)]
struct Tex {
    name: String,
    fmt: i64,
    w: usize,
    h: usize,
    payload: Vec<u8>,
}

enum Decoded {
    Rgba { rgba: Vec<u8>, w: usize, h: usize },
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
            Decoded::Rgba { rgba, w, h }
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
            Decoded::Rgba { rgba, w, h }
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
                payload: bytes,
            });
        }
    }
    Ok(out)
}

fn pick_primary(ours: &[Tex], ups: &[Tex], want: Option<&str>) -> Option<(usize, usize)> {
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
        }
    }
    if matched.is_empty() {
        for k in 0..ours.len().min(ups.len()) {
            matched.push((k, k));
        }
    }
    if let Some(w) = want {
        if let Some(&m) = matched.iter().find(|(i, _)| ours[*i].name == w) {
            return Some(m);
        }
    }
    matched.first().copied()
}

fn save_side(outdir: &str, pair: &str, side: &str, tex: &Tex) -> Result<u64, String> {
    match decode_mip0(tex.fmt, &tex.payload, tex.w, tex.h) {
        Decoded::Rgba { rgba, w, h } => {
            if rgba.len() != w * h * 4 {
                return Err(format!(
                    "rgba size mismatch: {} vs {}",
                    rgba.len(),
                    w * h * 4
                ));
            }

            let mut flipped = vec![0u8; rgba.len()];
            for y in 0..h {
                flipped[y * w * 4..(y + 1) * w * 4]
                    .copy_from_slice(&rgba[(h - 1 - y) * w * 4..(h - y) * w * 4]);
            }
            let img = image::RgbaImage::from_raw(w as u32, h as u32, flipped)
                .ok_or_else(|| "image buffer alloc failed".to_string())?;
            let out = format!("{outdir}/{pair}-{side}.png");
            let tmp = format!("{outdir}/.{pair}-{side}.png.tmp");
            img.save_with_format(&tmp, image::ImageFormat::Png)
                .map_err(|e| format!("png save failed: {e}"))?;
            std::fs::rename(&tmp, &out).map_err(|e| format!("rename failed: {e}"))?;
            Ok(std::fs::metadata(&out).map(|m| m.len()).unwrap_or(0))
        }
        Decoded::Unhandled => Err(format!("unhandled fmt={}", tex.fmt)),
        Decoded::Failed(e) => Err(e),
    }
}

fn write_missing(outdir: &str, pair: &str, side: &str, err: &str) {
    let _ = std::fs::write(
        format!("{outdir}/{pair}-{side}.missing.txt"),
        format!("{err}\n"),
    );
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 2 {
        eprintln!("usage: texpng <tasks.jsonl> <outdir> [threads]");
        std::process::exit(2);
    }
    let outdir = args[1].clone();
    std::fs::create_dir_all(&outdir).expect("create outdir");
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
    let done = std::sync::atomic::AtomicUsize::new(0);
    let ok_images = std::sync::atomic::AtomicUsize::new(0);
    let missing = std::sync::atomic::AtomicUsize::new(0);
    let bytes_total = std::sync::atomic::AtomicU64::new(0);
    let total = tasks.len();
    tasks.par_iter().for_each(|task| {
        let pair = task["pair"].as_str().unwrap().to_string();
        let ours_path = task["ours"].as_str().unwrap().to_string();
        let up_path = task["upstream"].as_str().unwrap().to_string();
        let want = task["texName"].as_str().map(|s| s.to_string());
        let out_ok = |n: u64| {
            ok_images.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            bytes_total.fetch_add(n, std::sync::atomic::Ordering::Relaxed);
        };

        let have_up = std::path::Path::new(&format!("{outdir}/{pair}-up.png")).exists();
        let have_ab = std::path::Path::new(&format!("{outdir}/{pair}-abgen.png")).exists();
        if !(have_up && have_ab) {
            let result = std::panic::catch_unwind(|| {
                let ours = extract_textures(&ours_path);
                let ups = extract_textures(&up_path);
                (ours, ups)
            });
            match result {
                Err(_) => {
                    if !have_ab {
                        write_missing(&outdir, &pair, "abgen", "panic during bundle processing");
                        missing.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    }
                    if !have_up {
                        write_missing(&outdir, &pair, "up", "panic during bundle processing");
                        missing.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    }
                }
                Ok((ours_r, ups_r)) => {
                    let pick_solo = |texes: &[Tex]| -> Option<usize> {
                        if let Some(w) = want.as_deref() {
                            if let Some(i) = texes.iter().position(|t| t.name == w) {
                                return Some(i);
                            }
                        }
                        if texes.is_empty() {
                            None
                        } else {
                            Some(0)
                        }
                    };
                    let (sel_ours, sel_up): (Result<usize, String>, Result<usize, String>) =
                        match (&ours_r, &ups_r) {
                            (Ok(a), Ok(b)) => match pick_primary(a, b, want.as_deref()) {
                                Some((i, j)) => (Ok(i), Ok(j)),
                                None => {
                                    let e = |t: &[Tex]| {
                                        if t.is_empty() {
                                            Err("bundle has no Texture2D objects".to_string())
                                        } else {
                                            Ok(0)
                                        }
                                    };
                                    (e(a), e(b))
                                }
                            },
                            (Ok(a), Err(eb)) => (
                                pick_solo(a).ok_or("bundle has no Texture2D objects".to_string()),
                                Err(eb.clone()),
                            ),
                            (Err(ea), Ok(b)) => (
                                Err(ea.clone()),
                                pick_solo(b).ok_or("bundle has no Texture2D objects".to_string()),
                            ),
                            (Err(ea), Err(eb)) => (Err(ea.clone()), Err(eb.clone())),
                        };
                    if !have_ab {
                        let r = sel_ours.and_then(|i| {
                            let t = &ours_r.as_ref().unwrap()[i];
                            std::panic::catch_unwind(|| save_side(&outdir, &pair, "abgen", t))
                                .unwrap_or_else(|_| Err("panic during decode/save".into()))
                        });
                        match r {
                            Ok(n) => out_ok(n),
                            Err(e) => {
                                write_missing(&outdir, &pair, "abgen", &e);
                                missing.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                            }
                        }
                    }
                    if !have_up {
                        let r = sel_up.and_then(|j| {
                            let t = &ups_r.as_ref().unwrap()[j];
                            std::panic::catch_unwind(|| save_side(&outdir, &pair, "up", t))
                                .unwrap_or_else(|_| Err("panic during decode/save".into()))
                        });
                        match r {
                            Ok(n) => out_ok(n),
                            Err(e) => {
                                write_missing(&outdir, &pair, "up", &e);
                                missing.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                            }
                        }
                    }
                }
            }
        } else {
            for side in ["up", "abgen"] {
                if let Ok(m) = std::fs::metadata(format!("{outdir}/{pair}-{side}.png")) {
                    out_ok(m.len());
                }
            }
        }
        let n = done.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
        if n.is_multiple_of(200) || n == total {
            eprintln!(
                "progress {n}/{total} ok_images={} missing={} bytes={}",
                ok_images.load(std::sync::atomic::Ordering::Relaxed),
                missing.load(std::sync::atomic::Ordering::Relaxed),
                bytes_total.load(std::sync::atomic::Ordering::Relaxed)
            );
        }
    });
    println!(
        "{{\"tasks\":{},\"ok_images\":{},\"missing\":{},\"bytes_total\":{}}}",
        total,
        ok_images.load(std::sync::atomic::Ordering::Relaxed),
        missing.load(std::sync::atomic::Ordering::Relaxed),
        bytes_total.load(std::sync::atomic::Ordering::Relaxed)
    );
}
