pub const TF_RGB24: i64 = 3;

pub const TF_RGBA32_UNITY: i64 = 4;
pub const TF_RGBA32: i64 = 5;
pub const TF_DXT1: i64 = 10;
pub const TF_DXT5: i64 = 12;
pub const TF_BC7: i64 = 25;

pub const TF_BC5: i64 = 29;

pub const FM_POINT: i64 = 0;
pub const FM_BILINEAR: i64 = 1;
pub const FM_TRILINEAR: i64 = 2;

pub const WM_REPEAT: i64 = 0;
pub const WM_CLAMP: i64 = 1;

pub const GLTF_REPEAT: i64 = 10497;
pub const GLTF_CLAMP_TO_EDGE: i64 = 33071;
pub const GLTF_MIRRORED_REPEAT: i64 = 33648;

pub const fn sampler_wrap_mode(gltf_wrap: Option<i64>) -> i64 {
    match gltf_wrap {
        Some(GLTF_CLAMP_TO_EDGE) => WM_CLAMP,
        Some(GLTF_MIRRORED_REPEAT) => 2,
        _ => WM_REPEAT,
    }
}

pub const LINUX_MAX_TEXTURE_SIZE: u32 = 512;

pub const TEXTURE_IMPORTER_DEFAULT_MAX: u32 = 2048;

pub const LOAD_IMAGE_MAX_DIMENSION: u32 = 8192;

pub fn max_texture_size_for(target: &str) -> u32 {
    match (if target.is_empty() { "linux" } else { target })
        .to_lowercase()
        .as_str()
    {
        "linux" => 512,
        "windows" => 1024,
        "mac" | "osx" => 1024,
        "webgl" => 512,
        _ => 512,
    }
}

pub fn unity_load_image_would_succeed(src: &SourceImage) -> bool {
    let container_ok = matches!(src.container.as_str(), "PNG" | "JPEG");
    let within_dim =
        src.width <= LOAD_IMAGE_MAX_DIMENSION && src.height <= LOAD_IMAGE_MAX_DIMENSION;
    container_ok && within_dim
}

#[derive(Clone, Debug)]
pub struct Profile {
    pub target_w: u32,
    pub target_h: u32,
    pub texture_format: i64,
    pub mip_count: i32,
    pub is_alpha_channel_optional: bool,
    pub ignore_mipmap_limit: bool,
    pub filter_mode: i64,
    pub color_space: i64,
    pub lightmap_format: i64,
    pub compressed: bool,
}

#[derive(Clone, Debug)]
pub struct SourceImage {
    pub width: u32,
    pub height: u32,

    pub container: String,
    pub has_real_alpha: bool,
}

pub fn npot(x: u32) -> u32 {
    let x = x.max(1) as f64;
    let lo = 1u32 << (x.log2().floor() as u32);
    let hi = lo << 1;
    if (x - lo as f64) >= (hi as f64 - x) {
        hi
    } else {
        lo
    }
}

pub fn bc7_target_size(w: u32, h: u32, max_size: u32) -> (u32, u32) {
    let mut w = w.max(1);
    let mut h = h.max(1);
    if w > max_size || h > max_size {
        let factor = max_size as f64 / w.max(h) as f64;
        w = (w as f64 * factor) as u32;
        h = (h as f64 * factor) as u32;
    }
    (npot(w).max(1), npot(h).max(1))
}

pub fn default_mip_count(w: u32, h: u32) -> i32 {
    let m = w.max(h).max(1) as f64;
    (m.log2().floor() as i32) + 1
}

pub fn sampler_filter_mode(mag_filter: Option<i64>, min_filter: Option<i64>) -> i64 {
    if mag_filter == Some(9728) || min_filter == Some(9984) || min_filter == Some(9986) {
        return FM_POINT;
    }
    if min_filter == Some(9987) {
        return FM_TRILINEAR;
    }
    FM_BILINEAR
}

pub fn uncompressed_profile(src: &SourceImage, color_space: i64, filter_mode: i64) -> Profile {
    let fmt = if src.container == "PNG" {
        TF_RGBA32
    } else {
        TF_RGB24
    };
    Profile {
        target_w: src.width,
        target_h: src.height,
        texture_format: fmt,
        mip_count: 1,
        is_alpha_channel_optional: false,
        ignore_mipmap_limit: true,
        filter_mode,
        color_space,
        lightmap_format: 0,
        compressed: false,
    }
}

pub fn bc7_profile(src: &SourceImage, color_space: i64, is_normal: bool, max_size: u32) -> Profile {
    let (w, h) = bc7_target_size(src.width, src.height, max_size);

    if w < 4 || h < 4 {
        return Profile {
            target_w: w,
            target_h: h,
            texture_format: TF_RGBA32_UNITY,
            mip_count: default_mip_count(w, h),
            is_alpha_channel_optional: false,
            ignore_mipmap_limit: false,
            filter_mode: FM_BILINEAR,
            color_space,
            lightmap_format: if is_normal { 3 } else { 0 },
            compressed: false,
        };
    }
    Profile {
        target_w: w,
        target_h: h,
        texture_format: TF_BC7,
        mip_count: default_mip_count(w, h),
        is_alpha_channel_optional: false,
        ignore_mipmap_limit: false,
        filter_mode: FM_BILINEAR,
        color_space,
        lightmap_format: if is_normal { 3 } else { 0 },
        compressed: true,
    }
}

pub fn standalone_texture_profile_named(
    src: &SourceImage,
    max_size: u32,
    usage_normal: Option<bool>,
) -> Profile {
    let is_normal = usage_normal.unwrap_or(false);

    let (tw, th) = bc7_target_size(src.width, src.height, max_size);
    if tw < 4 || th < 4 {
        return Profile {
            target_w: tw,
            target_h: th,
            texture_format: TF_RGBA32_UNITY,
            mip_count: default_mip_count(tw, th),
            is_alpha_channel_optional: false,
            ignore_mipmap_limit: false,
            filter_mode: FM_BILINEAR,
            color_space: if is_normal { 0 } else { 1 },
            lightmap_format: if is_normal { 3 } else { 0 },
            compressed: false,
        };
    }

    let capped =
        (src.width > max_size || src.height > max_size) && unity_load_image_would_succeed(src);
    let alpha_opt = !is_normal && !capped && !src.has_real_alpha;
    Profile {
        target_w: tw,
        target_h: th,
        texture_format: TF_BC7,
        mip_count: default_mip_count(tw, th),
        is_alpha_channel_optional: alpha_opt,
        ignore_mipmap_limit: false,
        filter_mode: FM_BILINEAR,
        color_space: if is_normal { 0 } else { 1 },
        lightmap_format: if is_normal { 3 } else { 0 },
        compressed: true,
    }
}

pub fn texture_profile(
    src: &SourceImage,
    colorspace: i64,
    is_normal: bool,
    mag_filter: Option<i64>,
    min_filter: Option<i64>,
    max_size: u32,
) -> (Profile, Profile) {
    let fm = sampler_filter_mode(mag_filter, min_filter);
    let unc = uncompressed_profile(src, colorspace, fm);
    let bc7 = bc7_profile(src, colorspace, is_normal, max_size);
    (unc, bc7)
}

pub fn dxt1_profile(src: &SourceImage, max_size: u32) -> Profile {
    let (w, h) = bc7_target_size(src.width, src.height, max_size);
    if w < 4 || h < 4 {
        return Profile {
            target_w: w,
            target_h: h,
            texture_format: TF_RGBA32_UNITY,
            mip_count: default_mip_count(w, h),
            is_alpha_channel_optional: false,
            ignore_mipmap_limit: false,
            filter_mode: FM_BILINEAR,
            color_space: 1,
            lightmap_format: 0,
            compressed: false,
        };
    }
    Profile {
        target_w: w,
        target_h: h,
        texture_format: TF_DXT1,
        mip_count: default_mip_count(w, h),
        is_alpha_channel_optional: true,
        ignore_mipmap_limit: false,
        filter_mode: FM_BILINEAR,
        color_space: 1,
        lightmap_format: 0,
        compressed: true,
    }
}

pub fn bc5_normal_profile(src: &SourceImage, max_size: u32) -> Profile {
    let (w, h) = bc7_target_size(src.width, src.height, max_size);
    if w < 4 || h < 4 {
        return Profile {
            target_w: w,
            target_h: h,
            texture_format: TF_RGBA32_UNITY,
            mip_count: default_mip_count(w, h),
            is_alpha_channel_optional: false,
            ignore_mipmap_limit: false,
            filter_mode: FM_BILINEAR,
            color_space: 0,
            lightmap_format: 3,
            compressed: false,
        };
    }
    Profile {
        target_w: w,
        target_h: h,
        texture_format: TF_BC5,
        mip_count: default_mip_count(w, h),
        is_alpha_channel_optional: false,
        ignore_mipmap_limit: false,
        filter_mode: FM_BILINEAR,
        color_space: 0,
        lightmap_format: 3,
        compressed: true,
    }
}

pub fn texture_profile_bc5_normal(
    src: &SourceImage,
    colorspace: i64,
    mag_filter: Option<i64>,
    min_filter: Option<i64>,
    max_size: u32,
) -> (Profile, Profile) {
    let fm = sampler_filter_mode(mag_filter, min_filter);
    let unc = uncompressed_profile(src, colorspace, fm);
    let bc5 = bc5_normal_profile(src, max_size);
    (unc, bc5)
}

pub fn texture_profile_dxt1(
    src: &SourceImage,
    colorspace: i64,
    mag_filter: Option<i64>,
    min_filter: Option<i64>,
    max_size: u32,
) -> (Profile, Profile) {
    let fm = sampler_filter_mode(mag_filter, min_filter);
    let unc = uncompressed_profile(src, colorspace, fm);
    let dxt1 = dxt1_profile(src, max_size);
    (unc, dxt1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn self_test() {
        assert_eq!(npot(384), 512);
        assert_eq!(npot(400), 512);
        assert_eq!(npot(170), 128);
        assert_eq!(npot(210), 256);
        assert_eq!(npot(96), 128);
        assert_eq!(npot(94), 64);
        assert_eq!(npot(512), 512);
        assert_eq!(npot(515), 512);
        assert_eq!(npot(745), 512);
        assert_eq!(npot(1023), 1024);
        assert_eq!(npot(1024), 1024);
        assert_eq!(npot(1), 1);
        assert_eq!(npot(2), 2);
        assert_eq!(npot(3), 4);
        assert_eq!(bc7_target_size(1500, 500, 512), (512, 128));
        assert_eq!(bc7_target_size(2726, 236, 512), (512, 32));
        assert_eq!(bc7_target_size(800, 600, 512), (512, 512));
        assert_eq!(bc7_target_size(1067, 1067, 512), (512, 512));
        assert_eq!(bc7_target_size(2048, 2048, 512), (512, 512));
        assert_eq!(bc7_target_size(256, 256, 512), (256, 256));
        assert_eq!(default_mip_count(512, 512), 10);
        assert_eq!(default_mip_count(512, 256), 10);
        assert_eq!(sampler_filter_mode(Some(9728), None), FM_POINT);
        assert_eq!(sampler_filter_mode(Some(9729), Some(9987)), FM_TRILINEAR);
        assert_eq!(sampler_filter_mode(None, None), FM_BILINEAR);

        assert_eq!(sampler_filter_mode(Some(9729), Some(9986)), FM_POINT);
        assert_eq!(sampler_filter_mode(Some(9728), Some(9984)), FM_POINT);

        assert_eq!(sampler_wrap_mode(None), WM_REPEAT);
        assert_eq!(sampler_wrap_mode(Some(GLTF_REPEAT)), WM_REPEAT);
        assert_eq!(sampler_wrap_mode(Some(GLTF_CLAMP_TO_EDGE)), WM_CLAMP);
        assert_eq!(sampler_wrap_mode(Some(GLTF_MIRRORED_REPEAT)), 2);
    }

    #[test]
    fn standalone_sub_block_falls_back_to_uncompressed_rgba32() {
        let src = SourceImage {
            width: 1,
            height: 1,
            container: "PNG".into(),
            has_real_alpha: true,
        };
        let p = standalone_texture_profile_named(&src, LINUX_MAX_TEXTURE_SIZE, None);
        assert_eq!(p.texture_format, TF_RGBA32_UNITY);
        assert_eq!(p.mip_count, 1);
        assert!(!p.compressed);
        assert_eq!(p.target_w, 1);
        assert_eq!(p.target_h, 1);

        let src = SourceImage {
            width: 3,
            height: 3,
            container: "PNG".into(),
            has_real_alpha: false,
        };
        let p = standalone_texture_profile_named(&src, LINUX_MAX_TEXTURE_SIZE, None);
        assert_eq!(p.texture_format, TF_BC7);
        assert!(p.compressed);
        assert_eq!((p.target_w, p.target_h), (4, 4));

        let src = SourceImage {
            width: 4,
            height: 4,
            container: "PNG".into(),
            has_real_alpha: false,
        };
        let p = standalone_texture_profile_named(&src, LINUX_MAX_TEXTURE_SIZE, None);
        assert_eq!(p.texture_format, TF_BC7);
        assert!(p.compressed);

        let src = SourceImage {
            width: 8,
            height: 2,
            container: "PNG".into(),
            has_real_alpha: false,
        };
        let p = standalone_texture_profile_named(&src, LINUX_MAX_TEXTURE_SIZE, None);
        assert_eq!(p.texture_format, TF_RGBA32_UNITY);

        let src = SourceImage {
            width: 384,
            height: 384,
            container: "PNG".into(),
            has_real_alpha: true,
        };
        let p = standalone_texture_profile_named(&src, max_texture_size_for("windows"), None);
        assert_eq!((p.target_w, p.target_h), (512, 512));
        assert_eq!(p.texture_format, TF_BC7);

        let src = SourceImage {
            width: 786,
            height: 1080,
            container: "PNG".into(),
            has_real_alpha: true,
        };
        let p = standalone_texture_profile_named(&src, max_texture_size_for("windows"), None);
        assert_eq!((p.target_w, p.target_h), (512, 1024));
    }

    #[test]
    fn bc7_sub_block_falls_back_to_uncompressed_rgba32() {
        let src = SourceImage {
            width: 1,
            height: 1,
            container: "PNG".into(),
            has_real_alpha: true,
        };
        let p = bc7_profile(&src, 1, false, 1024);
        assert_eq!(p.texture_format, TF_RGBA32_UNITY);
        assert_eq!(p.mip_count, 1);
        assert!(!p.compressed);
        assert!(!p.is_alpha_channel_optional);
        assert_eq!((p.target_w, p.target_h), (1, 1));

        let src = SourceImage {
            width: 256,
            height: 1,
            container: "PNG".into(),
            has_real_alpha: false,
        };
        let p = bc7_profile(&src, 1, false, 1024);
        assert_eq!(p.texture_format, TF_RGBA32_UNITY);
        assert_eq!(p.target_h, 1);
        assert!(!p.compressed);

        let src = SourceImage {
            width: 3,
            height: 3,
            container: "PNG".into(),
            has_real_alpha: false,
        };
        let p = bc7_profile(&src, 1, false, 1024);
        assert_eq!(p.texture_format, TF_BC7);
        assert_eq!((p.target_w, p.target_h), (4, 4));

        let src = SourceImage {
            width: 4,
            height: 4,
            container: "PNG".into(),
            has_real_alpha: false,
        };
        let p = bc7_profile(&src, 1, false, 1024);
        assert_eq!(p.texture_format, TF_BC7);
        assert!(p.compressed);

        let src = SourceImage {
            width: 2,
            height: 2,
            container: "PNG".into(),
            has_real_alpha: false,
        };
        let p = bc7_profile(&src, 0, true, 1024);
        assert_eq!(p.texture_format, TF_RGBA32_UNITY);
        assert_eq!(p.lightmap_format, 3);
    }
}
