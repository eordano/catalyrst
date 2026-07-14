//! Properties of this crate that the compiler cannot state, asserted against the crate's
//! own source text.
//!
//! Two of the safety properties in the design are absences -- "there is no second mint",
//! "nothing here is serializable" -- and an absence cannot be written as a failing unit
//! test. They are checked here by scanning the sources, which are pulled in with
//! `include_str!` so the test needs no working directory, no shell and no CI wiring: it
//! runs wherever `cargo test -p catalyrst-authenticated-principal` runs.
//!
//! This is a convention with a script attached, not a type guarantee. Say so when citing it.

const CARGO_TOML: &str = include_str!("../Cargo.toml");
const LIB: &str = include_str!("../src/lib.rs");
const CLAIMED: &str = include_str!("../src/claimed.rs");
const OPERATOR_CONFIGURED_ALLOWLIST: &str = include_str!("../src/operator_configured_allowlist.rs");
const PLATFORM_SERVICE_IDENTITY: &str = include_str!("../src/platform_service_identity.rs");
const PRINCIPAL: &str = include_str!("../src/principal.rs");
const REFUSAL: &str = include_str!("../src/refusal.rs");
const VERIFIED_WALLET_ADDRESS: &str = include_str!("../src/verified_wallet_address.rs");
const VERIFIER_REGISTRY: &str = include_str!("../src/verifier_registry.rs");

fn every_source_file() -> Vec<(&'static str, &'static str)> {
    vec![
        ("src/lib.rs", LIB),
        ("src/claimed.rs", CLAIMED),
        (
            "src/operator_configured_allowlist.rs",
            OPERATOR_CONFIGURED_ALLOWLIST,
        ),
        (
            "src/platform_service_identity.rs",
            PLATFORM_SERVICE_IDENTITY,
        ),
        ("src/principal.rs", PRINCIPAL),
        ("src/refusal.rs", REFUSAL),
        ("src/verified_wallet_address.rs", VERIFIED_WALLET_ADDRESS),
        ("src/verifier_registry.rs", VERIFIER_REGISTRY),
    ]
}

/// Source lines with comment and doc-comment lines removed -- this crate's documentation
/// quotes the forbidden constructs on purpose, and must not trip a scan for them.
fn code_lines(source: &str) -> impl Iterator<Item = &str> {
    source
        .lines()
        .map(str::trim)
        .filter(|line| !line.starts_with("//") && !line.starts_with("/*") && !line.starts_with('*'))
}

/// The manifest with its comments removed, for the same reason: the header comment names
/// every dependency the crate forbids.
fn manifest_without_comments() -> String {
    CARGO_TOML
        .lines()
        .map(str::trim)
        .filter(|line| !line.starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The scan must not be trivially satisfiable: if `include_str!` ever picked up an empty
/// or wrong file, every "contains no X" assertion below would pass vacuously.
#[test]
fn the_scan_is_reading_real_sources() {
    for (name, source) in every_source_file() {
        assert!(
            source.len() > 200,
            "{name} looks empty or truncated; the source scan would pass vacuously"
        );
    }
    assert!(VERIFIED_WALLET_ADDRESS.contains("pub struct VerifiedWalletAddress"));
    assert!(CLAIMED.contains("pub struct ClaimedWalletAddressNobodyHasVerified"));
}

/// P2: no second mint appears.
///
/// The chokepoint argument rests on `VerifiedWalletAddress` having exactly
/// one public constructor, taking an already-unforgeable `Signer` by value. A second
/// `pub fn ... -> Self` in that file would widen it silently.
#[test]
fn the_verified_wallet_address_has_exactly_one_public_constructor() {
    let public_constructors: Vec<&str> = code_lines(VERIFIED_WALLET_ADDRESS)
        .filter(|line| line.starts_with("pub fn ") && line.contains("-> Self"))
        .collect();

    assert_eq!(
        public_constructors,
        vec!["pub fn from_verified_signed_fetch(signer: Signer) -> Self {"],
        "the set of public constructors of VerifiedWalletAddress changed. \
         Adding one widens the chokepoint that every authorization decision downstream \
         rests on; see the crate docs before touching this."
    );
}

/// The one non-public constructor is the `#[cfg(test)]` hatch, it is `pub(crate)`, and it
/// really is behind `#[cfg(test)]` with no cargo feature attached.
#[test]
fn the_only_other_constructor_is_the_cfg_test_hatch() {
    let lines: Vec<&str> = VERIFIED_WALLET_ADDRESS.lines().map(str::trim).collect();
    let constructor_positions: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter(|(_, line)| {
            !line.starts_with("//") && line.contains("fn ") && line.contains("-> Self")
        })
        .map(|(index, _)| index)
        .collect();

    assert_eq!(
        constructor_positions.len(),
        2,
        "expected exactly two constructors: the mint and the cfg(test) hatch"
    );

    let hatch = constructor_positions[1];
    assert!(
        lines[hatch].starts_with("pub(crate) fn unchecked_for_unit_tests_within_this_crate_only"),
        "unexpected second constructor: {}",
        lines[hatch]
    );
    assert_eq!(
        lines[hatch - 1],
        "#[cfg(test)]",
        "the test hatch must sit directly behind #[cfg(test)], with no cargo feature: a \
         feature would be unified across a workspace test run and arm the hatch fleet-wide"
    );
}

/// The crate must never enable `catalyrst-crypto`'s `test-signer` feature, in dependencies
/// or dev-dependencies. Enabling it there would arm `Signer::unchecked_for_test` for every
/// crate in a `cargo test --workspace` build, and the chokepoint argument would no longer
/// hold in test builds anywhere.
#[test]
fn the_crate_never_enables_the_test_signer_feature() {
    assert!(
        !manifest_without_comments().contains("test-signer"),
        "Cargo.toml enables test-signer; that arms Signer::unchecked_for_test workspace-wide"
    );
}

/// The repository's generated-artefacts gate (`catalyrst/sites/scripts/generated-artefacts.mts`)
/// decides whether a crate emits TypeScript bindings by substring-matching the derive path
/// `ts_rs` + `::TS` anywhere under `src/`, **comments included**. A crate it believes emits
/// bindings must declare `[package.metadata.generated]`, and this crate must never appear
/// in that index. So the spelling is banned from `src/` entirely, prose and all -- the
/// crate documentation says "a `ts-rs` `TS` derive" instead.
///
/// This test lives in `tests/`, which that gate does not scan, so it may name the spelling.
#[test]
fn the_generated_artefacts_spelling_appears_nowhere_under_src() {
    let banned = format!("ts_rs{}TS", "::");
    for (name, source) in every_source_file() {
        assert!(
            !source.contains(&banned),
            "{name} contains the ts-rs derive path spelling, which makes the repository's \
             generated-artefacts gate believe this crate emits TypeScript bindings"
        );
    }
}

/// P5, first half: nothing in this crate is serializable, and nothing here can appear in a
/// generated TypeScript or OpenAPI type.
#[test]
fn nothing_in_this_crate_is_serializable_or_exported_to_the_frontend() {
    for (name, source) in every_source_file() {
        for line in code_lines(source) {
            assert!(
                !line.contains("Serialize"),
                "{name} mentions Serialize: {line}"
            );
            assert!(!line.contains("ts_rs"), "{name} mentions ts_rs: {line}");
            assert!(!line.contains("utoipa"), "{name} mentions utoipa: {line}");
            assert!(
                !line.contains("ToSchema"),
                "{name} mentions ToSchema: {line}"
            );
        }
    }
    let manifest = manifest_without_comments();
    for forbidden in ["ts-rs", "utoipa", "sqlx"] {
        assert!(
            !manifest.contains(forbidden),
            "Cargo.toml depends on {forbidden}, which this crate forbids permanently"
        );
    }
    assert!(
        manifest.contains("default-features = false"),
        "catalyrst-types must be taken without default features so its sqlx feature stays off"
    );
}

/// P5, second half: exactly the `Claimed*` types carry `Deserialize`.
///
/// A `Deserialize` on a verified type would let a request body become an identity, which is
/// the whole defect. It is legal on a claim, because a claim proves nothing.
#[test]
fn deserialize_appears_only_on_the_claimed_types() {
    for (name, source) in every_source_file() {
        if name == "src/claimed.rs" {
            continue;
        }
        for line in code_lines(source) {
            assert!(
                !line.contains("Deserialize"),
                "{name} mentions Deserialize outside claimed.rs: {line}"
            );
        }
    }

    let derives_in_claimed: Vec<&str> = code_lines(CLAIMED)
        .filter(|line| line.contains("Deserialize"))
        .collect();
    assert_eq!(
        derives_in_claimed.len(),
        2,
        "expected exactly two Deserialize derives in claimed.rs, found {derives_in_claimed:?}"
    );

    // Each derive must sit directly above a type whose name starts with `Claimed`.
    let lines: Vec<&str> = CLAIMED.lines().map(str::trim).collect();
    for (index, line) in lines.iter().enumerate() {
        if line.starts_with("//") || !line.contains("Deserialize") {
            continue;
        }
        let declaration = lines[index + 1..]
            .iter()
            .find(|following| following.contains("pub struct"))
            .expect("a derive is followed by a struct declaration");
        assert!(
            declaration.contains("pub struct Claimed"),
            "Deserialize is derived on a type that is not a Claimed* type: {declaration}"
        );
    }
}

/// P4: no conversion exists between a claim and a verified identity, in either direction,
/// and no claim can be compared to one.
#[test]
fn no_conversion_exists_between_claimed_and_verified_types() {
    for (name, source) in every_source_file() {
        for line in code_lines(source) {
            let declares_a_conversion = line.starts_with("impl From<")
                || line.starts_with("impl TryFrom<")
                || line.starts_with("impl std::str::FromStr")
                || line.starts_with("impl FromStr");
            assert!(
                !declares_a_conversion,
                "{name} declares a conversion, which this crate forbids: {line}"
            );
        }
    }
    for forbidden in ["impl AsRef<str> for", "PartialEq<str>", "PartialEq<String>"] {
        assert!(
            !code_lines(VERIFIED_WALLET_ADDRESS).any(|line| line.contains(forbidden)),
            "verified_wallet_address.rs declares {forbidden}, which would let it be compared \
             to a bare string again"
        );
        assert!(
            !code_lines(CLAIMED).any(|line| line.contains(forbidden)),
            "claimed.rs declares {forbidden}"
        );
    }
}

/// P15: the two enums that every downstream gate matches on are not `#[non_exhaustive]`,
/// so adding a kind of caller or a kind of refusal breaks those matches on purpose.
#[test]
fn the_forcing_enums_are_not_non_exhaustive() {
    for (name, source) in [
        ("src/principal.rs", PRINCIPAL),
        ("src/refusal.rs", REFUSAL),
        ("src/verifier_registry.rs", VERIFIER_REGISTRY),
    ] {
        assert!(
            !code_lines(source).any(|line| line.contains("non_exhaustive")),
            "{name} is #[non_exhaustive]; that reintroduces the `_` catch-all this crate \
             exists to remove"
        );
    }
}

/// The service identity's constructor stays crate-private, so the only way to obtain one is
/// the shared-secret comparison.
#[test]
fn the_platform_service_identity_has_no_public_constructor() {
    let public_constructors: Vec<&str> = code_lines(PLATFORM_SERVICE_IDENTITY)
        .filter(|line| line.starts_with("pub fn ") && line.contains("-> Self"))
        .collect();
    assert!(
        public_constructors.is_empty(),
        "AuthenticatedPlatformServiceIdentity gained a public constructor: \
         {public_constructors:?}. The only mint must remain \
         establish_platform_service_identity_by_comparing_presented_shared_secret."
    );
}

/// This crate performs no I/O. A dependency that could reach a socket or a disk would make
/// the "no decisions here, only vocabulary" claim false.
#[test]
fn the_crate_has_no_io_dependencies() {
    let manifest = manifest_without_comments();
    for forbidden in ["tokio", "reqwest", "axum", "sqlx", "std::fs", "std::net"] {
        assert!(
            !manifest.contains(forbidden),
            "Cargo.toml depends on {forbidden}; this crate does no I/O"
        );
    }
}
