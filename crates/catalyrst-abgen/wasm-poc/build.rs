fn main() {
    if std::env::var("CARGO_CFG_TARGET_ARCH").as_deref() != Ok("wasm32") {
        return;
    }
    let libc_dir = std::env::var("WASI_LIBC_LIB")
        .expect("WASI_LIBC_LIB not set — build inside wasm-poc/toolchain's devShell");
    let libcxx_dir = std::env::var("WASI_LIBCXX_LIB")
        .expect("WASI_LIBCXX_LIB not set — build inside wasm-poc/toolchain's devShell");
    println!("cargo:rustc-link-search=native={libc_dir}");
    println!("cargo:rustc-link-search=native={libcxx_dir}");
    println!("cargo:rustc-link-lib=static=setjmp");
    println!("cargo:rustc-link-lib=static=c++");
    println!("cargo:rustc-link-lib=static=c++abi");
    println!("cargo:rustc-link-lib=static=c");
}
