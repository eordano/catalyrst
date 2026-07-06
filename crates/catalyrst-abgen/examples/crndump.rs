use abgen::unity::bundle_file::{Bundle, FileContent};

fn bc_decode(fmt: i64, bytes: &[u8], w: usize, h: usize) -> Option<Vec<u8>> {
    let mut out = vec![0u32; w * h];
    let ok = match fmt {
        25 => texture2ddecoder::decode_bc7(bytes, w, h, &mut out).is_ok(),
        12 => texture2ddecoder::decode_bc3(bytes, w, h, &mut out).is_ok(),
        10 => texture2ddecoder::decode_bc1(bytes, w, h, &mut out).is_ok(),
        _ => false,
    };
    if !ok {
        return None;
    }
    let mut rgba = vec![0u8; w * h * 4];
    for (i, px) in out.iter().enumerate() {
        rgba[i * 4] = ((px >> 16) & 0xff) as u8;
        rgba[i * 4 + 1] = ((px >> 8) & 0xff) as u8;
        rgba[i * 4 + 2] = (px & 0xff) as u8;
        rgba[i * 4 + 3] = ((px >> 24) & 0xff) as u8;
    }
    Some(rgba)
}

fn main() {
    let mut args = std::env::args().skip(1);
    let outdir = args.next().expect("usage: crndump <outdir> <bundle>...");
    std::fs::create_dir_all(&outdir).expect("create outdir");
    for path in args {
        println!("== {path}");
        let data = match std::fs::read(&path) {
            Ok(d) => d,
            Err(e) => {
                println!("  read error: {e}");
                continue;
            }
        };
        let bundle = match Bundle::load_bytes(&data) {
            Ok(b) => b,
            Err(e) => {
                println!("  parse error: {e:#}");
                continue;
            }
        };
        let tag = std::path::Path::new(&path)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("bundle")
            .to_string();
        let mut raws: std::collections::HashMap<String, Vec<u8>> = std::collections::HashMap::new();
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
                if obj.class_id != 28 {
                    continue;
                }
                let v = match sf.read_typetree(obj) {
                    Ok(v) => v,
                    Err(_) => continue,
                };
                let name = v.get("m_Name").and_then(|x| x.as_str()).unwrap_or("?");
                let fmt = v
                    .get("m_TextureFormat")
                    .and_then(|x| x.as_i64())
                    .unwrap_or(-1);
                let w = v.get("m_Width").and_then(|x| x.as_i64()).unwrap_or(0) as usize;
                let h = v.get("m_Height").and_then(|x| x.as_i64()).unwrap_or(0) as usize;
                let cs = v.get("m_ColorSpace").and_then(|x| x.as_i64()).unwrap_or(-1);
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
                if bytes.is_empty() || w == 0 || h == 0 {
                    println!("  Texture2D {name} fmt={fmt} {w}x{h}: no payload");
                    continue;
                }
                let rgba = match fmt {
                    28 | 29 => match crunch_ffi::crn_decompress_level0(&bytes) {
                        Some(d) => {
                            println!(
                                "  Texture2D {name} fmt={fmt} {w}x{h} cs={cs} CRN(fmt={} levels={})",
                                d.format, d.levels
                            );
                            Some(d.rgba)
                        }
                        None => {
                            println!("  Texture2D {name} fmt={fmt}: CRN DECODE FAILED");
                            None
                        }
                    },
                    25 | 12 | 10 => {
                        let r = bc_decode(fmt, &bytes, w, h);
                        println!(
                            "  Texture2D {name} fmt={fmt} {w}x{h} cs={cs} bc-decode={}",
                            if r.is_some() { "ok" } else { "FAILED" }
                        );
                        r
                    }
                    _ => {
                        println!("  Texture2D {name} fmt={fmt} {w}x{h} cs={cs}: unhandled fmt");
                        None
                    }
                };
                let Some(rgba) = rgba else { continue };
                if rgba.len() != w * h * 4 {
                    println!("    size mismatch: {} vs {}", rgba.len(), w * h * 4);
                    continue;
                }
                let mut flipped = vec![0u8; rgba.len()];
                for y in 0..h {
                    flipped[y * w * 4..(y + 1) * w * 4]
                        .copy_from_slice(&rgba[(h - 1 - y) * w * 4..(h - y) * w * 4]);
                }
                let mut m = [0f64; 4];
                for px in flipped.chunks_exact(4) {
                    for c in 0..4 {
                        m[c] += px[c] as f64;
                    }
                }
                let n = (w * h) as f64;
                println!(
                    "    means R={:.1} G={:.1} B={:.1} A={:.1}",
                    m[0] / n,
                    m[1] / n,
                    m[2] / n,
                    m[3] / n
                );
                let img =
                    image::RgbaImage::from_raw(w as u32, h as u32, flipped).expect("image buffer");
                let out = format!("{outdir}/{tag}-{name}-fmt{fmt}.png");
                img.save(&out).expect("save png");
                println!("    -> {out}");
            }
        }
    }
}
