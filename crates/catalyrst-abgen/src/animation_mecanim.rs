use crate::animation::glb;
use crate::value::Value;

const INTERP_STEP: &str = "STEP";
const INTERP_CUBICSPLINE: &str = "CUBICSPLINE";
const INTERP_LINEAR: &str = "LINEAR";

const SAMPLE_RATE: f64 = 60.0;
const WRAP_LOOP: i64 = 2;
const LOOP_PARAMETER: &str = "Loop";

const PARAM_TYPE_BOOL: i64 = 4;
const PARAM_TYPE_TRIGGER: i64 = 9;

const ATTR_POSITION: i64 = 1;
const ATTR_ROTATION: i64 = 2;
const ATTR_SCALE: i64 = 3;
const TRANSFORM_CLASS_ID: i64 = 4;

const SELECTOR_EXIT_DEST: i64 = 30000;

fn crc32(s: &str) -> u32 {
    crate::hashes::crc32(s.as_bytes())
}

fn conv_translation(v: &[f64]) -> Vec<f64> {
    let g = |i: usize| v.get(i).copied().unwrap_or(0.0);
    vec![-g(0), g(1), g(2)]
}
fn conv_scale(v: &[f64]) -> Vec<f64> {
    let g = |i: usize| v.get(i).copied().unwrap_or(1.0);
    vec![g(0), g(1), g(2)]
}
fn conv_rotation(q: &[f64]) -> Vec<f64> {
    let g = |i: usize, d: f64| q.get(i).copied().unwrap_or(d);
    vec![g(0, 0.0), -g(1, 0.0), -g(2, 0.0), g(3, 1.0)]
}

#[derive(Clone, Copy)]
struct Key {
    time: f64,
    value: f64,
    slope: f64,
    a: f64,
    b: f64,
}

fn bake_scalar_keys(times: &[f64], scalar_vals: &[f64], interp: &str) -> Vec<Key> {
    let n = times.len();
    if n == 0 {
        return Vec::new();
    }
    if n == 1 {
        return vec![Key {
            time: times[0],
            value: scalar_vals[0],
            slope: 0.0,
            a: 0.0,
            b: 0.0,
        }];
    }
    let mut out = Vec::with_capacity(n);
    if interp == INTERP_STEP {
        for i in 0..n {
            out.push(Key {
                time: times[i],
                value: scalar_vals[i],
                slope: 0.0,
                a: 0.0,
                b: 0.0,
            });
        }
        return out;
    }

    for i in 0..n {
        let v = scalar_vals[i];
        let (slope, a, b) = if i < n - 1 {
            let dt = times[i + 1] - times[i];
            if dt > 0.0 {
                let dt = dt as f32;
                let dv = (scalar_vals[i + 1] - v) as f32;
                let sec = dv / dt;
                let inv = 1.0f32 / dt;
                let inv3 = inv * inv * inv;
                let a = (2.0f32 * (sec * dt - dv)) * inv3;

                let t1 = sec * dt;
                let bq = (((3.0f32 * dv - t1) - t1) - t1) * (inv * inv);

                let b = if bq == 0.0 { 0.0f32 } else { bq };
                (sec as f64, a as f64, b as f64)
            } else {
                (0.0, 0.0, 0.0)
            }
        } else {
            (0.0, 0.0, 0.0)
        };
        out.push(Key {
            time: times[i],
            value: v,
            slope,
            a,
            b,
        });
    }
    out
}

fn encode_streamed_clip(curves: &[(i64, Vec<Key>)]) -> Vec<Value> {
    let mut times: Vec<f64> = Vec::new();
    for (_, keys) in curves.iter() {
        for k in keys.iter() {
            times.push(k.time);
        }
    }
    if times.is_empty() {
        return Vec::new();
    }
    times.sort_by(|a, b| a.partial_cmp(b).unwrap());
    times.dedup();

    let neg = f32::from_bits(0xFF7F_FFFF);
    let pos = f32::INFINITY;

    let find_time_index = |t: f64| -> usize {
        times
            .iter()
            .position(|&x| x.to_bits() == t.to_bits())
            .or_else(|| times.iter().position(|&x| x == t))
            .expect("time present")
    };
    let mut by_time: Vec<Vec<(i64, [f32; 4])>> = vec![Vec::new(); times.len()];
    for (ci, keys) in curves.iter() {
        for k in keys.iter() {
            let ti = find_time_index(k.time);
            by_time[ti].push((
                *ci,
                [k.a as f32, k.b as f32, k.slope as f32, k.value as f32],
            ));
        }
    }

    for frame in by_time.iter_mut() {
        frame.sort_by_key(|(ci, _)| *ci);
    }

    let mut buf: Vec<u8> = Vec::new();
    let emit_frame = |buf: &mut Vec<u8>, t: f32, keymap: &[(i64, [f32; 4])]| {
        buf.extend_from_slice(&t.to_le_bytes());
        buf.extend_from_slice(&(keymap.len() as i32).to_le_bytes());
        for (ci, coeffs) in keymap.iter() {
            buf.extend_from_slice(&(*ci as i32).to_le_bytes());
            for c in coeffs.iter() {
                buf.extend_from_slice(&c.to_le_bytes());
            }
        }
    };

    let mut lead: Vec<(i64, [f32; 4])> = curves
        .iter()
        .filter_map(|(ci, keys)| keys.first().map(|k| (*ci, [0.0, 0.0, 0.0, k.value as f32])))
        .collect();
    lead.sort_by_key(|(ci, _)| *ci);
    emit_frame(&mut buf, neg, &lead);
    for ti in 0..times.len() {
        let t = times[ti] as f32;
        emit_frame(&mut buf, t, &by_time[ti]);
    }
    emit_frame(&mut buf, pos, &[]);

    let mut out: Vec<Value> = Vec::with_capacity(buf.len() / 4);
    let mut i = 0;
    while i + 4 <= buf.len() {
        let u = u32::from_le_bytes([buf[i], buf[i + 1], buf[i + 2], buf[i + 3]]);
        out.push(Value::Int(u as i64));
        i += 4;
    }
    out
}

fn gather_clip_curves(
    g: &glb::Glb,
    anim: &serde_json::Value,
    names: &[String],
    parent: &[i64],
) -> (Vec<(String, i64, bool)>, Vec<(i64, Vec<Key>)>) {
    let channels = anim["channels"].as_array().cloned().unwrap_or_default();
    let samplers = anim["samplers"].as_array().cloned().unwrap_or_default();

    let mut node_order: Vec<usize> = Vec::new();

    let mut per_node: Vec<(usize, Vec<(String, usize, String)>)> = Vec::new();

    let node_pos = |po: &Vec<(usize, Vec<(String, usize, String)>)>, n: usize| -> Option<usize> {
        po.iter().position(|(k, _)| *k == n)
    };

    for ch in &channels {
        let tgt = &ch["target"];
        let node = match tgt["node"].as_i64() {
            Some(n) if n >= 0 => n as usize,
            _ => continue,
        };
        let sampler_idx = ch["sampler"].as_u64().expect("channel sampler") as usize;
        let interp = samplers[sampler_idx]["interpolation"]
            .as_str()
            .unwrap_or(INTERP_LINEAR)
            .to_string();
        let path = tgt["path"].as_str().unwrap_or("").to_string();
        if node_pos(&per_node, node).is_none() {
            node_order.push(node);
            per_node.push((node, Vec::new()));
        }
        let idx = node_pos(&per_node, node).unwrap();

        if !per_node[idx].1.iter().any(|(p, _, _)| *p == path) {
            per_node[idx].1.push((path, sampler_idx, interp));
        }
    }

    let get_chan = |node: usize, gpath: &str| -> Option<(usize, String)> {
        let idx = node_pos(&per_node, node)?;
        per_node[idx]
            .1
            .iter()
            .find(|(p, _, _)| p == gpath)
            .map(|(_, s, i)| (*s, i.clone()))
    };

    let mut ordered_bindings: Vec<(String, i64, bool)> = Vec::new();
    let mut scalar_curves: Vec<(i64, Vec<Key>)> = Vec::new();
    let mut gidx: i64 = 0;

    let attrs: [(i64, &str, usize, fn(&[f64]) -> Vec<f64>); 3] = [
        (ATTR_POSITION, "translation", 3, conv_translation),
        (ATTR_ROTATION, "rotation", 4, conv_rotation),
        (ATTR_SCALE, "scale", 3, conv_scale),
    ];

    for (attr, gpath, dim, conv) in attrs.iter() {
        for &node_idx in node_order.iter() {
            let (sampler_idx, interp) = match get_chan(node_idx, gpath) {
                Some(v) => v,
                None => continue,
            };
            let path = glb::animation_path(node_idx, names, parent);
            let sampler = &samplers[sampler_idx];
            let input_idx = sampler["input"].as_u64().expect("sampler input") as usize;
            let times: Vec<f64> = glb::read_accessor(g, input_idx)
                .into_iter()
                .map(|t| t[0])
                .collect();
            let output_idx = sampler["output"].as_u64().expect("sampler output") as usize;
            let mut raw = glb::read_accessor(g, output_idx);
            if interp == INTERP_CUBICSPLINE {
                let n = times.len();
                let mut v = Vec::with_capacity(n);
                for i in 0..n {
                    v.push(raw[i * 3 + 1].clone());
                }
                raw = v;
            }
            let mut vals: Vec<Vec<f64>> = raw.iter().map(|v| conv(v)).collect();
            let mut times = times;
            if interp == INTERP_LINEAR {
                let mut kt: Vec<f64> = Vec::with_capacity(times.len());
                let mut kv: Vec<Vec<f64>> = Vec::with_capacity(vals.len());
                for (i, &t) in times.iter().enumerate() {
                    if i > 0 && *kt.last().unwrap() >= t {
                        continue;
                    }
                    kt.push(t);
                    kv.push(vals[i].clone());
                }
                times = kt;
                vals = kv;
                if *attr == ATTR_ROTATION {
                    for i in 1..vals.len() {
                        let dot: f32 = (vals[i - 1][0] as f32) * (vals[i][0] as f32)
                            + (vals[i - 1][1] as f32) * (vals[i][1] as f32)
                            + ((vals[i - 1][2] as f32) * (vals[i][2] as f32)
                                + (vals[i - 1][3] as f32) * (vals[i][3] as f32));
                        if dot < 0.0 {
                            for c in vals[i].iter_mut() {
                                *c = -*c;
                            }
                        }
                    }
                }
            }
            ordered_bindings.push((path, *attr, interp == INTERP_STEP));
            for comp in 0..*dim {
                let comp_vals: Vec<f64> = vals.iter().map(|v| v[comp]).collect();
                scalar_curves.push((gidx, bake_scalar_keys(&times, &comp_vals, &interp)));
                gidx += 1;
            }
        }
    }
    (ordered_bindings, scalar_curves)
}

pub const CONST_CURVE_VALUE_TOL: f32 = 9.536_743e-7;

pub const CONST_CURVE_SLOPE_TOL: f32 = 8.940705e-07;

fn classify_constant(
    scalar_curves: &[(i64, Vec<Key>)],
    bindings: &[(String, i64, bool)],
    value_tol: f32,
) -> Vec<bool> {
    let curve_const = |keys: &[Key]| -> bool {
        match keys.first() {
            None => true,
            Some(k0) => {
                let v0 = k0.value as f32;
                keys.iter().all(|k| {
                    ((k.value as f32) - v0).abs() < value_tol
                        && (k.slope as f32).abs() < CONST_CURVE_SLOPE_TOL
                })
            }
        }
    };
    let mut class = vec![false; scalar_curves.len()];
    let mut ci = 0usize;
    for (_, attr, is_step) in bindings.iter() {
        let dim = if *attr == ATTR_ROTATION { 4 } else { 3 };
        let all_const = !*is_step && (ci..ci + dim).all(|i| curve_const(&scalar_curves[i].1));
        for flag in class.iter_mut().skip(ci).take(dim) {
            *flag = all_const;
        }
        ci += dim;
    }
    class
}

pub fn clip_partition_counts(glb_bytes: &[u8], tol: f32) -> Vec<(String, usize, usize)> {
    let g = glb::parse(glb_bytes);
    let animations = match g.json["animations"].as_array() {
        Some(a) if !a.is_empty() => a.clone(),
        _ => return Vec::new(),
    };
    let (names, parent) = glb::node_names_and_parents(&g);
    let mut out = Vec::new();
    for anim in animations.iter() {
        let (ordered_bindings, scalar_curves) = gather_clip_curves(&g, anim, &names, &parent);
        let class = classify_constant(&scalar_curves, &ordered_bindings, tol);
        let nconst = class.iter().filter(|&&c| c).count();
        out.push((
            anim["name"].as_str().unwrap_or("").to_string(),
            scalar_curves.len() - nconst,
            nconst,
        ));
    }
    out
}

pub fn binding_tie_audit(
    glb_bytes: &[u8],
) -> Vec<(String, Vec<(String, i64, bool, usize, bool, u32, u32)>)> {
    let g = glb::parse(glb_bytes);
    let animations = match g.json["animations"].as_array() {
        Some(a) if !a.is_empty() => a.clone(),
        _ => return Vec::new(),
    };
    let (names, parent) = glb::node_names_and_parents(&g);
    let mut out = Vec::new();
    for anim in animations.iter() {
        let (ordered_bindings, scalar_curves) = gather_clip_curves(&g, anim, &names, &parent);
        let class = classify_constant(&scalar_curves, &ordered_bindings, CONST_CURVE_VALUE_TOL);
        let mut rows = Vec::new();
        let mut ci = 0usize;
        for (path, attr, is_step) in ordered_bindings.iter() {
            let dim = if *attr == ATTR_ROTATION { 4 } else { 3 };
            let mut vmax = 0f32;
            let mut smax = 0f32;
            for i in ci..ci + dim {
                let keys = &scalar_curves[i].1;
                if let Some(k0) = keys.first() {
                    let v0 = k0.value as f32;
                    for k in keys.iter() {
                        vmax = vmax.max(((k.value as f32) - v0).abs());
                        smax = smax.max((k.slope as f32).abs());
                    }
                }
            }
            let our_collapse = (ci..ci + dim).all(|i| class[i]);
            rows.push((
                path.clone(),
                *attr,
                *is_step,
                dim,
                our_collapse,
                vmax.to_bits(),
                smax.to_bits(),
            ));
            ci += dim;
        }
        out.push((anim["name"].as_str().unwrap_or("").to_string(), rows));
    }
    out
}

pub fn binding_key_dump(
    glb_bytes: &[u8],
) -> Vec<(String, Vec<(String, i64, bool, Vec<Vec<f64>>)>)> {
    let g = glb::parse(glb_bytes);
    let animations = match g.json["animations"].as_array() {
        Some(a) if !a.is_empty() => a.clone(),
        _ => return Vec::new(),
    };
    let (names, parent) = glb::node_names_and_parents(&g);
    let mut out = Vec::new();
    for anim in animations.iter() {
        let (ordered_bindings, scalar_curves) = gather_clip_curves(&g, anim, &names, &parent);
        let mut rows = Vec::new();
        let mut ci = 0usize;
        for (path, attr, is_step) in ordered_bindings.iter() {
            let dim = if *attr == ATTR_ROTATION { 4 } else { 3 };
            let mut comps = Vec::new();
            for i in ci..ci + dim {
                comps.push(scalar_curves[i].1.iter().map(|k| k.value).collect());
            }
            rows.push((path.clone(), *attr, *is_step, comps));
            ci += dim;
        }
        out.push((anim["name"].as_str().unwrap_or("").to_string(), rows));
    }
    out
}

pub fn binding_max_diffs(glb_bytes: &[u8]) -> Vec<(String, Vec<(String, i64, bool, usize, f32)>)> {
    let g = glb::parse(glb_bytes);
    let animations = match g.json["animations"].as_array() {
        Some(a) if !a.is_empty() => a.clone(),
        _ => return Vec::new(),
    };
    let (names, parent) = glb::node_names_and_parents(&g);
    let mut out = Vec::new();
    for anim in animations.iter() {
        let (ordered_bindings, scalar_curves) = gather_clip_curves(&g, anim, &names, &parent);
        let mut rows = Vec::new();
        let mut ci = 0usize;
        for (path, attr, is_step) in ordered_bindings.iter() {
            let dim = if *attr == ATTR_ROTATION { 4 } else { 3 };
            let mut md = 0f32;
            for i in ci..ci + dim {
                let keys = &scalar_curves[i].1;
                if let Some(k0) = keys.first() {
                    let v0 = k0.value as f32;
                    for k in keys.iter() {
                        md = md.max(((k.value as f32) - v0).abs());
                    }
                }
            }
            rows.push((path.clone(), *attr, *is_step, dim, md));
            ci += dim;
        }
        out.push((anim["name"].as_str().unwrap_or("").to_string(), rows));
    }
    out
}

fn partition_curves(
    scalar_curves: &[(i64, Vec<Key>)],
    class: &[bool],
) -> (Vec<(i64, Vec<Key>)>, Vec<f64>, i64) {
    let mut streamed: Vec<(i64, Vec<Key>)> = Vec::new();
    let mut constant: Vec<f64> = Vec::new();
    let mut new_i: i64 = 0;
    for (idx, (_, keys)) in scalar_curves.iter().enumerate() {
        if class[idx] {
            let v = keys.first().map(|k| k.value).unwrap_or(0.0);
            constant.push(v);
        } else {
            streamed.push((new_i, keys.clone()));
            new_i += 1;
        }
    }
    let n = streamed.len() as i64;
    (streamed, constant, n)
}

pub fn build_mecanim_clips(glb_bytes: &[u8], base_clip_tree: &Value) -> Vec<Value> {
    let g = glb::parse(glb_bytes);
    let animations = match g.json["animations"].as_array() {
        Some(a) if !a.is_empty() => a.clone(),
        _ => return Vec::new(),
    };
    let (names, parent) = glb::node_names_and_parents(&g);

    let mut clips: Vec<Value> = Vec::new();
    for (ai, anim) in animations.iter().enumerate() {
        let (ordered_bindings, scalar_curves) = gather_clip_curves(&g, anim, &names, &parent);
        let class = classify_constant(&scalar_curves, &ordered_bindings, CONST_CURVE_VALUE_TOL);
        let (streamed, constant, n_streamed) = partition_curves(&scalar_curves, &class);

        let mut binding_const: Vec<bool> = Vec::with_capacity(ordered_bindings.len());
        {
            let mut ci = 0usize;
            for (_, attr, _) in ordered_bindings.iter() {
                let dim = if *attr == ATTR_ROTATION { 4 } else { 3 };
                binding_const.push(class[ci..ci + dim].iter().all(|&c| c));
                ci += dim;
            }
        }
        let mut generic_bindings: Vec<Value> = Vec::new();
        for constant_pass in [false, true] {
            for (bi, (path, attr, _)) in ordered_bindings.iter().enumerate() {
                if binding_const[bi] != constant_pass {
                    continue;
                }
                generic_bindings.push(map! {
                    "path" => crc32(path) as i64,
                    "attribute" => *attr,
                    "script" => crate::value::pptr(0, 0),
                    "typeID" => TRANSFORM_CLASS_ID,
                    "customType" => 0,
                    "isPPtrCurve" => 0,
                    "isIntCurve" => 0,
                    "isSerializeReferenceCurve" => 0,
                });
            }
        }

        let mut stop_time = 0.0f64;
        let mut begin_time = f64::INFINITY;
        for (_, keys) in scalar_curves.iter() {
            for k in keys.iter() {
                if k.time > stop_time {
                    stop_time = k.time;
                }
                if k.time < begin_time {
                    begin_time = k.time;
                }
            }
        }
        if !begin_time.is_finite() {
            begin_time = 0.0;
        }

        let mut tree = base_clip_tree.clone();
        let name = anim["name"]
            .as_str()
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("Clip_{ai}"));
        tree.insert("m_Name", name);
        tree.insert("m_Legacy", false);
        tree.insert("m_Compressed", false);
        tree.insert("m_UseHighQualityCurve", true);
        for k in [
            "m_RotationCurves",
            "m_CompressedRotationCurves",
            "m_EulerCurves",
            "m_PositionCurves",
            "m_ScaleCurves",
            "m_FloatCurves",
            "m_PPtrCurves",
        ] {
            tree.insert(k, arr![]);
        }
        tree.insert("m_SampleRate", SAMPLE_RATE);
        tree.insert("m_WrapMode", WRAP_LOOP);
        tree.insert(
            "m_Bounds",
            map! {
                "m_Center" => map!{"x" => 0.0, "y" => 0.0, "z" => 0.0},
                "m_Extent" => map!{"x" => 0.0, "y" => 0.0, "z" => 0.0},
            },
        );
        tree.insert(
            "m_ClipBindingConstant",
            map! {
                "genericBindings" => Value::Array(generic_bindings),
                "pptrCurveMapping" => arr![],
            },
        );
        tree.insert("m_HasGenericRootTransform", false);
        tree.insert("m_HasMotionFloatCurves", false);
        tree.insert("m_Events", arr![]);

        let streamed_data = encode_streamed_clip(&streamed);
        let mc = tree
            .get_mut("m_MuscleClip")
            .expect("base clip tree missing m_MuscleClip");
        {
            let clip_data = mc
                .get_mut("m_Clip")
                .and_then(|c| c.get_mut("data"))
                .expect("m_Clip.data");
            clip_data.insert(
                "m_StreamedClip",
                map! {
                    "data" => Value::Array(streamed_data),
                    "curveCount" => n_streamed,
                    "discreteCurveCount" => 0,
                },
            );

            let dense_frames =
                ((stop_time as f32 - begin_time as f32) * SAMPLE_RATE as f32) as i64 + 2;
            clip_data.insert(
                "m_DenseClip",
                map! {
                    "m_FrameCount" => dense_frames,
                    "m_CurveCount" => 0,
                    "m_SampleRate" => SAMPLE_RATE,
                    "m_BeginTime" => begin_time,
                    "m_SampleArray" => arr![],
                },
            );
            let constant_vals: Vec<Value> = constant.iter().map(|&v| Value::Float(v)).collect();
            clip_data.insert(
                "m_ConstantClip",
                map! { "data" => Value::Array(constant_vals) },
            );
        }
        mc.insert("m_StartTime", 0.0);
        mc.insert("m_StopTime", stop_time);

        let eval0 = |k: &Key| -> f64 {
            let a = k.a as f32;
            let b = k.b as f32;
            let c = k.slope as f32;
            let d = k.value as f32;
            ((((a * 0.0 + b) * 0.0) + c) * 0.0 + d) as f64
        };
        let mut delta: Vec<Value> = Vec::new();
        for (_, keys) in streamed.iter() {
            let start = keys.first().map(&eval0).unwrap_or(0.0);
            let stop = keys.last().map(&eval0).unwrap_or(0.0);
            delta.push(map! { "m_Start" => start, "m_Stop" => stop });
        }
        for &v in constant.iter() {
            delta.push(map! { "m_Start" => v, "m_Stop" => v });
        }
        mc.insert("m_ValueArrayDelta", Value::Array(delta));

        tree.insert("m_MuscleClipSize", 0);
        clips.push(tree);
    }
    clips
}

pub fn build_animator_component(go_pid: i64, controller_pid: i64) -> Value {
    map! {
        "m_GameObject" => crate::value::pptr(0, go_pid),
        "m_Enabled" => 1,
        "m_Avatar" => crate::value::pptr(0, 0),
        "m_Controller" => crate::value::pptr(0, controller_pid),
        "m_CullingMode" => 0,
        "m_UpdateMode" => 0,
        "m_ApplyRootMotion" => false,
        "m_LinearVelocityBlending" => false,
        "m_StabilizeFeet" => false,
        "m_AnimatePhysics" => false,
        "m_HasTransformHierarchy" => true,
        "m_AllowConstantClipSamplingOptimization" => true,
        "m_KeepAnimatorStateOnDisable" => false,
        "m_WriteDefaultValuesOnDisable" => false,
    }
}

fn condition(mode: i64, event_id: u32, threshold: f64, exit_time: f64) -> Value {
    map! {
        "data" => map! {
            "m_ConditionMode" => mode,
            "m_EventID" => event_id as i64,
            "m_EventThreshold" => threshold,
            "m_ExitTime" => exit_time,
        }
    }
}

fn empty_blend_tree(clip_id: i64) -> Value {
    arr![map! {
        "data" => map! {
            "m_NodeArray" => arr![map! {
                "data" => map! {
                    "m_BlendType" => 0,
                    "m_BlendEventID" => 0xFFFFFFFFu32 as i64,
                    "m_BlendEventYID" => 0xFFFFFFFFu32 as i64,
                    "m_ChildIndices" => arr![],
                    "m_Blend1dData" => map!{ "data" => map!{ "m_ChildThresholdArray" => arr![] } },
                    "m_Blend2dData" => map!{ "data" => map!{
                        "m_ChildPositionArray" => arr![],
                        "m_ChildMagnitudeArray" => arr![],
                        "m_ChildPairVectorArray" => arr![],
                        "m_ChildPairAvgMagInvArray" => arr![],
                        "m_ChildNeighborListArray" => arr![],
                    }},
                    "m_BlendDirectData" => map!{ "data" => map!{
                        "m_ChildBlendEventIDArray" => arr![],
                        "m_NormalizedBlendValues" => false,
                    }},
                    "m_ClipID" => clip_id,
                    "m_Duration" => 1.0,
                    "m_CycleOffset" => 0.0,
                    "m_Mirror" => false,
                }
            }]
        }
    }]
}

fn transition(
    name: &str,
    full_name: &str,
    dest: i64,
    event_id: u32,
    tos: &mut Vec<(u32, String)>,
    mode: i64,
) -> Value {
    tos_set(tos, crc32(name), name);
    tos_set(tos, crc32(full_name), full_name);
    map! {
        "data" => map! {
            "m_ConditionConstantArray" => arr![condition(mode, event_id, 0.0, 0.0)],
            "m_DestinationState" => dest,
            "m_FullPathID" => crc32(full_name) as i64,
            "m_ID" => crc32(name) as i64,
            "m_UserID" => 0,
            "m_TransitionDuration" => 0.0,
            "m_TransitionOffset" => 0.0,
            "m_ExitTime" => 1.0,
            "m_HasExitTime" => true,
            "m_HasFixedDuration" => true,
            "m_InterruptionSource" => 0,
            "m_OrderedInterruption" => true,
            "m_CanTransitionToSelf" => true,
        }
    }
}

fn state(
    name: &str,
    full_name: &str,
    transitions: Vec<Value>,
    tos: &mut Vec<(u32, String)>,
    clip_id: i64,
) -> Value {
    tos_set(tos, crc32(name), name);
    tos_set(tos, crc32(full_name), full_name);
    map! {
        "data" => map! {
            "m_TransitionConstantArray" => Value::Array(transitions),
            "m_BlendTreeConstantIndexArray" => arr![0],
            "m_BlendTreeConstantArray" => empty_blend_tree(clip_id),
            "m_NameID" => crc32(name) as i64,
            "m_PathID" => crc32(full_name) as i64,
            "m_FullPathID" => crc32(full_name) as i64,
            "m_TagID" => 0,
            "m_SpeedParamID" => 0,
            "m_MirrorParamID" => 0,
            "m_CycleOffsetParamID" => 0,
            "m_TimeParamID" => 0,
            "m_Speed" => 1.0,
            "m_CycleOffset" => 0.0,
            "m_IKOnFeet" => false,
            "m_WriteDefaultValues" => true,
            "m_Loop" => false,
            "m_Mirror" => false,
        }
    }
}

fn tos_set(tos: &mut Vec<(u32, String)>, hash: u32, name: &str) {
    if let Some(slot) = tos.iter_mut().find(|(h, _)| *h == hash) {
        slot.1 = name.to_string();
    } else {
        tos.push((hash, name.to_string()));
    }
}

pub fn build_animator_controller(
    clip_specs: &[(String, i64)],
    base_controller_tree: &Value,
) -> Value {
    let layer_name = "Base Layer";
    let mut tos: Vec<(u32, String)> = vec![(0u32, String::new())];

    let mut value_array: Vec<Value> = vec![map! {
        "m_ID" => crc32(LOOP_PARAMETER) as i64,
        "m_Type" => PARAM_TYPE_BOOL,
        "m_Index" => 0,
    }];
    let mut bool_values: Vec<Value> = vec![Value::Bool(true)];
    tos_set(&mut tos, crc32(LOOP_PARAMETER), LOOP_PARAMETER);
    tos_set(&mut tos, crc32("GravityWeight"), "GravityWeight");
    for (i, (clip_name, _pid)) in clip_specs.iter().enumerate() {
        value_array.push(map! {
            "m_ID" => crc32(clip_name) as i64,
            "m_Type" => PARAM_TYPE_TRIGGER,
            "m_Index" => (i + 1) as i64,
        });
        bool_values.push(Value::Bool(false));
        tos_set(&mut tos, crc32(clip_name), clip_name);
    }

    let mut states: Vec<Value> = Vec::new();
    let mut any_state_transitions: Vec<Value> = Vec::new();
    let mut animation_clips: Vec<Value> = Vec::new();
    for (i, (clip_name, pid)) in clip_specs.iter().enumerate() {
        let name0 = clip_name.replace('.', "_");
        let name1 = if name0 == *clip_name {
            format!("{clip_name} 0")
        } else {
            name0.clone()
        };
        let full0 = format!("{layer_name}.{name0}");
        let full1 = format!("{layer_name}.{name1}");

        let t01 = transition(
            &format!("{name0} -> {name1}"),
            &format!("{full0} -> {full1}"),
            (2 * i + 1) as i64,
            crc32(LOOP_PARAMETER),
            &mut tos,
            1,
        );
        let t10 = transition(
            &format!("{name1} -> {name0}"),
            &format!("{full1} -> {full0}"),
            (2 * i) as i64,
            crc32(LOOP_PARAMETER),
            &mut tos,
            1,
        );
        states.push(state(&name0, &full0, vec![t01], &mut tos, (2 * i) as i64));
        states.push(state(&name1, &full1, vec![t10], &mut tos, (2 * i) as i64));
        animation_clips.push(crate::value::pptr(0, *pid));
        animation_clips.push(crate::value::pptr(0, *pid));

        let any_name = format!("AnyState -> {name0}");
        let any_full = format!("Entry -> {full0}");
        tos_set(&mut tos, crc32(&any_name), &any_name);
        tos_set(&mut tos, crc32(&any_full), &any_full);
        any_state_transitions.push(map! {
            "data" => map! {
                "m_ConditionConstantArray" => arr![condition(1, crc32(clip_name), 0.0, 0.0)],
                "m_DestinationState" => (2 * i) as i64,
                "m_FullPathID" => crc32(&any_full) as i64,
                "m_ID" => crc32(&any_name) as i64,
                "m_UserID" => 0,
                "m_TransitionDuration" => 0.0,
                "m_TransitionOffset" => 0.0,
                "m_ExitTime" => 0.75,
                "m_HasExitTime" => false,
                "m_HasFixedDuration" => true,
                "m_InterruptionSource" => 0,
                "m_OrderedInterruption" => true,
                "m_CanTransitionToSelf" => true,
            }
        });
    }

    tos_set(&mut tos, crc32(layer_name), layer_name);

    let state_machine = map! {
        "data" => map! {
            "m_StateConstantArray" => Value::Array(states),
            "m_AnyStateTransitionConstantArray" => Value::Array(any_state_transitions),
            "m_SelectorStateConstantArray" => arr![
                map! {
                    "data" => map! {
                        "m_TransitionConstantArray" => arr![map! {
                            "data" => map! {
                                "m_Destination" => 0,
                                "m_ConditionConstantArray" => arr![],
                            }
                        }],
                        "m_FullPathID" => crc32(layer_name) as i64,
                        "m_IsEntry" => true,
                    }
                },
                map! {
                    "data" => map! {
                        "m_TransitionConstantArray" => arr![map! {
                            "data" => map! {
                                "m_Destination" => SELECTOR_EXIT_DEST,
                                "m_ConditionConstantArray" => arr![],
                            }
                        }],
                        "m_FullPathID" => crc32(layer_name) as i64,
                        "m_IsEntry" => false,
                    }
                },
            ],
            "m_DefaultState" => 0,
            "m_SynchronizedLayerCount" => 1,
        }
    };

    let layer = map! {
        "data" => map! {
            "m_StateMachineIndex" => 0,
            "m_StateMachineSynchronizedLayerIndex" => 0,
            "m_BodyMask" => map! {
                "word0" => 0xFFFFFFFFu32 as i64,
                "word1" => 0xFFFFFFFFu32 as i64,
                "word2" => 524287,
            },
            "m_SkeletonMask" => map!{ "data" => map!{ "m_Data" => arr![] } },
            "m_Binding" => crc32(layer_name) as i64,
            "(int&)m_LayerBlendingMode" => 0,
            "m_DefaultWeight" => 0.0,
            "m_IKPass" => false,
            "m_SyncedLayerAffectsTiming" => false,
        }
    };

    let mut tos_sorted = tos.clone();
    tos_sorted.sort_by_key(|(h, _)| *h);
    let tos_value: Vec<Value> = tos_sorted
        .into_iter()
        .map(|(h, name)| arr![h as i64, name])
        .collect();

    let mut tree = base_controller_tree.clone();
    tree.insert("m_Name", "animatorController");
    tree.insert(
        "m_Controller",
        map! {
            "m_LayerArray" => arr![layer],
            "m_StateMachineArray" => arr![state_machine],
            "m_Values" => map!{ "data" => map!{ "m_ValueArray" => Value::Array(value_array) } },
            "m_DefaultValues" => map!{ "data" => map! {
                "m_PositionValues" => arr![],
                "m_QuaternionValues" => arr![],
                "m_ScaleValues" => arr![],
                "m_FloatValues" => arr![],
                "m_IntValues" => arr![],
                "m_BoolValues" => Value::Array(bool_values),
                "m_EntityIdValues" => arr![],
            }},
        },
    );
    tree.insert("m_AnimationClips", Value::Array(animation_clips));
    tree.insert("m_TOS", Value::Array(tos_value));
    tree.insert(
        "m_StateMachineBehaviourVectorDescription",
        map! {
            "m_StateMachineBehaviourRanges" => arr![],
            "m_StateMachineBehaviourIndices" => arr![],
        },
    );
    tree.insert("m_StateMachineBehaviours", arr![]);
    tree.insert("m_MultiThreadedStateMachine", true);

    let k = clip_specs.len() as i64;
    let controller_size = match k {
        0 | 1 => 1570,
        _ => 2515 + 937 * (k - 2),
    };
    tree.insert("m_ControllerSize", controller_size);
    tree
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc32_known_values() {
        assert_eq!(crc32("Base Layer"), 0x2d18_2308);
        assert_eq!(crc32("Loop"), 0x016d_b2d0);
        assert_eq!(crc32("GravityWeight"), 0x7d7f_be84);
    }

    fn k(v: f64) -> Key {
        Key {
            time: 0.0,
            value: v,
            slope: 0.0,
            a: 0.0,
            b: 0.0,
        }
    }
    fn flat(v: f64) -> Vec<Key> {
        vec![k(v), Key { time: 1.0, ..k(v) }]
    }
    fn varying(a: f64, b: f64) -> Vec<Key> {
        vec![k(a), Key { time: 1.0, ..k(b) }]
    }

    #[test]
    fn constant_collapse_is_binding_atomic() {
        let scalar_curves: Vec<(i64, Vec<Key>)> = vec![
            (0, flat(1.0)),
            (1, varying(0.0, 2.0)),
            (2, flat(3.0)),
            (3, flat(0.0)),
            (4, flat(0.0)),
            (5, varying(0.0, 0.1)),
            (6, flat(1.0)),
            (7, flat(1.0)),
            (8, flat(1.0)),
            (9, flat(1.0)),
        ];
        let bindings: Vec<(String, i64, bool)> = vec![
            ("p".into(), ATTR_POSITION, false),
            ("p".into(), ATTR_ROTATION, false),
            ("p".into(), ATTR_SCALE, false),
        ];

        let class = classify_constant(&scalar_curves, &bindings, CONST_CURVE_VALUE_TOL);
        assert_eq!(&class[0..3], &[false, false, false]);
        assert_eq!(&class[3..7], &[false, false, false, false]);
        assert_eq!(&class[7..10], &[true, true, true]);

        let (streamed, constant, n) = partition_curves(&scalar_curves, &class);
        assert_eq!(constant, vec![1.0, 1.0, 1.0]);
        assert_eq!(n, 7);
        let idxs: Vec<i64> = streamed.iter().map(|(i, _)| *i).collect();
        assert_eq!(idxs, vec![0, 1, 2, 3, 4, 5, 6]);
    }

    #[test]
    fn step_curves_never_collapse() {
        let scalar_curves: Vec<(i64, Vec<Key>)> =
            vec![(0, flat(1.0)), (1, flat(2.0)), (2, flat(3.0))];
        let bindings: Vec<(String, i64, bool)> = vec![("p".into(), ATTR_POSITION, true)];
        let class = classify_constant(&scalar_curves, &bindings, CONST_CURVE_VALUE_TOL);
        assert_eq!(class, vec![false, false, false]);
    }

    #[test]
    fn near_constant_within_tolerance_collapses() {
        let near: Vec<Key> = vec![
            k(0.5),
            Key {
                time: 1.0,
                ..k(0.5 + 1.19e-7)
            },
        ];
        let wide: Vec<Key> = vec![
            k(0.5),
            Key {
                time: 1.0,
                ..k(0.5 + 1.0e-6)
            },
        ];
        let scalar_curves: Vec<(i64, Vec<Key>)> = vec![
            (0, near),
            (1, flat(0.2)),
            (2, flat(0.3)),
            (3, wide),
            (4, flat(0.2)),
            (5, flat(0.3)),
        ];
        let bindings: Vec<(String, i64, bool)> = vec![
            ("a".into(), ATTR_POSITION, false),
            ("b".into(), ATTR_POSITION, false),
        ];
        let class = classify_constant(&scalar_curves, &bindings, CONST_CURVE_VALUE_TOL);
        assert_eq!(&class[0..3], &[true, true, true]);
        assert_eq!(&class[3..6], &[false, false, false]);
    }

    #[test]
    fn rotation_all_constant_collapses() {
        let scalar_curves: Vec<(i64, Vec<Key>)> = vec![
            (0, flat(0.0)),
            (1, flat(0.0)),
            (2, flat(0.0)),
            (3, flat(1.0)),
        ];
        let bindings: Vec<(String, i64, bool)> = vec![("p".into(), ATTR_ROTATION, false)];
        let class = classify_constant(&scalar_curves, &bindings, CONST_CURVE_VALUE_TOL);
        assert_eq!(class, vec![true, true, true, true]);
        let (_s, constant, n) = partition_curves(&scalar_curves, &class);
        assert_eq!(n, 0);
        assert_eq!(constant, vec![0.0, 0.0, 0.0, 1.0]);
    }

    #[test]
    fn streamed_xcheck() {
        let c0 = bake_scalar_keys(&[0.0, 1.0], &[0.0, 1.0], "LINEAR");
        let c1 = bake_scalar_keys(&[0.0, 0.5, 1.0], &[2.0, 3.0, 4.0], "LINEAR");
        let data = encode_streamed_clip(&[(0i64, c0), (1i64, c1)]);
        let nums: Vec<u32> = data.iter().map(|v| v.as_i64().unwrap() as u32).collect();

        let expect: Vec<u32> = vec![
            4286578687, 2, 0, 0, 0, 0, 0, 1, 0, 0, 0, 1073741824, 0, 2, 0, 0, 0, 1065353216, 0, 1,
            0, 0, 1073741824, 1073741824, 1056964608, 1, 1, 0, 0, 1073741824, 1077936128,
            1065353216, 2, 0, 0, 0, 0, 1065353216, 1, 0, 0, 0, 1082130432, 2139095040, 0,
        ];
        assert_eq!(nums, expect);
    }

    #[test]
    fn streamed_clip_leading_and_trailing_frames() {
        let keys = bake_scalar_keys(&[0.0, 1.0], &[0.0, 1.0], INTERP_LINEAR);
        let data = encode_streamed_clip(&[(0i64, keys)]);

        assert!(!data.is_empty());
        let first = data[0].as_i64().unwrap() as u32;
        assert_eq!(first, 0xFF7F_FFFF);
    }

    #[test]
    fn controller_default_state_and_loop_param() {
        let base = map! { "m_Name" => "x" };
        let ctrl = build_animator_controller(&[("Wave".to_string(), 5)], &base);

        let va = ctrl
            .get("m_Controller")
            .unwrap()
            .get("m_Values")
            .unwrap()
            .get("data")
            .unwrap()
            .get("m_ValueArray")
            .unwrap()
            .as_array()
            .unwrap();
        assert_eq!(va.len(), 2);
        assert_eq!(va[0].get("m_Type").unwrap().as_i64(), Some(PARAM_TYPE_BOOL));
        assert_eq!(
            va[1].get("m_Type").unwrap().as_i64(),
            Some(PARAM_TYPE_TRIGGER)
        );

        let states = ctrl
            .get("m_Controller")
            .unwrap()
            .get("m_StateMachineArray")
            .unwrap()
            .as_array()
            .unwrap()[0]
            .get("data")
            .unwrap()
            .get("m_StateConstantArray")
            .unwrap()
            .as_array()
            .unwrap();
        assert_eq!(states.len(), 2);
    }
}
