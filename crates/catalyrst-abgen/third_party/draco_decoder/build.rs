use std::path::Path;
use std::process::Command;

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
        println!("cargo:warning=Skipping build.rs on wasm32 target");
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
