use std::path::{Path, PathBuf};
use std::process::Command;

// Decoder-only wasm build: no cmake, no cxx — compile the draco decoder
// sources plus the plain-C bridge (cpp/decoder_api_c.cc) with the cc crate.
// Skips encoder/io/tool/test translation units by filename.
fn wasm_skip(path: &Path) -> bool {
    let p = path.to_string_lossy().replace('\\', "/");
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    for dir in [
        "/io/",
        "/tools/",
        "/javascript/",
        "/maya/",
        "/unity/",
        "/animation/",
        "/material/",
        "/scene/",
        "/texture/",
    ] {
        if p.contains(dir) {
            return true;
        }
    }
    name.contains("encod")
        || name.contains("_test")
        || name.contains("test_")
        || name.contains("transcod")
}

fn wasm_collect_cc(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("read draco src dir") {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            wasm_collect_cc(&path, out);
        } else if path.extension().is_some_and(|e| e == "cc") && !wasm_skip(&path) {
            out.push(path);
        }
    }
}

fn build_wasm_decoder() {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let out_dir = std::env::var("OUT_DIR").unwrap();

    let features_dir = Path::new(&out_dir).join("draco");
    std::fs::create_dir_all(&features_dir).expect("create draco features dir");
    std::fs::write(
        features_dir.join("draco_features.h"),
        "#ifndef DRACO_FEATURES_H_\n#define DRACO_FEATURES_H_\n\
         #define DRACO_MESH_COMPRESSION_SUPPORTED\n\
         #define DRACO_NORMAL_ENCODING_SUPPORTED\n\
         #define DRACO_STANDARD_EDGEBREAKER_SUPPORTED\n\
         #define DRACO_PREDICTIVE_EDGEBREAKER_SUPPORTED\n\
         #define DRACO_POINT_CLOUD_COMPRESSION_SUPPORTED\n\
         #define DRACO_BACKWARDS_COMPATIBILITY_SUPPORTED\n\
         #endif\n",
    )
    .expect("write draco_features.h");

    let draco_src = Path::new(&manifest_dir).join("third_party/draco/src/draco");
    let mut sources = Vec::new();
    wasm_collect_cc(&draco_src, &mut sources);
    sources.sort();

    let mut build = cc::Build::new();
    build
        .cpp(true)
        .cpp_link_stdlib(None::<&str>)
        .include(format!("{manifest_dir}/third_party/draco/src"))
        .include(&out_dir)
        .flag("-std=c++17")
        .flag("-fno-exceptions")
        .flag("-fno-rtti")
        .warnings(false);
    for s in &sources {
        build.file(s);
    }
    build.file("cpp/decoder_api_c.cc");
    println!("cargo:rerun-if-changed=cpp/decoder_api_c.cc");
    println!("cargo:rerun-if-changed=src/ffi_c.rs");

    build.compile("draco_decoder_c");
}

fn run_cmake_command(args: &[&str], current_dir: &String, stage: &str) {
    let status = Command::new("cmake")
        .args(args)
        .current_dir(current_dir)
        .status()
        .unwrap_or_else(|_| panic!("Failed to execute cmake for {stage}"));
    assert!(status.success(), "Draco {stage} failed");
}

fn main() {
    if std::env::var("DOCS_RS").is_ok() {
        println!("cargo:warning=Skipping native build on docs.rs");
        return;
    }

    if std::env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default() == "wasm32" {
        build_wasm_decoder();
        return;
    }

    let target = std::env::var("TARGET").unwrap();
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let out_dir = std::env::var("OUT_DIR").unwrap();

    let draco_src = format!("{manifest_dir}/third_party/draco");
    let draco_build = format!("{out_dir}/draco-build");
    let draco_install = if target.contains("windows-msvc") {
        format!("{draco_build}/Release")
    } else {
        format!("{draco_build}/install")
    };

    if !Path::new(&draco_build).exists() {
        std::fs::create_dir_all(&draco_build).unwrap();
    }

    let status = Command::new("cmake")
        .args([
            &draco_src,
            "-DBUILD_SHARED_LIBS=OFF",
            "-DCMAKE_BUILD_TYPE=Release",
            "-DDRACO_TESTS=OFF",
            &format!("-DCMAKE_INSTALL_PREFIX={draco_build}/install"),
        ])
        .current_dir(&draco_build)
        .status()
        .expect("Failed to run CMake");
    assert!(status.success(), "CMake configuration failed");

    let is_apple = target.contains("apple-darwin");
    let build_args = if target.contains("windows-msvc") {
        vec!["--build", ".", "--config", "Release"]
    } else if is_apple {
        vec!["--build", ".", "--target", "draco_static"]
    } else {
        vec!["--build", "."]
    };
    run_cmake_command(&build_args, &draco_build, "build");
    if !is_apple {
        let install_args = if target.contains("windows-msvc") {
            vec!["--install", ".", "--config", "Release"]
        } else {
            vec!["--install", "."]
        };
        run_cmake_command(&install_args, &draco_build, "install");
    }

    let mut build = cxx_build::bridge("src/ffi.rs");
    build
        .file("cpp/decoder_api.cc")
        .include("include")
        .include("third_party/draco/src")
        .include(&draco_build)
        .include(format!("{draco_install}/include"))
        .flag_if_supported("-std=c++17")
        .warnings(false);

    if is_apple {
        build.flag("-mmacosx-version-min=15.5");
    }

    build.compile("decoder_api");

    if target.contains("windows-msvc") {
        println!("cargo:rustc-link-search=native={draco_install}");
    } else if is_apple {
        println!("cargo:rustc-link-search=native={draco_build}");
    } else {
        let lib64 = format!("{draco_install}/lib64");
        let lib_dir = if Path::new(&lib64).exists() {
            lib64
        } else {
            format!("{draco_install}/lib")
        };
        println!("cargo:rustc-link-search=native={lib_dir}");
    }
    println!("cargo:rustc-link-lib=static=draco");

    println!("cargo:rerun-if-changed=cpp/decoder_api.cc");
    println!("cargo:rerun-if-changed=include/decoder_api.h");
    println!("cargo:rerun-if-changed=src/ffi.rs");
}
