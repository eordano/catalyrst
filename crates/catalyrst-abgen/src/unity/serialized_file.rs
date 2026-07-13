#![allow(non_snake_case)]
use super::streams::{Reader, Writer};
use super::typetree;
use super::typetree_node::TypeTreeNode;
use crate::value::Value;
use anyhow::Result;

#[derive(Clone, Debug, Default)]
pub struct SerializedType {
    pub class_id: i32,
    pub is_stripped: bool,
    pub script_type_index: i16,
    pub script_id: Option<[u8; 16]>,
    pub old_type_hash: Option<[u8; 16]>,
    pub node: Option<TypeTreeNode>,

    pub type_dependencies: Vec<i32>,
}

#[derive(Clone, Debug, Default)]
pub struct Object {
    pub path_id: i64,

    pub type_id: i32,
    pub class_id: i32,
    pub type_name: String,
    pub data: Vec<u8>,
}

#[derive(Clone, Debug, Default)]
pub struct FileIdentifier {
    pub temp_empty: String,
    pub guid: [u8; 16],
    pub r#type: i32,
    pub path: String,
}

#[derive(Clone, Debug, Default)]
pub struct SerializedFile {
    pub version: u32,
    pub big_endian: bool,
    pub reserved: [u8; 3],
    pub unity_version: String,
    pub target_platform: i32,
    pub enable_type_tree: bool,
    pub big_id_enabled: i32,
    pub unknown: i64,
    pub types: Vec<SerializedType>,
    pub objects: Vec<Object>,

    pub script_types: Vec<(i32, i64)>,
    pub externals: Vec<FileIdentifier>,
    pub ref_types: Vec<SerializedType>,
    pub user_information: String,
}

fn read_serialized_type(
    r: &mut Reader,
    version: u32,
    enable_type_tree: bool,
    is_ref_type: bool,
) -> SerializedType {
    let mut st = SerializedType {
        class_id: r.read_i32(),
        ..SerializedType::default()
    };

    if version >= 16 {
        st.is_stripped = r.read_bool();
    }
    if version >= 17 {
        st.script_type_index = r.read_i16();
    }
    if version >= 13 {
        let read_script_id = (is_ref_type && st.script_type_index >= 0)
            || (version < 16 && st.class_id < 0)
            || (version >= 16 && st.class_id == 114);
        if read_script_id {
            let mut id = [0u8; 16];
            id.copy_from_slice(r.read_bytes(16));
            st.script_id = Some(id);
        }
        let mut h = [0u8; 16];
        h.copy_from_slice(r.read_bytes(16));
        st.old_type_hash = Some(h);
    }

    if enable_type_tree {
        st.node = Some(TypeTreeNode::parse_blob(r, version));
        if version >= 21 {
            if is_ref_type {
                let _ = r.read_cstr();
                let _ = r.read_cstr();
                let _ = r.read_cstr();
            } else {
                let count = r.read_i32();
                let mut deps = Vec::with_capacity(count.max(0) as usize);
                for _ in 0..count.max(0) {
                    deps.push(r.read_i32());
                }
                st.type_dependencies = deps;
            }
        }
    }

    st
}

fn write_serialized_type(
    st: &SerializedType,
    w: &mut Writer,
    version: u32,
    enable_type_tree: bool,
    is_ref_type: bool,
) {
    w.write_i32(st.class_id);
    if version >= 16 {
        w.write_bool(st.is_stripped);
    }
    if version >= 17 {
        w.write_i16(st.script_type_index);
    }
    if version >= 13 {
        let write_script_id = (is_ref_type && st.script_type_index >= 0)
            || (version < 16 && st.class_id < 0)
            || (version >= 16 && st.class_id == 114);
        if write_script_id {
            w.write_bytes(&st.script_id.unwrap_or([0u8; 16]));
        }
        w.write_bytes(&st.old_type_hash.unwrap_or([0u8; 16]));
    }
    if enable_type_tree {
        if let Some(node) = &st.node {
            node.dump_blob(w, version);
        }
        if version >= 21 && !is_ref_type {
            w.write_i32(st.type_dependencies.len() as i32);
            for d in &st.type_dependencies {
                w.write_i32(*d);
            }
        }
    }
}

impl SerializedFile {
    pub fn parse(data: &[u8]) -> Result<SerializedFile> {
        let mut r = Reader::new(data, true);

        let _legacy_metadata_size = r.read_u32();
        let _legacy_file_size = r.read_u32();
        let version = r.read_u32();
        let mut data_offset = r.read_u32() as i64;

        let mut sf = SerializedFile {
            version,
            ..SerializedFile::default()
        };

        let big_endian;
        if version >= 9 {
            big_endian = r.read_bool();
            let mut reserved = [0u8; 3];
            reserved.copy_from_slice(r.read_bytes(3));
            sf.reserved = reserved;
            if version >= 22 {
                let _metadata_size = r.read_u32();
                let _file_size = r.read_i64();
                data_offset = r.read_i64();
                sf.unknown = r.read_i64();
            }
        } else {
            big_endian = false;
        }
        sf.big_endian = big_endian;

        r.big_endian = big_endian;

        if version >= 7 {
            sf.unity_version = r.read_cstr();
        }
        if version >= 8 {
            sf.target_platform = r.read_i32();
        }
        if version >= 13 {
            sf.enable_type_tree = r.read_bool();
        }

        let type_count = r.read_i32();
        let mut types = Vec::with_capacity(type_count.max(0) as usize);
        for _ in 0..type_count.max(0) {
            types.push(read_serialized_type(
                &mut r,
                version,
                sf.enable_type_tree,
                false,
            ));
        }
        sf.types = types;

        if (7..14).contains(&version) {
            sf.big_id_enabled = r.read_i32();
        }

        let object_count = r.read_i32();
        let mut objects = Vec::with_capacity(object_count.max(0) as usize);
        for _ in 0..object_count.max(0) {
            let obj = read_object(&mut r, &sf, data_offset, data)?;
            objects.push(obj);
        }
        sf.objects = objects;

        if version >= 11 {
            let script_count = r.read_i32();
            let mut scripts = Vec::with_capacity(script_count.max(0) as usize);
            for _ in 0..script_count.max(0) {
                let local_idx = r.read_i32();
                if version < 14 {
                    let local_id = r.read_i32() as i64;
                    scripts.push((local_idx, local_id));
                } else {
                    r.align_stream(4);
                    let local_id = r.read_i64();
                    scripts.push((local_idx, local_id));
                }
            }
            sf.script_types = scripts;
        }

        let externals_count = r.read_i32();
        let mut externals = Vec::with_capacity(externals_count.max(0) as usize);
        for _ in 0..externals_count.max(0) {
            let mut fi = FileIdentifier::default();
            if version >= 6 {
                fi.temp_empty = r.read_cstr();
            }
            if version >= 5 {
                let mut guid = [0u8; 16];
                guid.copy_from_slice(r.read_bytes(16));
                fi.guid = guid;
                fi.r#type = r.read_i32();
            }
            fi.path = r.read_cstr();
            externals.push(fi);
        }
        sf.externals = externals;

        if version >= 20 {
            let ref_type_count = r.read_i32();
            let mut ref_types = Vec::with_capacity(ref_type_count.max(0) as usize);
            for _ in 0..ref_type_count.max(0) {
                ref_types.push(read_serialized_type(
                    &mut r,
                    version,
                    sf.enable_type_tree,
                    true,
                ));
            }
            sf.ref_types = ref_types;
        }

        if version >= 5 {
            sf.user_information = r.read_cstr();
        }

        Ok(sf)
    }

    pub fn read_typetree(&self, obj: &Object) -> Result<Value> {
        let st = &self.types[obj.type_id as usize];
        let node = st
            .node
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("type {} has no type tree", obj.type_id))?;
        typetree::read_typetree(&obj.data, node, self.big_endian)
    }

    pub fn save(&self) -> Vec<u8> {
        let version = self.version;
        let mut meta = Writer::new(self.big_endian);
        let mut data_w = Writer::new(self.big_endian);

        if version >= 7 {
            meta.write_cstr(&self.unity_version);
        }
        if version >= 8 {
            meta.write_i32(self.target_platform);
        }
        if version >= 13 {
            meta.write_bool(self.enable_type_tree);
        }

        meta.write_i32(self.types.len() as i32);
        for st in &self.types {
            write_serialized_type(st, &mut meta, version, self.enable_type_tree, false);
        }

        if (7..14).contains(&version) {
            meta.write_i32(self.big_id_enabled);
        }

        let mut objs: Vec<&Object> = self.objects.iter().collect();
        objs.sort_by_key(|o| o.path_id);
        meta.write_i32(objs.len() as i32);
        let last_idx = objs.len().saturating_sub(1);
        for (i, obj) in objs.iter().enumerate() {
            meta.align_stream(4);
            meta.write_i64(obj.path_id);

            let byte_start = data_w.position() as i64;
            if version >= 22 {
                meta.write_i64(byte_start);
            } else {
                meta.write_u32(byte_start as u32);
            }
            meta.write_u32(obj.data.len() as u32);
            meta.write_i32(obj.type_id);

            data_w.write_bytes(&obj.data);

            if i != last_idx {
                data_w.align_stream(16);
            }
        }

        if version >= 11 {
            meta.write_i32(self.script_types.len() as i32);
            for (local_idx, local_id) in &self.script_types {
                meta.write_i32(*local_idx);
                if version < 14 {
                    meta.write_i32(*local_id as i32);
                } else {
                    meta.align_stream(4);
                    meta.write_i64(*local_id);
                }
            }
        }

        meta.write_i32(self.externals.len() as i32);
        for fi in &self.externals {
            if version >= 6 {
                meta.write_cstr(&fi.temp_empty);
            }
            if version >= 5 {
                meta.write_bytes(&fi.guid);
                meta.write_i32(fi.r#type);
            }
            meta.write_cstr(&fi.path);
        }

        if version >= 20 {
            meta.write_i32(self.ref_types.len() as i32);
            for st in &self.ref_types {
                write_serialized_type(st, &mut meta, version, self.enable_type_tree, true);
            }
        }

        if version >= 5 {
            meta.write_cstr(&self.user_information);
        }

        let meta_bytes = meta.into_bytes();
        let data_bytes = data_w.into_bytes();

        let mut w = Writer::new(true);

        let metadata_size = meta_bytes.len();
        let mut header_size = 16usize;

        header_size += if version < 22 { 4 } else { 4 + 28 };
        let mut data_offset = header_size + metadata_size;
        data_offset += (16 - data_offset % 16) % 16;
        let file_size = data_offset + data_bytes.len();

        if version < 22 {
            w.write_u32(metadata_size as u32);
            w.write_u32(file_size as u32);
            w.write_u32(version);
            w.write_u32(data_offset as u32);
            w.write_bool(self.big_endian);
            w.write_bytes(&self.reserved);
        } else {
            w.write_u32(0);
            w.write_u32(0);
            w.write_u32(version);
            w.write_u32(0);
            w.write_bool(self.big_endian);
            w.write_bytes(&self.reserved);
            w.write_u32(metadata_size as u32);
            w.write_i64(file_size as i64);
            w.write_i64(data_offset as i64);
            w.write_i64(self.unknown);
        }

        w.write_bytes(&meta_bytes);
        w.align_stream(16);
        w.write_bytes(&data_bytes);

        w.into_bytes()
    }
}

fn read_object(
    r: &mut Reader,
    sf: &SerializedFile,
    data_offset: i64,
    file: &[u8],
) -> Result<Object> {
    let version = sf.version;
    let path_id = if sf.big_id_enabled != 0 {
        r.read_i64()
    } else if version < 14 {
        r.read_i32() as i64
    } else {
        r.align_stream(4);
        r.read_i64()
    };

    let mut byte_start = if version >= 22 {
        r.read_i64()
    } else {
        r.read_u32() as i64
    };
    byte_start += data_offset;
    let byte_size = r.read_u32();
    let type_id = r.read_i32();

    let class_id = if version < 16 {
        r.read_u16() as i32
    } else {
        sf.types[type_id as usize].class_id
    };

    if version < 11 {
        let _is_destroyed = r.read_u16();
    }
    if (11..17).contains(&version) {
        let _script_type_index = r.read_i16();
    }
    if version == 15 || version == 16 {
        let _is_stripped = r.read_i8();
    }

    let start = byte_start as usize;
    let end = start + byte_size as usize;
    let obj_data = file[start..end].to_vec();

    Ok(Object {
        path_id,
        type_id,
        class_id,
        type_name: class_name(class_id).to_string(),
        data: obj_data,
    })
}

pub const fn class_name(class_id: i32) -> &'static str {
    match class_id {
        1 => "GameObject",
        4 => "Transform",
        21 => "Material",
        23 => "MeshRenderer",
        28 => "Texture2D",
        33 => "MeshFilter",
        43 => "Mesh",
        49 => "TextAsset",
        64 => "MeshCollider",
        74 => "AnimationClip",
        91 => "AnimatorController",
        95 => "Animator",
        111 => "Animation",
        137 => "SkinnedMeshRenderer",
        142 => "AssetBundle",
        _ => "Object",
    }
}
