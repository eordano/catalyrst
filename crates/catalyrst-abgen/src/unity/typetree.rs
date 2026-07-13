use super::streams::{Reader, Writer};
use super::typetree_node::TypeTreeNode;
use crate::value::{Map, Value};
use anyhow::{anyhow, Result};

const K_ALIGN_BYTES: u32 = 0x4000;

const fn is_aligned(meta_flag: u32) -> bool {
    (meta_flag & K_ALIGN_BYTES) != 0
}

pub fn read_typetree(data: &[u8], node: &TypeTreeNode, big_endian: bool) -> Result<Value> {
    let mut reader = Reader::new(data, big_endian);
    let v = read_value(node, &mut reader)?;
    Ok(v)
}

pub fn write_typetree(value: &Value, node: &TypeTreeNode, big_endian: bool) -> Vec<u8> {
    let mut writer = Writer::new(big_endian);
    write_value(value, node, &mut writer);
    writer.into_bytes()
}

fn read_primitive(ty: &str, r: &mut Reader) -> Option<Value> {
    let v = match ty {
        "SInt8" => Value::Int(r.read_i8() as i64),
        "UInt8" | "char" => Value::Int(r.read_u8() as i64),
        "short" | "SInt16" => Value::Int(r.read_i16() as i64),
        "unsigned short" | "UInt16" => Value::Int(r.read_u16() as i64),
        "int" | "SInt32" => Value::Int(r.read_i32() as i64),
        "unsigned int" | "UInt32" | "Type*" => Value::Int(r.read_u32() as i64),
        "long long" | "SInt64" => Value::Int(r.read_i64()),
        "unsigned long long" | "UInt64" | "FileSize" => Value::Int(r.read_u64() as i64),
        "float" => Value::Float(r.read_f32() as f64),
        "double" => Value::Float(r.read_f64()),
        "bool" => Value::Bool(r.read_bool()),
        "string" => Value::Str(r.read_aligned_string()),
        "TypelessData" => Value::Bytes(r.read_byte_array()),
        _ => return None,
    };
    Some(v)
}

fn read_value(node: &TypeTreeNode, r: &mut Reader) -> Result<Value> {
    let mut align = is_aligned(node.m_MetaFlag);

    let value = if let Some(v) = read_primitive(&node.m_Type, r) {
        v
    } else if node.m_Type == "pair" {
        let first = read_value(&node.m_Children[0], r)?;
        let second = read_value(&node.m_Children[1], r)?;
        Value::Array(vec![first, second])
    } else if !node.m_Children.is_empty() && node.m_Children[0].m_Type == "Array" {
        let array_node = &node.m_Children[0];
        if is_aligned(array_node.m_MetaFlag) {
            align = true;
        }
        let size = r.read_i32();
        if size < 0 {
            return Err(anyhow!("Negative length read from TypeTree"));
        }
        let subtype = &array_node.m_Children[1];

        read_value_array(subtype, r, size as usize)?
    } else {
        let mut m = Map::new();
        for child in &node.m_Children {
            let cv = read_value(child, r)?;
            m.insert(child.m_Name.clone(), cv);
        }
        Value::Map(m)
    };

    if align {
        r.align_stream(4);
    }
    Ok(value)
}

fn read_value_array(subtype: &TypeTreeNode, r: &mut Reader, size: usize) -> Result<Value> {
    let sub_align = is_aligned(subtype.m_MetaFlag);

    match subtype.m_Type.as_str() {
        "UInt8" | "char" | "SInt8" => {
            let bytes = r.read_bytes_vec(size);

            if sub_align {
                r.align_stream(4);
            }
            return Ok(Value::Bytes(bytes));
        }
        _ => {}
    }

    let mut out = Vec::with_capacity(size);
    for _ in 0..size {
        out.push(read_value(subtype, r)?);
    }
    Ok(Value::Array(out))
}

fn write_primitive(ty: &str, value: &Value, w: &mut Writer) -> bool {
    match ty {
        "SInt8" => w.write_i8(value.as_i64().unwrap_or(0) as i8),
        "UInt8" | "char" => w.write_u8(value.as_i64().unwrap_or(0) as u8),
        "short" | "SInt16" => w.write_i16(value.as_i64().unwrap_or(0) as i16),
        "unsigned short" | "UInt16" => w.write_u16(value.as_i64().unwrap_or(0) as u16),
        "int" | "SInt32" => w.write_i32(value.as_i64().unwrap_or(0) as i32),
        "unsigned int" | "UInt32" | "Type*" => w.write_u32(value.as_i64().unwrap_or(0) as u32),
        "long long" | "SInt64" => w.write_i64(value.as_i64().unwrap_or(0)),
        "unsigned long long" | "UInt64" | "FileSize" => {
            w.write_u64(value.as_i64().unwrap_or(0) as u64)
        }
        "float" => w.write_f32(value.as_f64().unwrap_or(0.0) as f32),
        "double" => w.write_f64(value.as_f64().unwrap_or(0.0)),
        "bool" => w.write_bool(value.as_bool().unwrap_or(false)),
        "string" => w.write_aligned_string(value.as_str().unwrap_or("")),
        "TypelessData" => write_byte_buffer(value, w),
        _ => return false,
    }
    true
}

fn write_byte_buffer(value: &Value, w: &mut Writer) {
    match value {
        Value::Bytes(b) => w.write_byte_array(b),
        Value::Array(a) => {
            w.write_i32(a.len() as i32);
            for v in a {
                w.write_u8(v.as_i64().unwrap_or(0) as u8);
            }
        }
        _ => w.write_i32(0),
    }
}

fn write_value(value: &Value, node: &TypeTreeNode, w: &mut Writer) {
    let mut align = is_aligned(node.m_MetaFlag);

    if write_primitive(&node.m_Type, value, w) {
    } else if node.m_Type == "pair" {
        let arr = value.as_array().unwrap_or(&[]);
        write_value(arr.first().unwrap_or(&Value::Null), &node.m_Children[0], w);
        write_value(arr.get(1).unwrap_or(&Value::Null), &node.m_Children[1], w);
    } else if !node.m_Children.is_empty() && node.m_Children[0].m_Type == "Array" {
        let array_node = &node.m_Children[0];
        if is_aligned(array_node.m_MetaFlag) {
            align = true;
        }
        let subtype = &array_node.m_Children[1];
        write_vector(value, subtype, w);
    } else {
        let m = value.as_map();
        for child in &node.m_Children {
            let cv = m.and_then(|m| m.get(&child.m_Name)).unwrap_or(&Value::Null);
            write_value(cv, child, w);
        }
    }

    if align {
        w.align_stream(4);
    }
}

fn write_vector(value: &Value, subtype: &TypeTreeNode, w: &mut Writer) {
    let sub_align = is_aligned(subtype.m_MetaFlag);
    let is_byteish = matches!(subtype.m_Type.as_str(), "UInt8" | "char" | "SInt8");

    if is_byteish {
        match value {
            Value::Bytes(b) => {
                w.write_i32(b.len() as i32);
                w.write_bytes(b);
            }
            Value::Array(a) => {
                w.write_i32(a.len() as i32);
                for v in a {
                    w.write_u8(v.as_i64().unwrap_or(0) as u8);
                }
            }
            _ => w.write_i32(0),
        }
        if sub_align {
            w.align_stream(4);
        }
        return;
    }

    let arr = value.as_array().unwrap_or(&[]);
    w.write_i32(arr.len() as i32);
    for v in arr {
        write_value(v, subtype, w);
    }
}
