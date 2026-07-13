use abgen::unity::bundle_file::{Bundle, FileContent};
use abgen::value::Value;
use anyhow::{bail, Context, Result};

#[derive(Default)]
struct Facts {
    externals: Vec<String>,
    deps: Vec<String>,
    roots: Vec<(String, [f64; 3])>,
    materials: Vec<MatFacts>,
    textures: Vec<TexFacts>,
    meshes: Vec<MeshFacts>,
    metadata: Option<serde_json::Value>,
    container: Vec<String>,
}

struct MatFacts {
    name: String,
    shader: (i64, i64),
    plane: Option<[f64; 4]>,
    vertical: Option<[f64; 4]>,
}

struct TexFacts {
    name: String,
    fmt: i64,
    w: i64,
    h: i64,
    mips: i64,
}

struct MeshFacts {
    name: String,
    vertex_count: i64,
    index_format: i64,
    total_tris: i64,
}

fn vec3(v: Option<&Value>) -> [f64; 3] {
    let get = |k: &str| {
        v.and_then(|m| m.get(k))
            .and_then(|x| x.as_f64())
            .unwrap_or(f64::NAN)
    };
    [get("x"), get("y"), get("z")]
}

fn color4(v: &Value) -> [f64; 4] {
    let get = |k: &str| v.get(k).and_then(|x| x.as_f64()).unwrap_or(f64::NAN);
    [get("r"), get("g"), get("b"), get("a")]
}

fn saved_color(mat: &Value, name: &str) -> Option<[f64; 4]> {
    let colors = mat.get("m_SavedProperties")?.get("m_Colors")?.as_array()?;
    for entry in colors {
        let pair = entry.as_array()?;
        if pair.first()?.as_str()? == name {
            return Some(color4(pair.get(1)?));
        }
    }
    None
}

fn extract_facts(path: &str) -> Result<Facts> {
    let data = std::fs::read(path).with_context(|| format!("read {path}"))?;
    let bundle = Bundle::load_bytes(&data).with_context(|| format!("parse bundle {path}"))?;
    let mut f = Facts::default();
    let mut transforms: Vec<(i64, i64, [f64; 3])> = Vec::new();
    let mut go_names: std::collections::HashMap<i64, String> = std::collections::HashMap::new();
    for file in &bundle.files {
        let FileContent::Serialized(sf) = &file.content else {
            continue;
        };
        for ext in &sf.externals {
            f.externals.push(ext.path.clone());
        }
        for obj in &sf.objects {
            let v = sf
                .read_typetree(obj)
                .with_context(|| format!("typetree pid {}", obj.path_id))?;
            let name = v
                .get("m_Name")
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string();
            match obj.class_id {
                1 => {
                    go_names.insert(obj.path_id, name);
                }
                4 => {
                    let go = v
                        .get("m_GameObject")
                        .and_then(|p| p.get("m_PathID"))
                        .and_then(|x| x.as_i64())
                        .unwrap_or(0);
                    let father = v
                        .get("m_Father")
                        .and_then(|p| p.get("m_PathID"))
                        .and_then(|x| x.as_i64())
                        .unwrap_or(0);
                    transforms.push((go, father, vec3(v.get("m_LocalPosition"))));
                }
                21 => {
                    let fid = v
                        .get("m_Shader")
                        .and_then(|p| p.get("m_FileID"))
                        .and_then(|x| x.as_i64())
                        .unwrap_or(-1);
                    let pid = v
                        .get("m_Shader")
                        .and_then(|p| p.get("m_PathID"))
                        .and_then(|x| x.as_i64())
                        .unwrap_or(-1);
                    f.materials.push(MatFacts {
                        plane: saved_color(&v, "_PlaneClipping"),
                        vertical: saved_color(&v, "_VerticalClipping"),
                        name,
                        shader: (fid, pid),
                    });
                }
                28 => {
                    f.textures.push(TexFacts {
                        name,
                        fmt: v
                            .get("m_TextureFormat")
                            .and_then(|x| x.as_i64())
                            .unwrap_or(-1),
                        w: v.get("m_Width").and_then(|x| x.as_i64()).unwrap_or(-1),
                        h: v.get("m_Height").and_then(|x| x.as_i64()).unwrap_or(-1),
                        mips: v.get("m_MipCount").and_then(|x| x.as_i64()).unwrap_or(-1),
                    });
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
                    let total_idx: i64 = v
                        .get("m_SubMeshes")
                        .and_then(|s| s.as_array())
                        .map(|a| {
                            a.iter()
                                .filter_map(|sm| sm.get("indexCount").and_then(|x| x.as_i64()))
                                .sum()
                        })
                        .unwrap_or(0);
                    f.meshes.push(MeshFacts {
                        name,
                        vertex_count: vc,
                        index_format: idxfmt,
                        total_tris: total_idx / 3,
                    });
                }
                49 if name == "metadata" => {
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
                    f.metadata = serde_json::from_str(&script).ok();
                }
                142 => {
                    if let Some(deps) = v.get("m_Dependencies").and_then(|d| d.as_array()) {
                        for d in deps {
                            if let Some(s) = d.as_str() {
                                f.deps.push(s.to_string());
                            }
                        }
                    }
                    if let Some(cont) = v.get("m_Container").and_then(|c| c.as_array()) {
                        for kv in cont {
                            if let Some(key) = kv
                                .as_array()
                                .and_then(|p| p.first())
                                .and_then(|k| k.as_str())
                            {
                                f.container.push(key.to_string());
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }
    for (go, father, pos) in transforms {
        if father == 0 {
            let name = go_names.get(&go).cloned().unwrap_or_default();
            f.roots.push((name, pos));
        }
    }
    f.roots.sort_by(|a, b| a.0.cmp(&b.0));
    f.materials.sort_by(|a, b| a.name.cmp(&b.name));
    f.textures.sort_by(|a, b| a.name.cmp(&b.name));
    f.meshes.sort_by(|a, b| a.name.cmp(&b.name));
    f.container.sort();
    f.externals.sort();
    f.deps.sort();
    Ok(f)
}

fn approx(a: f64, b: f64) -> bool {
    if a == b {
        return true;
    }
    if a.is_nan() || b.is_nan() {
        return false;
    }
    (a - b).abs() <= 1e-4 * b.abs().max(1.0)
}

fn approx4(a: &Option<[f64; 4]>, b: &Option<[f64; 4]>) -> bool {
    match (a, b) {
        (Some(x), Some(y)) => x.iter().zip(y.iter()).all(|(p, q)| approx(*p, *q)),
        (None, None) => true,
        _ => false,
    }
}

struct Checker {
    failures: usize,
}

impl Checker {
    fn check(&mut self, label: &str, ok: bool, detail: String) {
        if ok {
            println!("PASS {label}: {detail}");
        } else {
            self.failures += 1;
            println!("FAIL {label}: {detail}");
        }
    }
}

pub(crate) fn cmd_compare(argv: &[String]) -> Result<i32> {
    if argv.len() != 2 {
        bail!("compare needs exactly two bundle paths");
    }
    let ours = extract_facts(&argv[0])?;
    let prod = extract_facts(&argv[1])?;
    let mut c = Checker { failures: 0 };

    c.check(
        "root-count",
        ours.roots.len() == 1 && prod.roots.len() == 1,
        format!("ours={} prod={}", ours.roots.len(), prod.roots.len()),
    );
    if let (Some(a), Some(b)) = (ours.roots.first(), prod.roots.first()) {
        c.check(
            "root-name",
            a.0 == b.0,
            format!("ours={} prod={}", a.0, b.0),
        );
        c.check(
            "root-position",
            a.1.iter().zip(b.1.iter()).all(|(p, q)| approx(*p, *q)),
            format!("ours={:?} prod={:?}", a.1, b.1),
        );
    }

    c.check(
        "assetbundle-deps",
        ours.deps == prod.deps,
        format!("ours={:?} prod={:?}", ours.deps, prod.deps),
    );
    c.check(
        "externals",
        ours.externals == prod.externals,
        format!("ours={:?} prod={:?}", ours.externals, prod.externals),
    );

    for m in ours.materials.iter().chain(prod.materials.iter()) {
        c.check(
            &format!("shader-pptr[{}]", m.name),
            m.shader == (1, 2_346_303_084_350_958_154),
            format!("{:?}", m.shader),
        );
    }
    let our_mat_names: Vec<&String> = ours.materials.iter().map(|m| &m.name).collect();
    let prod_mat_names: Vec<&String> = prod.materials.iter().map(|m| &m.name).collect();
    c.check(
        "material-names",
        our_mat_names == prod_mat_names,
        format!("ours={our_mat_names:?} prod={prod_mat_names:?}"),
    );
    if our_mat_names == prod_mat_names {
        for (a, b) in ours.materials.iter().zip(prod.materials.iter()) {
            c.check(
                &format!("plane-clipping[{}]", a.name),
                approx4(&a.plane, &b.plane),
                format!("ours={:?} prod={:?}", a.plane, b.plane),
            );
            c.check(
                &format!("vertical-clipping[{}]", a.name),
                approx4(&a.vertical, &b.vertical),
                format!("ours={:?} prod={:?}", a.vertical, b.vertical),
            );
        }
    }

    c.check(
        "texture-count",
        ours.textures.len() == prod.textures.len(),
        format!("ours={} prod={}", ours.textures.len(), prod.textures.len()),
    );
    if ours.textures.len() == prod.textures.len() {
        for (a, b) in ours.textures.iter().zip(prod.textures.iter()) {
            c.check(
                &format!("texture[{}]", a.name),
                a.name == b.name
                    && a.fmt == b.fmt
                    && a.fmt == 25
                    && a.w == b.w
                    && a.h == b.h
                    && a.mips == b.mips,
                format!(
                    "ours name={} fmt={} {}x{} mips={} | prod name={} fmt={} {}x{} mips={}",
                    a.name, a.fmt, a.w, a.h, a.mips, b.name, b.fmt, b.w, b.h, b.mips
                ),
            );
        }
    }

    c.check(
        "mesh-count",
        ours.meshes.len() == prod.meshes.len(),
        format!("ours={} prod={}", ours.meshes.len(), prod.meshes.len()),
    );
    if ours.meshes.len() == prod.meshes.len() {
        for (a, b) in ours.meshes.iter().zip(prod.meshes.iter()) {
            c.check(
                &format!("mesh[{}]", a.name),
                a.name == b.name
                    && a.vertex_count == b.vertex_count
                    && a.index_format == b.index_format
                    && a.total_tris == b.total_tris,
                format!(
                    "ours name={} verts={} idxfmt={} tris={} | prod name={} verts={} idxfmt={} tris={}",
                    a.name,
                    a.vertex_count,
                    a.index_format,
                    a.total_tris,
                    b.name,
                    b.vertex_count,
                    b.index_format,
                    b.total_tris
                ),
            );
        }
    }

    match (&ours.metadata, &prod.metadata) {
        (Some(a), Some(b)) => {
            for field in ["version", "dependencies", "mainAsset"] {
                c.check(
                    &format!("metadata.{field}"),
                    a.get(field) == b.get(field),
                    format!("ours={:?} prod={:?}", a.get(field), b.get(field)),
                );
            }
            let ts_ours = a.get("timestamp").and_then(|v| v.as_i64()).unwrap_or(0);
            let ts_prod = b.get("timestamp").and_then(|v| v.as_i64()).unwrap_or(0);
            if ts_ours == 0 {
                println!("INFO metadata.timestamp: ours=0 (not injected; exempt) prod={ts_prod}");
            } else {
                c.check(
                    "metadata.timestamp",
                    ts_ours == ts_prod,
                    format!("ours={ts_ours} prod={ts_prod}"),
                );
            }
        }
        (a, b) => {
            c.check(
                "metadata-present",
                a.is_some() == b.is_some(),
                format!("ours={} prod={}", a.is_some(), b.is_some()),
            );
        }
    }

    c.check(
        "container-keys",
        ours.container == prod.container,
        format!("ours={:?} prod={:?}", ours.container, prod.container),
    );

    if c.failures == 0 {
        println!("ALL CHECKS PASSED");
        Ok(0)
    } else {
        println!("{} CHECK(S) FAILED", c.failures);
        Ok(1)
    }
}
