//! Properties of the admin-console auth that the compiler cannot state, asserted against
//! the crate's own source text -- the same convention as
//! `catalyrst-authenticated-principal/tests/source_discipline.rs`.
//!
//! `AdminSession` is the proof token that ~60 admin handlers trust. Its safety property is
//! an absence -- "there is no second mint" -- and an absence cannot be written as a failing
//! unit test. It is checked here by scanning the sources.
//!
//! This is a convention with a script attached, not a type guarantee. Say so when citing it.

use std::fs;
use std::path::{Path, PathBuf};

use regex::Regex;

const AUTH: &str = include_str!("../src/admin/auth.rs");

/// Source lines with comment and doc-comment lines removed -- the documentation quotes the
/// forbidden constructs on purpose, and must not trip a scan for them.
fn code_lines(source: &str) -> impl Iterator<Item = &str> {
    source
        .lines()
        .map(str::trim)
        .filter(|line| !line.starts_with("//") && !line.starts_with("/*") && !line.starts_with('*'))
}

fn rust_sources_under(dir: &Path, out: &mut Vec<(PathBuf, String)>) {
    for entry in fs::read_dir(dir).expect("src/ is readable") {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            rust_sources_under(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            let source = fs::read_to_string(&path).expect("source file is readable");
            out.push((path, source));
        }
    }
}

fn every_crate_source() -> Vec<(PathBuf, String)> {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut out = Vec::new();
    rust_sources_under(&src, &mut out);
    assert!(
        out.len() > 10,
        "the source walk found almost nothing; the scan would pass vacuously"
    );
    out
}

/// The scan must not be trivially satisfiable.
#[test]
fn the_scan_is_reading_real_sources() {
    assert!(
        AUTH.len() > 200,
        "src/admin/auth.rs looks empty or truncated; the source scan would pass vacuously"
    );
    assert!(AUTH.contains("pub struct AdminSession"));
    assert!(AUTH.contains("fn from_request_parts"));
}

/// The field stays private: a `pub address` would let any code in this crate mint an
/// `AdminSession` from a bare string, bypassing the SIWE + HMAC + allowlist check that
/// every admin handler trusts.
#[test]
fn the_admin_session_field_is_not_public() {
    let start = AUTH
        .find("pub struct AdminSession {")
        .expect("AdminSession declaration exists");
    let end = start + AUTH[start..].find('}').expect("declaration closes");
    let declaration = &AUTH[start..end];
    // Match a public field by shape rather than by a trailing comma: the last
    // field of a struct is written without one (`pub address: String`), and a
    // `ends_with(",")` check would wave it straight through.
    let pub_field = Regex::new(r"^pub(\([^)]*\))?\s+[A-Za-z_]\w*\s*:").expect("valid regex");
    assert!(
        !code_lines(declaration).any(|line| pub_field.is_match(line)),
        "AdminSession gained a public field; that is a second mint for the admin proof token"
    );
    assert!(
        code_lines(declaration).any(|line| line == "address: String,"),
        "AdminSession no longer holds its private address field where this scan expects it"
    );
}

/// The session type must not become deserializable: a `Deserialize` on it would let a
/// request body become an admin identity.
#[test]
fn the_admin_session_derives_nothing() {
    let lines: Vec<&str> = AUTH.lines().map(str::trim).collect();
    let declaration = lines
        .iter()
        .position(|line| line.starts_with("pub struct AdminSession"))
        .expect("AdminSession declaration exists");
    // Collect every attribute line in the contiguous prelude above the struct,
    // not just the nearest one: a `#[derive(Deserialize)]` can hide behind a
    // later `#[serde(...)]` or a doc comment sitting between it and the struct,
    // and inspecting only the closest line would miss it.
    let mut attrs: Vec<&str> = Vec::new();
    for line in lines[..declaration].iter().rev() {
        if line.is_empty() || line.starts_with("//") || line.starts_with("///") {
            continue;
        }
        if line.starts_with("#[") {
            attrs.push(*line);
            continue;
        }
        break;
    }
    assert!(
        !attrs.iter().any(|line| line.starts_with("#[derive")),
        "AdminSession gained a derive ({attrs:?}); Deserialize/Clone-style derives widen \
         how a session value can come into existence"
    );
}

/// P2 for the admin console: exactly one construction site, inside the `FromRequestParts`
/// impl in `src/admin/auth.rs`. A second `AdminSession {{ ... }}` anywhere in the crate is a
/// second mint.
#[test]
fn admin_session_is_constructed_only_in_from_request_parts() {
    let mut constructions: Vec<(PathBuf, String)> = Vec::new();
    for (path, source) in every_crate_source() {
        for line in code_lines(&source) {
            let constructs = (line.contains("AdminSession {") || line.contains("AdminSession{"))
                && !line.contains("struct AdminSession")
                && !line.starts_with("impl");
            if constructs {
                constructions.push((path.clone(), line.to_string()));
            }
        }
    }

    assert_eq!(
        constructions.len(),
        1,
        "expected exactly one AdminSession construction site, found: {constructions:?}"
    );
    let (path, line) = &constructions[0];
    assert!(
        path.ends_with("src/admin/auth.rs"),
        "AdminSession is constructed outside src/admin/auth.rs: {path:?}"
    );
    assert_eq!(
        line, "Some(address) => Ok(AdminSession { address }),",
        "the construction site changed shape; verify it still sits on the \
         session::verify success path inside from_request_parts"
    );

    // The one construction must sit inside the FromRequestParts impl: between the start of
    // `fn from_request_parts` and the next `fn ` after it.
    let start = AUTH
        .find("fn from_request_parts")
        .expect("from_request_parts exists");
    let end = AUTH[start + 1..]
        .find("\n    fn ")
        .map(|offset| start + 1 + offset)
        .unwrap_or(AUTH.len());
    assert!(
        AUTH[start..end].contains("Ok(AdminSession { address })"),
        "the construction moved out of from_request_parts"
    );
}
