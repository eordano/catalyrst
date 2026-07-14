//! Magic-byte validation for community thumbnails.
//!
//! Port of upstream `social-service-ea`'s `src/logic/community/image-signature.ts` (#444). The
//! previous behaviour accepted any binary blob under the content-store size cap as a community
//! thumbnail, stored it content-addressed, and served it back with a hardcoded
//! `Content-Type: image/png` -- a stored-arbitrary-file / content-type-confusion hazard. We now
//! reject anything whose leading bytes are not a recognised PNG/JPEG/GIF/WebP signature, bound
//! the size, and serve the detected media type rather than a fixed one.
//!
//! Only fixed signature bytes are inspected -- no variable-depth container parsing.

/// Smallest accepted thumbnail. Bytes below this are almost certainly not a real image and are
/// rejected before any storage or database work happens (upstream: 1KB floor).
pub const MIN_THUMBNAIL_BYTES: usize = 1024;

/// Largest accepted thumbnail. Aligned with the content store's own body cap
/// ([`crate::rest::content_store::MAX_BODY_BYTES`]) so validation and storage agree; upstream
/// caps at 500KB but our store physically refuses anything larger than [`MAX_THUMBNAIL_BYTES`].
pub const MAX_THUMBNAIL_BYTES: usize = crate::rest::content_store::MAX_BODY_BYTES;

/// A recognised image media type, as it is both stored-as and served-as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageMimeType {
    Png,
    Jpeg,
    Gif,
    Webp,
}

impl ImageMimeType {
    pub fn as_str(self) -> &'static str {
        match self {
            ImageMimeType::Png => "image/png",
            ImageMimeType::Jpeg => "image/jpeg",
            ImageMimeType::Gif => "image/gif",
            ImageMimeType::Webp => "image/webp",
        }
    }
}

/// Why a thumbnail was rejected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThumbnailError {
    /// Fewer than [`MIN_THUMBNAIL_BYTES`] or more than [`MAX_THUMBNAIL_BYTES`].
    Size,
    /// Leading bytes match none of the accepted signatures.
    UnsupportedSignature,
}

impl ThumbnailError {
    pub fn message(self) -> &'static str {
        match self {
            ThumbnailError::Size => "Thumbnail size must be between 1KB and 256KB",
            ThumbnailError::UnsupportedSignature => {
                "Thumbnail must start with a supported PNG, JPEG, GIF or WebP signature"
            }
        }
    }
}

const PNG: [u8; 8] = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a];
const JPEG: [u8; 3] = [0xff, 0xd8, 0xff];
const GIF87A: [u8; 6] = [0x47, 0x49, 0x46, 0x38, 0x37, 0x61];
const GIF89A: [u8; 6] = [0x47, 0x49, 0x46, 0x38, 0x39, 0x61];

/// Reads the media type a buffer's leading bytes announce, or `None` when they match no
/// supported signature. The answer travels with the bytes, so the thumbnail is stored and
/// served as what it actually is.
pub fn detect_image_mime_type(buffer: &[u8]) -> Option<ImageMimeType> {
    if buffer.starts_with(&PNG) {
        return Some(ImageMimeType::Png);
    }
    if buffer.starts_with(&JPEG) {
        return Some(ImageMimeType::Jpeg);
    }
    if buffer.starts_with(&GIF87A) || buffer.starts_with(&GIF89A) {
        return Some(ImageMimeType::Gif);
    }
    // RIFF containers name their format at offset 8, after the 4-byte size field.
    if buffer.len() >= 12 && &buffer[0..4] == b"RIFF" && &buffer[8..12] == b"WEBP" {
        return Some(ImageMimeType::Webp);
    }
    None
}

/// Validates an uploaded thumbnail: size bounds first, then signature. On success returns the
/// detected media type. Callers MUST run this before any authorization or database write, so an
/// unrecognised blob never reaches the content store.
pub fn validate_thumbnail(buffer: &[u8]) -> Result<ImageMimeType, ThumbnailError> {
    if buffer.len() < MIN_THUMBNAIL_BYTES || buffer.len() > MAX_THUMBNAIL_BYTES {
        return Err(ThumbnailError::Size);
    }
    detect_image_mime_type(buffer).ok_or(ThumbnailError::UnsupportedSignature)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn padded(prefix: &[u8], len: usize) -> Vec<u8> {
        let mut v = prefix.to_vec();
        v.resize(len.max(prefix.len()), 0x00);
        v
    }

    #[test]
    fn detects_each_supported_signature() {
        assert_eq!(detect_image_mime_type(&PNG), Some(ImageMimeType::Png));
        assert_eq!(detect_image_mime_type(&JPEG), Some(ImageMimeType::Jpeg));
        assert_eq!(detect_image_mime_type(&GIF87A), Some(ImageMimeType::Gif));
        assert_eq!(detect_image_mime_type(&GIF89A), Some(ImageMimeType::Gif));
        let mut webp = b"RIFF".to_vec();
        webp.extend_from_slice(&[0, 0, 0, 0]); // size field
        webp.extend_from_slice(b"WEBP");
        assert_eq!(detect_image_mime_type(&webp), Some(ImageMimeType::Webp));
    }

    #[test]
    fn rejects_non_image_bytes() {
        assert_eq!(detect_image_mime_type(b"<html></html>"), None);
        assert_eq!(detect_image_mime_type(b"#!/bin/sh\n"), None);
        assert_eq!(detect_image_mime_type(&[]), None);
        // RIFF container that is not a WEBP form type.
        let mut wav = b"RIFF".to_vec();
        wav.extend_from_slice(&[0, 0, 0, 0]);
        wav.extend_from_slice(b"WAVE");
        assert_eq!(detect_image_mime_type(&wav), None);
    }

    #[test]
    fn validate_enforces_size_floor() {
        // A valid PNG signature but under the 1KB floor is rejected on size.
        assert_eq!(validate_thumbnail(&PNG), Err(ThumbnailError::Size));
    }

    #[test]
    fn validate_rejects_arbitrary_blob_at_valid_size() {
        let blob = padded(b"not-an-image", MIN_THUMBNAIL_BYTES);
        assert_eq!(
            validate_thumbnail(&blob),
            Err(ThumbnailError::UnsupportedSignature)
        );
    }

    #[test]
    fn validate_accepts_a_well_formed_png() {
        let png = padded(&PNG, MIN_THUMBNAIL_BYTES);
        assert_eq!(validate_thumbnail(&png), Ok(ImageMimeType::Png));
    }

    #[test]
    fn validate_rejects_oversize() {
        let png = padded(&PNG, MAX_THUMBNAIL_BYTES + 1);
        assert_eq!(validate_thumbnail(&png), Err(ThumbnailError::Size));
    }
}
