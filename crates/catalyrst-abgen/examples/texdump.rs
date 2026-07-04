use abgen::unity::bundle_file::{Bundle, FileContent};

fn main() {
    for path in std::env::args().skip(1) {
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
        for f in &bundle.files {
            if let FileContent::Raw(bytes) = &f.content {
                println!("  raw file {} bytes={}", f.name, bytes.len());
            }
            if let FileContent::Serialized(sf) = &f.content {
                let mut per_class: std::collections::HashMap<i32, (usize, usize)> =
                    std::collections::HashMap::new();
                for obj in &sf.objects {
                    let e = per_class.entry(obj.class_id).or_default();
                    e.0 += 1;
                    e.1 += obj.data.len();
                }
                let mut cs: Vec<_> = per_class.into_iter().collect();
                cs.sort_by_key(|x| std::cmp::Reverse(x.1 .1));
                for (cid, (n, bytes)) in cs {
                    println!("  class {cid} n={n} bytes={bytes}");
                }
                for obj in &sf.objects {
                    match obj.class_id {
                        28 => {
                            let v = match sf.read_typetree(obj) {
                                Ok(v) => v,
                                Err(_) => continue,
                            };
                            let name = v.get("m_Name").and_then(|x| x.as_str()).unwrap_or("?");
                            let fmt = v
                                .get("m_TextureFormat")
                                .and_then(|x| x.as_i64())
                                .unwrap_or(-1);
                            let w = v.get("m_Width").and_then(|x| x.as_i64()).unwrap_or(-1);
                            let h = v.get("m_Height").and_then(|x| x.as_i64()).unwrap_or(-1);
                            let mips = v.get("m_MipCount").and_then(|x| x.as_i64()).unwrap_or(-1);
                            let lf = v
                                .get("m_LightmapFormat")
                                .and_then(|x| x.as_i64())
                                .unwrap_or(-1);
                            let cs = v.get("m_ColorSpace").and_then(|x| x.as_i64()).unwrap_or(-1);
                            println!(
                                "  Texture2D pid={} name={name} fmt={fmt} {w}x{h} mips={mips} lightmapFmt={lf} colorSpace={cs}",
                                obj.path_id
                            );
                        }
                        21 => {
                            let v = match sf.read_typetree(obj) {
                                Ok(v) => v,
                                Err(_) => continue,
                            };
                            let name = v.get("m_Name").and_then(|x| x.as_str()).unwrap_or("?");
                            let mut floats = String::new();
                            if let Some(items) = v
                                .get("m_SavedProperties")
                                .and_then(|sp| sp.get("m_Floats"))
                                .and_then(|x| x.as_array())
                            {
                                for it in items {
                                    let Some(kv) = it.as_array() else { continue };
                                    let k = kv.first().and_then(|x| x.as_str()).unwrap_or("");
                                    if k.contains("UVs")
                                        || k.contains("UVChannel")
                                        || k.contains("Rotation")
                                    {
                                        let val =
                                            kv.get(1).and_then(|x| x.as_f64()).unwrap_or(f64::NAN);
                                        floats.push_str(&format!("{k}={val} "));
                                    }
                                }
                            }
                            let mut texrefs = String::new();
                            if let Some(envs) = v
                                .get("m_SavedProperties")
                                .and_then(|sp| sp.get("m_TexEnvs"))
                                .and_then(|x| x.as_array())
                            {
                                for e in envs {
                                    let Some(pair) = e.as_array() else { continue };
                                    let slot = pair.first().and_then(|x| x.as_str()).unwrap_or("?");
                                    let pid = pair
                                        .get(1)
                                        .and_then(|p| p.get("m_Texture"))
                                        .and_then(|t| t.get("m_PathID"))
                                        .and_then(|x| x.as_i64())
                                        .unwrap_or(0);
                                    if pid != 0 {
                                        texrefs.push_str(&format!("{slot}->{pid} "));
                                    }
                                }
                            }
                            println!(
                                "  Material pid={} name={name} uv/rot-props: {floats} texrefs: {texrefs}",
                                obj.path_id
                            );
                        }
                        _ => {}
                    }
                }
            }
        }
    }
}
