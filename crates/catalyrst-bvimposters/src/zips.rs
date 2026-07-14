use std::io::{Cursor, Read, Write};
use std::path::Path;

use anyhow::{anyhow, Context, Result};

use crate::key::ImposterKey;

pub fn spec_crc(bytes: &[u8], spec_member: &str) -> Result<u32> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).context("unreadable zip")?;
    let mut member = archive
        .by_name(spec_member)
        .with_context(|| format!("missing zip member {spec_member}"))?;
    let mut buf = Vec::new();
    member
        .read_to_end(&mut buf)
        .context("reading spec member")?;
    let value: serde_json::Value =
        serde_json::from_slice(&buf).context("spec member is not json")?;
    let crc = value
        .get("crc")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| anyhow!("spec member has no crc field"))?;
    u32::try_from(crc).map_err(|_| anyhow!("spec crc out of range"))
}

pub fn verify_zip(bytes: &[u8], key: &ImposterKey) -> Result<()> {
    let crc = spec_crc(bytes, &key.spec_member_name())?;
    if crc != key.crc {
        return Err(anyhow!(
            "spec crc {} does not match key crc {}",
            crc,
            key.crc
        ));
    }
    Ok(())
}

pub fn extract_spec(bytes: &[u8], key: &ImposterKey) -> Result<Vec<u8>> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).context("unreadable zip")?;
    let name = key.spec_member_name();
    let mut member = archive
        .by_name(&name)
        .with_context(|| format!("missing zip member {name}"))?;
    let mut buf = Vec::new();
    member
        .read_to_end(&mut buf)
        .context("reading spec member")?;
    Ok(buf)
}

pub fn stored_zip_bytes(members: &[(String, Vec<u8>)]) -> Result<Vec<u8>> {
    let mut cursor = Cursor::new(Vec::new());
    {
        let mut zw = zip::ZipWriter::new(&mut cursor);
        let options: zip::write::FileOptions<'_, ()> =
            zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Stored);
        for (name, data) in members {
            zw.start_file(name, options)
                .with_context(|| format!("adding {name} to the zip"))?;
            zw.write_all(data)
                .with_context(|| format!("writing {name} into the zip"))?;
        }
        zw.finish().context("finalizing the zip")?;
    }
    Ok(cursor.into_inner())
}

pub fn write_stored_zip(target: &Path, members: &[(String, Vec<u8>)]) -> Result<()> {
    let bytes = stored_zip_bytes(members)?;
    let mut f =
        std::fs::File::create(target).with_context(|| format!("creating {}", target.display()))?;
    f.write_all(&bytes)
        .with_context(|| format!("writing {}", target.display()))?;
    f.sync_all()
        .with_context(|| format!("syncing {}", target.display()))?;
    Ok(())
}

#[cfg(test)]
pub fn test_zip_bytes(x: i32, y: i32, crc: u32) -> Vec<u8> {
    let spec = serde_json::json!({"imposters": {}, "crc": crc});
    stored_zip_bytes(&[
        (
            format!("{x},{y}-spec.json"),
            serde_json::to_vec(&spec).unwrap(),
        ),
        (format!("{x},{y}.boimp"), vec![1u8; 64]),
        (format!("{x},{y}-floor.boimp"), vec![2u8; 32]),
    ])
    .unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::key::ImposterKey;

    #[test]
    fn verify_matches_spec_crc() {
        let key = ImposterKey::new(0, 0, 100, 3504527830).unwrap();
        let bytes = test_zip_bytes(0, 100, 3504527830);
        verify_zip(&bytes, &key).unwrap();
    }

    #[test]
    fn verify_rejects_crc_mismatch() {
        let key = ImposterKey::new(0, 0, 100, 999).unwrap();
        let bytes = test_zip_bytes(0, 100, 3504527830);
        assert!(verify_zip(&bytes, &key).is_err());
    }

    #[test]
    fn verify_rejects_garbage() {
        let key = ImposterKey::new(0, 0, 100, 999).unwrap();
        assert!(verify_zip(b"not a zip", &key).is_err());
    }

    #[test]
    fn extracts_spec_member() {
        let key = ImposterKey::new(0, 0, 100, 3504527830).unwrap();
        let bytes = test_zip_bytes(0, 100, 3504527830);
        let spec = extract_spec(&bytes, &key).unwrap();
        let value: serde_json::Value = serde_json::from_slice(&spec).unwrap();
        assert_eq!(value["crc"].as_u64(), Some(3504527830));
    }
}
