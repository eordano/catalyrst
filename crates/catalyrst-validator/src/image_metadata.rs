const PNG_SIGNATURE: [u8; 8] = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a];
const PNG_IHDR: &[u8; 4] = b"IHDR";
const PNG_IEND: &[u8; 4] = b"IEND";
const PNG_IHDR_CHUNK_LENGTH: u32 = 13;
const PNG_FIRST_CHUNK_END: usize = 33;
const PNG_CHUNK_OVERHEAD: usize = 12;
const PNG_MAX_CHUNK_LENGTH: u32 = 0x7fff_ffff;

pub const MAX_THUMBNAIL_DIMENSION_IN_PX: i64 = 1024;
pub const DEFAULT_FACE_THUMBNAIL_SIZE: i64 = 256;

const INVALID_FORMAT_ERROR: &str =
    "Invalid or unknown image format. Only 'PNG' format is accepted.";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageFormat {
    Png,
    Jpeg,
    Webp,
    Gif,
    Bmp,
}

impl ImageFormat {
    fn display_name(self) -> &'static str {
        match self {
            ImageFormat::Png => "PNG",
            ImageFormat::Jpeg => "JPEG",
            ImageFormat::Webp => "WebP",
            ImageFormat::Gif => "GIF",
            ImageFormat::Bmp => "BMP",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImageMetadata {
    pub format: ImageFormat,
    pub width: i64,
    pub height: i64,
}

pub fn read_image_metadata(input: &[u8]) -> Result<ImageMetadata, String> {
    let metadata = if is_png(input) {
        read_png(input)?
    } else if is_jpeg(input) {
        read_jpeg(input)?
    } else if is_webp(input) {
        read_webp(input)?
    } else if is_gif(input) {
        read_gif(input)?
    } else if is_bmp(input) {
        read_bmp(input)?
    } else {
        return Err("Unsupported image format".to_string());
    };
    assert_positive_dimensions(&metadata)?;
    Ok(metadata)
}

fn assert_positive_dimensions(metadata: &ImageMetadata) -> Result<(), String> {
    let name = metadata.format.display_name();
    if metadata.width <= 0 {
        return Err(format!(
            "Malformed {name}: non-positive width {}",
            metadata.width
        ));
    }
    if metadata.height <= 0 {
        return Err(format!(
            "Malformed {name}: non-positive height {}",
            metadata.height
        ));
    }
    Ok(())
}

fn u16_be(b: &[u8], o: usize) -> u16 {
    u16::from_be_bytes([b[o], b[o + 1]])
}

fn u16_le(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([b[o], b[o + 1]])
}

fn u32_be(b: &[u8], o: usize) -> u32 {
    u32::from_be_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}

fn u32_le(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}

fn i32_le(b: &[u8], o: usize) -> i32 {
    i32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}

fn u24_le(b: &[u8], o: usize) -> u32 {
    (b[o] as u32) | ((b[o + 1] as u32) << 8) | ((b[o + 2] as u32) << 16)
}

fn is_png(buffer: &[u8]) -> bool {
    buffer.len() >= PNG_FIRST_CHUNK_END && buffer[0..8] == PNG_SIGNATURE
}

fn read_png(buffer: &[u8]) -> Result<ImageMetadata, String> {
    if u32_be(buffer, 8) != PNG_IHDR_CHUNK_LENGTH {
        return Err("Malformed PNG: IHDR chunk length is not 13".to_string());
    }
    if &buffer[12..16] != PNG_IHDR {
        return Err("Malformed PNG: missing IHDR chunk".to_string());
    }
    let width = u32_be(buffer, 16) as i64;
    let height = u32_be(buffer, 20) as i64;
    validate_png_bit_depth_and_color_type(buffer[24], buffer[25])?;
    validate_png_ihdr_methods(buffer[26], buffer[27], buffer[28])?;
    validate_png_chunk_chain(buffer)?;
    Ok(ImageMetadata {
        format: ImageFormat::Png,
        width,
        height,
    })
}

fn png_allowed_bit_depths(color_type: u8) -> Option<&'static [u8]> {
    match color_type {
        0 => Some(&[1, 2, 4, 8, 16]),
        2 => Some(&[8, 16]),
        3 => Some(&[1, 2, 4, 8]),
        4 => Some(&[8, 16]),
        6 => Some(&[8, 16]),
        _ => None,
    }
}

fn validate_png_bit_depth_and_color_type(bit_depth: u8, color_type: u8) -> Result<(), String> {
    match png_allowed_bit_depths(color_type) {
        None => Err(format!("Malformed PNG: invalid color type {color_type}")),
        Some(allowed) if !allowed.contains(&bit_depth) => Err(format!(
            "Malformed PNG: invalid bit depth {bit_depth} for color type {color_type}"
        )),
        Some(_) => Ok(()),
    }
}

fn validate_png_ihdr_methods(compression: u8, filter: u8, interlace: u8) -> Result<(), String> {
    if compression != 0 {
        return Err(format!(
            "Malformed PNG: invalid compression method {compression}"
        ));
    }
    if filter != 0 {
        return Err(format!("Malformed PNG: invalid filter method {filter}"));
    }
    if interlace != 0 && interlace != 1 {
        return Err(format!(
            "Malformed PNG: invalid interlace method {interlace}"
        ));
    }
    Ok(())
}

fn validate_png_chunk_chain(buffer: &[u8]) -> Result<(), String> {
    let mut i = PNG_FIRST_CHUNK_END;
    while i + PNG_CHUNK_OVERHEAD <= buffer.len() {
        let chunk_data_length = u32_be(buffer, i);
        if chunk_data_length > PNG_MAX_CHUNK_LENGTH {
            return Err("Malformed PNG: chunk length exceeds 2^31-1".to_string());
        }
        let chunk_type = &buffer[i + 4..i + 8];
        if chunk_type == PNG_IHDR {
            return Err("Malformed PNG: duplicate IHDR chunk".to_string());
        }
        if chunk_type == PNG_IEND {
            if chunk_data_length != 0 {
                return Err("Malformed PNG: IEND chunk must have zero length".to_string());
            }
            if i + PNG_CHUNK_OVERHEAD != buffer.len() {
                return Err("Malformed PNG: data after IEND chunk".to_string());
            }
            return Ok(());
        }
        i += PNG_CHUNK_OVERHEAD + chunk_data_length as usize;
    }
    Err("Malformed PNG: missing IEND chunk".to_string())
}

fn is_jpeg(buffer: &[u8]) -> bool {
    buffer.len() >= 4 && buffer[0] == 0xff && buffer[1] == 0xd8 && buffer[2] == 0xff
}

fn read_jpeg(buffer: &[u8]) -> Result<ImageMetadata, String> {
    if buffer.len() < 4 || buffer[buffer.len() - 2] != 0xff || buffer[buffer.len() - 1] != 0xd9 {
        return Err("Malformed JPEG: missing EOI marker".to_string());
    }
    let mut i = 2usize;
    while i + 8 < buffer.len() {
        if buffer[i] != 0xff {
            i += 1;
            continue;
        }
        let marker = buffer[i + 1];
        if marker == 0x01 || (0xd0..=0xd9).contains(&marker) {
            i += 2;
            continue;
        }
        if marker == 0xda {
            break;
        }
        let is_start_of_frame =
            (0xc0..=0xcf).contains(&marker) && marker != 0xc4 && marker != 0xc8 && marker != 0xcc;
        if is_start_of_frame {
            return Ok(ImageMetadata {
                format: ImageFormat::Jpeg,
                height: u16_be(buffer, i + 5) as i64,
                width: u16_be(buffer, i + 7) as i64,
            });
        }
        let segment_length = u16_be(buffer, i + 2) as usize;
        if segment_length < 2 {
            break;
        }
        i += 2 + segment_length;
    }
    Err("Malformed JPEG: no SOFn marker found".to_string())
}

fn is_webp(buffer: &[u8]) -> bool {
    buffer.len() >= 16 && &buffer[0..4] == b"RIFF" && &buffer[8..12] == b"WEBP"
}

fn read_webp(buffer: &[u8]) -> Result<ImageMetadata, String> {
    if u32_le(buffer, 4) as usize != buffer.len() - 8 {
        return Err("Malformed WebP: RIFF chunk size does not match buffer length".to_string());
    }
    let variant = &buffer[12..16];
    if variant == b"VP8 " {
        if buffer.len() < 30 {
            return Err("Malformed WebP: VP8 chunk truncated".to_string());
        }
        assert_webp_simple_sub_chunk_size(buffer, "VP8")?;
        if buffer[23] != 0x9d || buffer[24] != 0x01 || buffer[25] != 0x2a {
            return Err("Malformed WebP: invalid VP8 keyframe sync code".to_string());
        }
        return Ok(ImageMetadata {
            format: ImageFormat::Webp,
            width: (u16_le(buffer, 26) & 0x3fff) as i64,
            height: (u16_le(buffer, 28) & 0x3fff) as i64,
        });
    }
    if variant == b"VP8L" {
        if buffer.len() < 25 {
            return Err("Malformed WebP: VP8L chunk truncated".to_string());
        }
        assert_webp_simple_sub_chunk_size(buffer, "VP8L")?;
        if buffer[20] != 0x2f {
            return Err("Malformed WebP: invalid VP8L signature byte".to_string());
        }
        let b0 = buffer[21] as u32;
        let b1 = buffer[22] as u32;
        let b2 = buffer[23] as u32;
        let b3 = buffer[24] as u32;
        return Ok(ImageMetadata {
            format: ImageFormat::Webp,
            width: (1 + ((b0 | (b1 << 8)) & 0x3fff)) as i64,
            height: (1 + (((b1 >> 6) | (b2 << 2) | (b3 << 10)) & 0x3fff)) as i64,
        });
    }
    if variant == b"VP8X" {
        if buffer.len() < 30 {
            return Err("Malformed WebP: VP8X chunk truncated".to_string());
        }
        if u32_le(buffer, 16) != 10 {
            return Err("Malformed WebP: VP8X chunk size must be 10".to_string());
        }
        return Ok(ImageMetadata {
            format: ImageFormat::Webp,
            width: 1 + u24_le(buffer, 24) as i64,
            height: 1 + u24_le(buffer, 27) as i64,
        });
    }
    Err("Malformed WebP: unknown variant".to_string())
}

fn assert_webp_simple_sub_chunk_size(buffer: &[u8], label: &str) -> Result<(), String> {
    let declared = u32_le(buffer, 16) as usize;
    let expected_payload = buffer.len() - 20;
    let expected_payload_without_pad = buffer.len() - 21;
    if declared != expected_payload && declared != expected_payload_without_pad {
        return Err(format!(
            "Malformed WebP: {label} chunk size does not match buffer length"
        ));
    }
    Ok(())
}

fn is_gif(buffer: &[u8]) -> bool {
    buffer.len() >= 14 && (&buffer[0..6] == b"GIF87a" || &buffer[0..6] == b"GIF89a")
}

fn read_gif(buffer: &[u8]) -> Result<ImageMetadata, String> {
    if buffer[buffer.len() - 1] != 0x3b {
        return Err("Malformed GIF: missing trailer byte".to_string());
    }
    Ok(ImageMetadata {
        format: ImageFormat::Gif,
        width: u16_le(buffer, 6) as i64,
        height: u16_le(buffer, 8) as i64,
    })
}

const BMP_BITMAPCOREHEADER_SIZE: u32 = 12;

fn is_bmp(buffer: &[u8]) -> bool {
    buffer.len() >= 22 && buffer[0] == 0x42 && buffer[1] == 0x4d
}

fn read_bmp(buffer: &[u8]) -> Result<ImageMetadata, String> {
    if u32_le(buffer, 2) as usize != buffer.len() {
        return Err("Malformed BMP: file size header does not match buffer length".to_string());
    }
    let dib_header_size = u32_le(buffer, 14);
    if dib_header_size == BMP_BITMAPCOREHEADER_SIZE {
        return Ok(ImageMetadata {
            format: ImageFormat::Bmp,
            width: u16_le(buffer, 18) as i64,
            height: u16_le(buffer, 20) as i64,
        });
    }
    if buffer.len() < 26 {
        return Err("Malformed BMP: BITMAPINFOHEADER truncated".to_string());
    }
    Ok(ImageMetadata {
        format: ImageFormat::Bmp,
        width: i32_le(buffer, 18) as i64,
        height: (i32_le(buffer, 22) as i64).abs(),
    })
}

pub fn check_wearable_thumbnail_image(buffer: &[u8]) -> Vec<String> {
    match read_image_metadata(buffer) {
        Ok(md) => {
            if md.format != ImageFormat::Png {
                vec![INVALID_FORMAT_ERROR.to_string()]
            } else if md.width > MAX_THUMBNAIL_DIMENSION_IN_PX
                || md.height > MAX_THUMBNAIL_DIMENSION_IN_PX
            {
                vec![format!(
                    "Invalid thumbnail image size (width = {} / height = {})",
                    md.width, md.height
                )]
            } else {
                Vec::new()
            }
        }
        Err(_) => vec!["Couldn't parse thumbnail, please check image format.".to_string()],
    }
}

pub fn check_face256_thumbnail_image(buffer: &[u8]) -> Vec<String> {
    match read_image_metadata(buffer) {
        Ok(md) => {
            if md.format != ImageFormat::Png {
                vec![INVALID_FORMAT_ERROR.to_string()]
            } else if md.width != DEFAULT_FACE_THUMBNAIL_SIZE
                || md.height != DEFAULT_FACE_THUMBNAIL_SIZE
            {
                vec![format!(
                    "Invalid face256 thumbnail image size (width = {} / height = {})",
                    md.width, md.height
                )]
            } else {
                Vec::new()
            }
        }
        Err(_) => vec!["Couldn't parse face256 thumbnail, please check image format.".to_string()],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_png(width: u32, height: u32, bit_depth: u8, color_type: u8) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(&PNG_SIGNATURE);
        buf.extend_from_slice(&PNG_IHDR_CHUNK_LENGTH.to_be_bytes());
        buf.extend_from_slice(PNG_IHDR);
        buf.extend_from_slice(&width.to_be_bytes());
        buf.extend_from_slice(&height.to_be_bytes());
        buf.push(bit_depth);
        buf.push(color_type);
        buf.push(0);
        buf.push(0);
        buf.push(0);
        buf.extend_from_slice(&[0, 0, 0, 0]);
        buf.extend_from_slice(&0u32.to_be_bytes());
        buf.extend_from_slice(PNG_IEND);
        buf.extend_from_slice(&[0, 0, 0, 0]);
        buf
    }

    fn rgba_png(width: u32, height: u32) -> Vec<u8> {
        make_png(width, height, 8, 6)
    }

    fn minimal_jpeg() -> Vec<u8> {
        vec![0xff, 0xd8, 0xff, 0xd9]
    }

    fn jpeg_with_sof(width: u16, height: u16) -> Vec<u8> {
        let mut buf = vec![0xff, 0xd8];
        buf.push(0xff);
        buf.push(0xc0);
        buf.extend_from_slice(&17u16.to_be_bytes());
        buf.push(8);
        buf.extend_from_slice(&height.to_be_bytes());
        buf.extend_from_slice(&width.to_be_bytes());
        buf.push(3);
        buf.extend_from_slice(&[1, 0x11, 0, 2, 0x11, 0, 3, 0x11, 0]);
        buf.push(0xff);
        buf.push(0xd9);
        buf
    }

    #[test]
    fn reads_png_dimensions() {
        let md = read_image_metadata(&rgba_png(300, 200)).expect("valid png");
        assert_eq!(md.format, ImageFormat::Png);
        assert_eq!((md.width, md.height), (300, 200));
    }

    #[test]
    fn wearable_thumbnail_valid_png_within_limit_passes() {
        assert!(check_wearable_thumbnail_image(&rgba_png(1024, 1024)).is_empty());
        assert!(check_wearable_thumbnail_image(&rgba_png(512, 256)).is_empty());
        assert!(check_wearable_thumbnail_image(&rgba_png(1, 1)).is_empty());
    }

    #[test]
    fn wearable_thumbnail_over_limit_reports_size_error() {
        let errors = check_wearable_thumbnail_image(&rgba_png(2048, 512));
        assert_eq!(
            errors,
            vec!["Invalid thumbnail image size (width = 2048 / height = 512)".to_string()]
        );
        let errors = check_wearable_thumbnail_image(&rgba_png(512, 2048));
        assert_eq!(
            errors,
            vec!["Invalid thumbnail image size (width = 512 / height = 2048)".to_string()]
        );
    }

    #[test]
    fn wearable_thumbnail_non_png_reports_format_error() {
        let errors = check_wearable_thumbnail_image(&jpeg_with_sof(100, 100));
        assert_eq!(errors, vec![INVALID_FORMAT_ERROR.to_string()]);
    }

    #[test]
    fn wearable_thumbnail_unparseable_reports_parse_error() {
        let errors = check_wearable_thumbnail_image(&[0, 1, 2, 3, 4, 5, 6, 7]);
        assert_eq!(
            errors,
            vec!["Couldn't parse thumbnail, please check image format.".to_string()]
        );
        let errors = check_wearable_thumbnail_image(&minimal_jpeg());
        assert_eq!(
            errors,
            vec!["Couldn't parse thumbnail, please check image format.".to_string()]
        );
    }

    #[test]
    fn face256_exact_size_passes() {
        assert!(check_face256_thumbnail_image(&rgba_png(256, 256)).is_empty());
    }

    #[test]
    fn face256_wrong_size_reports_error() {
        let errors = check_face256_thumbnail_image(&rgba_png(1024, 1024));
        assert_eq!(
            errors,
            vec!["Invalid face256 thumbnail image size (width = 1024 / height = 1024)".to_string()]
        );
        let errors = check_face256_thumbnail_image(&rgba_png(128, 256));
        assert_eq!(
            errors,
            vec!["Invalid face256 thumbnail image size (width = 128 / height = 256)".to_string()]
        );
    }

    #[test]
    fn face256_non_png_reports_format_error() {
        let errors = check_face256_thumbnail_image(&jpeg_with_sof(256, 256));
        assert_eq!(errors, vec![INVALID_FORMAT_ERROR.to_string()]);
    }

    #[test]
    fn face256_unparseable_reports_parse_error() {
        let errors = check_face256_thumbnail_image(&[9, 9, 9, 9]);
        assert_eq!(
            errors,
            vec!["Couldn't parse face256 thumbnail, please check image format.".to_string()]
        );
    }

    #[test]
    fn png_invalid_color_type_is_error() {
        let bytes = make_png(64, 64, 8, 5);
        assert!(read_image_metadata(&bytes).is_err());
    }

    #[test]
    fn png_invalid_bit_depth_for_color_type_is_error() {
        let bytes = make_png(64, 64, 1, 2);
        assert!(read_image_metadata(&bytes).is_err());
    }

    #[test]
    fn png_zero_dimension_is_error() {
        let bytes = rgba_png(0, 64);
        assert!(read_image_metadata(&bytes).is_err());
    }

    #[test]
    fn png_missing_iend_is_error() {
        let mut bytes = rgba_png(64, 64);
        bytes.truncate(bytes.len() - 12);
        assert!(read_image_metadata(&bytes).is_err());
    }

    #[test]
    fn png_trailing_data_after_iend_is_error() {
        let mut bytes = rgba_png(64, 64);
        bytes.push(0x00);
        assert!(read_image_metadata(&bytes).is_err());
    }

    #[test]
    fn jpeg_with_sof_is_recognised_with_dimensions() {
        let md = read_image_metadata(&jpeg_with_sof(640, 480)).expect("valid jpeg");
        assert_eq!(md.format, ImageFormat::Jpeg);
        assert_eq!((md.width, md.height), (640, 480));
    }

    #[test]
    fn empty_and_short_buffers_are_unsupported() {
        assert!(read_image_metadata(&[]).is_err());
        assert!(read_image_metadata(&[0x89]).is_err());
    }
}
