use crate::hashes::Sha256;
use anyhow::{anyhow, bail, Result};
use std::collections::HashMap;

pub const GLTF_EXTENSIONS: [&str; 2] = [".glb", ".gltf"];

const GLB_MAGIC: u32 = 0x46546C67;
const GLB_CHUNK_TYPE_JSON: u32 = 0x4E4F534A;
const GLB_HEADER_BYTES: usize = 12;
const GLB_CHUNK_HEADER_BYTES: usize = 8;

pub fn file_extension(name: &str) -> String {
    let name = name.to_lowercase();
    match name.rfind('.') {
        Some(i) => name[i..].to_string(),
        None => String::new(),
    }
}

fn read_u32_le(data: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        data[offset],
        data[offset + 1],
        data[offset + 2],
        data[offset + 3],
    ])
}

fn extract_gltf_json(data: &[u8], ext: &str) -> Result<String> {
    if ext == ".gltf" {
        return Ok(String::from_utf8(data.to_vec())?);
    }

    if data.len() < GLB_HEADER_BYTES + GLB_CHUNK_HEADER_BYTES {
        bail!("glb too short: {} bytes", data.len());
    }
    let magic = read_u32_le(data, 0);
    if magic != GLB_MAGIC {
        bail!("glb magic mismatch: got 0x{:x}", magic);
    }
    let version = read_u32_le(data, 4);
    if version != 2 {
        bail!("unsupported glb version: {}", version);
    }

    let chunk_length = read_u32_le(data, GLB_HEADER_BYTES) as usize;
    let chunk_type = read_u32_le(data, GLB_HEADER_BYTES + 4);
    if chunk_type != GLB_CHUNK_TYPE_JSON {
        bail!("glb first chunk is not JSON (type 0x{:x})", chunk_type);
    }
    let json_start = GLB_HEADER_BYTES + GLB_CHUNK_HEADER_BYTES;
    let json_end = json_start + chunk_length;
    if json_end > data.len() {
        bail!("glb JSON chunk overruns buffer");
    }

    let mut end = json_end;
    while end > json_start && matches!(data[end - 1], 0x00 | 0x20 | 0x09 | 0x0A | 0x0D) {
        end -= 1;
    }
    Ok(String::from_utf8(data[json_start..end].to_vec())?)
}

pub fn parse_gltf_image_uris(data: &[u8], ext: &str) -> Result<Vec<String>> {
    let doc: serde_json::Value = serde_json::from_str(&extract_gltf_json(data, ext)?)?;
    if !doc.is_object() {
        bail!("glTF root must be an object");
    }
    let mut uris: Vec<String> = Vec::new();
    let arr = match doc.get("images").and_then(|v| v.as_array()) {
        Some(a) => a,
        None => return Ok(uris),
    };
    for entry in arr {
        if !entry.is_object() {
            continue;
        }

        if entry.get("bufferView").and_then(|v| v.as_i64()).is_some() {
            continue;
        }
        let uri = match entry.get("uri").and_then(|v| v.as_str()) {
            Some(u) => u,
            None => continue,
        };
        if uri.starts_with("data:") || uri.is_empty() {
            continue;
        }
        uris.push(uri.to_string());
    }
    Ok(uris)
}

fn parse_gltf_dep_refs(data: &[u8], ext: &str) -> Result<Vec<String>> {
    let doc: serde_json::Value = serde_json::from_str(&extract_gltf_json(data, ext)?)?;
    if !doc.is_object() {
        bail!("glTF root must be an object");
    }

    let mut uris: Vec<String> = Vec::new();
    for key in ["images", "buffers"] {
        let arr = match doc.get(key).and_then(|v| v.as_array()) {
            Some(a) => a,
            None => continue,
        };
        for entry in arr {
            if !entry.is_object() {
                continue;
            }
            let uri = match entry.get("uri").and_then(|v| v.as_str()) {
                Some(u) => u,
                None => continue,
            };
            if uri.starts_with("data:") {
                continue;
            }
            if !uris.iter().any(|u| u == uri) {
                uris.push(uri.to_string());
            }
        }
    }
    uris.sort();
    Ok(uris)
}

fn has_scheme(uri: &str) -> bool {
    let bytes = uri.as_bytes();
    if bytes.is_empty() || !bytes[0].is_ascii_alphabetic() {
        return false;
    }
    let mut i = 1;
    while i < bytes.len() {
        let c = bytes[i];
        if c == b':' {
            return true;
        }
        if c.is_ascii_alphanumeric() || c == b'+' || c == b'.' || c == b'-' {
            i += 1;
        } else {
            return false;
        }
    }
    false
}

fn percent_decode(uri: &str) -> Result<String> {
    let bytes = uri.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hi = (bytes[i + 1] as char).to_digit(16);
            let lo = (bytes[i + 2] as char).to_digit(16);
            if let (Some(hi), Some(lo)) = (hi, lo) {
                out.push((hi * 16 + lo) as u8);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    Ok(String::from_utf8(out)?)
}

fn posix_dirname(path: &str) -> String {
    match path.rfind('/') {
        None => String::new(),
        Some(i) => {
            let head = &path[..=i];

            let trimmed = head.trim_end_matches('/');
            if trimmed.is_empty() {
                head.to_string()
            } else {
                trimmed.to_string()
            }
        }
    }
}

fn posix_join(base: &str, path: &str) -> String {
    if base.is_empty() {
        return path.to_string();
    }
    if base.ends_with('/') {
        format!("{base}{path}")
    } else {
        format!("{base}/{path}")
    }
}

fn posix_normpath(path: &str) -> String {
    if path.is_empty() {
        return ".".to_string();
    }

    let initial_slashes = if path.starts_with('/') {
        if path.starts_with("//") && !path.starts_with("///") {
            2
        } else {
            1
        }
    } else {
        0
    };

    let mut comps: Vec<&str> = Vec::new();
    for comp in path.split('/') {
        if comp.is_empty() || comp == "." {
            continue;
        }
        if comp != ".."
            || (initial_slashes == 0 && comps.is_empty())
            || (!comps.is_empty() && comps.last() == Some(&".."))
        {
            comps.push(comp);
        } else if !comps.is_empty() {
            comps.pop();
        }
    }

    let mut result = comps.join("/");
    let prefix = "/".repeat(initial_slashes);
    result = format!("{prefix}{result}");
    if result.is_empty() {
        ".".to_string()
    } else {
        result
    }
}

pub fn resolve_uri_to_content_file(uri: &str, glb_file: &str) -> Result<String> {
    if uri.is_empty() {
        bail!("glTF URI is empty");
    }
    if has_scheme(uri) {
        bail!("glTF URI \"{}\" has a URI scheme", uri);
    }
    if uri.starts_with("//") {
        bail!("glTF URI \"{}\" is protocol-relative", uri);
    }
    if uri.starts_with('/') {
        bail!("glTF URI \"{}\" is an absolute path", uri);
    }
    if uri.contains('?') || uri.contains('#') {
        bail!("glTF URI \"{}\" contains a query/fragment", uri);
    }

    let decoded = percent_decode(uri)?;
    let base = posix_dirname(glb_file);
    let joined = if base == "." || base.is_empty() {
        decoded
    } else {
        posix_join(&base, &decoded)
    };
    let normalized = posix_normpath(&joined);
    if normalized.starts_with("../") || normalized == ".." {
        bail!("glTF URI \"{}\" escapes entity root", uri);
    }
    Ok(normalized)
}

pub fn compute_deps_digest(deps: &[(String, String)]) -> String {
    let mut ordered: Vec<&(String, String)> = deps.iter().collect();
    ordered.sort_by(|a, b| (a.0.as_str(), a.1.as_str()).cmp(&(b.0.as_str(), b.1.as_str())));
    let payload: Vec<[&str; 2]> = ordered
        .iter()
        .map(|(f, h)| [f.as_str(), h.as_str()])
        .collect();

    let json = serde_json::to_string(&payload).expect("serialize deps");
    let mut hasher = Sha256::new();
    hasher.update(json.as_bytes());
    let digest = hasher.finalize();
    let hex = digest
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    hex[..32].to_string()
}

pub fn deps_digest_for_glb(
    glb_bytes: &[u8],
    glb_file: &str,
    content_by_file: &HashMap<String, String>,
    tolerant: bool,
) -> Result<String> {
    let ext = file_extension(glb_file);
    let uris = parse_gltf_dep_refs(glb_bytes, &ext)?;
    let mut seen: Vec<String> = Vec::new();
    let mut deps: Vec<(String, String)> = Vec::new();
    for uri in &uris {
        let resolved = resolve_uri_to_content_file(uri, glb_file)?;
        let h = match content_by_file.get(&resolved.to_lowercase()) {
            Some(h) => h,
            None if tolerant => {
                continue;
            }
            None => {
                let base = resolved.rsplit('/').next().unwrap_or(&resolved);
                let elsewhere = content_by_file
                    .keys()
                    .find(|k| k.rsplit('/').next() == Some(base));
                return Err(match elsewhere {
                    Some(other) => anyhow!(
                        "dep \"{}\" -> \"{}\" not in entity content \
                         (but \"{}\" is deployed at \"{}\" — mis-pathed kit-pack asset; \
                         republish with the texture in the referenced folder)",
                        uri,
                        resolved,
                        base,
                        other
                    ),
                    None => anyhow!(
                        "dep \"{}\" -> \"{}\" not in entity content \
                         (texture not deployed in this entity)",
                        uri,
                        resolved
                    ),
                });
            }
        };
        let key = format!("{resolved}\0{h}");
        if seen.iter().any(|k| k == &key) {
            continue;
        }
        seen.push(key);
        deps.push((resolved, h.clone()));
    }
    Ok(compute_deps_digest(&deps))
}

pub fn canonical_filename(
    hash: &str,
    ext: &str,
    target: &str,
    digest: Option<&str>,
) -> Result<String> {
    if GLTF_EXTENSIONS.contains(&ext.to_lowercase().as_str()) {
        let digest = match digest {
            Some(d) if !d.is_empty() => d,
            _ => bail!("missing deps digest for glb/gltf {}", hash),
        };
        return Ok(format!("{hash}_{digest}_{target}"));
    }
    Ok(format!("{hash}_{target}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deps_digest_known_input() {
        let d = compute_deps_digest(&[("a/b.bin".to_string(), "hashX".to_string())]);
        assert_eq!(d, "441ffddfaeacac66582c65c0fc84f688");
    }

    #[test]
    fn deps_digest_empty() {
        assert_eq!(compute_deps_digest(&[]), "4f53cda18c2baa0c0354bb5f9a3ecbe5");
    }

    #[test]
    fn file_extension_basic() {
        assert_eq!(file_extension("foo.GLB"), ".glb");
        assert_eq!(file_extension("foo"), "");
        assert_eq!(file_extension("a/b.PNG"), ".png");
    }

    #[test]
    fn canonical_glb_requires_digest() {
        assert!(canonical_filename("h", ".glb", "windows", None).is_err());
        assert_eq!(
            canonical_filename("h", ".glb", "windows", Some("dig")).unwrap(),
            "h_dig_windows"
        );
        assert_eq!(
            canonical_filename("h", ".png", "mac", None).unwrap(),
            "h_mac"
        );
    }

    #[test]
    fn resolve_basic() {
        assert_eq!(
            resolve_uri_to_content_file("tex/a.png", "models/scene.glb").unwrap(),
            "models/tex/a.png"
        );
        assert!(resolve_uri_to_content_file("../../etc", "scene.glb").is_err());
        assert!(resolve_uri_to_content_file("/abs", "scene.glb").is_err());
        assert!(resolve_uri_to_content_file("http://x/y", "scene.glb").is_err());
    }
}

#[cfg(test)]
mod xcheck {
    use super::*;
    #[test]
    fn cross_reference() {
        assert_eq!(
            compute_deps_digest(&[
                ("z.bin".into(), "h2".into()),
                ("a/ünïcode.png".into(), "h1".into()),
                ("a/ünïcode.png".into(), "h1".into()),
            ]),
            "57c04462ea1a65b02ba45e7dd9f55490"
        );
        assert_eq!(
            resolve_uri_to_content_file("tex%2Fa.png", "models/scene.glb").unwrap(),
            "models/tex/a.png"
        );
        assert_eq!(
            resolve_uri_to_content_file("./a.bin", "scene.glb").unwrap(),
            "a.bin"
        );
        assert_eq!(
            resolve_uri_to_content_file("sub/../a.bin", "models/scene.glb").unwrap(),
            "models/a.bin"
        );
        assert_eq!(file_extension("."), ".");
        assert_eq!(file_extension("noext."), ".");
    }
}
