// crn_wrapper.h — C ABI over BinomialLLC/crunch's `crn_compress` entry point.
// Public domain; mirrors crnlib's license.txt.
//
// The wrapper takes a flat RGBA32 mip-chain (mip 0 first) plus per-mip
// dimensions. Each mip's pixel data is `mip_w[i] * mip_h[i] * 4` bytes,
// laid out top-down, row-major, RGBA byte order. Output is a malloc'd
// buffer holding the .CRN file bytes; the caller must release it via
// `crn_ffi_free`.
//
// Returns 0 on success, non-zero on failure (output pointer is left
// untouched on failure).

#ifndef CRN_WRAPPER_H
#define CRN_WRAPPER_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

// Format selector — currently only DXN_XY (BC5 / 3DC) is exposed; widen
// later if we need DXT1/DXT5/etc.
enum crn_ffi_format {
    CRN_FFI_FMT_DXN_XY = 0,
};

// Compresses `mip_count` mip levels of RGBA32 data into a single .CRN
// stream using crnlib's `crn_compress`. `mip_rgba` is the concatenation
// of all mip-level pixel buffers (mip 0 first). `mip_w` / `mip_h` carry
// the per-mip dimensions. `quality_level` is in [0, 255] (crnlib's
// `cCRNMinQualityLevel` .. `cCRNMaxQualityLevel`); pass 255 for the
// upstream default. `num_helper_threads` is forwarded to
// `crn_comp_params::m_num_helper_threads`; pass 0 to disable threading.
//
// On success returns 0 and writes the .CRN bytes pointer + size to
// `*out_data` / `*out_size`. The pointer is malloc'd by crnlib and
// must be released with `crn_ffi_free`.
int crn_ffi_compress_rgba(
    enum crn_ffi_format fmt,
    const uint8_t* mip_rgba,
    size_t mip_rgba_len,
    const uint32_t* mip_w,
    const uint32_t* mip_h,
    uint32_t mip_count,
    uint32_t quality_level,
    uint32_t num_helper_threads,
    uint8_t** out_data,
    uint32_t* out_size);

// Releases a buffer returned by `crn_ffi_compress_rgba`.
void crn_ffi_free(uint8_t* p);

#ifdef __cplusplus
} // extern "C"
#endif

#endif // CRN_WRAPPER_H
