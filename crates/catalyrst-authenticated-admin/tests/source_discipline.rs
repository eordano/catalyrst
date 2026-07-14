//! Properties of the admin extractor that the compiler cannot state, asserted against the
//! crate's own source text -- the same convention as
//! `catalyrst-server/tests/source_discipline.rs` and
//! `catalyrst-authenticated-principal/tests/source_discipline.rs`.
//!
//! [`AuthenticatedAdminIdentity`] is the proof token every adopting admin handler will
//! trust. Its safety property is an absence -- "there is no second mint, and no public field"
//! -- and an absence cannot be written as a failing unit test. It is checked here by scanning
//! the source: this test FAILS if a second constructor or a public field appears.
//!
//! This is a convention with a script attached, not a type guarantee. Say so when citing it.

use std::fs;
use std::path::{Path, PathBuf};

use regex::Regex;

const LIB: &str = include_str!("../src/lib.rs");

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
        !out.is_empty(),
        "the source walk found nothing; the scan would pass vacuously"
    );
    out
}

/// The scan must not be trivially satisfiable.
#[test]
fn the_scan_is_reading_real_sources() {
    assert!(
        LIB.len() > 200,
        "src/lib.rs looks empty or truncated; the source scan would pass vacuously"
    );
    assert!(LIB.contains("pub struct AuthenticatedAdminIdentity"));
    assert!(LIB.contains("fn from_request_parts"));
}

/// The field stays private: a `pub principal` would let any handler mint an
/// `AuthenticatedAdminIdentity` from a bare value and hand it to a gate, bypassing the bearer
/// verification that the `FromRequestParts` impl performs.
#[test]
fn the_admin_identity_field_is_not_public() {
    let start = LIB
        .find("pub struct AuthenticatedAdminIdentity {")
        .expect("AuthenticatedAdminIdentity declaration exists");
    let end = start + LIB[start..].find('}').expect("declaration closes");
    let declaration = &LIB[start..end];
    // Match a public field by shape rather than by a trailing comma: the last field of a
    // struct is written without one, and an `ends_with(",")` check would wave it through.
    let pub_field = Regex::new(r"^pub(\([^)]*\))?\s+[A-Za-z_]\w*\s*:").expect("valid regex");
    assert!(
        !code_lines(declaration).any(|line| pub_field.is_match(line)),
        "AuthenticatedAdminIdentity gained a public field; that is a second mint for the \
         admin proof token"
    );
    assert!(
        code_lines(declaration).any(|line| line == "principal: AuthenticatedPrincipal,"),
        "AuthenticatedAdminIdentity no longer holds its private principal field where this \
         scan expects it"
    );
}

/// The identity must not become deserializable or freely cloneable/defaultable: a
/// `Deserialize` would let a request body become an admin identity; a `Clone`/`Default`
/// widens how a value can come into existence.
#[test]
fn the_admin_identity_derives_nothing() {
    let lines: Vec<&str> = LIB.lines().map(str::trim).collect();
    let declaration = lines
        .iter()
        .position(|line| line.starts_with("pub struct AuthenticatedAdminIdentity"))
        .expect("AuthenticatedAdminIdentity declaration exists");
    // Collect every attribute line in the contiguous prelude above the struct, not just the
    // nearest one: a `#[derive(Deserialize)]` can hide behind a later `#[serde(...)]` or a
    // doc comment, and inspecting only the closest line would miss it.
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
        "AuthenticatedAdminIdentity gained a derive ({attrs:?}); Deserialize/Clone-style \
         derives widen how an admin identity can come into existence"
    );
}

/// P2 for the admin extractor: exactly one construction site, inside the `FromRequestParts`
/// impl in `src/lib.rs`. A second `AuthenticatedAdminIdentity {{ ... }}` anywhere in the crate
/// is a second mint -- this fails if one appears.
#[test]
fn admin_identity_is_constructed_only_in_from_request_parts() {
    let mut constructions: Vec<(PathBuf, String)> = Vec::new();
    for (path, source) in every_crate_source() {
        for line in code_lines(&source) {
            // A struct-literal construction is `AuthenticatedAdminIdentity {`. Exclude the
            // three other ways the name can sit immediately before a `{`: the struct
            // declaration, an `impl ... {` header, and a `-> AuthenticatedAdminIdentity {`
            // return type (where the `{` opens a fn body, not a literal).
            let constructs = (line.contains("AuthenticatedAdminIdentity {")
                || line.contains("AuthenticatedAdminIdentity{"))
                && !line.contains("struct AuthenticatedAdminIdentity")
                && !line.contains("-> AuthenticatedAdminIdentity")
                && !line.starts_with("impl");
            if constructs {
                constructions.push((path.clone(), line.to_string()));
            }
        }
    }

    assert_eq!(
        constructions.len(),
        1,
        "expected exactly one AuthenticatedAdminIdentity construction site, found: {constructions:?}"
    );
    let (path, line) = &constructions[0];
    assert!(
        path.ends_with("src/lib.rs"),
        "AuthenticatedAdminIdentity is constructed outside src/lib.rs: {path:?}"
    );
    assert_eq!(
        line, "Ok(AuthenticatedAdminIdentity {",
        "the construction site changed shape; verify it still sits on the success path \
         inside from_request_parts"
    );

    // The one construction must sit inside the FromRequestParts impl: between the start of
    // `fn from_request_parts` and the next 4-space-indented `fn ` after it.
    let start = LIB
        .find("fn from_request_parts")
        .expect("from_request_parts exists");
    let end = LIB[start + 1..]
        .find("\n    fn ")
        .map(|offset| start + 1 + offset)
        .unwrap_or(LIB.len());
    assert!(
        LIB[start..end].contains("Ok(AuthenticatedAdminIdentity {"),
        "the construction moved out of from_request_parts"
    );
}
