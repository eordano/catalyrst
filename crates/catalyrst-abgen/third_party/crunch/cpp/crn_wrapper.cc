// crn_wrapper.cc — see crn_wrapper.h for the public C ABI contract.
//
// Implementation notes:
//   * `crn_compress` returns a buffer allocated via `crnlib_malloc`. We
//     forward the pointer through `crn_ffi_compress_rgba` and release
//     it later through `crn_ffi_free` -> `crn_free_block`.
//   * For BC5/DXN normal maps we MUST clear `cCRNCompFlagPerceptual`
//     (crnlib.h says so explicitly in its banner comment) — perceptual
//     colorspace distance is wrong for normal-map data.
//   * `m_alpha_component = 1` so the DXN encoder treats the input's
//     `.g` channel as the "alpha" (= second BC5 channel = Y). The
//     caller is expected to hand us RGBA where R=X and G=Y already
//     (the BC5 swizzle), see abgen-rs/src/bc5_pure.rs::repack_for_bc5.

#include "crn_wrapper.h"
#include "../inc/crnlib.h"

#include <cstdlib>
#include <cstring>

// Defined in crnlib/crnlib.cpp — runs the one-time crnlib global init
// (table builds, threading, decomp memory callbacks) on first use
// instead of at process startup.
extern "C" void crn_ffi_ensure_global_init(void);

extern "C" int crn_ffi_compress_rgba(
    enum crn_ffi_format fmt,
    const uint8_t* mip_rgba,
    size_t mip_rgba_len,
    const uint32_t* mip_w,
    const uint32_t* mip_h,
    uint32_t mip_count,
    uint32_t quality_level,
    uint32_t num_helper_threads,
    uint8_t** out_data,
    uint32_t* out_size)
{
    if (!mip_rgba || !mip_w || !mip_h || !out_data || !out_size) return 1;
    if (mip_count == 0 || mip_count > 16) return 2; // cCRNMaxLevels = 16.

    crn_ffi_ensure_global_init();

    crn_format crn_fmt;
    switch (fmt) {
        case CRN_FFI_FMT_DXN_XY: crn_fmt = cCRNFmtDXN_XY; break;
        case CRN_FFI_FMT_DXT5:   crn_fmt = cCRNFmtDXT5;   break;
        default: return 3;
    }

    // Walk the flat input and stash a pointer per mip. We also verify
    // the total length matches the sum of per-mip byte sizes.
    const uint32_t* mip_ptrs[16] = {0};
    size_t cursor = 0;
    for (uint32_t i = 0; i < mip_count; ++i) {
        const size_t want = (size_t)mip_w[i] * (size_t)mip_h[i] * 4u;
        if (cursor + want > mip_rgba_len) return 4;
        mip_ptrs[i] = reinterpret_cast<const uint32_t*>(mip_rgba + cursor);
        cursor += want;
    }
    if (cursor != mip_rgba_len) return 5;

    crn_comp_params params;
    params.m_format = crn_fmt;
    params.m_width = mip_w[0];
    params.m_height = mip_h[0];
    params.m_levels = mip_count;
    params.m_faces = 1;
    params.m_quality_level = quality_level;
    params.m_num_helper_threads = num_helper_threads;
    if (crn_fmt == cCRNFmtDXN_XY) {
        // DXN/3DC / normal-map data is NOT perceptual sRGB.
        params.set_flag(cCRNCompFlagPerceptual, false);
        // BC5 packs two channels. Tell crnlib the "alpha" slot maps to .g
        // (input channel 1) so the DXN encoder picks up Y from the second
        // channel of our (R=X, G=Y) input.
        params.m_alpha_component = 1;
    } else {
        // Plain DXT5 color data: keep crnlib's defaults (perceptual sRGB
        // weighting on, alpha from input channel 3).
        params.set_flag(cCRNCompFlagPerceptual, true);
        params.m_alpha_component = 3;
    }
    // CRN file output (not DDS).
    params.m_file_type = cCRNFileTypeCRN;
    // Match Unity's TextureImporter container fields: every CRN payload in
    // upstream ab-cdn Unity bundles carries m_userdata0=1, m_userdata1=0
    // (verified on entity QmNSsgKt3xYRfcXdPM4Uh7dvG5NaJWuaorHnqVmFRuLCig's
    // DXT5Crunched textures). Unity treats this as the "new crunch" marker.
    params.m_userdata0 = 1;
    params.m_userdata1 = 0;

    for (uint32_t i = 0; i < mip_count; ++i) {
        params.m_pImages[0][i] = mip_ptrs[i];
    }

    crn_uint32 compressed_size = 0;
    void* compressed = crn_compress(params, compressed_size, NULL, NULL);
    if (!compressed) return 6;
    if (compressed_size == 0) {
        crn_free_block(compressed);
        return 7;
    }

    *out_data = reinterpret_cast<uint8_t*>(compressed);
    *out_size = compressed_size;
    return 0;
}

extern "C" void crn_ffi_free(uint8_t* p) {
    if (!p) return;
    // Defensive: any buffer we free was produced by a compress call,
    // which already ran the global init — but keep free safe to call
    // in any order.
    crn_ffi_ensure_global_init();
    crn_free_block(p);
}

extern "C" int crn_ffi_decompress_rgba(
    const uint8_t* crn_data,
    uint32_t crn_len,
    uint8_t** out_rgba,
    uint32_t* out_w,
    uint32_t* out_h,
    uint32_t* out_levels,
    uint32_t* out_format)
{
    if (!crn_data || !out_rgba || !out_w || !out_h || !out_levels || !out_format) return 1;
    // Smallest sensible CRN header: sig(2) header_size(2) crc(2) data_size(4)
    // data_crc(2) width(2) height(2) levels(1) faces(1) format(1) = 19 bytes.
    if (crn_len < 19 || crn_data[0] != 'H' || crn_data[1] != 'x') return 2;

    crn_ffi_ensure_global_init();

    // The CRN header is packed big-endian (crn_packed_uint); m_format is
    // the single byte at offset 18.
    *out_format = crn_data[18];

    crn_uint32 dds_size = crn_len;
    void* dds = crn_decompress_crn_to_dds(crn_data, dds_size);
    if (!dds) return 3;

    crn_uint32* images[cCRNMaxFaces * cCRNMaxLevels] = {0};
    crn_texture_desc desc;
    std::memset(&desc, 0, sizeof(desc));
    if (!crn_decompress_dds_to_images(dds, dds_size, images, desc)) {
        crn_free_block(dds);
        return 4;
    }
    crn_free_block(dds);

    const size_t nbytes = (size_t)desc.m_width * (size_t)desc.m_height * 4u;
    uint8_t* copy = reinterpret_cast<uint8_t*>(std::malloc(nbytes ? nbytes : 1));
    if (!copy) {
        crn_free_all_images(images, desc);
        return 5;
    }
    // Level 0, face 0. Each texel is a crn_uint32 with r,g,b,a in memory
    // order (r first), i.e. already the byte layout we want.
    std::memcpy(copy, images[0], nbytes);
    crn_free_all_images(images, desc);

    *out_rgba = copy;
    *out_w = desc.m_width;
    *out_h = desc.m_height;
    *out_levels = desc.m_levels;
    return 0;
}

extern "C" void crn_ffi_free_decoded(uint8_t* p) {
    std::free(p);
}
