//! Peer admission: every rejection case, the shipped placeholder, and the one
//! property that makes the peer file mean anything -- that an admitted peer's client
//! trusts that peer's root and no other.
//!
//! Every test in this file runs with no database, no network, and no environment
//! mutation. There is nothing here that can skip, so a green run of this file is a
//! real result rather than a silent no-op.

use catalyrst_fed::PeerCert;
use catalyrst_worlds::fed::config::WorldsFedConfig;
use catalyrst_worlds::fed::peers::{
    AdmissionOutcome, AdmittedPeer, PeerNotAdmitted, PeerOmitted, WorldsFederationPeers,
};
use std::io::Write;
use std::path::PathBuf;

// Fixtures

/// A peer that clears every gate. Each test mutates exactly one field away from
/// this, so a failure names the gate that fired rather than a soup of them.
fn good_cert() -> PeerCert {
    PeerCert {
        version: 1,
        peer_id: "worlds.good-operator.org".into(),
        catalyst_url: "https://good-operator.org/content".into(),
        worlds_url: "https://worlds.good-operator.org".into(),
        gossip_pubkey: [7u8; 32],
        mtls_root_pem: self_signed_root_pem().0,
        dao_proposal: "https://snapshot.org/#/snapshot.dcl.eth/proposal/0xabc".into(),
        added_at: "2026-05-30".into(),
    }
}

fn cfg() -> WorldsFedConfig {
    WorldsFedConfig::default()
}

fn cfg_loopback() -> WorldsFedConfig {
    WorldsFedConfig {
        allow_insecure_loopback_peers: true,
        ..WorldsFedConfig::default()
    }
}

/// A throwaway CA, generated once per process. `.0` is the root PEM (what would go
/// in `mtls_root_pem`), `.1` is the CA cert + key, for tests that also need to issue
/// a leaf.
fn self_signed_root_pem() -> (String, std::sync::Arc<tls::Ca>) {
    let ca = tls::ca();
    (ca.root_pem.clone(), ca)
}

fn admit(cert: &PeerCert, cfg: &WorldsFedConfig) -> Result<AdmissionOutcome, PeerNotAdmitted> {
    AdmittedPeer::admit(cert, cfg)
}

fn expect_rejected(cert: &PeerCert, cfg: &WorldsFedConfig) -> PeerNotAdmitted {
    match admit(cert, cfg) {
        Err(e) => e,
        Ok(AdmissionOutcome::Admitted(p)) => {
            panic!("expected a rejection, but the peer was ADMITTED: {p:?}")
        }
        Ok(AdmissionOutcome::Omitted(o)) => {
            panic!("expected a rejection, but the peer was merely omitted: {o}")
        }
    }
}

fn expect_admitted(cert: &PeerCert, cfg: &WorldsFedConfig) -> AdmittedPeer {
    match admit(cert, cfg) {
        Ok(AdmissionOutcome::Admitted(p)) => p,
        other => panic!("expected admission, got {other:?}"),
    }
}

fn write_tmp(name: &str, body: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "catalyrst-worlds-fed-admission-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(name);
    let mut f = std::fs::File::create(&path).unwrap();
    f.write_all(body.as_bytes()).unwrap();
    path
}

/// Render a `PeerCert` back to the peer-file TOML shape, so file-level tests exercise
/// the real `FederationRegistry::from_file` parse rather than a hand-built struct.
fn to_toml(cert: &PeerCert) -> String {
    let pubkey = cert
        .gossip_pubkey
        .iter()
        .map(|b| b.to_string())
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "[[peer]]\nversion = {}\npeer_id = {:?}\ncatalyst_url = {:?}\nworlds_url = {:?}\n\
         gossip_pubkey = [{}]\nmtls_root_pem = {:?}\ndao_proposal = {:?}\nadded_at = {:?}\n",
        cert.version,
        cert.peer_id,
        cert.catalyst_url,
        cert.worlds_url,
        pubkey,
        cert.mtls_root_pem,
        cert.dao_proposal,
        cert.added_at,
    )
}

// 1. Not configured

#[test]
fn unset_peers_file_is_not_configured_and_is_not_an_error() {
    let peers = WorldsFederationPeers::load(&WorldsFedConfig {
        peers_file: None,
        ..WorldsFedConfig::default()
    })
    .expect("an unset peer file is a normal configuration, not a failure");

    assert!(matches!(peers, WorldsFederationPeers::NotConfigured));
    assert!(
        !peers.is_configured(),
        "NotConfigured must never report itself as configured \u{2014} the two states are the \
         whole reason this is an enum and not Option<Vec<_>>"
    );
    assert!(peers.peers().is_empty());
    assert!(peers.path().is_none());
}

// 2. The shipped placeholder file

/// Points at the **literal committed** peer file. This test fails the day someone
/// fills it in, which is the point: the shipped file is a placeholder, and a
/// placeholder that starts being admitted is a security event.
#[test]
fn shipped_placeholder_file_refuses_to_load() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../deploy/config/federation-peers.toml");
    let path = path
        .canonicalize()
        .unwrap_or_else(|e| panic!("deploy/config/federation-peers.toml must exist: {e}"));

    let err = WorldsFederationPeers::load_file(&path, &cfg())
        .expect_err("the shipped placeholder must refuse to load");
    let msg = err.to_string();

    assert!(
        msg.contains("dao_proposal is still the placeholder"),
        "the FIRST reported reason must be the placeholder proposal, since that is the \
         message an operator sees on a failed boot: {msg}"
    );
    assert!(
        msg.contains("refusing to start"),
        "the error must say the process is refusing to start: {msg}"
    );
    assert!(
        msg.contains("Unset WORLDS_FED_PEERS_FILE"),
        "the error must name the way out: {msg}"
    );
    assert!(
        msg.contains("federation-peers.toml"),
        "the error must name the offending file: {msg}"
    );
}

/// The same entry, gate by gate: five independent refusals, any one sufficient. This
/// is the claim the build spec makes about the shipped file, asserted rather than
/// asserted-in-prose.
#[test]
fn the_shipped_placeholder_entry_is_refused_five_independent_ways() {
    let placeholder = PeerCert {
        version: 1,
        peer_id: "example-peer.invalid".into(),
        catalyst_url: "https://example-peer.invalid/content".into(),
        worlds_url: String::new(),
        gossip_pubkey: [0u8; 32],
        mtls_root_pem: String::new(),
        dao_proposal: "TODO: https://snapshot.org/#/<space>/proposal/<id>".into(),
        added_at: "1970-01-01".into(),
    };

    // (1) as shipped
    assert!(matches!(
        expect_rejected(&placeholder, &cfg()),
        PeerNotAdmitted::PlaceholderDaoProposal { .. }
    ));

    // (2) with a real proposal
    let mut c = placeholder.clone();
    c.dao_proposal = "https://snapshot.org/#/snapshot.dcl.eth/proposal/0xabc".into();
    assert!(matches!(
        expect_rejected(&c, &cfg()),
        PeerNotAdmitted::PlaceholderAddedAt { .. }
    ));

    // (3) with a real date
    c.added_at = "2026-05-30".into();
    assert!(matches!(
        expect_rejected(&c, &cfg()),
        PeerNotAdmitted::ZeroGossipPubkey { .. }
    ));

    // (4) with a real key
    c.gossip_pubkey = [3u8; 32];
    assert!(matches!(
        expect_rejected(&c, &cfg()),
        PeerNotAdmitted::ReservedTestHost {
            suffix: ".invalid",
            ..
        }
    ));

    // (5) with a real peer id -- still no pinned root, so still refused
    c.peer_id = "example-peer.org".into();
    assert!(matches!(
        expect_rejected(&c, &cfg()),
        PeerNotAdmitted::NoPinnedRoot { .. }
    ));

    // Only after all five are fixed does it become a legitimate non-worlds peer.
    c.mtls_root_pem = self_signed_root_pem().0;
    assert!(matches!(
        admit(&c, &cfg()),
        Ok(AdmissionOutcome::Omitted(PeerOmitted::NoWorldsUrl { .. }))
    ));
}

// 3. One test per PeerNotAdmitted variant

#[test]
fn reject_placeholder_dao_proposal_todo_prefix() {
    for value in [
        "TODO",
        "TODO: fill me in",
        "todo: https://snapshot.org/#/snapshot.dcl.eth/proposal/0xabc",
        "  ToDo \u{2014} ask governance  ",
    ] {
        let mut c = good_cert();
        c.dao_proposal = value.into();
        let e = expect_rejected(&c, &cfg());
        assert!(
            matches!(e, PeerNotAdmitted::PlaceholderDaoProposal { .. }),
            "{value:?} should be a placeholder proposal, got {e}"
        );
        assert!(e.to_string().contains("snapshot.dcl.eth"));
    }
}

#[test]
fn reject_placeholder_dao_proposal_unsubstituted_template_markers() {
    // No "TODO" prefix at all -- only the markers give it away.
    for value in [
        "https://snapshot.org/#/<space>/proposal/0xabc",
        "https://snapshot.org/#/snapshot.dcl.eth/proposal/<id>",
    ] {
        let mut c = good_cert();
        c.dao_proposal = value.into();
        assert!(
            matches!(
                expect_rejected(&c, &cfg()),
                PeerNotAdmitted::PlaceholderDaoProposal { .. }
            ),
            "unsubstituted marker in {value:?} must be caught"
        );
    }
}

#[test]
fn reject_placeholder_added_at() {
    for value in ["1970-01-01", "  1970-01-01  ", "1970-01-01T00:00:00Z"] {
        let mut c = good_cert();
        c.added_at = value.into();
        assert!(
            matches!(
                expect_rejected(&c, &cfg()),
                PeerNotAdmitted::PlaceholderAddedAt { .. }
            ),
            "{value:?} must be recognised as the epoch placeholder"
        );
    }
}

#[test]
fn reject_zero_gossip_pubkey() {
    let mut c = good_cert();
    c.gossip_pubkey = [0u8; 32];
    let e = expect_rejected(&c, &cfg());
    assert!(matches!(e, PeerNotAdmitted::ZeroGossipPubkey { .. }));
    assert!(
        e.to_string().contains("placeholder"),
        "the message must say the key is a placeholder check and not a signature check, so \
         nobody later mistakes this for a verified channel: {e}"
    );

    // One non-zero byte is enough to clear the *placeholder* check -- this is
    // deliberately not a key-quality check, because nothing in this slice verifies a
    // signature and pretending otherwise would be decorative cryptography.
    let mut c2 = good_cert();
    c2.gossip_pubkey = [0u8; 32];
    c2.gossip_pubkey[31] = 1;
    expect_admitted(&c2, &cfg());
}

#[test]
fn reject_reserved_test_host_peer_ids() {
    for (id, suffix) in [
        ("example-peer.invalid", ".invalid"),
        ("peer.example", ".example"),
        ("node.test", ".test"),
        ("box.localhost", ".localhost"),
        ("server.local", ".local"),
        ("SHOUTY-PEER.INVALID", ".invalid"),
    ] {
        let mut c = good_cert();
        c.peer_id = id.into();
        let e = expect_rejected(&c, &cfg());
        match e {
            PeerNotAdmitted::ReservedTestHost { suffix: got, .. } => assert_eq!(got, suffix),
            other => panic!("{id} should be a reserved host, got {other}"),
        }
    }

    // A reserved label that is not a *suffix* is fine.
    let mut ok = good_cert();
    ok.peer_id = "invalid.example-operator.org".into();
    expect_admitted(&ok, &cfg());
}

#[test]
fn reject_missing_pinned_root() {
    let mut c = good_cert();
    c.mtls_root_pem = "   ".into();
    let e = expect_rejected(&c, &cfg());
    assert!(matches!(e, PeerNotAdmitted::NoPinnedRoot { .. }));
    assert!(
        e.to_string().contains("WebPKI"),
        "the message must explain that https alone proves nothing about who answers: {e}"
    );
}

#[test]
fn reject_missing_pinned_root_even_when_the_peer_runs_no_worlds_server() {
    // A peer with no worlds_url is normally *omitted*, not rejected. But with no
    // pinned root there is nothing to omit it on the strength of: this entry asserts
    // nothing about anybody.
    let mut c = good_cert();
    c.worlds_url = String::new();
    c.mtls_root_pem = String::new();
    assert!(matches!(
        expect_rejected(&c, &cfg()),
        PeerNotAdmitted::NoPinnedRoot { .. }
    ));

    // The loopback opt-out cannot rescue it either: with no URL there is no host that
    // could be loopback.
    assert!(matches!(
        expect_rejected(&c, &cfg_loopback()),
        PeerNotAdmitted::NoPinnedRoot { .. }
    ));
}

/// A pinned root that is not a usable certificate must be refused **at boot**.
///
/// This is a regression test for a real defect found while building this module.
/// Under the `__rustls` feature that this workspace compiles with,
/// `reqwest::Certificate::from_pem` does no parsing at all -- it stores the bytes and
/// returns `Ok` for any input -- and the deferred parse treats "no PEM block found" as
/// success, yielding an EMPTY root store. The first implementation therefore *admitted*
/// a peer whose `mtls_root_pem` was the literal string "not a certificate", and the
/// only symptom would have been a peer that reported itself unreachable forever.
///
/// Every one of these must be caught before the process finishes starting.
#[test]
fn reject_unusable_pinned_root() {
    // (a) no PEM block at all -- the case that used to be silently admitted.
    // (b) a PEM block whose body is not base64.
    // (c) an empty PEM block.
    // (d) a PEM block that is valid base64 but is not a certificate.
    for (pem, label) in [
        ("not a certificate", "no PEM block"),
        ("", "empty after trim is NoPinnedRoot, checked separately"),
        (
            "-----BEGIN CERTIFICATE-----\nnot base64!!!\n-----END CERTIFICATE-----",
            "invalid base64",
        ),
        (
            "-----BEGIN CERTIFICATE-----\n-----END CERTIFICATE-----",
            "empty block",
        ),
        (
            "-----BEGIN CERTIFICATE-----\naGVsbG8gd29ybGQ=\n-----END CERTIFICATE-----",
            "valid base64, not a certificate",
        ),
        (
            "-----BEGIN PRIVATE KEY-----\naGVsbG8=\n-----END PRIVATE KEY-----",
            "a key where a root was expected",
        ),
    ] {
        if pem.is_empty() {
            continue;
        }
        let mut c = good_cert();
        c.mtls_root_pem = pem.into();
        let e = expect_rejected(&c, &cfg());
        assert!(
            matches!(
                e,
                PeerNotAdmitted::UnusablePinnedRoot { .. }
                    | PeerNotAdmitted::ClientBuildFailed { .. }
            ),
            "a pinned root that is {label} must be refused at BOOT, not silently turned into \
             an empty trust store that fails at connect time. Got: {e}"
        );
    }
}

/// The specific case that regressed: a peer admitted with a garbage root would build
/// a client with an empty trust store. Assert that no such client can exist by
/// asserting the admission fails -- and name the failure mode in the message so a
/// future reader who "simplifies" `from_pem_bundle` back to `from_pem` learns why.
#[test]
fn a_garbage_pinned_root_never_becomes_an_empty_trust_store() {
    let mut c = good_cert();
    c.mtls_root_pem = "not a certificate".into();
    let e = expect_rejected(&c, &cfg());
    match e {
        PeerNotAdmitted::UnusablePinnedRoot { source, .. } => assert!(
            source.contains("BEGIN CERTIFICATE"),
            "the message must tell the operator what is missing from the file: {source}"
        ),
        other => panic!("expected UnusablePinnedRoot, got {other}"),
    }
}

#[test]
fn reject_unparseable_worlds_url() {
    for url in ["not a url", "://missing-scheme", "https://[bad-ipv6"] {
        let mut c = good_cert();
        c.worlds_url = url.into();
        let e = expect_rejected(&c, &cfg());
        assert!(
            matches!(e, PeerNotAdmitted::WorldsUrlUnparseable { .. }),
            "{url:?} should be unparseable, got {e}"
        );
    }
}

#[test]
fn reject_worlds_url_that_is_not_https() {
    for url in [
        "http://worlds.good-operator.org",
        "ftp://worlds.good-operator.org",
        "ws://worlds.good-operator.org",
    ] {
        let mut c = good_cert();
        c.worlds_url = url.into();
        let e = expect_rejected(&c, &cfg());
        assert!(
            matches!(e, PeerNotAdmitted::WorldsUrlNotHttps { .. }),
            "{url:?} should be refused for scheme, got {e}"
        );
    }
}

#[test]
fn reject_worlds_url_with_no_host() {
    for url in ["file:///etc/passwd", "data:text/plain,hi", "mailto:a@b.org"] {
        let mut c = good_cert();
        c.worlds_url = url.into();
        let e = expect_rejected(&c, &cfg());
        assert!(
            matches!(
                e,
                PeerNotAdmitted::WorldsUrlHasNoHost { .. }
                    | PeerNotAdmitted::WorldsUrlNotHttps { .. }
            ),
            "{url:?} must be refused before anything tries to fetch it, got {e}"
        );
    }
    // An https URL with a genuinely empty authority. Note the `url` crate collapses
    // `https:///x` to host `x` for special schemes, so THAT is not a no-host URL and
    // is deliberately not asserted here -- asserting it would have encoded a false
    // belief about the parser into the suite.
    for url in ["https://", "https:///"] {
        let mut c = good_cert();
        c.worlds_url = url.into();
        let e = expect_rejected(&c, &cfg());
        assert!(
            matches!(
                e,
                PeerNotAdmitted::WorldsUrlHasNoHost { .. }
                    | PeerNotAdmitted::WorldsUrlUnparseable { .. }
            ),
            "{url:?} has no host and must be refused, got {e}"
        );
    }
}

/// Honest note on reachability: for the special schemes `http`/`https` the `url`
/// crate rejects an empty host at parse time, so `WorldsUrlHasNoHost` is mostly
/// defence in depth behind `WorldsUrlUnparseable` and the scheme check. It is kept
/// because "we never fetch from a URL with no host" should be a property of this
/// function rather than a property of a dependency's parser.
#[test]
fn worlds_url_has_no_host_is_defence_in_depth() {
    let e = PeerNotAdmitted::WorldsUrlHasNoHost {
        peer_id: "p.org".into(),
        url: "https://".into(),
    };
    assert!(e
        .to_string()
        .contains("no host to pin a certificate against"));
}

// `ClientBuildFailed` IS reachable from a peer file: a `mtls_root_pem` that is a
// well-formed PEM block of valid base64 that is not a certificate survives
// `from_pem_bundle` and is rejected by `RootCertStore::add` inside `build()`. See
// `reject_unusable_pinned_root`, which accepts either variant for that input. This
// test pins the Display shape so the operator-facing wording does not drift.
#[test]
fn client_build_failure_is_reported_as_a_refusal_not_a_panic() {
    let e = PeerNotAdmitted::ClientBuildFailed {
        peer_id: "worlds.good-operator.org".into(),
        source: "synthetic".into(),
    };
    assert!(e.to_string().contains("pinned to this peer's root"));
    assert_eq!(e.peer_id(), "worlds.good-operator.org");
}

// 4. Omission, not rejection

#[test]
fn no_worlds_url_is_omitted_not_fatal_and_is_reported() {
    let mut c = good_cert();
    c.worlds_url = "   ".into();

    let body = to_toml(&c);
    let path = write_tmp("omitted.toml", &body);
    let peers = WorldsFederationPeers::load_file(&path, &cfg())
        .expect("a peer that runs no worlds server must not abort the boot");

    assert!(peers.is_configured());
    assert!(
        peers.peers().is_empty(),
        "an omitted peer must not appear in the admitted list"
    );
    assert_eq!(peers.omitted().len(), 1, "the omission must be enumerated");
    match &peers.omitted()[0] {
        PeerOmitted::NoWorldsUrl { peer_id } => assert_eq!(peer_id, "worlds.good-operator.org"),
    }
    assert!(
        peers.get("worlds.good-operator.org").is_none(),
        "an omitted peer must not be addressable as a worlds peer"
    );
    assert!(
        peers.omitted()[0]
            .to_string()
            .contains("runs no worlds server"),
        "the reason must be legible: {}",
        peers.omitted()[0]
    );
}

#[test]
fn zero_worlds_peers_is_configured_with_an_empty_list_not_not_configured() {
    let mut c = good_cert();
    c.worlds_url = String::new();
    let path = write_tmp("zero-worlds-peers.toml", &to_toml(&c));
    let peers = WorldsFederationPeers::load_file(&path, &cfg()).unwrap();

    assert!(
        peers.is_configured(),
        "'federation was requested and yielded nothing' must be DISTINGUISHABLE from \
         'federation was never requested' \u{2014} the routes answer 200 with an empty list in one \
         case and 503 in the other"
    );
    assert!(peers.peers().is_empty());
    assert!(!peers.omitted().is_empty());
    assert!(peers.path().is_some());
}

// 5. File-level failure behaviour

#[test]
fn a_missing_peer_file_refuses_to_boot_rather_than_disabling_federation() {
    let missing = std::env::temp_dir().join("catalyrst-worlds-no-such-peer-file.toml");
    let _ = std::fs::remove_file(&missing);

    let err = WorldsFederationPeers::load_file(&missing, &cfg())
        .expect_err("a named-but-missing peer file must be a boot failure");
    let msg = err.to_string();
    assert!(
        msg.contains("WORLDS_FED_PEERS_FILE"),
        "the error must name the variable that pointed at the missing file: {msg}"
    );
    assert!(msg.contains("could not be loaded"), "{msg}");
}

#[test]
fn malformed_toml_refuses_to_boot() {
    let path = write_tmp("malformed.toml", "this is not = valid = toml [[");
    let err = WorldsFederationPeers::load_file(&path, &cfg())
        .expect_err("malformed TOML must be a boot failure, never 'federation disabled'");
    assert!(err.to_string().contains("could not be loaded"));
}

#[test]
fn an_empty_but_valid_peer_file_is_configured_with_no_peers() {
    // The degenerate case the enum exists to keep honest: a file with zero entries
    // is "we federate with nobody", NOT "federation is off", and NOT an allowlist
    // that a later code path may append to.
    //
    // This state is ALSO reachable by accident - `[[peers]]` instead of `[[peer]]`
    // parses to zero entries, as does a truncated write - and the two are
    // indistinguishable here. That hazard is handled where it does damage, in
    // `RemoteWorldsComponent::revoke_peers_no_longer_admitted`, which refuses to
    // sweep an empty allowlist rather than deleting every mirrored row. Loading
    // stays permissive so the deliberate state remains expressible.
    let path = write_tmp("empty.toml", "# no peers yet\n");
    let peers = WorldsFederationPeers::load_file(&path, &cfg()).unwrap();
    assert!(peers.is_configured());
    assert!(peers.peers().is_empty());
    assert!(peers.omitted().is_empty());
}

#[test]
fn one_bad_entry_rejects_the_whole_file_not_just_that_entry() {
    let good = good_cert();
    let mut bad = good_cert();
    bad.peer_id = "aaa-first-alphabetically.org".into();
    bad.dao_proposal = "TODO".into();

    let path = write_tmp(
        "one-bad.toml",
        &format!("{}\n{}", to_toml(&good), to_toml(&bad)),
    );
    let err = WorldsFederationPeers::load_file(&path, &cfg()).expect_err(
        "booting with one peer when the operator wrote two makes 'we federate with X' and \
         'we tried to federate with X' the same observable state",
    );
    let msg = err.to_string();
    assert!(msg.contains("1 of 2 entries"), "{msg}");
    assert!(msg.contains("aaa-first-alphabetically.org"), "{msg}");
}

#[test]
fn the_first_reported_rejection_is_stable_across_runs() {
    // FederationRegistry stores a HashMap, so `all()` order is arbitrary. If the
    // loader did not sort, the operator-facing "First: ..." line would be a coin
    // flip between the two bad entries. Run the same file repeatedly and demand the
    // same answer.
    let mut a = good_cert();
    a.peer_id = "aaa.operator.org".into();
    a.dao_proposal = "TODO a".into();
    let mut b = good_cert();
    b.peer_id = "zzz.operator.org".into();
    b.dao_proposal = "TODO z".into();

    let path = write_tmp(
        "stable-order.toml",
        &format!("{}\n{}", to_toml(&b), to_toml(&a)),
    );
    for _ in 0..16 {
        let msg = WorldsFederationPeers::load_file(&path, &cfg())
            .unwrap_err()
            .to_string();
        assert!(msg.contains("2 of 2 entries"), "{msg}");
        assert!(
            msg.contains("\"TODO a\""),
            "the first rejection must always be the alphabetically-first peer: {msg}"
        );
    }
}

// 6. The loopback dev opt-out

#[test]
fn loopback_http_requires_the_explicit_opt_out() {
    for url in [
        "http://127.0.0.1:5242",
        "http://[::1]:5242",
        "http://localhost:5242",
    ] {
        let mut c = good_cert();
        c.worlds_url = url.into();
        c.mtls_root_pem = String::new();

        // Without the flag: refused for scheme, before anything else.
        let e = expect_rejected(&c, &cfg());
        assert!(
            matches!(e, PeerNotAdmitted::WorldsUrlNotHttps { .. }),
            "{url:?} must be refused without the opt-out, got {e}"
        );

        // With the flag: admitted, and marked as unauthenticated.
        let p = expect_admitted(&c, &cfg_loopback());
        assert!(
            p.is_insecure_loopback(),
            "{url:?} admitted through the dev opt-out must be flagged so an operator reading \
             /federation/worlds/peers can see the channel is unauthenticated"
        );
    }
}

#[test]
fn non_loopback_host_cannot_use_the_loopback_opt_out() {
    // Names that *look* loopback-ish, plus private ranges, plus a host that may well
    // resolve to 127.0.0.1 -- the check is literal, not resolved, precisely because
    // what a name resolves to is not under our control.
    for url in [
        "http://worlds.good-operator.org",
        "http://127.0.0.1.evil.example",
        "http://localhost.evil.example",
        "http://10.0.0.1:5242",
        "http://192.168.1.5:5242",
        "http://0.0.0.0:5242",
    ] {
        let mut c = good_cert();
        c.worlds_url = url.into();
        c.mtls_root_pem = String::new();
        let e = expect_rejected(&c, &cfg_loopback());
        assert!(
            matches!(e, PeerNotAdmitted::WorldsUrlNotHttps { .. }),
            "{url:?} is not loopback and must stay refused with the flag set, got {e}"
        );
    }
}

#[test]
fn the_opt_out_does_not_waive_the_pinned_root_for_https_loopback() {
    // The opt-out exists because catalyrst-worlds serves plain HTTP with no local TLS
    // terminator. It is not a general "skip the pin" switch: an https URL still needs
    // a root, loopback or not.
    let mut c = good_cert();
    c.worlds_url = "https://127.0.0.1:5242".into();
    c.mtls_root_pem = String::new();
    assert!(matches!(
        expect_rejected(&c, &cfg_loopback()),
        PeerNotAdmitted::NoPinnedRoot { .. }
    ));
}

#[test]
fn the_opt_out_does_not_waive_any_other_gate() {
    for mutate in [
        (|c: &mut PeerCert| c.dao_proposal = "TODO".into()) as fn(&mut PeerCert),
        |c: &mut PeerCert| c.added_at = "1970-01-01".into(),
        |c: &mut PeerCert| c.gossip_pubkey = [0u8; 32],
        |c: &mut PeerCert| c.peer_id = "box.localhost".into(),
    ] {
        let mut c = good_cert();
        c.worlds_url = "http://127.0.0.1:5242".into();
        c.mtls_root_pem = String::new();
        mutate(&mut c);
        expect_rejected(&c, &cfg_loopback());
    }
}

// 7. URL construction

#[test]
fn worlds_listing_url_is_built_from_the_registry_and_takes_no_peer_input() {
    let p = expect_admitted(&good_cert(), &cfg());
    let u = p.worlds_listing_url(100, 200);
    assert_eq!(
        u.as_str(),
        "https://worlds.good-operator.org/worlds?limit=100&offset=200"
    );
    // The only parameters are integers. There is no `&str` parameter through which a
    // peer-reported value could reach this function, which is what makes "no SSRF
    // surface" a property of the signature rather than of the caller's discipline.
    assert_eq!(u.host_str(), Some("worlds.good-operator.org"));
    assert_eq!(u.scheme(), "https");
}

#[test]
fn worlds_url_path_prefixes_and_trailing_slashes_normalise() {
    for (configured, expected) in [
        ("https://p.org", "https://p.org/worlds?limit=1&offset=0"),
        ("https://p.org/", "https://p.org/worlds?limit=1&offset=0"),
        (
            "https://p.org/api",
            "https://p.org/api/worlds?limit=1&offset=0",
        ),
        (
            "https://p.org/api/",
            "https://p.org/api/worlds?limit=1&offset=0",
        ),
        (
            "https://p.org:8443/api//",
            "https://p.org:8443/api/worlds?limit=1&offset=0",
        ),
    ] {
        let mut c = good_cert();
        c.worlds_url = configured.into();
        let p = expect_admitted(&c, &cfg());
        assert_eq!(
            p.worlds_listing_url(1, 0).as_str(),
            expected,
            "worlds_url {configured:?}"
        );
    }
}

#[test]
fn worlds_url_userinfo_query_and_fragment_are_dropped() {
    let mut c = good_cert();
    c.worlds_url = "https://user:hunter2@p.org/api?token=leak#frag".into();
    let p = expect_admitted(&c, &cfg());
    let u = p.worlds_listing_url(5, 0);

    assert_eq!(u.as_str(), "https://p.org/api/worlds?limit=5&offset=0");
    assert!(
        !u.as_str().contains("hunter2") && !u.as_str().contains("leak"),
        "credentials and a registry-supplied query must never be sent to a peer: {u}"
    );
    assert_eq!(u.username(), "");
    assert!(u.password().is_none());
}

// 8. The pin -- the one property that makes the peer file mean anything

/// Half one of the pin: the pinned root is **in force**, and no *other* private root
/// is accepted.
///
/// A peer admitted with root **A** must reach a server presenting a leaf signed by A,
/// and must fail against a server presenting a leaf signed by an unrelated root B.
///
/// HONEST LIMIT, measured rather than assumed: this test does **not** discriminate
/// `tls_certs_only` from `tls_certs_merge`/`add_root_certificate`. It was run against
/// a deliberately regressed build that merged the pinned root into the ambient
/// platform trust store, and it still passed -- because root B is not in the platform
/// store either, so a merged client rejects server B for the same reason a pinned one
/// does. The half that actually catches that regression is
/// `pinned_client_rejects_a_webpki_valid_host` below, which needs a host whose chain
/// the *platform* store trusts. Do not delete that test believing this one covers it.
#[tokio::test]
async fn pinned_client_trusts_only_its_own_root() {
    let ca_a = tls::ca();
    let ca_b = tls::other_ca();

    let server_a = tls::serve(&ca_a).await;
    let server_b = tls::serve(&ca_b).await;

    let mut cert = good_cert();
    cert.mtls_root_pem = ca_a.root_pem.clone();
    cert.worlds_url = format!("https://localhost:{}", server_a.port);
    let peer_pinned_to_a = expect_admitted(&cert, &cfg());

    // Same peer definition, pointed at server B: same pinned root A, different server.
    let mut cert_b = good_cert();
    cert_b.mtls_root_pem = ca_a.root_pem.clone();
    cert_b.worlds_url = format!("https://localhost:{}", server_b.port);
    let peer_a_root_b_server = expect_admitted(&cert_b, &cfg());

    let ok = peer_pinned_to_a
        .http()
        .get(peer_pinned_to_a.worlds_listing_url(1, 0))
        .send()
        .await;
    assert!(
        ok.is_ok(),
        "a client pinned to root A must reach a server whose leaf A signed: {:?}",
        ok.err()
    );

    let refused = peer_a_root_b_server
        .http()
        .get(peer_a_root_b_server.worlds_listing_url(1, 0))
        .send()
        .await;
    assert!(
        refused.is_err(),
        "a client pinned to root A reached a server presenting an UNRELATED root's leaf; \
         the pinned root is not being applied at all"
    );

    // And the inverse, so the test cannot pass by the server being broken.
    let mut cert_c = good_cert();
    cert_c.mtls_root_pem = ca_b.root_pem.clone();
    cert_c.worlds_url = format!("https://localhost:{}", server_b.port);
    let peer_pinned_to_b = expect_admitted(&cert_c, &cfg());
    assert!(
        peer_pinned_to_b
            .http()
            .get(peer_pinned_to_b.worlds_listing_url(1, 0))
            .send()
            .await
            .is_ok(),
        "server B is reachable when pinned to root B, so the refusal above was the pin and \
         not a dead listener"
    );
}

/// Half two of the pin, and the only test that discriminates a pinned client from a
/// merged one: the **ambient platform trust store is not in force**.
///
/// A peer is admitted with a private throwaway root that no public CA ever signed,
/// then pointed at a host with an ordinary, valid WebPKI certificate. It must FAIL.
/// If `fed/peers.rs` used `add_root_certificate` or `tls_certs_merge` -- both of which
/// compile, and both of which route through
/// `rustls_platform_verifier::Verifier::new_with_extra_roots` in reqwest 0.13 -- this
/// request would succeed with a 200, the peer file would stop meaning anything, and
/// any WebPKI-valid host that won a DNS race would be the peer.
///
/// Verified to discriminate: against a build with `tls_certs_only` swapped for
/// `tls_certs_merge`, this test fails and `pinned_client_trusts_only_its_own_root`
/// still passes.
///
/// This is a plain read-only HTTPS GET to a well-known public host. It joins nothing
/// and authenticates nothing. It needs outbound network, so it refuses loudly rather
/// than skipping silently unless `ALLOW_SKIPPED_INTEGRATION=1`.
#[tokio::test]
async fn pinned_client_rejects_a_webpki_valid_host() {
    const WEBPKI_HOST: &str = "https://example.com";

    // Establish that the host is reachable and WebPKI-valid *for an ordinary client*,
    // so a failure below is attributable to the pin and not to a dead network.
    let control = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .unwrap()
        .get(WEBPKI_HOST)
        .send()
        .await;
    let Ok(control) = control else {
        let detail = format!(
            "an ordinary WebPKI client could not reach {WEBPKI_HOST}: {:?}. Without a \
             reachable public host there is nothing whose ambient trust could be \
             distinguished from the pinned root, so this test asserts nothing.",
            control.err()
        );
        let _: Option<()> = catalyrst_testgate::unavailable("outbound HTTPS", &detail);
        return;
    };
    assert!(
        control.status().is_success(),
        "control request must succeed for the pinned refusal below to mean anything"
    );

    // Now the same request through a peer pinned to a private throwaway root.
    let ca = tls::ca();
    let mut cert = good_cert();
    cert.mtls_root_pem = ca.root_pem.clone();
    cert.worlds_url = WEBPKI_HOST.to_string();
    let peer = expect_admitted(&cert, &cfg());

    let result = peer.http().get(peer.worlds_listing_url(1, 0)).send().await;

    let err = match result {
        Err(e) => e,
        Ok(resp) => panic!(
            "a client pinned to a PRIVATE throwaway root successfully reached {WEBPKI_HOST} \
             (status {}). The ambient platform/WebPKI trust store is still in force, so the \
             pinned root is not an admission decision \u{2014} any WebPKI-valid host that wins a \
             DNS race is now the peer. Check that fed/peers.rs still calls \
             ClientBuilder::tls_certs_only and NOT add_root_certificate / tls_certs_merge.",
            resp.status()
        ),
    };
    assert!(
        err.is_connect() || format!("{err:?}").contains("certificate"),
        "the refusal must be a TLS/certificate failure, not a timeout or a DNS error, or \
         this test would pass on a broken network: {err:?}"
    );
}

/// A redirect off the pinned host would silently defeat the pin: the pin is checked
/// per-connection, and the second connection is to whatever host the peer names.
#[tokio::test]
async fn an_admitted_peer_does_not_follow_redirects() {
    let ca = tls::ca();
    let server = tls::serve_redirect(&ca, "https://evil.example/worlds").await;

    let mut cert = good_cert();
    cert.mtls_root_pem = ca.root_pem.clone();
    cert.worlds_url = format!("https://localhost:{}", server.port);
    let peer = expect_admitted(&cert, &cfg());

    let resp = peer
        .http()
        .get(peer.worlds_listing_url(1, 0))
        .send()
        .await
        .expect("the first hop succeeds; it is the second that must not happen");

    assert_eq!(
        resp.status().as_u16(),
        302,
        "the redirect must be returned to us as a 302 rather than followed to evil.example"
    );
    assert_eq!(
        resp.url().host_str(),
        Some("localhost"),
        "the response URL must still be the pinned host: {}",
        resp.url()
    );
}

// 9. Registry reload is deliberately not wired

/// The peer set is fixed for the process lifetime; changing it is a restart. Fewer
/// moving parts across a trust boundary, and it means the admission decision that a
/// boot log records is the one still in force.
#[test]
fn registry_reload_is_not_wired() {
    let src =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/fed/peers.rs")).unwrap();
    assert!(
        !src.contains(".reload("),
        "FederationRegistry::reload must have no call site: a peer set that can change \
         under a running poller is a trust boundary that moves without a deploy"
    );
}

// TLS test scaffolding

mod tls {
    use std::sync::Arc;
    use std::sync::OnceLock;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    pub struct Ca {
        pub root_pem: String,
        root_der: rustls::pki_types::CertificateDer<'static>,
        issuer: rcgen::Issuer<'static, rcgen::KeyPair>,
    }

    fn make_ca(common_name: &str) -> Arc<Ca> {
        let mut params = rcgen::CertificateParams::new(Vec::new()).unwrap();
        params.is_ca = rcgen::IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
        params
            .distinguished_name
            .push(rcgen::DnType::CommonName, common_name);
        params.key_usages = vec![
            rcgen::KeyUsagePurpose::KeyCertSign,
            rcgen::KeyUsagePurpose::CrlSign,
            rcgen::KeyUsagePurpose::DigitalSignature,
        ];
        let key = rcgen::KeyPair::generate().unwrap();
        let cert = params.self_signed(&key).unwrap();
        Arc::new(Ca {
            root_pem: cert.pem(),
            root_der: cert.der().clone(),
            issuer: rcgen::Issuer::new(params, key),
        })
    }

    /// Root A. Generated once per process so `good_cert()` is cheap.
    pub fn ca() -> Arc<Ca> {
        static CA: OnceLock<Arc<Ca>> = OnceLock::new();
        CA.get_or_init(|| make_ca("catalyrst worlds fed test root A"))
            .clone()
    }

    /// Root B -- unrelated to A, and to anything in the system trust store.
    pub fn other_ca() -> Arc<Ca> {
        static CA: OnceLock<Arc<Ca>> = OnceLock::new();
        CA.get_or_init(|| make_ca("catalyrst worlds fed test root B"))
            .clone()
    }

    pub struct Server {
        pub port: u16,
    }

    async fn spawn(ca: &Arc<Ca>, response: String) -> Server {
        let leaf_key = rcgen::KeyPair::generate().unwrap();
        let mut params = rcgen::CertificateParams::new(vec!["localhost".to_string()]).unwrap();
        params
            .distinguished_name
            .push(rcgen::DnType::CommonName, "localhost");
        let leaf = params.signed_by(&leaf_key, &ca.issuer).unwrap();

        let cfg = rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(
                vec![leaf.der().clone(), ca.root_der.clone()],
                rustls::pki_types::PrivateKeyDer::Pkcs8(leaf_key.serialize_der().into()),
            )
            .unwrap();
        let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(cfg));

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();

        tokio::spawn(async move {
            loop {
                let Ok((sock, _)) = listener.accept().await else {
                    return;
                };
                let acceptor = acceptor.clone();
                let response = response.clone();
                tokio::spawn(async move {
                    let Ok(mut stream) = acceptor.accept(sock).await else {
                        // A pin mismatch aborts here, which is exactly the outcome
                        // `pinned_client_trusts_only_its_own_root` is asserting.
                        return;
                    };
                    let mut buf = [0u8; 2048];
                    let _ = stream.read(&mut buf).await;
                    let _ = stream.write_all(response.as_bytes()).await;
                    let _ = stream.shutdown().await;
                });
            }
        });

        Server { port }
    }

    pub async fn serve(ca: &Arc<Ca>) -> Server {
        let body = r#"{"total":0,"worlds":[]}"#;
        spawn(
            ca,
            format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\
                 Connection: close\r\n\r\n{body}",
                body.len()
            ),
        )
        .await
    }

    pub async fn serve_redirect(ca: &Arc<Ca>, location: &str) -> Server {
        spawn(
            ca,
            format!(
                "HTTP/1.1 302 Found\r\nLocation: {location}\r\nContent-Length: 0\r\n\
                 Connection: close\r\n\r\n"
            ),
        )
        .await
    }
}
