use super::finalize::commit_objects;
use super::finalize::ExternalsPolicy;
use super::texture::decode_source_image;
use super::texture::detect_container;
use super::texture::encode_inglb_bc7_stub;
use super::texture::encode_standalone_dxt5;
use super::texture::encode_texture_bc7;
use super::texture::mean_color_image;
use super::texture::standalone_key_extension;
use super::texture::standalone_texture_readable;
use super::*;

pub(super) struct StandaloneTextureBuilder<'a> {
    proto: &'a HashMap<String, SerializedType>,
    base: &'a HashMap<String, Value>,
    root_hash: String,
    bundle_name: String,
    source_file: Option<String>,
    target: &'static str,
    model_referenced: bool,
    color_space_override: Option<i64>,
    standalone_normal: bool,
    toggles: Toggles,
    objects: BTreeMap<i64, (String, Value)>,
    order: Vec<i64>,
    stream: Option<(i64, Vec<u8>)>,
}

impl<'a> StandaloneTextureBuilder<'a> {
    pub(super) fn new(
        proto: &'a HashMap<String, SerializedType>,
        base: &'a HashMap<String, Value>,
        root_hash: String,
        bundle_name: String,
        source_file: Option<String>,
        model_referenced: bool,
        color_space_override: Option<i64>,
        standalone_normal: bool,
        toggles: Toggles,
    ) -> Self {
        let target = target_from_bundle_name(&bundle_name);
        StandaloneTextureBuilder {
            proto,
            base,
            root_hash,
            bundle_name,
            source_file,
            target,
            model_referenced,
            color_space_override,
            standalone_normal,
            toggles,
            objects: BTreeMap::new(),
            order: Vec::new(),
            stream: None,
        }
    }

    fn set_obj(&mut self, pid: i64, type_name: &str, tree: Value) {
        if !self.objects.contains_key(&pid) {
            self.order.push(pid);
        }
        self.objects.insert(pid, (type_name.to_string(), tree));
    }

    fn texture_pid(&self) -> i64 {
        let guid = pathids::asset_guid(&self.root_hash);
        pathids::prefab_packed_path_id(&guid, TEXTURE_LOCAL_ID, pathids::FILE_TYPE_META_ASSET)
    }

    fn metadata_pid(&self) -> i64 {
        let guid = pathids::asset_guid(&format!("{}/metadata", self.root_hash));
        pathids::prefab_packed_path_id(&guid, 4900000, pathids::FILE_TYPE_META_ASSET)
    }

    fn texture_tree(
        &self,
        prof: &texprofile::Profile,
        data: Vec<u8>,
        mips: i32,
        name: &str,
        readable: bool,
    ) -> Value {
        let mut t = self.base.get("Texture2D").cloned().unwrap_or(Value::Null);
        t.insert("m_Name", name);
        t.insert("m_Width", prof.target_w);
        t.insert("m_Height", prof.target_h);
        t.insert("m_TextureFormat", prof.texture_format);
        t.insert("m_MipCount", mips);
        t.insert("m_CompleteImageSize", data.len() as i64);
        t.insert("m_IsReadable", readable);
        t.insert("m_ColorSpace", prof.color_space);
        t.insert("m_LightmapFormat", prof.lightmap_format);
        t.insert("m_IsAlphaChannelOptional", prof.is_alpha_channel_optional);
        t.insert("m_IgnoreMipmapLimit", prof.ignore_mipmap_limit);
        if let Some(ts) = t.get_mut("m_TextureSettings") {
            ts.insert("m_FilterMode", prof.filter_mode);
        }
        t.insert("image data", Value::Bytes(data));
        t.insert(
            "m_StreamData",
            map! {"offset" => 0, "size" => 0, "path" => ""},
        );
        t
    }

    pub(super) fn build(
        &mut self,
        raw: &[u8],
        bundle: &mut Bundle,
        memo: Option<&mut unity::bundle_file::ChunkMemo>,
    ) -> Result<Vec<u8>> {
        let decoded = decode_source_image(raw);

        let mut tex_pid: Option<i64> = None;
        if let Some(img) = &decoded {
            let (w, h) = img.dimensions();
            let container = detect_container(raw);
            let has_real_alpha = img.as_raw().iter().skip(3).step_by(4).any(|&a| a < 255);
            let src = texprofile::SourceImage {
                width: w,
                height: h,
                container,
                has_real_alpha,
            };
            let load_image_ok = texprofile::unity_load_image_would_succeed(&src);
            let cap = if load_image_ok {
                texprofile::max_texture_size_for(self.target)
            } else {
                texprofile::TEXTURE_IMPORTER_DEFAULT_MAX
            };

            let usage_normal: Option<bool> = if self.standalone_normal {
                Some(true)
            } else if self.color_space_override == Some(0) {
                Some(false)
            } else {
                None
            };
            let mut prof = texprofile::standalone_texture_profile_named(&src, cap, usage_normal);
            if let Some(cs) = self.color_space_override {
                prof.color_space = cs;
            }

            if self.target == "webgl" && prof.compressed {
                prof.texture_format = texprofile::TF_DXT5;
            }

            let fancy_buf;
            let img: &RgbaImage = if raw.len() >= 2
                && raw[0] == 0xFF
                && raw[1] == 0xD8
                && (prof.target_w, prof.target_h) == (w, h)
            {
                match libjpeg9c::decode_rgba(raw, true) {
                    Some((rgba, fw, fh)) if (fw, fh) == (w, h) => {
                        fancy_buf =
                            RgbaImage::from_raw(fw, fh, rgba).expect("jpeg fancy buffer size");
                        &fancy_buf
                    }
                    _ => img,
                }
            } else {
                img
            };

            let real_textures = self.toggles.real_textures;
            let max_size = texprofile::max_texture_size_for(self.target);
            let oversized = (w > max_size || h > max_size) && load_image_ok;
            let stub_canonical = oversized
                && !real_textures
                && prof.compressed
                && prof.texture_format == texprofile::TF_BC7;
            let stubbed_buf;
            let img: &RgbaImage = if oversized && !real_textures {
                stubbed_buf = mean_color_image(img);
                &stubbed_buf
            } else {
                img
            };

            let bled_src;
            let img: &RgbaImage = if has_real_alpha && prof.compressed {
                let mut buf = img.as_raw().clone();
                crate::alpha_bleed::alpha_bleed_inplace(&mut buf, w, h);
                bled_src =
                    RgbaImage::from_raw(w, h, buf).expect("alpha-bleed buffer size mismatch");
                &bled_src
            } else {
                img
            };

            let resized;
            let pil: &RgbaImage = if (prof.target_w, prof.target_h) != (w, h) {
                let buf = crate::resize::box_downscale_rgba(
                    img.as_raw(),
                    w as usize,
                    h as usize,
                    prof.target_w as usize,
                    prof.target_h as usize,
                );
                resized = RgbaImage::from_raw(prof.target_w, prof.target_h, buf)
                    .expect("resize buffer size mismatch");
                &resized
            } else {
                img
            };
            let bc7_profile = match self.target {
                "windows" | "mac" | "linux" if !self.model_referenced => {
                    bc7_pure::Bc7Profile::Basic
                }
                _ => bc7_pure::Bc7Profile::Slow,
            };
            let (data, mips) = if stub_canonical {
                encode_inglb_bc7_stub(
                    prof.target_w,
                    prof.target_h,
                    prof.mip_count,
                    prof.lightmap_format == 3,
                )
            } else if prof.compressed && prof.texture_format == texprofile::TF_DXT5 {
                encode_standalone_dxt5(pil, &prof, usage_normal)
            } else if prof.compressed {
                encode_texture_bc7(
                    pil,
                    prof.mip_count,
                    prof.color_space == 1,
                    usage_normal,
                    bc7_profile,
                )
            } else {
                debug_assert_eq!(prof.texture_format, texprofile::TF_RGBA32_UNITY);

                bc7_pure::encode_rgba32_mip_chain(
                    pil.as_raw(),
                    prof.target_w,
                    prof.target_h,
                    Some(prof.mip_count),
                    true,
                    prof.color_space == 1,
                )
            };
            let pid = self.texture_pid();

            let do_stream =
                self.target != "webgl" && self.model_referenced && prof.texture_format == 25;
            let readable = standalone_texture_readable(self.model_referenced, prof.compressed);
            let tree =
                self.texture_tree(&prof, data.clone(), mips, &self.root_hash.clone(), readable);
            self.set_obj(pid, "Texture2D", tree);
            tex_pid = Some(pid);
            if do_stream {
                self.stream = Some((pid, data));
            }
        }

        let meta_pid_opt = if emits_metadata_textasset(&self.root_hash, self.toggles.v38_compat) {
            let mut meta = self.base.get("TextAsset").cloned().unwrap_or(Value::Null);
            meta.insert("m_Name", "metadata");
            let version = metadata_version_for_target(self.target, self.toggles.v38_compat);
            let ts = metadata_timestamp(self.toggles);

            meta.insert(
                "m_Script",
                format!(
                    r#"{{"timestamp":{ts},"version":"{version}","dependencies":[],"mainAsset":""}}"#
                ),
            );
            let meta_pid = self.metadata_pid();
            self.set_obj(meta_pid, "TextAsset", meta);
            Some(meta_pid)
        } else {
            None
        };

        let mut ab = self.base.get("AssetBundle").cloned().unwrap_or(Value::Null);
        let lower = self.bundle_name.to_ascii_lowercase();
        ab.insert("m_Name", lower.clone());
        ab.insert("m_AssetBundleName", lower);
        ab.insert("m_Dependencies", Value::Array(vec![]));

        let tex_ext = standalone_key_extension(self.source_file.as_deref(), raw);
        let tex_key = format!("{}{}", self.root_hash, tex_ext);
        let mut entries: Vec<sbp_order::Entry> = Vec::new();
        let (objects, asset) = match tex_pid {
            Some(p) => (
                vec![sbp_order::Obj::new(0, p)],
                Some(sbp_order::Obj::new(0, p)),
            ),
            None => (vec![], Some(sbp_order::Obj::new(0, 0))),
        };
        entries.push(sbp_order::Entry {
            guid: pathids::asset_guid(&self.root_hash),
            key: tex_key,
            objects,
            asset,
        });
        if let Some(meta_pid) = meta_pid_opt {
            entries.push(sbp_order::Entry {
                guid: pathids::asset_guid(&format!("{}/metadata", self.root_hash)),
                key: "metadata.json".into(),
                objects: vec![sbp_order::Obj::new(0, meta_pid)],
                asset: Some(sbp_order::Obj::new(0, meta_pid)),
            });
        }

        let (preload, container) = sbp_order::build_preload_and_container(&entries);
        let (preload_v, container_v) = sbp_order::to_values(&preload, &container);
        ab.insert("m_PreloadTable", preload_v);
        ab.insert("m_Container", container_v);
        ab.insert("m_MainAsset", sbp_order::empty_main_asset());
        self.set_obj(1, "AssetBundle", ab);

        self.commit(bundle)?;
        bundle.save_lz4_memo(memo)
    }

    /// Serialize the already-encoded texture object graph under a sibling
    /// platform's bundle name (encode-once, serialize-twice). Standalone
    /// bundles carry no platform strings beyond the AssetBundle name and the
    /// commit-level CAB/target bytes, and the metadata TextAsset is
    /// target-invariant within the shareable {windows, mac} pair the caller
    /// guarantees, so only the AssetBundle name needs recomputing.
    pub(super) fn rebuild_for(
        &mut self,
        bundle_name: &str,
        bundle: &mut Bundle,
        memo: Option<&mut unity::bundle_file::ChunkMemo>,
    ) -> Result<Vec<u8>> {
        self.bundle_name = bundle_name.to_string();
        self.target = target_from_bundle_name(bundle_name);
        if let Some((_, ab)) = self.objects.get_mut(&1) {
            let lower = self.bundle_name.to_ascii_lowercase();
            ab.insert("m_Name", lower.clone());
            ab.insert("m_AssetBundleName", lower);
        }
        self.commit(bundle)?;
        bundle.save_lz4_memo(memo)
    }

    fn commit(&self, bundle: &mut Bundle) -> Result<()> {
        let blobs: Vec<ress::TextureBlob> = self
            .stream
            .as_ref()
            .map(|(pid, data)| vec![ress::TextureBlob::new(*pid, data.clone(), "")])
            .unwrap_or_default();
        commit_objects(
            bundle,
            &self.bundle_name,
            self.target,
            self.proto,
            &self.objects,
            &blobs,
            &HashSet::new(),
            ExternalsPolicy::Clear,
        )
    }
}
