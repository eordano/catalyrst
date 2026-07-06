use super::*;

fn is_psd(raw: &[u8]) -> bool {
    raw.len() >= 4 && &raw[0..4] == b"8BPS"
}

fn png_gamma_is_nontrivial(gama_100k: u32) -> bool {
    let gamma = gama_100k as f64 / 100_000.0;
    if gamma <= 0.0 {
        return false;
    }
    let exp = 1.0 / (gamma * 2.2);

    let mid = crate::detmath::pow(128.0f64 / 255.0, exp) * 255.0;
    (mid - 128.0).abs() >= 0.5
}

pub(super) fn png_gamma_to_apply(raw: &[u8]) -> Option<u32> {
    if raw.len() < 8 || &raw[0..8] != b"\x89PNG\r\n\x1a\n" {
        return None;
    }
    let mut pos = 8usize;
    let mut gama: Option<u32> = None;
    let mut has_srgb = false;
    while pos + 8 <= raw.len() {
        let len = u32::from_be_bytes([raw[pos], raw[pos + 1], raw[pos + 2], raw[pos + 3]]) as usize;
        let typ = &raw[pos + 4..pos + 8];
        let dstart = pos + 8;
        let dend = dstart + len;
        if dend + 4 > raw.len() {
            break;
        }
        match typ {
            b"gAMA" if len >= 4 => {
                gama = Some(u32::from_be_bytes([
                    raw[dstart],
                    raw[dstart + 1],
                    raw[dstart + 2],
                    raw[dstart + 3],
                ]));
            }
            b"sRGB" => has_srgb = true,
            b"IDAT" | b"IEND" => break,
            _ => {}
        }
        pos = dend + 4;
    }
    match gama {
        Some(g) if !has_srgb && png_gamma_is_nontrivial(g) => Some(g),
        _ => None,
    }
}

pub(super) fn apply_png_gamma(img: &mut RgbaImage, gama_100k: u32) {
    let gamma = gama_100k as f64 / 100_000.0;
    let exp = 1.0 / (gamma * 2.2);
    let mut lut = [0u8; 256];
    for (v, slot) in lut.iter_mut().enumerate() {
        let out = crate::detmath::pow(v as f64 / 255.0, exp) * 255.0;
        *slot = (out + 0.5).floor().clamp(0.0, 255.0) as u8;
    }

    let buf: &mut [u8] = img.as_mut();
    for px in buf.chunks_exact_mut(4) {
        px[0] = lut[px[0] as usize];
        px[1] = lut[px[1] as usize];
        px[2] = lut[px[2] as usize];
    }
}

fn source_extension(raw: &[u8]) -> &'static str {
    if raw.len() >= 8 && &raw[0..8] == b"\x89PNG\r\n\x1a\n" {
        ".png"
    } else if raw.len() >= 2 && raw[0] == 0xFF && raw[1] == 0xD8 {
        ".jpg"
    } else if is_psd(raw) {
        ".psd"
    } else {
        ".png"
    }
}

pub fn source_image_decodes(raw: &[u8]) -> bool {
    decode_source_image(raw).is_some()
}

pub(super) fn decode_source_image(raw: &[u8]) -> Option<RgbaImage> {
    if is_psd(raw) {
        let p = psd::Psd::from_bytes(raw).ok()?;
        let (w, h) = (p.width(), p.height());
        let rgba = p.rgba();
        if rgba.len() != (w as usize) * (h as usize) * 4 {
            return None;
        }
        return RgbaImage::from_raw(w, h, rgba);
    }

    if raw.len() >= 2 && raw[0] == 0xFF && raw[1] == 0xD8 {
        if let Ok((rgba, w, h)) = crate::ffi::decode_jpeg_rgba_box(raw) {
            return RgbaImage::from_raw(w, h, rgba);
        }
    }
    let mut img = image::load_from_memory(raw).ok().map(|d| d.to_rgba8())?;

    if let Some(g) = png_gamma_to_apply(raw) {
        apply_png_gamma(&mut img, g);
    }
    Some(img)
}

pub(super) fn standalone_key_extension(source_file: Option<&str>, raw: &[u8]) -> String {
    if let Some(sf) = source_file {
        let last_seg = sf.rsplit(['/', '\\']).next().unwrap_or(sf);
        if let Some(dot) = last_seg.rfind('.') {
            let ext = &last_seg[dot..];

            let lo = ext.to_ascii_lowercase();
            if matches!(lo.as_str(), ".png" | ".jpg" | ".jpeg" | ".psd") {
                return ext.to_string();
            }
        }
    }
    source_extension(raw).to_string()
}

pub(super) fn looks_like_normal_map(rgba: &[u8]) -> bool {
    let n = rgba.len() / 4;
    if n == 0 {
        return false;
    }

    for i in 0..n {
        if rgba[i * 4 + 3] != 255 {
            return false;
        }
    }
    let mut hits = 0usize;
    for i in 0..n {
        let nx = rgba[i * 4] as f64 / 127.5 - 1.0;
        let ny = rgba[i * 4 + 1] as f64 / 127.5 - 1.0;
        let nz = rgba[i * 4 + 2] as f64 / 127.5 - 1.0;
        let mag = (nx * nx + ny * ny + nz * nz).sqrt();
        if (mag - 1.0).abs() < 0.30 && nz >= -0.1 {
            hits += 1;
        }
    }
    (hits as f64 / n as f64) >= 0.95
}

pub(super) fn pack_normal_map(rgba: &[u8]) -> Vec<u8> {
    let n = rgba.len() / 4;
    let mut out = vec![0u8; n * 4];
    for i in 0..n {
        let r = rgba[i * 4];
        let g = rgba[i * 4 + 1];
        out[i * 4] = 255;
        out[i * 4 + 1] = g;
        out[i * 4 + 2] = g;
        out[i * 4 + 3] = r;
    }
    out
}

const DXT5_CRUNCH_QUALITY: u32 = 255;

const INGLB_BC7_CANONICAL_BLOCK: [u8; 16] = [
    0x20, 0x5a, 0xbf, 0xd6, 0xaf, 0xf5, 0x37, 0x37, 0xaf, 0xaa, 0xaa, 0xaa, 0x00, 0x00, 0x00, 0x00,
];

const INGLB_BC7_CANONICAL_BLOCK_NORMAL: [u8; 16] = [
    0x20, 0xff, 0xbf, 0xd6, 0xaf, 0xf5, 0x37, 0x37, 0xaf, 0xaa, 0xaa, 0xaa, 0x00, 0x00, 0x00, 0x00,
];

pub(super) fn encode_inglb_bc7_stub(
    width: u32,
    height: u32,
    mips: i32,
    normal_lf: bool,
) -> (Vec<u8>, i32) {
    let total = bc7_pure::compute_mip_chain_size(width, height, mips);
    let blocks = total / 16;
    let block = if normal_lf {
        &INGLB_BC7_CANONICAL_BLOCK_NORMAL
    } else {
        &INGLB_BC7_CANONICAL_BLOCK
    };
    let mut out = Vec::with_capacity(total);
    for _ in 0..blocks {
        out.extend_from_slice(block);
    }
    (out, mips)
}

const INGLB_DXT5_CANONICAL_BLOCK: [u8; 16] = [
    0xcd, 0xcd, 0x49, 0x92, 0x24, 0x49, 0x92, 0x24, 0x7a, 0xd6, 0x57, 0xbe, 0xaa, 0xaa, 0xaa, 0xaa,
];

fn encode_inglb_dxt5_stub(width: u32, height: u32, mips: i32) -> (Vec<u8>, i32) {
    let total = bc7_pure::compute_mip_chain_size(width, height, mips);
    let blocks = total / 16;
    let mut out = Vec::with_capacity(total);
    for _ in 0..blocks {
        out.extend_from_slice(&INGLB_DXT5_CANONICAL_BLOCK);
    }
    (out, mips)
}

pub(super) fn encode_dxt5_mip_chain_real(img: &RgbaImage, mips: i32, srgb: bool) -> (Vec<u8>, i32) {
    let (w, h) = img.dimensions();
    let (w, h) = (w as usize, h as usize);
    let src = img.as_raw();
    let mut flipped = vec![0u8; w * h * 4];
    for y in 0..h {
        flipped[y * w * 4..(y + 1) * w * 4]
            .copy_from_slice(&src[(h - 1 - y) * w * 4..(h - y) * w * 4]);
    }
    let mut cur: Vec<f32> = vec![0f32; w * h * 4];
    for i in 0..(w * h) {
        let r = flipped[i * 4];
        let g = flipped[i * 4 + 1];
        let b = flipped[i * 4 + 2];
        if srgb {
            cur[i * 4] = bc7_pure::srgb_to_linear_u8(r);
            cur[i * 4 + 1] = bc7_pure::srgb_to_linear_u8(g);
            cur[i * 4 + 2] = bc7_pure::srgb_to_linear_u8(b);
        } else {
            cur[i * 4] = r as f32;
            cur[i * 4 + 1] = g as f32;
            cur[i * 4 + 2] = b as f32;
        }
        cur[i * 4 + 3] = flipped[i * 4 + 3] as f32;
    }
    let (mut cw, mut ch) = (w, h);
    let params = texpresso::Params {
        algorithm: texpresso::Algorithm::IterativeClusterFit,
        weights: texpresso::COLOUR_WEIGHTS_PERCEPTUAL,
        weigh_colour_by_alpha: false,
    };
    let mut parts: Vec<u8> = Vec::new();
    for m in 0..mips {
        let pw = cw.max(1);
        let ph = ch.max(1);
        let mut level_px = vec![0u8; pw * ph * 4];
        for i in 0..(pw * ph) {
            if srgb {
                level_px[i * 4] = bc7_pure::linear_to_srgb_u8(cur[i * 4]);
                level_px[i * 4 + 1] = bc7_pure::linear_to_srgb_u8(cur[i * 4 + 1]);
                level_px[i * 4 + 2] = bc7_pure::linear_to_srgb_u8(cur[i * 4 + 2]);
            } else {
                level_px[i * 4] = bc7_pure::round_half_up_u8(cur[i * 4]);
                level_px[i * 4 + 1] = bc7_pure::round_half_up_u8(cur[i * 4 + 1]);
                level_px[i * 4 + 2] = bc7_pure::round_half_up_u8(cur[i * 4 + 2]);
            }
            level_px[i * 4 + 3] = bc7_pure::round_half_up_u8(cur[i * 4 + 3]);
        }
        let size = texpresso::Format::Bc3.compressed_size(pw, ph);
        let mut level = vec![0u8; size];
        texpresso::Format::Bc3.compress(&level_px, pw, ph, params, &mut level);
        parts.extend_from_slice(&level);
        if m < mips - 1 {
            let (next, nw, nh) = bc7_pure::box_halve(&cur, cw, ch);
            cur = next;
            cw = nw;
            ch = nh;
        }
    }
    (parts, mips)
}

pub(super) fn encode_texture_bc7(
    img: &RgbaImage,
    mips: i32,
    srgb: bool,
    normal_override: Option<bool>,
    profile: bc7_pure::Bc7Profile,
) -> (Vec<u8>, i32) {
    let (w, h) = img.dimensions();
    let rgba = img.as_raw();
    let is_normal = normal_override.unwrap_or_else(|| !srgb && looks_like_normal_map(rgba));
    let packed;
    let pixels: &[u8] = if is_normal {
        packed = pack_normal_map(rgba);
        &packed
    } else {
        rgba
    };
    let perceptual = srgb && !is_normal;
    bc7_pure::encode_bc7_mip_chain_with_profile(
        pixels,
        w,
        h,
        Some(mips),
        true,
        srgb,
        perceptual,
        profile,
    )
}

pub(super) fn encode_standalone_dxt5(
    img: &RgbaImage,
    prof: &texprofile::Profile,
    usage_normal: Option<bool>,
) -> (Vec<u8>, i32) {
    let srgb = prof.color_space == 1;
    let is_normal = usage_normal.unwrap_or_else(|| !srgb && looks_like_normal_map(img.as_raw()));
    if is_normal {
        let (w, h) = img.dimensions();
        let packed = RgbaImage::from_raw(w, h, pack_normal_map(img.as_raw()))
            .expect("packed normal-map buffer size mismatch");
        encode_dxt5_mip_chain_real(&packed, prof.mip_count, srgb)
    } else {
        encode_dxt5_mip_chain_real(img, prof.mip_count, srgb)
    }
}

pub(super) fn standalone_texture_readable(model_referenced: bool, compressed: bool) -> bool {
    !(model_referenced && compressed)
}

pub(super) fn detect_container(raw: &[u8]) -> String {
    if raw.len() >= 8 && &raw[0..8] == b"\x89PNG\r\n\x1a\n" {
        "PNG".to_string()
    } else if raw.len() >= 2 && raw[0] == 0xFF && raw[1] == 0xD8 {
        "JPEG".to_string()
    } else {
        String::new()
    }
}

pub(super) fn mean_color_image(img: &RgbaImage) -> RgbaImage {
    let (w, h) = img.dimensions();
    let raw = img.as_raw();
    let n = (w as u64) * (h as u64);
    let (mut sr, mut sg, mut sb, mut sa) = (0u64, 0u64, 0u64, 0u64);
    for c in raw.chunks_exact(4) {
        sr += c[0] as u64;
        sg += c[1] as u64;
        sb += c[2] as u64;
        sa += c[3] as u64;
    }
    let mean = [
        (sr / n) as u8,
        (sg / n) as u8,
        (sb / n) as u8,
        (sa / n) as u8,
    ];
    let mut buf = Vec::with_capacity(raw.len());
    for _ in 0..n {
        buf.extend_from_slice(&mean);
    }
    RgbaImage::from_raw(w, h, buf).expect("mean-color buffer size mismatch")
}

impl<'a> Builder<'a> {
    fn source_image(&self, scene: &Scene, idx: usize) -> texprofile::SourceImage {
        let img = scene.images[idx].as_ref().unwrap();
        let (w, h) = img.dimensions();
        let container = scene
            .image_bytes
            .get(idx)
            .and_then(|o| o.as_ref())
            .map(|raw| detect_container(raw))
            .unwrap_or_default();
        let has_real_alpha = img.as_raw().iter().skip(3).step_by(4).any(|&a| a < 255);
        texprofile::SourceImage {
            width: w,
            height: h,
            container,
            has_real_alpha,
        }
    }

    pub(super) fn external_texture(
        &mut self,
        scene: &Scene,
        img_idx: Option<usize>,
    ) -> Option<(i64, i64)> {
        let idx = img_idx?;
        if idx >= scene.image_uri.len() {
            return None;
        }
        let uri = scene.image_uri[idx].clone()?;
        if uri.is_empty() {
            return None;
        }
        let resolver = self.resolve_hash?;
        if let Some(p) = self.ext_tex_pptr.get(&idx) {
            return Some(*p);
        }
        let ext_hash = resolver(&uri)?;
        if ext_hash.is_empty() {
            return None;
        }
        let bundle_file =
            crate::naming::canonical_filename(&ext_hash, ".png", self.target, None).ok()?;
        let file_id = match self.ext_bundle_fileid.get(&bundle_file) {
            Some(&f) => f,
            None => {
                let fid = 2 + self.ext_bundle_files.len() as i64;
                self.ext_bundle_fileid.insert(bundle_file.clone(), fid);
                self.ext_bundle_files.push(bundle_file);
                fid
            }
        };
        let tex_guid = pathids::asset_guid(&ext_hash);
        let tex_pid = pathids::prefab_packed_path_id(
            &tex_guid,
            TEXTURE_LOCAL_ID,
            pathids::FILE_TYPE_META_ASSET,
        );
        let pptr = (file_id, tex_pid);
        self.ext_tex_pptr.insert(idx, pptr);
        Some(pptr)
    }

    pub(super) fn build_sampler_canon(&mut self, scene: &Scene) {
        let effective = |idx: usize,
                         raw: Option<usize>|
         -> (Option<i64>, Option<i64>, Option<i64>, Option<i64>) {
            match raw.and_then(|si| scene.samplers.get(si).copied()) {
                Some(s) => (s.mag_filter, s.min_filter, s.wrap_s, s.wrap_t),
                None => {
                    let (mag, mn) = scene
                        .image_sampler
                        .get(idx)
                        .copied()
                        .unwrap_or((None, None));
                    let (ws, wt) = scene.image_wrap.get(idx).copied().unwrap_or((None, None));
                    (mag, mn, ws, wt)
                }
            }
        };

        let mut per_image_first: HashMap<
            (usize, (Option<i64>, Option<i64>, Option<i64>, Option<i64>)),
            Option<usize>,
        > = HashMap::new();
        for tr in &scene.texture_refs {
            let sig = effective(tr.image, tr.sampler);
            let canon = *per_image_first.entry((tr.image, sig)).or_insert(tr.sampler);
            self.sampler_canon.insert((tr.image, tr.sampler), canon);
        }
    }

    pub(super) fn texture(&mut self, scene: &Scene, tex: Option<TexRef>) -> Option<i64> {
        let tex = tex?;
        let idx = tex.image;
        if idx >= scene.images.len() || scene.images[idx].is_none() {
            return None;
        }

        if !texprofile::unity_load_image_would_succeed(&self.source_image(scene, idx)) {
            return None;
        }

        let canon = self
            .sampler_canon
            .get(&(idx, tex.sampler))
            .copied()
            .unwrap_or(tex.sampler);
        let key = (idx, canon);
        if let Some(&pid) = self.tex_pid.get(&key) {
            return Some(pid);
        }

        let first_sampler = *self.tex_first_sampler.entry(idx).or_insert(canon);
        let name = if canon == first_sampler {
            format!("image_{idx}")
        } else {
            match canon {
                Some(s) => format!("image_{idx}_sampler{s}"),
                None => format!("image_{idx}"),
            }
        };
        self.tex_name.insert(key, name.clone());

        let colorspace = *self.colorspaces.get(&idx).unwrap_or(&1);
        let sampler = canon.and_then(|si| scene.samplers.get(si).copied());
        let (mag, mn, ws, wt) = match sampler {
            Some(s) => (s.mag_filter, s.min_filter, s.wrap_s, s.wrap_t),
            None => {
                let (mag, mn) = scene
                    .image_sampler
                    .get(idx)
                    .copied()
                    .unwrap_or((None, None));
                let (ws, wt) = scene.image_wrap.get(idx).copied().unwrap_or((None, None));
                (mag, mn, ws, wt)
            }
        };
        let is_normal = scene.normal_images.contains(&idx);
        let src = self.source_image(scene, idx);
        let is_dxt1 = self.dxt1_images.contains(&idx);

        let is_bc5_normal = self.bc5_normal_images.contains(&idx);

        let max_size = if self.lod.is_some() {
            texprofile::max_texture_size_for(self.target).min(512)
        } else {
            texprofile::max_texture_size_for(self.target)
        };
        let (mut unc_p, mut bc7_p) = if is_bc5_normal {
            texprofile::texture_profile_bc5_normal(&src, colorspace, mag, mn, max_size)
        } else if is_dxt1 {
            texprofile::texture_profile_dxt1(&src, colorspace, mag, mn, max_size)
        } else {
            texprofile::texture_profile(&src, colorspace, is_normal, mag, mn, max_size)
        };

        if self.lod.is_some() && bc7_p.compressed {
            let side = bc7_p.target_w.max(bc7_p.target_h);
            bc7_p.target_w = side;
            bc7_p.target_h = side;
            bc7_p.mip_count = texprofile::default_mip_count(side, side);
        }

        if self.spec_color_only_images.contains(&idx) {
            unc_p.color_space = 0;
            if bc7_p.texture_format == texprofile::TF_BC7 {
                bc7_p.texture_format = texprofile::TF_DXT5;
            }
        }

        if self.unbound_images.contains(&idx) && bc7_p.compressed {
            bc7_p.texture_format = texprofile::TF_DXT5;
            bc7_p.color_space = 1;
        }

        if self.target == "webgl" && bc7_p.compressed {
            bc7_p.texture_format = texprofile::TF_DXT5;
        }

        let unc_wrap_u = texprofile::sampler_wrap_mode(ws);
        let unc_wrap_v = texprofile::sampler_wrap_mode(wt);
        let img = scene.images[idx].clone().unwrap();

        let n_distinct_samplers = self
            .image_distinct_samplers
            .get(&idx)
            .map(|s| s.len())
            .unwrap_or(1);
        let multi_sampler_uncompressed = !unc_p.compressed && n_distinct_samplers > 1;

        if !self.toggles.v38_compat && self.lod.is_none() {
            let mut inglb_tree = self.texture_tree_with_wrap(
                &img,
                &name,
                &unc_p,
                Some((unc_wrap_u, unc_wrap_v)),
                Some(&src),
                false,
            );
            if multi_sampler_uncompressed {
                inglb_tree.insert("m_IsReadable", true);
            }
            let inglb = self.add(
                "Texture2D",
                inglb_tree,
                Role::Glb("Texture2D".into(), format!("textures/{name}")),
            );
            if multi_sampler_uncompressed {
                self.force_inline_tex.insert(inglb);
            }
            self.scene_object_pids.push(inglb);
        }

        let real_tex = self.toggles.real_textures;
        let ext_tree = self.texture_tree_with_wrap(
            &img,
            &name,
            &bc7_p,
            None,
            Some(&src),
            !real_tex && !multi_sampler_uncompressed && self.lod.is_none(),
        );
        let ext = self.add("Texture2D", ext_tree, Role::Tex(name.clone()));
        self.tex_pid.insert(key, ext);
        let entry_key = if self.lod.is_some() {
            let container = scene
                .image_bytes
                .get(idx)
                .and_then(|o| o.as_deref())
                .map(detect_container)
                .unwrap_or_default();
            if container == "JPEG" {
                format!("{name}.jpg")
            } else {
                format!("{name}.png")
            }
        } else {
            format!("{name}.png")
        };
        if self.lod.is_some() {
            self.force_inline_tex.insert(ext);
        }
        self.texture_entries.push((entry_key, ext));
        Some(ext)
    }

    fn texture_tree_with_wrap(
        &self,
        img: &RgbaImage,
        name: &str,
        prof: &texprofile::Profile,
        wrap: Option<(i64, i64)>,
        src: Option<&texprofile::SourceImage>,
        force_inglb_stub: bool,
    ) -> Value {
        let mut t = self.base_clone("Texture2D");
        let (data, mips): (Vec<u8>, i32) = if prof.compressed {
            let (ow, oh) = img.dimensions();
            let max_size = texprofile::max_texture_size_for(self.target);
            let load_image_ok = src
                .map(texprofile::unity_load_image_would_succeed)
                .unwrap_or(true);

            let stub_bc7 = prof.texture_format == texprofile::TF_BC7
                && prof.color_space == 1
                && (ow > max_size || oh > max_size)
                && load_image_ok
                && self.lod.is_none()
                && !self.toggles.real_textures;
            if force_inglb_stub && prof.texture_format == texprofile::TF_DXT5 {
                let (data, mips) =
                    encode_inglb_dxt5_stub(prof.target_w, prof.target_h, prof.mip_count);
                t.insert("m_Name", name);
                t.insert("m_Width", prof.target_w);
                t.insert("m_Height", prof.target_h);
                t.insert("m_TextureFormat", prof.texture_format);
                t.insert("m_MipCount", mips);
                t.insert("m_CompleteImageSize", data.len() as i64);
                t.insert("m_IsReadable", false);
                t.insert("m_ColorSpace", prof.color_space);
                t.insert("m_LightmapFormat", prof.lightmap_format);
                t.insert("m_IsAlphaChannelOptional", prof.is_alpha_channel_optional);
                t.insert("m_IgnoreMipmapLimit", prof.ignore_mipmap_limit);
                if let Some(ts) = t.get_mut("m_TextureSettings") {
                    ts.insert("m_FilterMode", prof.filter_mode);
                    if let Some((wu, wv)) = wrap {
                        ts.insert("m_WrapU", wu);
                        ts.insert("m_WrapV", wv);
                    }
                }
                t.insert("image data", Value::Bytes(data));
                t.insert(
                    "m_StreamData",
                    map! {"offset" => 0, "size" => 0, "path" => ""},
                );
                return t;
            }
            if force_inglb_stub && prof.texture_format == texprofile::TF_BC7 {
                let (data, mips) = encode_inglb_bc7_stub(
                    prof.target_w,
                    prof.target_h,
                    prof.mip_count,
                    prof.lightmap_format == 3,
                );
                t.insert("m_Name", name);
                t.insert("m_Width", prof.target_w);
                t.insert("m_Height", prof.target_h);
                t.insert("m_TextureFormat", prof.texture_format);
                t.insert("m_MipCount", mips);
                t.insert("m_CompleteImageSize", data.len() as i64);
                t.insert("m_IsReadable", false);
                t.insert("m_ColorSpace", prof.color_space);
                t.insert("m_LightmapFormat", prof.lightmap_format);
                t.insert("m_IsAlphaChannelOptional", prof.is_alpha_channel_optional);
                t.insert("m_IgnoreMipmapLimit", prof.ignore_mipmap_limit);
                if let Some(ts) = t.get_mut("m_TextureSettings") {
                    ts.insert("m_FilterMode", prof.filter_mode);
                    if let Some((wu, wv)) = wrap {
                        ts.insert("m_WrapU", wu);
                        ts.insert("m_WrapV", wv);
                    }
                }
                t.insert("image data", Value::Bytes(data));
                t.insert(
                    "m_StreamData",
                    map! {"offset" => 0, "size" => 0, "path" => ""},
                );
                return t;
            }
            let stubbed_src;
            let img: &RgbaImage = if stub_bc7 {
                stubbed_src = mean_color_image(img);
                &stubbed_src
            } else {
                img
            };
            let resized;
            let src: &RgbaImage = if (prof.target_w, prof.target_h) != (ow, oh) {
                let buf = crate::resize::box_downscale_rgba(
                    img.as_raw(),
                    ow as usize,
                    oh as usize,
                    prof.target_w as usize,
                    prof.target_h as usize,
                );
                resized = RgbaImage::from_raw(prof.target_w, prof.target_h, buf)
                    .expect("resize buffer size mismatch");
                &resized
            } else {
                img
            };
            if stub_bc7 && prof.texture_format == texprofile::TF_BC7 {
                encode_inglb_bc7_stub(
                    prof.target_w,
                    prof.target_h,
                    prof.mip_count,
                    prof.lightmap_format == 3,
                )
            } else if prof.texture_format == texprofile::TF_DXT5 {
                encode_dxt5_mip_chain_real(src, prof.mip_count, prof.color_space == 1)
            } else if prof.texture_format == texprofile::TF_DXT1 {
                let (sw, sh) = src.dimensions();
                crate::dxt1_pure::encode_dxt1_mip_chain(
                    src.as_raw(),
                    sw,
                    sh,
                    Some(prof.mip_count),
                    true,
                    prof.color_space == 1,
                )
            } else if prof.texture_format == texprofile::TF_DXT5_CRUNCHED {
                let (sw, sh) = src.dimensions();
                crate::bc5_pure::encode_dxt5_crn_dual_use_mip_chain(
                    src.as_raw(),
                    sw,
                    sh,
                    Some(prof.mip_count),
                    true,
                    DXT5_CRUNCH_QUALITY,
                )
                .unwrap_or_else(|| {
                    panic!(
                        "crunch DXT5 compression failed for texture {name} ({sw}x{sh}, {} mips)",
                        prof.mip_count
                    )
                })
            } else {
                encode_texture_bc7(
                    src,
                    prof.mip_count,
                    prof.color_space == 1,
                    Some(prof.lightmap_format == 3),
                    bc7_pure::Bc7Profile::Slow,
                )
            }
        } else if prof.texture_format == texprofile::TF_RGBA32 {
            (dxt_unity::encode_rgba32(img, true), 1)
        } else if prof.texture_format == texprofile::TF_RGBA32_UNITY {
            let (ow, oh) = img.dimensions();
            let resized;
            let src: &RgbaImage = if (prof.target_w, prof.target_h) != (ow, oh) {
                let buf = crate::resize::box_downscale_rgba(
                    img.as_raw(),
                    ow as usize,
                    oh as usize,
                    prof.target_w as usize,
                    prof.target_h as usize,
                );
                resized = RgbaImage::from_raw(prof.target_w, prof.target_h, buf)
                    .expect("resize buffer size mismatch");
                &resized
            } else {
                img
            };
            let (data, mips) = bc7_pure::encode_rgba32_mip_chain(
                src.as_raw(),
                prof.target_w,
                prof.target_h,
                Some(prof.mip_count),
                true,
                prof.color_space == 1,
            );
            if force_inglb_stub {
                (vec![0xcd_u8; data.len()], mips)
            } else {
                (data, mips)
            }
        } else {
            (dxt_unity::encode_rgb24(img, true), 1)
        };

        t.insert("m_Name", name);
        t.insert("m_Width", prof.target_w);
        t.insert("m_Height", prof.target_h);
        t.insert("m_TextureFormat", prof.texture_format);
        t.insert("m_MipCount", mips);
        t.insert("m_CompleteImageSize", data.len() as i64);
        t.insert("m_IsReadable", false);
        t.insert("m_ColorSpace", prof.color_space);
        t.insert("m_LightmapFormat", prof.lightmap_format);
        t.insert("m_IsAlphaChannelOptional", prof.is_alpha_channel_optional);
        t.insert("m_IgnoreMipmapLimit", prof.ignore_mipmap_limit);
        if let Some(ts) = t.get_mut("m_TextureSettings") {
            ts.insert("m_FilterMode", prof.filter_mode);
            if let Some((wu, wv)) = wrap {
                ts.insert("m_WrapU", wu);
                ts.insert("m_WrapV", wv);
            }
        }
        t.insert("image data", Value::Bytes(data));
        t.insert(
            "m_StreamData",
            map! {"offset" => 0, "size" => 0, "path" => ""},
        );
        t
    }
}
