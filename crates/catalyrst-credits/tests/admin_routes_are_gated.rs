//! The credits leg of the compile-forced admin gate (see `docs/auth-arc-plan.md` S4).
//!
//! The compiler forces the bearer check *if* an admin handler names the `RequireAdmin` extractor
//! in its signature. It cannot force a *new* admin route to declare it, and it cannot stop
//! someone re-introducing a hand-rolled `authorize_admin()` body call on a fresh handler. This
//! scan closes that residual gap for credits, whose admin surface is a single sub-router
//! (`handlers/admin/`): it reads every handler the router registers and asserts each one takes
//! the extractor. It is a convention with a script attached, not a type guarantee -- the same
//! species of guard the workspace already trusts in
//! `catalyrst-server/tests/source_discipline.rs`,
//! `catalyrst-authenticated-admin/tests/source_discipline.rs`, and the badges pilot's
//! `admin_routes_are_gated.rs`. Cite it as such.

use std::fs;
use std::path::{Path, PathBuf};

fn read_src(rel: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("src").join(rel);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("reading {}: {e}", path.display()))
}

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

/// Source lines with `//`/`///` comment lines removed -- the module docs quote the forbidden
/// constructs (`authorize_admin`, `RequireAdmin(`) on purpose and must not trip a scan for them.
fn code_lines(source: &str) -> impl Iterator<Item = &str> {
    source
        .lines()
        .map(str::trim)
        .filter(|line| !line.starts_with("//"))
}

/// The handler identifiers the admin router registers, extracted from the `get/post/put/delete`
/// routing combinators in `handlers/admin/mod.rs`. Deriving them from the router (rather than
/// hard-coding a list) is the whole point: a newly added route is scanned automatically.
fn registered_handlers(router_src: &str) -> Vec<String> {
    let bytes = router_src.as_bytes();
    let mut names = Vec::new();
    for method in ["get", "post", "put", "delete"] {
        let needle = format!("{method}(");
        let mut from = 0usize;
        while let Some(rel) = router_src[from..].find(&needle) {
            let idx = from + rel;
            from = idx + needle.len();
            // Require a word boundary before the method token so `target(` / `delete_pack`
            // never masquerade as a routing combinator.
            if idx > 0 {
                let prev = bytes[idx - 1];
                if prev.is_ascii_alphanumeric() || prev == b'_' {
                    continue;
                }
            }
            let ident: String = router_src[from..]
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect();
            if !ident.is_empty() {
                names.push(ident);
            }
        }
    }
    names.sort();
    names.dedup();
    names
}

/// Return the balanced-paren argument list of `fn <name>(...)`, i.e. everything between the `(`
/// that follows the function name and its matching `)`. Handles the nested parens in
/// `State<AppState>` / `Option<Json<GrantBody>>`. `None` if the function is not defined here.
fn signature_args(source: &str, fn_name: &str) -> Option<String> {
    let needle = format!("fn {fn_name}(");
    let start = source.find(&needle)?;
    let open = start + needle.len() - 1;
    let mut depth = 0usize;
    for (offset, ch) in source[open..].char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(source[open + 1..open + offset].to_string());
                }
            }
            _ => {}
        }
    }
    panic!("unbalanced parens in {fn_name} signature");
}

/// (1) The old forgeable gate is gone: no `src/` file defines a hand-rolled admin-gate function.
/// If one comes back, this fails -- a body-call gate is exactly what the extractor replaces.
#[test]
fn no_hand_rolled_admin_gate_function_remains() {
    let banned = [
        "fn authorize_admin",
        "fn authorize_with_token",
        "fn bearer_token",
        "fn timing_safe_eq",
        "fn require_admin",
        "fn check_admin",
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

/// (2) Every handler the admin router registers names the `RequireAdmin` extractor in its
/// signature. axum then refuses to register the route unless `RequireAdmin` resolves as an
/// extractor, and `RequireAdmin` only resolves by running the shared bearer verification.
#[test]
fn every_registered_admin_handler_takes_the_admin_extractor() {
    let router = read_src("handlers/admin/mod.rs");
    let ops = read_src("handlers/admin/ops.rs");
    let catalog = read_src("handlers/admin/catalog.rs");

    let handlers = registered_handlers(&router);
    assert!(
        handlers.len() >= 10,
        "the admin router scan found only {handlers:?}; the parse likely broke and the check \
         below would be near-vacuous"
    );

    for name in handlers {
        let args = signature_args(&ops, &name)
            .or_else(|| signature_args(&catalog, &name))
            .unwrap_or_else(|| {
                panic!("admin route handler `{name}` is registered but not defined in ops.rs/catalog.rs")
            });
        assert!(
            args.contains("RequireAdmin"),
            "admin route handler `{name}` does not take the RequireAdmin extractor; its admin \
             gate is not compiler-forced. Signature args were: {args}"
        );
    }
}

/// (3) `RequireAdmin` is an unforgeable proof token: exactly one construction site in the crate,
/// in `handlers/admin/common.rs`, and that file delegates to the shared, verified
/// `AuthenticatedAdminIdentity` mint. A second construction -- or a public inner field -- would be
/// a second mint that skips verification.
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
        constructions[0].0.ends_with("common.rs"),
        "RequireAdmin is constructed outside handlers/admin/common.rs: {:?}",
        constructions[0].0
    );

    let common = read_src("handlers/admin/common.rs");
    assert!(
        common.contains("AuthenticatedAdminIdentity::from_request_parts"),
        "handlers/admin/common.rs no longer delegates to the shared verified extractor; \
         RequireAdmin could be minted without the bearer check"
    );
    // The inner field stays private (a bare tuple field, no `pub`): a `pub` field would let a
    // sibling module or downstream crate mint a RequireAdmin from an unverified value.
    assert!(
        common.contains("pub(crate) struct RequireAdmin(AuthenticatedAdminIdentity);"),
        "RequireAdmin's inner identity field changed shape or gained visibility; it must stay a \
         private single field wrapping the verified AuthenticatedAdminIdentity"
    );
}
