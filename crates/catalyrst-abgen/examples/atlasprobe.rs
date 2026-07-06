use abgen::unity::bundle_file::{Bundle, FileContent};
use abgen::value::Value;
use std::collections::HashMap;

struct Level {
    w: usize,
    h: usize,
    rgba: Vec<u8>,
}

struct Tex {
    name: String,
    fmt: i64,
    w: usize,
    h: usize,
    mip_count: i64,
    payload: Vec<u8>,
}

fn luma(px: &[u8]) -> f64 {
    0.2126 * px[0] as f64 + 0.7152 * px[1] as f64 + 0.0722 * px[2] as f64
}

fn percentile(sorted: &[f64], q: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let idx = ((sorted.len() - 1) as f64 * q).round() as usize;
    sorted[idx]
}

fn level_stats(tag: &str, lv: &Level) {
    let n = lv.w * lv.h;
    let mut sum = [0f64; 4];
    let mut lumas: Vec<f64> = Vec::with_capacity(n);
    let mut near_black = 0usize;
    let mut a0 = 0usize;
    let mut a255 = 0usize;
    for px in lv.rgba.chunks_exact(4) {
        for (c, s) in sum.iter_mut().enumerate() {
            *s += px[c] as f64;
        }
        lumas.push(luma(px));
        if px[0].max(px[1]).max(px[2]) < 16 {
            near_black += 1;
        }
        if px[3] == 0 {
            a0 += 1;
        }
        if px[3] == 255 {
            a255 += 1;
        }
    }
    lumas.sort_unstable_by(f64::total_cmp);
    let mean_luma = lumas.iter().sum::<f64>() / n as f64;
    println!(
        "{tag} {}x{} meanRGBA=({:.1},{:.1},{:.1},{:.1}) meanLuma={:.1} p10={:.1} p50={:.1} p90={:.1} nearBlack={:.4} a0={:.4} a255={:.4}",
        lv.w,
        lv.h,
        sum[0] / n as f64,
        sum[1] / n as f64,
        sum[2] / n as f64,
        sum[3] / n as f64,
        mean_luma,
        percentile(&lumas, 0.10),
        percentile(&lumas, 0.50),
        percentile(&lumas, 0.90),
        near_black as f64 / n as f64,
        a0 as f64 / n as f64,
        a255 as f64 / n as f64
    );
}

fn pxkey(px: &[u8]) -> u32 {
    u32::from_le_bytes([px[0], px[1], px[2], px[3]])
}

fn modal(counts: &HashMap<u32, usize>) -> (u32, usize) {
    counts
        .iter()
        .max_by_key(|(_, c)| **c)
        .map(|(k, c)| (*k, *c))
        .unwrap_or((0, 0))
}

fn mip0_analysis(lv: &Level) {
    let (w, h) = (lv.w, lv.h);
    let n = w * h;
    let mut global: HashMap<u32, usize> = HashMap::new();
    for px in lv.rgba.chunks_exact(4) {
        *global.entry(pxkey(px)).or_insert(0) += 1;
    }
    let (gcol, gcnt) = modal(&global);
    let mut border: HashMap<u32, usize> = HashMap::new();
    let bump = |x: usize, y: usize, m: &mut HashMap<u32, usize>| {
        let px = &lv.rgba[(y * w + x) * 4..(y * w + x) * 4 + 4];
        *m.entry(pxkey(px)).or_insert(0) += 1;
    };
    for x in 0..w {
        bump(x, 0, &mut border);
        bump(x, h - 1, &mut border);
    }
    for y in 0..h {
        bump(0, y, &mut border);
        bump(w - 1, y, &mut border);
    }
    let border_n = 2 * w + 2 * h;
    let (bcol, bcnt) = modal(&border);
    let bg = bcol.to_le_bytes();
    let near_bg = |px: &[u8]| {
        px[0].abs_diff(bg[0]) <= 8
            && px[1].abs_diff(bg[1]) <= 8
            && px[2].abs_diff(bg[2]) <= 8
            && px[3].abs_diff(bg[3]) <= 8
    };
    let mask: Vec<bool> = lv.rgba.chunks_exact(4).map(near_bg).collect();
    let bg_count = mask.iter().filter(|m| **m).count();
    let mut bbox = (usize::MAX, usize::MAX, 0usize, 0usize);
    let mut bg_luma_sum = 0f64;
    let mut fg_luma_sum = 0f64;
    for (i, px) in lv.rgba.chunks_exact(4).enumerate() {
        if mask[i] {
            bg_luma_sum += luma(px);
        } else {
            fg_luma_sum += luma(px);
            let (x, y) = (i % w, i / w);
            bbox.0 = bbox.0.min(x);
            bbox.1 = bbox.1.min(y);
            bbox.2 = bbox.2.max(x);
            bbox.3 = bbox.3.max(y);
        }
    }
    let mut boundary = 0usize;
    let mut boundary_dl = 0f64;
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            if x + 1 < w && mask[i] != mask[i + 1] {
                boundary += 1;
                boundary_dl += (luma(&lv.rgba[i * 4..i * 4 + 4])
                    - luma(&lv.rgba[(i + 1) * 4..(i + 1) * 4 + 4]))
                .abs();
            }
            if y + 1 < h && mask[i] != mask[i + w] {
                boundary += 1;
                boundary_dl += (luma(&lv.rgba[i * 4..i * 4 + 4])
                    - luma(&lv.rgba[(i + w) * 4..(i + w) * 4 + 4]))
                .abs();
            }
        }
    }
    let fg_count = n - bg_count;
    println!(
        "  modalGlobal=#{gcol:08x} frac={:.4} modalBorder=#{bcol:08x} borderFrac={:.4}",
        gcnt as f64 / n as f64,
        bcnt as f64 / border_n as f64
    );
    println!(
        "  occupancy(vs borderModal L8)={:.4} bgFrac={:.4} bgMeanLuma={:.1} fgMeanLuma={:.1}",
        fg_count as f64 / n as f64,
        bg_count as f64 / n as f64,
        if bg_count > 0 {
            bg_luma_sum / bg_count as f64
        } else {
            -1.0
        },
        if fg_count > 0 {
            fg_luma_sum / fg_count as f64
        } else {
            -1.0
        }
    );
    if fg_count > 0 {
        println!(
            "  contentBbox=({},{})..({},{}) bboxCoverage={:.4} boundaryPairs={boundary} boundaryFrac={:.5} boundaryMeanDLuma={:.1}",
            bbox.0,
            bbox.1,
            bbox.2,
            bbox.3,
            ((bbox.2 - bbox.0 + 1) * (bbox.3 - bbox.1 + 1)) as f64 / n as f64,
            boundary as f64 / (2 * n) as f64,
            if boundary > 0 {
                boundary_dl / boundary as f64
            } else {
                -1.0
            }
        );
    }
}

fn box_down(lv: &Level) -> Level {
    let w = (lv.w / 2).max(1);
    let h = (lv.h / 2).max(1);
    let mut rgba = vec![0u8; w * h * 4];
    for y in 0..h {
        for x in 0..w {
            for c in 0..4 {
                let mut s = 0u32;
                for dy in 0..2 {
                    for dx in 0..2 {
                        let sx = (x * 2 + dx).min(lv.w - 1);
                        let sy = (y * 2 + dy).min(lv.h - 1);
                        s += lv.rgba[(sy * lv.w + sx) * 4 + c] as u32;
                    }
                }
                rgba[(y * w + x) * 4 + c] = ((s + 2) / 4) as u8;
            }
        }
    }
    Level { w, h, rgba }
}

fn srgb_to_lin(c: u8) -> f64 {
    let x = c as f64 / 255.0;
    if x <= 0.04045 {
        x / 12.92
    } else {
        ((x + 0.055) / 1.055).powf(2.4)
    }
}

fn lin_to_srgb(x: f64) -> u8 {
    let y = if x <= 0.0031308 {
        x * 12.92
    } else {
        1.055 * x.powf(1.0 / 2.4) - 0.055
    };
    (y * 255.0).round().clamp(0.0, 255.0) as u8
}

fn box_down_linear(lv: &Level) -> Level {
    let w = (lv.w / 2).max(1);
    let h = (lv.h / 2).max(1);
    let mut rgba = vec![0u8; w * h * 4];
    for y in 0..h {
        for x in 0..w {
            let mut acc = [0f64; 4];
            for dy in 0..2 {
                for dx in 0..2 {
                    let sx = (x * 2 + dx).min(lv.w - 1);
                    let sy = (y * 2 + dy).min(lv.h - 1);
                    let px = &lv.rgba[(sy * lv.w + sx) * 4..(sy * lv.w + sx) * 4 + 4];
                    for c in 0..3 {
                        acc[c] += srgb_to_lin(px[c]);
                    }
                    acc[3] += px[3] as f64;
                }
            }
            for c in 0..3 {
                rgba[(y * w + x) * 4 + c] = lin_to_srgb(acc[c] / 4.0);
            }
            rgba[(y * w + x) * 4 + 3] = (acc[3] / 4.0).round() as u8;
        }
    }
    Level { w, h, rgba }
}

fn bc7_levels(payload: &[u8], w0: usize, h0: usize) -> Vec<Level> {
    let mut lvls = Vec::new();
    let mut off = 0usize;
    let (mut w, mut h) = (w0, h0);
    loop {
        let sz = w.div_ceil(4) * h.div_ceil(4) * 16;
        if off + sz > payload.len() {
            break;
        }
        let mut out = vec![0u32; w * h];
        if texture2ddecoder::decode_bc7(&payload[off..off + sz], w, h, &mut out).is_err() {
            break;
        }
        let mut rgba = vec![0u8; w * h * 4];
        for (i, px) in out.iter().enumerate() {
            rgba[i * 4] = ((px >> 16) & 0xff) as u8;
            rgba[i * 4 + 1] = ((px >> 8) & 0xff) as u8;
            rgba[i * 4 + 2] = (px & 0xff) as u8;
            rgba[i * 4 + 3] = ((px >> 24) & 0xff) as u8;
        }
        lvls.push(Level { w, h, rgba });
        off += sz;
        if w == 1 && h == 1 {
            break;
        }
        w = (w / 2).max(1);
        h = (h / 2).max(1);
    }
    println!(
        "  payloadBytes={} consumed={} levels={}",
        payload.len(),
        off,
        lvls.len()
    );
    lvls
}

fn save_png(outdir: &str, name: &str, lv: &Level, flip: bool) {
    let mut data = lv.rgba.clone();
    if flip {
        let row = lv.w * 4;
        for y in 0..lv.h {
            data[y * row..(y + 1) * row]
                .copy_from_slice(&lv.rgba[(lv.h - 1 - y) * row..(lv.h - y) * row]);
        }
    }
    if let Some(img) = image::RgbaImage::from_raw(lv.w as u32, lv.h as u32, data) {
        let path = format!("{outdir}/{name}.png");
        if let Err(e) = img.save_with_format(&path, image::ImageFormat::Png) {
            eprintln!("png save {path}: {e}");
        }
    }
}

fn extract_textures(bundle: &Bundle) -> Vec<Tex> {
    let mut raws: HashMap<String, Vec<u8>> = HashMap::new();
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
            let Ok(v) = sf.read_typetree(obj) else {
                continue;
            };
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
            let mip_count = v.get("m_MipCount").and_then(|x| x.as_i64()).unwrap_or(-1);
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
                mip_count,
                payload: bytes,
            });
        }
    }
    out
}

fn vfmt_size(fmt: i64) -> Option<usize> {
    match fmt {
        0 => Some(4),
        1 | 4 | 5 => Some(2),
        2 | 3 | 6 | 7 => Some(1),
        _ => None,
    }
}

fn mesh_uv_report(v: &Value, raws: &HashMap<String, Vec<u8>>) {
    let name = v.get("m_Name").and_then(|x| x.as_str()).unwrap_or("?");
    let Some(vd) = v.get("m_VertexData") else {
        return;
    };
    let vcount = vd
        .get("m_VertexCount")
        .and_then(|x| x.as_i64())
        .unwrap_or(0) as usize;
    let Some(channels) = vd.get("m_Channels").and_then(|x| x.as_array()) else {
        return;
    };
    let ch: Vec<(usize, usize, i64, usize)> = channels
        .iter()
        .map(|c| {
            (
                c.get("stream").and_then(|x| x.as_i64()).unwrap_or(0) as usize,
                c.get("offset").and_then(|x| x.as_i64()).unwrap_or(0) as usize,
                c.get("format").and_then(|x| x.as_i64()).unwrap_or(0),
                (c.get("dimension").and_then(|x| x.as_i64()).unwrap_or(0) as usize) & 0xF,
            )
        })
        .collect();
    if ch.len() < 5 || ch[4].3 == 0 {
        println!("  Mesh {name}: no uv0 channel");
        return;
    }
    let (uv_stream, uv_off, uv_fmt, uv_dim) = ch[4];
    let n_streams = ch.iter().map(|c| c.0).max().unwrap_or(0) + 1;
    let mut strides = vec![0usize; n_streams];
    for &(s, _, f, d) in &ch {
        if d > 0 {
            strides[s] += d * vfmt_size(f).unwrap_or(0);
        }
    }
    let mut data = vd
        .get("m_DataSize")
        .and_then(|x| x.as_bytes())
        .map(|b| b.to_vec())
        .unwrap_or_default();
    if data.is_empty() {
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
                        data = raw[off..off + size].to_vec();
                    }
                }
            }
        }
    }
    if uv_fmt != 0 || uv_dim < 2 || data.is_empty() || vcount == 0 {
        println!(
            "  Mesh {name}: uv0 stream={uv_stream} offset={uv_off} fmt={uv_fmt} dim={uv_dim} dataLen={} (unsupported layout, skipped)",
            data.len()
        );
        return;
    }
    let mut base = 0usize;
    for st in strides.iter().take(uv_stream) {
        base += (vcount * st).div_ceil(16) * 16;
    }
    let stride = strides[uv_stream];
    let (mut umin, mut vmin, mut umax, mut vmax) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    let mut outside = 0usize;
    for i in 0..vcount {
        let o = base + i * stride + uv_off;
        if o + 8 > data.len() {
            break;
        }
        let u = f32::from_le_bytes(data[o..o + 4].try_into().unwrap());
        let vv = f32::from_le_bytes(data[o + 4..o + 8].try_into().unwrap());
        umin = umin.min(u);
        umax = umax.max(u);
        vmin = vmin.min(vv);
        vmax = vmax.max(vv);
        if !(-1e-4..=1.0001).contains(&u) || !(-1e-4..=1.0001).contains(&vv) {
            outside += 1;
        }
    }
    println!(
        "  Mesh {name}: verts={vcount} stride={stride} uvMin=({umin:.4},{vmin:.4}) uvMax=({umax:.4},{vmax:.4}) uvBboxCoverage={:.4} outside01Frac={:.4}",
        ((umax - umin) * (vmax - vmin)).max(0.0),
        outside as f64 / vcount as f64
    );
}

fn analyze_chain(prefix: &str, lvls: &[Level], outdir: &str, stem: &str, flip: bool) {
    for (k, lv) in lvls.iter().enumerate() {
        level_stats(&format!("{prefix} mip{k}"), lv);
    }
    if let Some(first) = lvls.first() {
        mip0_analysis(first);
        let mut sim = Level {
            w: first.w,
            h: first.h,
            rgba: first.rgba.clone(),
        };
        let mut k = 0usize;
        while sim.w > 1 || sim.h > 1 {
            sim = box_down(&sim);
            k += 1;
            level_stats(&format!("{prefix} sim{k}"), &sim);
            if k >= lvls.len() {
                break;
            }
        }
        let mut lin = Level {
            w: first.w,
            h: first.h,
            rgba: first.rgba.clone(),
        };
        let mut k = 0usize;
        while lin.w > 1 || lin.h > 1 {
            lin = box_down_linear(&lin);
            k += 1;
            level_stats(&format!("{prefix} lin{k}"), &lin);
            if k >= lvls.len() {
                break;
            }
        }
        for (k, lv) in lvls.iter().enumerate() {
            if k == 0 || lv.w == 64 || lv.w == 16 {
                save_png(outdir, &format!("{stem}-mip{k}"), lv, flip);
            }
        }
    }
}

fn cmd_bundle(path: &str, outdir: &str) {
    let data = std::fs::read(path).expect("read bundle");
    let bundle = Bundle::load_bytes(&data).expect("parse bundle");
    let stem_base = std::path::Path::new(path)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("bundle")
        .to_string();
    println!("== bundle {stem_base}");
    let texs = extract_textures(&bundle);
    for t in &texs {
        println!(
            "Texture2D name={} fmt={} {}x{} mipCount={}",
            t.name, t.fmt, t.w, t.h, t.mip_count
        );
        if t.fmt != 25 {
            println!("  (not BC7, skipped)");
            continue;
        }
        let lvls = bc7_levels(&t.payload, t.w, t.h);
        let stem = format!("{stem_base}-{}", t.name.replace([' ', '/'], "_"));
        analyze_chain("  ", &lvls, outdir, &stem, true);
    }
    let mut raws: HashMap<String, Vec<u8>> = HashMap::new();
    for f in &bundle.files {
        if let FileContent::Raw(bytes) = &f.content {
            raws.insert(f.name.clone(), bytes.clone());
        }
    }
    for f in &bundle.files {
        let FileContent::Serialized(sf) = &f.content else {
            continue;
        };
        for obj in &sf.objects {
            if obj.class_id != 43 {
                continue;
            }
            if let Ok(v) = sf.read_typetree(obj) {
                mesh_uv_report(&v, &raws);
            }
        }
    }
}

fn cmd_glb(path: &str, outdir: &str) {
    let data = std::fs::read(path).expect("read glb");
    let json_len = u32::from_le_bytes(data[12..16].try_into().unwrap()) as usize;
    let json: serde_json::Value = serde_json::from_slice(&data[20..20 + json_len]).expect("json");
    let mut bin_off = 20 + json_len;
    let mut bin: &[u8] = &[];
    while bin_off + 8 <= data.len() {
        let clen = u32::from_le_bytes(data[bin_off..bin_off + 4].try_into().unwrap()) as usize;
        let ctype = &data[bin_off + 4..bin_off + 8];
        if ctype == b"BIN\0" {
            bin = &data[bin_off + 8..bin_off + 8 + clen];
            break;
        }
        bin_off += 8 + clen;
    }
    let stem_base = std::path::Path::new(path)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("glb")
        .to_string();
    println!("== glb {stem_base}");
    let views = json.get("bufferViews").and_then(|v| v.as_array());
    let Some(images) = json.get("images").and_then(|v| v.as_array()) else {
        println!("  no images");
        return;
    };
    for (idx, im) in images.iter().enumerate() {
        let name = im.get("name").and_then(|v| v.as_str()).unwrap_or("?");
        let mime = im.get("mimeType").and_then(|v| v.as_str()).unwrap_or("?");
        let bv = im.get("bufferView").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
        let (off, len) = views
            .and_then(|vs| vs.get(bv))
            .map(|v| {
                (
                    v.get("byteOffset").and_then(|x| x.as_u64()).unwrap_or(0) as usize,
                    v.get("byteLength").and_then(|x| x.as_u64()).unwrap_or(0) as usize,
                )
            })
            .unwrap_or((0, 0));
        let raw = &bin[off..off + len];
        println!("image[{idx}] name={name} mime={mime} bytes={len}");
        match image::load_from_memory(raw) {
            Ok(img) => {
                let rgba = img.to_rgba8();
                let (w, h) = (rgba.width() as usize, rgba.height() as usize);
                let lv = Level {
                    w,
                    h,
                    rgba: rgba.into_raw(),
                };
                level_stats("  src", &lv);
                mip0_analysis(&lv);
                let mut sim = Level {
                    w: lv.w,
                    h: lv.h,
                    rgba: lv.rgba.clone(),
                };
                let mut k = 0usize;
                while (sim.w > 4 || sim.h > 4) && k < 12 {
                    sim = box_down(&sim);
                    k += 1;
                    level_stats(&format!("  sim{k}"), &sim);
                }
                save_png(outdir, &format!("{stem_base}-img{idx}"), &lv, false);
            }
            Err(e) => println!("  decode failed: {e}"),
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 3 {
        eprintln!("usage: atlasprobe <bundle|glb> <path> <outdir>");
        std::process::exit(2);
    }
    std::fs::create_dir_all(&args[2]).expect("create outdir");
    match args[0].as_str() {
        "bundle" => cmd_bundle(&args[1], &args[2]),
        "glb" => cmd_glb(&args[1], &args[2]),
        m => {
            eprintln!("unknown mode {m}");
            std::process::exit(2);
        }
    }
}
