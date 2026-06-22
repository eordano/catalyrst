use super::serialized_file::SerializedFile;
use super::streams::{Reader, Writer};
use anyhow::{anyhow, Result};
use std::path::Path;

const CHUNK_SIZE: usize = 0x20000;
const COMPRESSION_MASK: u32 = 0x3F;

const FLAG_BLOCKS_INFO_AT_END: u32 = 0x80;
const FLAG_BLOCK_INFO_NEED_PADDING: u32 = 0x200;

pub enum FileContent {
    Serialized(SerializedFile),
    Raw(Vec<u8>),
}

pub struct BundleEntry {
    pub name: String,
    pub content: FileContent,

    pub flags: u32,
}

pub struct Bundle {
    pub format_version: u32,
    pub version_player: String,
    pub version_engine: String,
    pub flags: u32,
    pub files: Vec<BundleEntry>,
}

pub struct DecompressedBundle {
    format_version: u32,
    version_player: String,
    version_engine: String,
    flags: u32,
    dir_nodes: Vec<DirNode>,
    blocks_data: Vec<u8>,
}

struct BlockInfo {
    uncompressed_size: u32,
    compressed_size: u32,
    flags: u16,
}

struct DirNode {
    offset: i64,
    size: i64,
    flags: u32,
    path: String,
}

impl Bundle {
    pub fn load(path: &Path) -> Result<Bundle> {
        let mm = crate::local_store::mmap_file(path)?;
        Bundle::load_bytes(&mm)
    }

    pub fn load_bytes(data: &[u8]) -> Result<Bundle> {
        Self::from_decompressed(&Self::decompress_bytes(data)?)
    }

    pub fn decompress_bytes(data: &[u8]) -> Result<DecompressedBundle> {
        let mut r = Reader::new(data, true);
        let signature = r.read_cstr();
        if signature != "UnityFS" {
            return Err(anyhow!("unsupported bundle signature: {signature}"));
        }
        let format_version = r.read_u32();
        let version_player = r.read_cstr();
        let version_engine = r.read_cstr();

        let _size = r.read_i64();
        let compressed_blocks_info_size = r.read_u32();
        let uncompressed_blocks_info_size = r.read_u32();
        let flags = r.read_u32();

        if format_version >= 7 {
            r.align_stream(16);
        }

        let start = r.position();
        let blocks_info_bytes: Vec<u8>;
        if flags & FLAG_BLOCKS_INFO_AT_END != 0 {
            let pos = data.len() - compressed_blocks_info_size as usize;
            blocks_info_bytes = data[pos..pos + compressed_blocks_info_size as usize].to_vec();
            r.pos = start;
        } else {
            blocks_info_bytes = r.read_bytes_vec(compressed_blocks_info_size as usize);
        }

        let blocks_info = decompress_block(
            &blocks_info_bytes,
            uncompressed_blocks_info_size as usize,
            flags & COMPRESSION_MASK,
        )?;

        let mut br = Reader::new(&blocks_info, true);
        let _uncompressed_data_hash = br.read_bytes_vec(16);
        let blocks_count = br.read_i32();
        let mut m_blocks = Vec::with_capacity(blocks_count.max(0) as usize);
        for _ in 0..blocks_count.max(0) {
            m_blocks.push(BlockInfo {
                uncompressed_size: br.read_u32(),
                compressed_size: br.read_u32(),
                flags: br.read_u16(),
            });
        }
        let nodes_count = br.read_i32();
        let mut dir_nodes = Vec::with_capacity(nodes_count.max(0) as usize);
        for _ in 0..nodes_count.max(0) {
            dir_nodes.push(DirNode {
                offset: br.read_i64(),
                size: br.read_i64(),
                flags: br.read_u32(),
                path: br.read_cstr(),
            });
        }

        if flags & FLAG_BLOCK_INFO_NEED_PADDING != 0 {
            r.align_stream(16);
        }

        let mut blocks_data: Vec<u8> = Vec::new();
        for block in &m_blocks {
            let comp = r.read_bytes_vec(block.compressed_size as usize);
            let dec = decompress_block(
                &comp,
                block.uncompressed_size as usize,
                block.flags as u32 & COMPRESSION_MASK,
            )?;
            blocks_data.extend_from_slice(&dec);
        }

        Ok(DecompressedBundle {
            format_version,
            version_player,
            version_engine,
            flags,
            dir_nodes,
            blocks_data,
        })
    }

    pub fn from_decompressed(d: &DecompressedBundle) -> Result<Bundle> {
        let DecompressedBundle {
            format_version,
            version_player,
            version_engine,
            flags,
            dir_nodes,
            blocks_data,
        } = d;
        let (format_version, flags) = (*format_version, *flags);

        let mut files = Vec::with_capacity(dir_nodes.len());
        for node in dir_nodes {
            let start = node.offset as usize;
            let end = start + node.size as usize;
            let slice = &blocks_data[start..end];

            let lower = node.path.to_lowercase();
            let is_resource =
                lower.ends_with(".ress") || lower.ends_with(".resource") || lower.ends_with(".res");
            let content = if !is_resource && is_serialized_file(slice) {
                FileContent::Serialized(SerializedFile::parse(slice)?)
            } else {
                FileContent::Raw(slice.to_vec())
            };
            files.push(BundleEntry {
                name: node.path.clone(),
                content,
                flags: node.flags,
            });
        }

        Ok(Bundle {
            format_version,
            version_player: version_player.clone(),
            version_engine: version_engine.clone(),
            flags,
            files,
        })
    }

    pub fn serialized(&self) -> Option<&SerializedFile> {
        self.files.iter().find_map(|e| match &e.content {
            FileContent::Serialized(sf) => Some(sf),
            _ => None,
        })
    }

    pub fn serialized_mut(&mut self) -> Option<&mut SerializedFile> {
        self.files.iter_mut().find_map(|e| match &mut e.content {
            FileContent::Serialized(sf) => Some(sf),
            _ => None,
        })
    }

    pub fn remove_file(&mut self, name: &str) {
        self.files.retain(|e| e.name != name);
    }

    pub fn save_lz4(&self) -> Result<Vec<u8>> {
        let mut file_data: Vec<u8> = Vec::new();
        let mut dir_nodes: Vec<DirNode> = Vec::new();
        let mut offset: i64 = 0;
        for entry in &self.files {
            let bytes = match &entry.content {
                FileContent::Serialized(sf) => sf.save(),
                FileContent::Raw(b) => b.clone(),
            };
            let len = bytes.len() as i64;
            file_data.extend_from_slice(&bytes);
            dir_nodes.push(DirNode {
                offset,
                size: len,
                flags: entry.flags,
                path: entry.name.clone(),
            });
            offset += len;
        }

        let data_flag: u32 = 0x243;
        let block_info_flag: u16 = 3;

        let (compressed_file_data, block_info) = chunk_based_compress(&file_data, block_info_flag);

        let mut bw = Writer::new(true);
        bw.write_bytes(&[0u8; 16]);
        bw.write_i32(block_info.len() as i32);
        for b in &block_info {
            bw.write_u32(b.uncompressed_size);
            bw.write_u32(b.compressed_size);
            bw.write_u16(b.flags);
        }
        bw.write_i32(dir_nodes.len() as i32);
        for node in &dir_nodes {
            bw.write_i64(node.offset);
            bw.write_i64(node.size);
            bw.write_u32(node.flags);
            bw.write_cstr(&node.path);
        }
        let block_data_uncompressed = bw.into_bytes();
        let uncompressed_block_data_size = block_data_uncompressed.len() as u32;

        let block_data = lz4hc_compress(&block_data_uncompressed);
        let compressed_block_data_size = block_data.len() as u32;

        let mut w = Writer::new(true);
        w.write_cstr("UnityFS");
        w.write_u32(self.format_version);
        w.write_cstr(&self.version_player);
        w.write_cstr(&self.version_engine);

        let size_pos = w.position();
        w.write_i64(0);
        w.write_u32(compressed_block_data_size);
        w.write_u32(uncompressed_block_data_size);
        w.write_u32(data_flag);

        if self.format_version >= 7 {
            w.align_stream(16);
        }

        w.write_bytes(&block_data);
        w.align_stream(16);
        w.write_bytes(&compressed_file_data);

        let end = w.buf.len() as i64;
        let size_bytes = end.to_be_bytes();
        w.buf[size_pos..size_pos + 8].copy_from_slice(&size_bytes);

        Ok(w.into_bytes())
    }
}

fn is_serialized_file(data: &[u8]) -> bool {
    if data.len() < 20 {
        return false;
    }

    let version = u32::from_be_bytes([data[8], data[9], data[10], data[11]]);
    (6..=100).contains(&version)
}

fn decompress_block(data: &[u8], uncompressed_size: usize, comp_flag: u32) -> Result<Vec<u8>> {
    match comp_flag {
        0 => Ok(data.to_vec()),
        2 | 3 => lz4_decompress(data, uncompressed_size),
        other => Err(anyhow!("unsupported compression flag {other}")),
    }
}

fn lz4_decompress(src: &[u8], dst_size: usize) -> Result<Vec<u8>> {
    crate::lz4::decompress(src, dst_size).map_err(|e| anyhow!("LZ4 decompress failed: {e}"))
}

fn lz4_cache_dir() -> Option<&'static std::path::Path> {
    use std::sync::OnceLock;
    static DIR: OnceLock<Option<std::path::PathBuf>> = OnceLock::new();
    DIR.get_or_init(|| {
        let d = std::env::var("ABGEN_BC7_CACHE").ok().filter(|s| !s.is_empty())?;
        let p = std::path::PathBuf::from(d);
        std::fs::create_dir_all(&p).ok()?;
        Some(p)
    })
    .as_deref()
}

fn lz4hc_compress(src: &[u8]) -> Vec<u8> {
    if src.is_empty() {
        return Vec::new();
    }
    if let Ok(dir) = std::env::var("ABGEN_LZ4_DUMP") {
        use std::sync::atomic::{AtomicU64, Ordering};
        static N: AtomicU64 = AtomicU64::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        let _ = std::fs::write(format!("{dir}/block-{n:04}.bin"), src);
    }
    lz4hc_compress_cached(src)
}

fn lz4hc_compress_cached(src: &[u8]) -> Vec<u8> {
    use sha1::{Digest, Sha1};
    use std::sync::{Arc, Mutex, OnceLock};

    let disk = match lz4_cache_dir() {
        Some(d) => d,
        None => return crate::lz4::compress_hc(src),
    };

    let mut hsh = Sha1::new();
    hsh.update(b"lz4hc-v1");
    hsh.update((src.len() as u64).to_le_bytes());
    hsh.update(src);
    let key: [u8; 20] = hsh.finalize().into();

    static MEM: OnceLock<Mutex<std::collections::HashMap<[u8; 20], Arc<Vec<u8>>>>> =
        OnceLock::new();
    let mem = MEM.get_or_init(|| Mutex::new(std::collections::HashMap::new()));
    if let Some(v) = mem.lock().unwrap().get(&key).cloned() {
        return (*v).clone();
    }

    let hexkey: String = key.iter().map(|b| format!("{b:02x}")).collect();
    if let Ok(bytes) = std::fs::read(disk.join(&hexkey)) {
        let val = Arc::new(bytes);
        mem.lock().unwrap().insert(key, val.clone());
        return (*val).clone();
    }

    let out = crate::lz4::compress_hc(src);
    let tmp = disk.join(format!("{hexkey}.tmp"));
    if std::fs::write(&tmp, &out).is_ok() {
        let _ = std::fs::rename(&tmp, disk.join(&hexkey));
    }
    mem.lock().unwrap().insert(key, Arc::new(out.clone()));
    out
}

fn chunk_based_compress(data: &[u8], block_info_flag: u16) -> (Vec<u8>, Vec<BlockInfo>) {
    let switch = (block_info_flag as u32) & COMPRESSION_MASK;
    if switch == 0 {
        return (
            data.to_vec(),
            vec![BlockInfo {
                uncompressed_size: data.len() as u32,
                compressed_size: data.len() as u32,
                flags: block_info_flag,
            }],
        );
    }

    use rayon::prelude::*;
    let pieces: Vec<(Vec<u8>, BlockInfo)> = data
        .par_chunks(CHUNK_SIZE)
        .map(|chunk| {
            let comp = lz4hc_compress(chunk);
            let uncompressed_size = chunk.len() as u32;

            if comp.len() > chunk.len() {
                (
                    chunk.to_vec(),
                    BlockInfo {
                        uncompressed_size,
                        compressed_size: uncompressed_size,
                        flags: block_info_flag ^ (switch as u16),
                    },
                )
            } else {
                let compressed_size = comp.len() as u32;
                (
                    comp,
                    BlockInfo {
                        uncompressed_size,
                        compressed_size,
                        flags: block_info_flag,
                    },
                )
            }
        })
        .collect();

    let total: usize = pieces.iter().map(|(b, _)| b.len()).sum();
    let mut out = Vec::with_capacity(total);
    let mut blocks = Vec::with_capacity(pieces.len());
    for (bytes, info) in pieces {
        out.extend_from_slice(&bytes);
        blocks.push(info);
    }
    (out, blocks)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn template() -> std::path::PathBuf {
        let root = std::env::var("ABGEN_ROOT")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|_| {
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .parent()
                    .unwrap()
                    .to_path_buf()
            });
        root.join("template").join("all-types.windows.bundle")
    }

    #[test]
    fn roundtrip_all_types_bundle() {
        let path = template();
        if !path.exists() {
            eprintln!("template missing, skipping: {}", path.display());
            return;
        }
        let bundle = Bundle::load(&path).expect("load template");
        let sf = bundle.serialized().expect("serialized");
        let n_objects = sf.objects.len();
        let n_types = sf.types.len();
        assert!(n_objects > 0, "expected objects in template");
        assert_eq!(n_types, 10, "template has 10 types");

        let saved = bundle.save_lz4().expect("save_lz4");
        let reloaded = Bundle::load_bytes(&saved).expect("reload saved");
        let sf2 = reloaded.serialized().expect("serialized reload");
        assert_eq!(sf2.objects.len(), n_objects, "object count preserved");
        assert_eq!(sf2.types.len(), n_types, "type count preserved");

        for obj in &sf.objects {
            let orig = sf.read_typetree(obj).expect("read orig typetree");
            let obj2 = sf2
                .objects
                .iter()
                .find(|o| o.path_id == obj.path_id)
                .expect("object present after reload");
            let again = sf2.read_typetree(obj2).expect("read reloaded typetree");
            assert_eq!(orig, again, "typetree mismatch for path_id {}", obj.path_id);
        }
    }

    #[test]
    #[ignore]
    fn emit_resaved_bundle() {
        let path = template();
        if !path.exists() {
            return;
        }
        let bundle = Bundle::load(&path).expect("load");
        let saved = bundle.save_lz4().expect("save");
        let out = std::env::temp_dir().join("abgen_rs_resaved.bundle");
        std::fs::write(&out, &saved).expect("write");
        eprintln!("wrote resaved bundle to {}", out.display());
    }

    #[test]
    fn typetree_value_roundtrip() {
        let path = template();
        if !path.exists() {
            return;
        }
        let bundle = Bundle::load(&path).expect("load");
        let sf = bundle.serialized().expect("sf");
        for obj in &sf.objects {
            let node = sf.types[obj.type_id as usize].node.as_ref().unwrap();
            let v = super::super::typetree::read_typetree(&obj.data, node, sf.big_endian).unwrap();
            let bytes = super::super::typetree::write_typetree(&v, node, sf.big_endian);
            let v2 = super::super::typetree::read_typetree(&bytes, node, sf.big_endian).unwrap();
            assert_eq!(v, v2, "value roundtrip for {}", obj.type_name);
        }
    }
}
