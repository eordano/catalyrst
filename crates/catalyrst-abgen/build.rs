//! Capture the abgen git commit at build time so generated manifests can carry
//! reproducible `<hash-of-inputs>+<commit>` provenance instead of a wall-clock
//! build date (see manifest::provenance). Falls back to "unknown" when built
//! outside a git checkout.
use std::process::Command;

fn main() {
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
    // Re-run when HEAD or the ref it points to moves.
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/refs");
}
