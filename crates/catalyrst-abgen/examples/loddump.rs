use abgen::unity::bundle_file::{Bundle, FileContent};
use abgen::value::Value;

fn pptr(v: &Value) -> String {
    let f = v.get("m_FileID").and_then(|x| x.as_i64()).unwrap_or(-999);
    let p = v.get("m_PathID").and_then(|x| x.as_i64()).unwrap_or(-999);
    format!("(file={f} path={p})")
}

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
        println!(
            "  header: format_version={} version_player={} version_engine={} flags=0x{:x}",
            bundle.format_version, bundle.version_player, bundle.version_engine, bundle.flags
        );
        for f in &bundle.files {
            match &f.content {
                FileContent::Raw(bytes) => {
                    println!(
                        "  raw file {} bytes={} entry_flags=0x{:x}",
                        f.name,
                        bytes.len(),
                        f.flags
                    );
                }
                FileContent::Serialized(sf) => {
                    println!(
                        "  serialized file {} entry_flags=0x{:x} sf_version={} unity_version={} target_platform={} objects={}",
                        f.name,
                        f.flags,
                        sf.version,
                        sf.unity_version,
                        sf.target_platform,
                        sf.objects.len()
                    );
                    for (i, ext) in sf.externals.iter().enumerate() {
                        let guid: String = ext.guid.iter().map(|b| format!("{b:02x}")).collect();
                        println!(
                            "    external[{}] path={} guid={} type={}",
                            i + 1,
                            ext.path,
                            guid,
                            ext.r#type
                        );
                    }
                    for obj in &sf.objects {
                        let v = match sf.read_typetree(obj) {
                            Ok(v) => v,
                            Err(e) => {
                                println!(
                                    "    class={} pid={} <typetree error: {e}>",
                                    obj.class_id, obj.path_id
                                );
                                continue;
                            }
                        };
                        let name = v.get("m_Name").and_then(|x| x.as_str()).unwrap_or("");
                        match obj.class_id {
                            1 => {
                                let comps: Vec<String> = v
                                    .get("m_Component")
                                    .and_then(|c| c.as_array())
                                    .map(|a| {
                                        a.iter()
                                            .filter_map(|c| {
                                                c.get("component").map(pptr).or_else(|| {
                                                    c.as_array().and_then(|kv| kv.get(1)).map(pptr)
                                                })
                                            })
                                            .collect()
                                    })
                                    .unwrap_or_default();
                                println!(
                                    "    GameObject pid={} name={name} components={comps:?}",
                                    obj.path_id
                                );
                            }
                            4 => {
                                let go = v.get("m_GameObject").map(pptr).unwrap_or_default();
                                let father = v.get("m_Father").map(pptr).unwrap_or_default();
                                let children: Vec<String> = v
                                    .get("m_Children")
                                    .and_then(|c| c.as_array())
                                    .map(|a| a.iter().map(pptr).collect())
                                    .unwrap_or_default();
                                let lp = v.get("m_LocalPosition");
                                let ls = v.get("m_LocalScale");
                                let pos = lp
                                    .map(|p| {
                                        format!(
                                            "({},{},{})",
                                            p.get("x").and_then(|x| x.as_f64()).unwrap_or(0.0),
                                            p.get("y").and_then(|x| x.as_f64()).unwrap_or(0.0),
                                            p.get("z").and_then(|x| x.as_f64()).unwrap_or(0.0)
                                        )
                                    })
                                    .unwrap_or_default();
                                let scale = ls
                                    .map(|p| {
                                        format!(
                                            "({},{},{})",
                                            p.get("x").and_then(|x| x.as_f64()).unwrap_or(0.0),
                                            p.get("y").and_then(|x| x.as_f64()).unwrap_or(0.0),
                                            p.get("z").and_then(|x| x.as_f64()).unwrap_or(0.0)
                                        )
                                    })
                                    .unwrap_or_default();
                                println!(
                                    "    Transform pid={} go={go} father={father} children={children:?} pos={pos} scale={scale}",
                                    obj.path_id
                                );
                            }
                            43 => {
                                let vc = v
                                    .get("m_VertexData")
                                    .and_then(|vd| vd.get("m_VertexCount"))
                                    .and_then(|x| x.as_i64())
                                    .unwrap_or(-1);
                                let idxfmt = v
                                    .get("m_IndexFormat")
                                    .and_then(|x| x.as_i64())
                                    .unwrap_or(-1);
                                let ib_len = v
                                    .get("m_IndexBuffer")
                                    .and_then(|x| {
                                        x.as_bytes()
                                            .map(|b| b.len() as i64)
                                            .or_else(|| x.as_array().map(|a| a.len() as i64))
                                    })
                                    .unwrap_or(-1);
                                let subs: Vec<String> = v
                                    .get("m_SubMeshes")
                                    .and_then(|s| s.as_array())
                                    .map(|a| {
                                        a.iter()
                                            .map(|sm| {
                                                format!(
                                                    "{{firstByte={} indexCount={} topology={} firstVertex={} vertexCount={}}}",
                                                    sm.get("firstByte").and_then(|x| x.as_i64()).unwrap_or(-1),
                                                    sm.get("indexCount").and_then(|x| x.as_i64()).unwrap_or(-1),
                                                    sm.get("topology").and_then(|x| x.as_i64()).unwrap_or(-1),
                                                    sm.get("firstVertex").and_then(|x| x.as_i64()).unwrap_or(-1),
                                                    sm.get("vertexCount").and_then(|x| x.as_i64()).unwrap_or(-1)
                                                )
                                            })
                                            .collect()
                                    })
                                    .unwrap_or_default();
                                let total_idx: i64 = v
                                    .get("m_SubMeshes")
                                    .and_then(|s| s.as_array())
                                    .map(|a| {
                                        a.iter()
                                            .filter_map(|sm| {
                                                sm.get("indexCount").and_then(|x| x.as_i64())
                                            })
                                            .sum()
                                    })
                                    .unwrap_or(0);
                                println!(
                                    "    Mesh pid={} name={name} vertexCount={vc} indexFormat={idxfmt} indexBufferLen={ib_len} submeshes={} totalIndices={total_idx} totalTris={}",
                                    obj.path_id,
                                    subs.len(),
                                    total_idx / 3
                                );
                                for s in subs {
                                    println!("      submesh {s}");
                                }
                            }
                            21 => {
                                let shader = v.get("m_Shader").map(pptr).unwrap_or_default();
                                println!(
                                    "    Material pid={} name={name} shader={shader}",
                                    obj.path_id
                                );
                            }
                            49 => {
                                let script = v
                                    .get("m_Script")
                                    .map(|s| {
                                        s.as_str().map(String::from).unwrap_or_else(|| {
                                            s.as_bytes()
                                                .map(|b| String::from_utf8_lossy(b).into_owned())
                                                .unwrap_or_default()
                                        })
                                    })
                                    .unwrap_or_default();
                                println!(
                                    "    TextAsset pid={} name={name} script={script}",
                                    obj.path_id
                                );
                            }
                            142 => {
                                println!(
                                    "    AssetBundle pid={} name={name} deps={:?} mainAsset={:?}",
                                    obj.path_id,
                                    v.get("m_Dependencies"),
                                    v.get("m_MainAsset").map(pptr)
                                );
                                if let Some(cont) = v.get("m_Container").and_then(|c| c.as_array())
                                {
                                    for kv in cont {
                                        let Some(pair) = kv.as_array() else { continue };
                                        let key =
                                            pair.first().and_then(|k| k.as_str()).unwrap_or("?");
                                        let info = pair.get(1);
                                        let asset = info
                                            .and_then(|i| i.get("asset"))
                                            .map(pptr)
                                            .unwrap_or_default();
                                        let pre = info
                                            .and_then(|i| i.get("preloadIndex"))
                                            .and_then(|x| x.as_i64())
                                            .unwrap_or(-1);
                                        let sz = info
                                            .and_then(|i| i.get("preloadSize"))
                                            .and_then(|x| x.as_i64())
                                            .unwrap_or(-1);
                                        println!(
                                            "      container {key} asset={asset} preloadIndex={pre} preloadSize={sz}"
                                        );
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
    }
}
