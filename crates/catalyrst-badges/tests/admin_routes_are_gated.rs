//! The badges pilot of the compile-forced admin gate (see `docs/auth-arc-plan.md` S4).
//!
//! The compiler forces the bearer check *if* an admin handler names the extractor in its
//! signature. It cannot force a *new* admin handler to declare it, and it cannot stop someone
//! re-introducing a hand-rolled `authorize_admin()` body call on a fresh route. This scan
//! closes that residual gap for badges. It is a convention with a script attached, not a type
//! guarantee -- the same species of guard the workspace already trusts in
//! `catalyrst-server/tests/source_discipline.rs` and
//! `catalyrst-authenticated-admin/tests/source_discipline.rs`. Cite it as such.

use std::fs;
use std::path::{Path, PathBuf};

fn rust_sources_under(dir: &Path, out: &mut Vec<(PathBuf, String)>) {
    for entry in fs::read_dir(dir).expect("directory is readable") {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            rust_sources_under(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            let source = fs::read_to_string(&path).expect("source file is readable");
            out.push((path, source));
        }
    }
}

fn every_src_file() -> Vec<(PathBuf, String)> {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut out = Vec::new();
    rust_sources_under(&src, &mut out);
    assert!(
        !out.is_empty(),
        "the source walk found nothing; every scan below would pass vacuously"
    );
    out
}

fn read_src(rel: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("src").join(rel);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("reading {}: {e}", path.display()))
}

/// Source lines with `//` comment and doc-comment lines removed -- the module docs quote the
/// forbidden constructs (`authorize_admin`, `RequireAdmin(`) on purpose and must not trip a
/// scan for them.
fn code_lines(source: &str) -> impl Iterator<Item = &str> {
    source
        .lines()
        .map(str::trim)
        .filter(|line| !line.starts_with("//"))
}

/// Return the balanced-paren argument list of `fn <name>(...)`, i.e. everything between the
/// `(` that follows the function name and its matching `)`. Handles the nested parens in
/// `State<AppState>` / `Path<(String, String)>`.
fn signature_args(source: &str, fn_name: &str) -> String {
    let needle = format!("fn {fn_name}(");
    let start = source
        .find(&needle)
        .unwrap_or_else(|| panic!("{fn_name} is defined in the scanned source"));
    let open = start + needle.len() - 1;
    let mut depth = 0usize;
    for (offset, ch) in source[open..].char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return source[open + 1..open + offset].to_string();
                }
            }
            _ => {}
        }
    }
    panic!("unbalanced parens in {fn_name} signature");
}

/// (1) The old forgeable gate is gone: no `src/` file defines a hand-rolled admin-gate
/// function. If one comes back, this fails -- a body-call gate is exactly what the extractor
/// replaces.
#[test]
fn no_hand_rolled_admin_gate_function_remains() {
    let banned = [
        "fn authorize_admin",
        "fn require_admin",
        "fn check_admin",
        "fn authorize(",
        "fn timing_safe_eq",
    ];
    let mut offenders: Vec<String> = Vec::new();
    for (path, source) in every_src_file() {
        for line in code_lines(&source) {
            for marker in banned {
                if line.contains(marker) {
                    offenders.push(format!("{}: {line}", path.display()));
                }
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "a hand-rolled admin-gate function reappeared; the gate must be the RequireAdmin \
         extractor, not a deletable body call: {offenders:?}"
    );
}

/// (2) Both badges mutation handlers name the admin extractor in their signature. axum then
/// refuses to register either route unless `RequireAdmin` resolves as an extractor, and
/// `RequireAdmin` only resolves by running the bearer verification.
#[test]
fn both_mutation_handlers_take_the_admin_extractor() {
    let handlers = read_src("handlers/badges.rs");
    for handler in ["grant_user_badge", "revoke_user_badge"] {
        let args = signature_args(&handlers, handler);
        assert!(
            args.contains("RequireAdmin"),
            "{handler} no longer takes the RequireAdmin extractor; its admin gate is not \
             compiler-forced. Signature args were: {args}"
        );
    }
}

/// (3) `RequireAdmin` is an unforgeable proof token: exactly one construction site in the
/// crate, in `src/admin.rs`, and that file delegates to the shared, verified
/// `AuthenticatedAdminIdentity` mint. A second construction -- or a public inner field -- would
/// be a second mint that skips verification.
#[test]
fn require_admin_is_minted_only_via_the_shared_verified_extractor() {
    let mut constructions: Vec<(PathBuf, String)> = Vec::new();
    for (path, source) in every_src_file() {
        for line in code_lines(&source) {
            let constructs = line.contains("RequireAdmin(")
                && !line.contains("struct RequireAdmin(")
                && !line.starts_with("impl");
            if constructs {
                constructions.push((path.clone(), line.to_string()));
            }
        }
    }
    assert_eq!(
        constructions.len(),
        1,
        "expected exactly one RequireAdmin construction site, found: {constructions:?}"
    );
    assert!(
        constructions[0].0.ends_with("admin.rs"),
        "RequireAdmin is constructed outside src/admin.rs: {:?}",
        constructions[0].0
    );

    let admin = read_src("admin.rs");
    assert!(
        admin.contains("AuthenticatedAdminIdentity::from_request_parts"),
        "src/admin.rs no longer delegates to the shared verified extractor; RequireAdmin \
         could be minted without the bearer check"
    );
    // The inner field stays private (a bare tuple field, no `pub`): a `pub` field would let a
    // sibling module or downstream crate mint a RequireAdmin from an unverified value.
    assert!(
        admin.contains("pub struct RequireAdmin(AuthenticatedAdminIdentity);"),
        "RequireAdmin's inner identity field changed shape or gained visibility; it must stay \
         a private single field wrapping the verified AuthenticatedAdminIdentity"
    );
}
