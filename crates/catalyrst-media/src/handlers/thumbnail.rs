use axum::body::Bytes;
use image::{codecs::jpeg::JpegEncoder, ImageFormat, ImageReader, Limits};
use std::io::Cursor;

static RESIZERS: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(2);

pub fn valid_width(width: u32) -> bool {
    matches!(width, 320 | 640 | 960)
}

fn resize(bytes: Bytes, width: u32) -> Result<(&'static str, Bytes), String> {
    let mut reader = ImageReader::new(Cursor::new(&bytes))
        .with_guessed_format()
        .map_err(|_| "Unrecognized image")?;
    let mut limits = Limits::default();
    limits.max_image_width = Some(8192);
    limits.max_image_height = Some(8192);
    limits.max_alloc = Some(128 * 1024 * 1024);
    reader.limits(limits);
    let image = reader
        .decode()
        .map_err(|_| "Image could not be decoded within limits")?;
    let image = image.thumbnail(width, width);
    let mut output = Vec::new();
    let transparent =
        image.color().has_alpha() && image.to_rgba8().pixels().any(|pixel| pixel.0[3] < 255);
    let kind = if transparent {
        image
            .write_to(&mut Cursor::new(&mut output), ImageFormat::Png)
            .map_err(|_| "Thumbnail encoding failed")?;
        "image/png"
    } else {
        JpegEncoder::new_with_quality(&mut output, 80)
            .encode_image(&image.to_rgb8())
            .map_err(|_| "Thumbnail encoding failed")?;
        "image/jpeg"
    };
    Ok((kind, Bytes::from(output)))
}

pub async fn render(bytes: Bytes, width: u32) -> Result<(&'static str, Bytes), String> {
    let permit = RESIZERS
        .acquire()
        .await
        .map_err(|_| "Thumbnail service unavailable")?;
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        resize(bytes, width)
    })
    .await
    .map_err(|_| "Thumbnail task failed")?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thumbnail_preserves_transparency_and_bounds_dimensions() {
        let image = image::RgbaImage::from_pixel(1600, 900, image::Rgba([32, 64, 96, 128]));
        let mut bytes = Cursor::new(Vec::new());
        image.write_to(&mut bytes, ImageFormat::Png).unwrap();
        let (kind, output) = resize(Bytes::from(bytes.into_inner()), 320).unwrap();
        let decoded = image::load_from_memory(&output).unwrap();
        assert_eq!(kind, "image/png");
        assert_eq!((decoded.width(), decoded.height()), (320, 180));
        assert_eq!(decoded.to_rgba8().get_pixel(0, 0).0[3], 128);
    }

    #[test]
    fn invalid_images_and_unbounded_sizes_are_rejected() {
        assert!(resize(Bytes::from_static(b"<svg></svg>"), 320).is_err());
        assert!(valid_width(640));
        assert!(!valid_width(0));
        assert!(!valid_width(8192));
    }

    #[test]
    fn opaque_rgba_and_webp_sources_get_compact_jpeg_thumbnails() {
        for format in [ImageFormat::Png, ImageFormat::WebP] {
            let image = image::RgbaImage::from_pixel(1600, 900, image::Rgba([32, 64, 96, 255]));
            let mut bytes = Cursor::new(Vec::new());
            image.write_to(&mut bytes, format).unwrap();
            let (kind, output) = resize(Bytes::from(bytes.into_inner()), 320).unwrap();
            let decoded = image::load_from_memory(&output).unwrap();
            assert_eq!(kind, "image/jpeg");
            assert_eq!((decoded.width(), decoded.height()), (320, 180));
            assert!(output.len() < 10_000);
        }
    }
}
