use std::process::Command;

fn main() {
    println!("cargo:rerun-if-env-changed=ABGEN_TURBOJPEG_LIB");

    println!("cargo:rustc-check-cfg=cfg(abgen_static_turbojpeg)");
    println!("cargo:rerun-if-env-changed=ABGEN_TURBOJPEG_STATIC_DIR");
    if let Ok(dir) = std::env::var("ABGEN_TURBOJPEG_STATIC_DIR") {
        if !dir.is_empty() {
            println!("cargo:rustc-link-search=native={dir}");
            println!("cargo:rustc-link-lib=static=turbojpeg_iso");
            println!("cargo:rustc-cfg=abgen_static_turbojpeg");
        }
    }

    let base = Command::new("git")
        .args(["rev-parse", "--short=12", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".to_string());

    let dirty = Command::new("git")
        .args(["status", "--porcelain", "--untracked-files=no"])
        .output()
        .ok()
        .map(|o| !o.stdout.is_empty())
        .unwrap_or(false);

    let commit = if dirty { format!("{base}-dirty") } else { base };
    println!("cargo:rustc-env=ABGEN_GIT_COMMIT={commit}");

    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/refs");
}
