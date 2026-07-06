#![allow(non_snake_case)]

use super::common_strings;
use super::streams::{Reader, Writer};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TypeTreeNode {
    pub m_Type: String,
    pub m_Name: String,
    pub m_Level: i32,
    pub m_ByteSize: i32,
    pub m_Index: i32,
    pub m_TypeFlags: i32,
    pub m_Version: i32,
    pub m_MetaFlag: u32,
    pub m_RefTypeHash: u64,
    pub m_Children: Vec<TypeTreeNode>,
}

struct RawNode {
    m_Version: i16,
    m_Level: u8,
    m_TypeFlags: u8,
    type_str_offset: u32,
    name_str_offset: u32,
    m_ByteSize: i32,
    m_Index: i32,
    m_MetaFlag: u32,
    m_RefTypeHash: u64,
}

impl TypeTreeNode {
    pub fn parse_blob(reader: &mut Reader, version: u32) -> TypeTreeNode {
        let node_count = reader.read_i32() as usize;
        let stringbuffer_size = reader.read_i32() as usize;

        let mut raw_nodes: Vec<RawNode> = Vec::with_capacity(node_count);
        for _ in 0..node_count {
            let m_Version = reader.read_i16();
            let m_Level = reader.read_u8();
            let m_TypeFlags = reader.read_u8();
            let type_str_offset = reader.read_u32();
            let name_str_offset = reader.read_u32();
            let m_ByteSize = reader.read_i32();
            let m_Index = reader.read_i32();
            let m_MetaFlag = reader.read_u32();
            let m_RefTypeHash = if version >= 19 { reader.read_u64() } else { 0 };
            raw_nodes.push(RawNode {
                m_Version,
                m_Level,
                m_TypeFlags,
                type_str_offset,
                name_str_offset,
                m_ByteSize,
                m_Index,
                m_MetaFlag,
                m_RefTypeHash,
            });
        }

        let string_buffer = reader.read_bytes_vec(stringbuffer_size);

        let read_string = |value: u32| -> String { resolve_string(&string_buffer, value) };

        let nodes: Vec<TypeTreeNode> = raw_nodes
            .iter()
            .map(|r| TypeTreeNode {
                m_Type: read_string(r.type_str_offset),
                m_Name: read_string(r.name_str_offset),
                m_Level: r.m_Level as i32,
                m_ByteSize: r.m_ByteSize,
                m_Index: r.m_Index,
                m_TypeFlags: r.m_TypeFlags as i32,
                m_Version: r.m_Version as i32,
                m_MetaFlag: r.m_MetaFlag,
                m_RefTypeHash: r.m_RefTypeHash,
                m_Children: Vec::new(),
            })
            .collect();

        build_tree(nodes)
    }

    pub fn dump_blob(&self, writer: &mut Writer, version: u32) {
        let mut node_writer = Writer::new(writer.big_endian);
        let mut string_buffer: Vec<u8> = Vec::new();

        let mut string_offsets: std::collections::HashMap<String, u32> =
            std::collections::HashMap::new();

        let nodes = self.traverse();
        let node_count = nodes.len();

        for node in &nodes {
            let type_off = intern_string(&node.m_Type, &mut string_offsets, &mut string_buffer);
            let name_off = intern_string(&node.m_Name, &mut string_offsets, &mut string_buffer);
            node_writer.write_i16(node.m_Version as i16);
            node_writer.write_u8(node.m_Level as u8);
            node_writer.write_u8(node.m_TypeFlags as u8);
            node_writer.write_u32(type_off);
            node_writer.write_u32(name_off);
            node_writer.write_i32(node.m_ByteSize);
            node_writer.write_i32(node.m_Index);
            node_writer.write_u32(node.m_MetaFlag);
            if version >= 19 {
                node_writer.write_u64(node.m_RefTypeHash);
            }
        }

        writer.write_i32(node_count as i32);
        writer.write_i32(string_buffer.len() as i32);
        writer.write_bytes(&node_writer.buf);
        writer.write_bytes(&string_buffer);
    }

    pub fn traverse(&self) -> Vec<&TypeTreeNode> {
        let mut out = Vec::new();
        let mut stack: Vec<&TypeTreeNode> = vec![self];
        while let Some(node) = stack.pop() {
            out.push(node);
            for child in node.m_Children.iter().rev() {
                stack.push(child);
            }
        }
        out
    }
}

fn resolve_string(buffer: &[u8], value: u32) -> String {
    let is_offset = (value & 0x80000000) == 0;
    if is_offset {
        let start = value as usize;
        let mut end = start;
        while end < buffer.len() && buffer[end] != 0 {
            end += 1;
        }
        String::from_utf8_lossy(&buffer[start..end]).into_owned()
    } else {
        let offset = value & 0x7FFFFFFF;
        common_strings::get(offset)
            .map(str::to_string)
            .unwrap_or_else(|| offset.to_string())
    }
}

fn intern_string(
    s: &str,
    string_offsets: &mut std::collections::HashMap<String, u32>,
    string_buffer: &mut Vec<u8>,
) -> u32 {
    if let Some(&off) = string_offsets.get(s) {
        return off;
    }

    let off = match common_strings::offset_of(s) {
        Some(common_off) => common_off | 0x80000000,
        None => {
            let local = string_buffer.len() as u32;
            string_buffer.extend_from_slice(s.as_bytes());
            string_buffer.push(0);
            local
        }
    };
    string_offsets.insert(s.to_string(), off);
    off
}

fn build_tree(nodes: Vec<TypeTreeNode>) -> TypeTreeNode {
    if nodes.is_empty() {
        return TypeTreeNode::default();
    }

    let mut arena: Vec<TypeTreeNode> = Vec::with_capacity(nodes.len());

    let mut parent_of: Vec<usize> = Vec::with_capacity(nodes.len());

    let mut path: Vec<usize> = Vec::new();

    for node in nodes.into_iter() {
        let level = node.m_Level as usize;
        let idx = arena.len();
        let parent = if level == 0 || path.is_empty() {
            usize::MAX
        } else {
            path[level - 1]
        };
        parent_of.push(parent);
        arena.push(node);

        path.truncate(level);
        path.push(idx);
    }

    let n = arena.len();
    let mut children: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut root_idx = 0usize;
    for i in 0..n {
        let p = parent_of[i];
        if p == usize::MAX {
            root_idx = i;
        } else {
            children[p].push(i);
        }
    }

    fn assemble(
        idx: usize,
        arena: &mut Vec<TypeTreeNode>,
        children: &[Vec<usize>],
    ) -> TypeTreeNode {
        let mut node = std::mem::take(&mut arena[idx]);
        let mut kids = Vec::with_capacity(children[idx].len());
        for &c in &children[idx] {
            kids.push(assemble(c, arena, children));
        }
        node.m_Children = kids;
        node
    }

    assemble(root_idx, &mut arena, &children)
}
