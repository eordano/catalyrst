use anyhow::Result;
use std::io::Write;

const BROTLI_QUALITY: u32 = 11;

const BROTLI_LGWIN: u32 = 22;

const BROTLI_BUFFER: usize = 4096;

pub fn brotli(data: &[u8]) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    {
        let mut w =
            brotli::CompressorWriter::new(&mut out, BROTLI_BUFFER, BROTLI_QUALITY, BROTLI_LGWIN);
        w.write_all(data)?;
        w.flush()?;
    }
    Ok(out)
}

pub fn brotli_decompress(data: &[u8]) -> Result<Vec<u8>> {
    use std::io::Read;
    let mut out = Vec::new();
    let mut r = brotli::Decompressor::new(data, BROTLI_BUFFER);
    r.read_to_end(&mut out)?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_small() {
        let data = b"hello asset-bundle CDN world";
        let comp = brotli(data).unwrap();
        let back = brotli_decompress(&comp).unwrap();
        assert_eq!(&back, data);
    }

    #[test]
    fn round_trip_large_and_compresses() {
        let data: Vec<u8> = std::iter::repeat_n(b"unity asset bundle chunk ", 4096)
            .flatten()
            .copied()
            .collect();
        let comp = brotli(&data).unwrap();
        assert!(
            comp.len() < data.len(),
            "brotli should shrink repetitive data"
        );
        let back = brotli_decompress(&comp).unwrap();
        assert_eq!(back, data);
    }

    #[test]
    fn round_trip_empty() {
        let comp = brotli(b"").unwrap();
        let back = brotli_decompress(&comp).unwrap();
        assert!(back.is_empty());
    }
}
