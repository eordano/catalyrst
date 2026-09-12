//! Every `/dash/admin/*` route's handler must name the `TelemetryAdmin` extractor, asserted
//! against the crate's own source text.
//!
//! The compiler forces the bearer check *if* a handler names `TelemetryAdmin`, but it cannot
//! force a *new* admin route to declare the argument at all. This scan closes that residual
//! gap, the same species of guard as `catalyrst-server/tests/source_discipline.rs`. It is a
//! convention with a script attached, not a type guarantee -- say so when citing it.
//!
//! Deliberately not flagged: the `/dash` gated reads/writes are still guarded by the
//! `require_telemetry_admin` route-layer middleware, a router-construction gate rather than a
//! per-handler argument, so it is out of scope by design.

const LIB: &str = include_str!("../src/lib.rs");
const ADMIN: &str = include_str!("../src/handlers/admin.rs");

/// The handler fn names registered under `/dash/admin/`, read straight from `lib.rs`. For
/// every `"/dash/admin/` path literal we take the next `handlers::admin::<ident>` that follows
/// it -- there is exactly one per route, and nothing else sits between a route's path and its
/// handler.
fn admin_route_handlers(lib: &str) -> Vec<String> {
    const HANDLER_PREFIX: &str = "handlers::admin::";
    let mut names = Vec::new();
    let mut cursor = 0;
    while let Some(rel) = lib[cursor..].find("\"/dash/admin/") {
        let path_pos = cursor + rel;
        let after_path = &lib[path_pos..];
        let handler_rel = after_path
            .find(HANDLER_PREFIX)
            .expect("a /dash/admin/ route must name a handlers::admin:: fn");
        let ident_start = path_pos + handler_rel + HANDLER_PREFIX.len();
        let ident: String = lib[ident_start..]
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        assert!(
            !ident.is_empty(),
            "empty handler ident after a /dash/admin/ route"
        );
        names.push(ident);
        cursor = ident_start;
    }
    names
}

/// Balanced across nested parens (`Query(aq): Query<...>`, `Json(b): Json<...>`).
fn handler_signature<'a>(src: &'a str, name: &str) -> &'a str {
    let needle = format!("fn {name}(");
    let start = src
        .find(&needle)
        .unwrap_or_else(|| panic!("handler `{name}` is defined in handlers/admin.rs"));
    let open = start + needle.len() - 1;
    let bytes = src.as_bytes();
    let mut depth = 0usize;
    for (offset, byte) in bytes[open..].iter().enumerate() {
        match byte {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return &src[start..=open + offset];
                }
            }
            _ => {}
        }
    }
    panic!("unterminated parameter list for `{name}`");
}

#[test]
fn the_scan_is_reading_real_sources() {
    assert!(
        LIB.contains("/dash/admin/"),
        "lib.rs has no /dash/admin/ routes; the scan would pass vacuously"
    );
    assert!(
        ADMIN.contains("pub struct TelemetryAdmin"),
        "the TelemetryAdmin extractor is missing from handlers/admin.rs"
    );
    assert!(
        ADMIN.contains("fn from_request_parts"),
        "TelemetryAdmin has no FromRequestParts impl; it would be forgeable"
    );
}

#[test]
fn every_dash_admin_route_names_the_telemetry_admin_extractor() {
    let handlers = admin_route_handlers(LIB);
    assert!(
        handlers.len() >= 8,
        "expected at least the 8 known /dash/admin/* routes, found {handlers:?}"
    );
    for name in &handlers {
        let signature = handler_signature(ADMIN, name);
        assert!(
            signature.contains("TelemetryAdmin"),
            "admin handler `{name}` does not name the TelemetryAdmin extractor, so its gate is \
             a deletable line rather than a demanded type:\n{signature}"
        );
    }
}
