use image::RgbaImage;

pub fn encode_rgba32(image: &RgbaImage, flip: bool) -> Vec<u8> {
    let (w, h) = image.dimensions();
    let src = image.as_raw();
    let mut out = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        let row = if flip { h - 1 - y } else { y };
        for x in 0..w {
            let i = ((row * w + x) * 4) as usize;
            out.push(src[i + 3]);
            out.push(src[i]);
            out.push(src[i + 1]);
            out.push(src[i + 2]);
        }
    }
    out
}

pub fn encode_rgb24(image: &RgbaImage, flip: bool) -> Vec<u8> {
    let (w, h) = image.dimensions();
    let src = image.as_raw();
    let mut out = Vec::with_capacity((w * h * 3) as usize);
    for y in 0..h {
        let row = if flip { h - 1 - y } else { y };
        for x in 0..w {
            let i = ((row * w + x) * 4) as usize;
            out.push(src[i]);
            out.push(src[i + 1]);
            out.push(src[i + 2]);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn one_pixel(r: u8, g: u8, b: u8, a: u8) -> RgbaImage {
        RgbaImage::from_raw(1, 1, vec![r, g, b, a]).unwrap()
    }

    #[test]
    fn rgba32_is_argb_byte_order() {
        let img = one_pixel(0x10, 0x20, 0x30, 0x40);
        let argb = encode_rgba32(&img, false);
        assert_eq!(argb, vec![0x40, 0x10, 0x20, 0x30], "ARGB byte order");
    }
}
