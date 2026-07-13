use std::path::Path;

fn main() {
    let crnlib = Path::new("crnlib");
    let inc = Path::new("inc");
    let wrapper = Path::new("cpp/crn_wrapper.cc");

    let crn_sources: &[&str] = &[
        "crn_arealist.cpp",
        "crn_assert.cpp",
        "crn_checksum.cpp",
        "crn_colorized_console.cpp",
        "crn_command_line_params.cpp",
        "crn_comp.cpp",
        "crn_console.cpp",
        "crn_core.cpp",
        "crn_data_stream.cpp",
        "crn_mipmapped_texture.cpp",
        "crn_decomp.cpp",
        "crn_dxt1.cpp",
        "crn_dxt5a.cpp",
        "crn_dxt.cpp",
        "crn_dxt_endpoint_refiner.cpp",
        "crn_dxt_fast.cpp",
        "crn_dxt_hc_common.cpp",
        "crn_dxt_hc.cpp",
        "crn_dxt_image.cpp",
        "crn_dynamic_string.cpp",
        "crn_file_utils.cpp",
        "crn_find_files.cpp",
        "crn_hash.cpp",
        "crn_hash_map.cpp",
        "crn_huffman_codes.cpp",
        "crn_image_utils.cpp",
        "crnlib.cpp",
        "crn_math.cpp",
        "crn_mem.cpp",
        "crn_pixel_format.cpp",
        "crn_platform.cpp",
        "crn_prefix_coding.cpp",
        "crn_qdxt1.cpp",
        "crn_qdxt5.cpp",
        "crn_rand.cpp",
        "crn_resample_filters.cpp",
        "crn_resampler.cpp",
        "crn_ryg_dxt.cpp",
        "crn_sparse_bit_array.cpp",
        "crn_stb_image.cpp",
        "crn_strutils.cpp",
        "crn_symbol_codec.cpp",
        "crn_texture_file_types.cpp",
        "crn_threaded_resampler.cpp",
        "crn_timer.cpp",
        "crn_utils.cpp",
        "crn_value.cpp",
        "crn_vector.cpp",
        "crn_texture_comp.cpp",
        "crn_texture_conversion.cpp",
        "crn_dds_comp.cpp",
        "crn_lzma_codec.cpp",
        "crn_ktx_texture.cpp",
        "crn_etc.cpp",
        "crn_rg_etc1.cpp",
        "crn_miniz.cpp",
        "crn_jpge.cpp",
        "crn_jpgd.cpp",
        "lzma_7zBuf2.cpp",
        "lzma_7zBuf.cpp",
        "lzma_7zCrc.cpp",
        "lzma_7zFile.cpp",
        "lzma_7zStream.cpp",
        "lzma_Alloc.cpp",
        "lzma_Bcj2.cpp",
        "lzma_Bra86.cpp",
        "lzma_Bra.cpp",
        "lzma_BraIA64.cpp",
        "lzma_LzFind.cpp",
        "lzma_LzmaDec.cpp",
        "lzma_LzmaEnc.cpp",
        "lzma_LzmaLib.cpp",
    ];

    let mut build = cc::Build::new();
    if std::env::var("CARGO_CFG_WINDOWS").is_ok() {
        build.define("WIN32", None);
        build.flag_if_supported("/EHsc");
    }
    build
        .cpp(true)
        .include(crnlib)
        .include(inc)
        .flag_if_supported("-std=c++14")
        .flag_if_supported("-fno-strict-aliasing")
        .flag_if_supported("-fno-math-errno")
        .flag_if_supported("-fomit-frame-pointer")
        .flag_if_supported("-Wno-unused-value")
        .flag_if_supported("-Wno-unused-but-set-variable")
        .flag_if_supported("-Wno-unused-variable")
        .flag_if_supported("-Wno-unused-function")
        .flag_if_supported("-Wno-unused-parameter")
        .flag_if_supported("-Wno-sign-compare")
        .flag_if_supported("-Wno-narrowing")
        .flag_if_supported("-Wno-c++11-narrowing")
        .flag_if_supported("-Wno-class-memaccess")
        .flag_if_supported("-Wno-deprecated-declarations")
        .flag_if_supported("-Wno-misleading-indentation")
        .flag_if_supported("-Wno-implicit-fallthrough")
        .flag_if_supported("-Wno-parentheses")
        .flag_if_supported("-Wno-reorder")
        .flag_if_supported("-Wno-strict-aliasing")
        .flag_if_supported("-Wno-write-strings")
        .flag_if_supported("-Wno-format-truncation")
        .flag_if_supported("-Wno-format-overflow")
        .flag_if_supported("-Wno-stringop-truncation")
        .flag_if_supported("-Wno-stringop-overflow")
        .flag_if_supported("-Wno-array-bounds")
        .flag_if_supported("-Wno-maybe-uninitialized")
        .flag_if_supported("-Wno-shift-negative-value")
        .flag_if_supported("-Wno-uninitialized")
        .flag_if_supported("-Wno-nonnull")
        .warnings(false);

    let is_wasm = std::env::var("CARGO_CFG_TARGET_ARCH").as_deref() == Ok("wasm32");
    if is_wasm {
        build.flag("-fno-exceptions");
        build.flag("-fno-rtti");
        build.cpp_link_stdlib(None::<&str>);
    }

    let threading_src = if std::env::var("CARGO_CFG_WINDOWS").is_ok() {
        Some("crn_threading_win32.cpp")
    } else if is_wasm {
        None
    } else {
        Some("crn_threading_pthreads.cpp")
    };

    for src in crn_sources.iter().chain(threading_src.iter()) {
        build.file(crnlib.join(src));
        println!("cargo:rerun-if-changed=crnlib/{src}");
    }

    build.file(wrapper);
    println!("cargo:rerun-if-changed=cpp/crn_wrapper.cc");
    println!("cargo:rerun-if-changed=cpp/crn_wrapper.h");

    build.compile("crnlib_combined");

    if std::env::var("CARGO_CFG_WINDOWS").is_err() && !is_wasm {
        println!("cargo:rustc-link-lib=pthread");
    }
}
