use super::curves::{
    classify_constant, encode_streamed_clip, gather_clip_curves, partition_curves, Key,
    CONST_CURVE_VALUE_TOL,
};
use super::{
    crc32, ATTR_ROTATION, LOOP_PARAMETER, PARAM_TYPE_BOOL, PARAM_TYPE_TRIGGER, SAMPLE_RATE,
    SELECTOR_EXIT_DEST, TRANSFORM_CLASS_ID, WRAP_LOOP,
};
use crate::animation::glb;
use crate::value::Value;

pub fn build_mecanim_clips(
    gltf: &serde_json::Value,
    buffers: &[Vec<u8>],
    base_clip_tree: &Value,
) -> Vec<Value> {
    let animations = match gltf["animations"].as_array() {
        Some(a) if !a.is_empty() => a.clone(),
        _ => return Vec::new(),
    };
    let (names, parent) = glb::node_names_and_parents_from_json(gltf);

    let mut clips: Vec<Value> = Vec::new();
    for (ai, anim) in animations.iter().enumerate() {
        let (ordered_bindings, scalar_curves) =
            gather_clip_curves(gltf, buffers, anim, &names, &parent);
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
