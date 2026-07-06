use anyhow::{anyhow, Context, Result};
use serde::Serialize;
use std::collections::{HashMap, HashSet};

#[cfg(not(target_arch = "wasm32"))]
#[path = "placements_native.rs"]
mod placements_native;
#[cfg(not(target_arch = "wasm32"))]
pub use placements_native::*;

pub const ISS_MANIFEST_BASE: &str =
    "https://lod-generator-unity-cdn.decentraland.org/lods-unity/manifests";
pub const ISS_SUFFIX: &str = "_InitialSceneState.json";
pub const MANIFEST_BUILDER_REPO: &str =
    "https://github.com/decentraland/scene-lod-entities-manifest-builder";
pub const MANIFEST_OUTPUT_DIR: &str = "output-manifests";
pub const MANIFEST_SUFFIX: &str = "-lod-manifest.json";

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Placement {
    pub glb_hash: Option<String>,
    pub glb_file: Option<String>,
    pub position: [f64; 3],
    pub rotation: [f64; 4],
    pub scale: [f64; 3],
}

impl Default for Placement {
    fn default() -> Self {
        Placement {
            glb_hash: None,
            glb_file: None,
            position: [0.0; 3],
            rotation: [0.0, 0.0, 0.0, 1.0],
            scale: [1.0; 3],
        }
    }
}

fn cmp_f64s(a: &[f64], b: &[f64]) -> std::cmp::Ordering {
    for (x, y) in a.iter().zip(b.iter()) {
        let o = x.total_cmp(y);
        if o != std::cmp::Ordering::Equal {
            return o;
        }
    }
    std::cmp::Ordering::Equal
}

pub fn sort_placements(list: &mut [Placement]) {
    list.sort_by(|a, b| {
        (a.glb_hash.as_deref(), a.glb_file.as_deref())
            .cmp(&(b.glb_hash.as_deref(), b.glb_file.as_deref()))
            .then_with(|| cmp_f64s(&a.position, &b.position))
            .then_with(|| cmp_f64s(&a.rotation, &b.rotation))
            .then_with(|| cmp_f64s(&a.scale, &b.scale))
    });
}

fn num_or(v: Option<&serde_json::Value>, default: f64) -> f64 {
    v.and_then(|x| x.as_f64()).unwrap_or(default)
}

fn vec3_or(v: Option<&serde_json::Value>, default: [f64; 3]) -> [f64; 3] {
    match v {
        Some(m) => [
            num_or(m.get("x"), default[0]),
            num_or(m.get("y"), default[1]),
            num_or(m.get("z"), default[2]),
        ],
        None => default,
    }
}

fn quat_or_identity(v: Option<&serde_json::Value>) -> [f64; 4] {
    match v {
        Some(m) => [
            num_or(m.get("x"), 0.0),
            num_or(m.get("y"), 0.0),
            num_or(m.get("z"), 0.0),
            num_or(m.get("w"), 1.0),
        ],
        None => [0.0, 0.0, 0.0, 1.0],
    }
}

pub fn parse_iss(bytes: &[u8]) -> Result<Vec<Placement>> {
    let v: serde_json::Value =
        serde_json::from_slice(bytes).context("ISS descriptor is not JSON")?;
    let assets = v
        .get("assets")
        .and_then(|a| a.as_array())
        .ok_or_else(|| anyhow!("ISS descriptor has no assets array"))?;
    let mut out = Vec::new();
    for a in assets {
        let Some(hash) = a.get("hash").and_then(|h| h.as_str()) else {
            continue;
        };
        out.push(Placement {
            glb_hash: Some(hash.to_string()),
            glb_file: None,
            position: vec3_or(a.get("position"), [0.0; 3]),
            rotation: quat_or_identity(a.get("rotation")),
            scale: vec3_or(a.get("scale"), [1.0; 3]),
        });
    }
    sort_placements(&mut out);
    Ok(out)
}

pub fn iss_descriptor(scene_id: &str, placements: &[(String, &Placement)]) -> serde_json::Value {
    let assets: Vec<serde_json::Value> = placements
        .iter()
        .map(|(hash, p)| {
            serde_json::json!({
                "hash": hash,
                "position": {"x": p.position[0], "y": p.position[1], "z": p.position[2]},
                "rotation": {"x": p.rotation[0], "y": p.rotation[1], "z": p.rotation[2], "w": p.rotation[3]},
                "scale": {"x": p.scale[0], "y": p.scale[1], "z": p.scale[2]},
            })
        })
        .collect();
    serde_json::json!({
        "version": 1,
        "sceneId": scene_id,
        "assets": assets,
    })
}

fn quat_mul(a: [f64; 4], b: [f64; 4]) -> [f64; 4] {
    [
        a[3] * b[0] + a[0] * b[3] + a[1] * b[2] - a[2] * b[1],
        a[3] * b[1] - a[0] * b[2] + a[1] * b[3] + a[2] * b[0],
        a[3] * b[2] + a[0] * b[1] - a[1] * b[0] + a[2] * b[3],
        a[3] * b[3] - a[0] * b[0] - a[1] * b[1] - a[2] * b[2],
    ]
}

fn quat_rotate(q: [f64; 4], v: [f64; 3]) -> [f64; 3] {
    let u = [q[0], q[1], q[2]];
    let s = q[3];
    let cross = |a: [f64; 3], b: [f64; 3]| {
        [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ]
    };
    let uv = cross(u, v);
    let uuv = cross(u, uv);
    [
        v[0] + 2.0 * (s * uv[0] + uuv[0]),
        v[1] + 2.0 * (s * uv[1] + uuv[1]),
        v[2] + 2.0 * (s * uv[2] + uuv[2]),
    ]
}

#[derive(Clone, Debug)]
struct Trs {
    position: [f64; 3],
    rotation: [f64; 4],
    scale: [f64; 3],
    parent: i64,
}

impl Default for Trs {
    fn default() -> Self {
        Trs {
            position: [0.0; 3],
            rotation: [0.0, 0.0, 0.0, 1.0],
            scale: [1.0; 3],
            parent: 0,
        }
    }
}

fn compose(parent: &Trs, child: &Trs) -> Trs {
    let scaled = [
        parent.scale[0] * child.position[0],
        parent.scale[1] * child.position[1],
        parent.scale[2] * child.position[2],
    ];
    let rotated = quat_rotate(parent.rotation, scaled);
    Trs {
        position: [
            parent.position[0] + rotated[0],
            parent.position[1] + rotated[1],
            parent.position[2] + rotated[2],
        ],
        rotation: quat_mul(parent.rotation, child.rotation),
        scale: [
            parent.scale[0] * child.scale[0],
            parent.scale[1] * child.scale[1],
            parent.scale[2] * child.scale[2],
        ],
        parent: 0,
    }
}

fn world_of(eid: i64, transforms: &HashMap<i64, Trs>, visiting: &mut HashSet<i64>) -> Trs {
    let Some(local) = transforms.get(&eid) else {
        return Trs::default();
    };
    if local.parent == 0 || !transforms.contains_key(&local.parent) || !visiting.insert(eid) {
        return local.clone();
    }
    let parent_world = world_of(local.parent, transforms, visiting);
    visiting.remove(&eid);
    compose(&parent_world, local)
}

#[derive(Clone, Debug, Default)]
pub struct ManifestPlacements {
    pub placements: Vec<Placement>,
    pub skipped_mesh_renderer: usize,
    pub unresolved_src: usize,
}

pub fn parse_lod_manifest_full(
    bytes: &[u8],
    content_by_file: &HashMap<String, String>,
) -> Result<ManifestPlacements> {
    let v: serde_json::Value = serde_json::from_slice(bytes).context("lod manifest is not JSON")?;
    let rows = v
        .as_array()
        .ok_or_else(|| anyhow!("lod manifest is not a JSON array"))?;
    let lowered: HashMap<String, &String> = content_by_file
        .iter()
        .map(|(k, v)| (k.to_lowercase(), v))
        .collect();
    let mut transforms: HashMap<i64, Trs> = HashMap::new();
    let mut gltf_srcs: Vec<(i64, String)> = Vec::new();
    let mut gltf_entities: HashSet<i64> = HashSet::new();
    let mut mesh_renderer_entities: HashSet<i64> = HashSet::new();
    for row in rows {
        let Some(eid) = row.get("entityId").and_then(|x| x.as_i64()) else {
            continue;
        };
        let name = row
            .get("componentName")
            .and_then(|x| x.as_str())
            .unwrap_or("");
        let data = row.get("data");
        match name {
            "core::Transform" => {
                transforms.insert(
                    eid,
                    Trs {
                        position: vec3_or(data.and_then(|d| d.get("position")), [0.0; 3]),
                        rotation: quat_or_identity(data.and_then(|d| d.get("rotation"))),
                        scale: vec3_or(data.and_then(|d| d.get("scale")), [1.0; 3]),
                        parent: data
                            .and_then(|d| d.get("parent"))
                            .and_then(|p| p.as_i64())
                            .unwrap_or(0),
                    },
                );
            }
            "core::GltfContainer" => {
                if let Some(src) = data.and_then(|d| d.get("src")).and_then(|s| s.as_str()) {
                    gltf_srcs.push((eid, src.to_string()));
                    gltf_entities.insert(eid);
                }
            }
            "core::MeshRenderer" => {
                mesh_renderer_entities.insert(eid);
            }
            _ => {}
        }
    }
    let mut out = ManifestPlacements {
        skipped_mesh_renderer: mesh_renderer_entities
            .iter()
            .filter(|e| !gltf_entities.contains(e))
            .count(),
        ..Default::default()
    };
    for (eid, src) in gltf_srcs {
        let world = world_of(eid, &transforms, &mut HashSet::new());
        let glb_hash = lowered.get(&src.to_lowercase()).map(|h| (*h).clone());
        if glb_hash.is_none() {
            out.unresolved_src += 1;
        }
        out.placements.push(Placement {
            glb_hash,
            glb_file: Some(src),
            position: world.position,
            rotation: world.rotation,
            scale: world.scale,
        });
    }
    sort_placements(&mut out.placements);
    Ok(out)
}

pub fn parse_lod_manifest(
    bytes: &[u8],
    content_by_file: &HashMap<String, String>,
) -> Result<Vec<Placement>> {
    Ok(parse_lod_manifest_full(bytes, content_by_file)?.placements)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f64, b: f64) -> bool {
        (a - b).abs() <= 1e-9 * b.abs().max(1.0)
    }

    fn approx3(a: [f64; 3], b: [f64; 3]) -> bool {
        a.iter().zip(b.iter()).all(|(x, y)| approx(*x, *y))
    }

    const ISS_FIXTURE: &str = r#"{
    "version": 1,
    "sceneId": "bafkreifz6o7w75gy5t3ymlelhk4kuir2t7324vchat5kevy5vbkjmicvim",
    "assets": [
        {
            "hash": "bafkreiak47hgur7axdwsv53bu6vja5bapfgka3tx4ac6spf2rvspk33ipa",
            "position": {
                "x": 88.8968276977539,
                "y": 0.2825070321559906,
                "z": 13.414548873901368
            },
            "rotation": {
                "x": 0.0,
                "y": 0.7071065902709961,
                "z": 0.0,
                "w": 0.7071070671081543
            },
            "scale": {
                "x": 0.9699999690055847,
                "y": 0.9700000286102295,
                "z": 0.9699999690055847
            }
        },
        {
            "hash": "bafkreiak47hgur7axdwsv53bu6vja5bapfgka3tx4ac6spf2rvspk33ipa",
            "position": {
                "x": 93.97085571289063,
                "y": 0.2825070321559906,
                "z": 8.382261276245118
            },
            "rotation": {
                "x": 0.0,
                "y": -2.980231954552437e-7,
                "z": 0.0,
                "w": 1.0
            },
            "scale": {
                "x": 0.9700000286102295,
                "y": 0.9700000286102295,
                "z": 0.9700000286102295
            }
        },
        {
            "hash": "aaadefaults",
            "position": {
                "x": 1.0,
                "y": 2.0,
                "z": 3.0
            }
        }
    ]
}"#;

    #[test]
    fn iss_fixture_parses() {
        let got = parse_iss(ISS_FIXTURE.as_bytes()).unwrap();
        assert_eq!(got.len(), 3);
        assert_eq!(got[0].glb_hash.as_deref(), Some("aaadefaults"));
        assert_eq!(got[0].position, [1.0, 2.0, 3.0]);
        assert_eq!(got[0].rotation, [0.0, 0.0, 0.0, 1.0]);
        assert_eq!(got[0].scale, [1.0, 1.0, 1.0]);
        assert_eq!(got[0].glb_file, None);
        let a = &got[1];
        assert_eq!(
            a.glb_hash.as_deref(),
            Some("bafkreiak47hgur7axdwsv53bu6vja5bapfgka3tx4ac6spf2rvspk33ipa")
        );
        assert_eq!(
            a.position,
            [88.8968276977539, 0.2825070321559906, 13.414548873901368]
        );
        assert_eq!(
            a.rotation,
            [0.0, 0.7071065902709961, 0.0, 0.7071070671081543]
        );
        assert_eq!(
            a.scale,
            [0.9699999690055847, 0.9700000286102295, 0.9699999690055847]
        );
        let b = &got[2];
        assert_eq!(
            b.position,
            [93.97085571289063, 0.2825070321559906, 8.382261276245118]
        );
        assert_eq!(b.rotation, [0.0, -2.980231954552437e-7, 0.0, 1.0]);
    }

    #[test]
    fn iss_descriptor_round_trips_bit_exact() {
        let want = parse_iss(ISS_FIXTURE.as_bytes()).unwrap();
        assert_eq!(want.len(), 3);
        let rebuilt: Vec<(String, &Placement)> = want
            .iter()
            .map(|p| (p.glb_hash.clone().unwrap(), p))
            .collect();
        let sid = "bafkreifz6o7w75gy5t3ymlelhk4kuir2t7324vchat5kevy5vbkjmicvim";
        let doc = iss_descriptor(sid, &rebuilt);
        assert_eq!(doc["version"], serde_json::json!(1));
        assert_eq!(doc["sceneId"], serde_json::json!(sid));
        assert_eq!(doc["assets"].as_array().unwrap().len(), 3);
        let bytes = serde_json::to_vec(&doc).unwrap();
        let got = parse_iss(&bytes).unwrap();
        assert_eq!(got, want);
        assert_eq!(got[0].glb_hash.as_deref(), Some("aaadefaults"));
        assert_eq!(got[0].rotation, [0.0, 0.0, 0.0, 1.0]);
        assert_eq!(got[0].scale, [1.0, 1.0, 1.0]);
        assert_eq!(
            got[1].scale,
            [0.9699999690055847, 0.9700000286102295, 0.9699999690055847]
        );
        assert_eq!(
            got[1].rotation,
            [0.0, 0.7071065902709961, 0.0, 0.7071070671081543]
        );
        assert_eq!(got[2].rotation, [0.0, -2.980231954552437e-7, 0.0, 1.0]);
        let again = serde_json::to_vec(&iss_descriptor(sid, &rebuilt)).unwrap();
        assert_eq!(bytes, again);
        let pretty = serde_json::to_string_pretty(&doc).unwrap();
        assert_eq!(parse_iss(pretty.as_bytes()).unwrap(), want);
        assert!(pretty.contains("0.9699999690055847"));
        assert!(pretty.contains("-2.980231954552437e-7"));
    }

    const MANIFEST_FIXTURE: &str = r#"[
  {
    "entityId": 512,
    "componentId": 1,
    "componentName": "core::Transform",
    "data": {
      "position": {
        "x": null,
        "y": 0,
        "z": null
      },
      "rotation": {
        "x": 0,
        "y": 0.7071067690849304,
        "z": 0,
        "w": 0.7071067690849304
      },
      "scale": {
        "x": 16,
        "y": 1,
        "z": 16
      },
      "parent": 0
    }
  },
  {
    "entityId": 512,
    "componentId": 1041,
    "componentName": "core::GltfContainer",
    "data": {
      "src": "assets/road-driveway-double.glb",
      "visibleMeshesCollisionMask": 2,
      "invisibleMeshesCollisionMask": 0
    }
  }
]"#;

    #[test]
    fn lod_manifest_fixture_parses() {
        let mut content = HashMap::new();
        content.insert(
            "Assets/Road-Driveway-Double.GLB".to_string(),
            "bafkreiroadhash".to_string(),
        );
        let got = parse_lod_manifest_full(MANIFEST_FIXTURE.as_bytes(), &content).unwrap();
        assert_eq!(got.placements.len(), 1);
        assert_eq!(got.skipped_mesh_renderer, 0);
        assert_eq!(got.unresolved_src, 0);
        let p = &got.placements[0];
        assert_eq!(p.glb_hash.as_deref(), Some("bafkreiroadhash"));
        assert_eq!(
            p.glb_file.as_deref(),
            Some("assets/road-driveway-double.glb")
        );
        assert_eq!(p.position, [0.0, 0.0, 0.0]);
        assert_eq!(
            p.rotation,
            [0.0, 0.7071067690849304, 0.0, 0.7071067690849304]
        );
        assert_eq!(p.scale, [16.0, 1.0, 16.0]);
    }

    #[test]
    fn lod_manifest_parent_chain_composes() {
        let s2 = std::f64::consts::FRAC_1_SQRT_2;
        let fixture = serde_json::json!([
            {
                "entityId": 600,
                "componentName": "core::Transform",
                "data": {
                    "position": {"x": 10.0, "y": 0.0, "z": 0.0},
                    "rotation": {"x": 0.0, "y": s2, "z": 0.0, "w": s2},
                    "scale": {"x": 2.0, "y": 2.0, "z": 2.0},
                    "parent": 0
                }
            },
            {
                "entityId": 601,
                "componentName": "core::Transform",
                "data": {
                    "position": {"x": 1.0, "y": 0.0, "z": 0.0},
                    "rotation": {"x": 0.0, "y": 0.0, "z": 0.0, "w": 1.0},
                    "scale": {"x": 1.0, "y": 1.0, "z": 1.0},
                    "parent": 600
                }
            },
            {
                "entityId": 601,
                "componentName": "core::GltfContainer",
                "data": {"src": "models/child.glb"}
            }
        ]);
        let bytes = serde_json::to_vec(&fixture).unwrap();
        let mut content = HashMap::new();
        content.insert("models/child.glb".to_string(), "hchild".to_string());
        let got = parse_lod_manifest_full(&bytes, &content).unwrap();
        assert_eq!(got.placements.len(), 1);
        let p = &got.placements[0];
        assert!(approx3(p.position, [10.0, 0.0, -2.0]), "{:?}", p.position);
        assert!(approx(p.rotation[1], s2) && approx(p.rotation[3], s2));
        assert!(approx3(p.scale, [2.0, 2.0, 2.0]));
    }

    #[test]
    fn lod_manifest_skips_and_unresolved_counted() {
        let fixture = serde_json::json!([
            {
                "entityId": 700,
                "componentName": "core::MeshRenderer",
                "data": {"mesh": {"$case": "box", "box": {"uvs": []}}}
            },
            {
                "entityId": 701,
                "componentName": "core::GltfContainer",
                "data": {"src": "models/missing.glb"}
            }
        ]);
        let bytes = serde_json::to_vec(&fixture).unwrap();
        let content = HashMap::new();
        let got = parse_lod_manifest_full(&bytes, &content).unwrap();
        assert_eq!(got.skipped_mesh_renderer, 1);
        assert_eq!(got.unresolved_src, 1);
        assert_eq!(got.placements.len(), 1);
        let p = &got.placements[0];
        assert_eq!(p.glb_hash, None);
        assert_eq!(p.glb_file.as_deref(), Some("models/missing.glb"));
        assert_eq!(p.position, [0.0, 0.0, 0.0]);
        assert_eq!(p.rotation, [0.0, 0.0, 0.0, 1.0]);
        assert_eq!(p.scale, [1.0, 1.0, 1.0]);
    }

    #[test]
    fn deterministic_ordering() {
        let mk = |hash: Option<&str>, file: Option<&str>, x: f64| Placement {
            glb_hash: hash.map(String::from),
            glb_file: file.map(String::from),
            position: [x, 0.0, 0.0],
            ..Default::default()
        };
        let mut a = vec![
            mk(Some("b"), None, 1.0),
            mk(Some("a"), None, 2.0),
            mk(Some("a"), None, -1.0),
            mk(None, Some("z.glb"), 0.0),
            mk(Some("b"), Some("f.glb"), 1.0),
        ];
        let mut b = a.clone();
        b.reverse();
        sort_placements(&mut a);
        sort_placements(&mut b);
        assert_eq!(a, b);
        assert_eq!(a[0].glb_hash, None);
        assert_eq!(a[1].glb_hash.as_deref(), Some("a"));
        assert_eq!(a[1].position[0], -1.0);
        assert_eq!(a[2].position[0], 2.0);
        assert_eq!(a[3].glb_file, None);
        assert_eq!(a[4].glb_file.as_deref(), Some("f.glb"));
    }

    #[test]
    fn plaza_iss_full_guarded() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../docs/testing/lodgen-firststab-20260708/prod/bafkreifz6o7w75gy5t3ymlelhk4kuir2t7324vchat5kevy5vbkjmicvim_InitialSceneState.json"
        );
        let Ok(bytes) = std::fs::read(path) else {
            return;
        };
        let got = parse_iss(&bytes).unwrap();
        assert_eq!(got.len(), 639);
        assert!(got.iter().all(|p| p.glb_hash.is_some()));
        assert!(got.iter().all(|p| {
            p.position.iter().all(|v| v.is_finite())
                && p.rotation.iter().all(|v| v.is_finite())
                && p.scale.iter().all(|v| v.is_finite())
        }));
        let hashes: HashSet<&str> = got.iter().filter_map(|p| p.glb_hash.as_deref()).collect();
        assert!(hashes.len() > 1);
    }
}
