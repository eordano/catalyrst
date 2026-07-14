use catalyrst_fed::FederationRegistry;
use std::io::Write;

fn write_tmp(name: &str, body: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("catalyrst-fed-peer-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(name);
    let mut f = std::fs::File::create(&path).unwrap();
    f.write_all(body.as_bytes()).unwrap();
    path
}

const VALID_TWO_PEERS: &str = r#"
[[peer]]
peer_id        = "interconnected.online"
catalyst_url   = "https://interconnected.online/content"
gossip_pubkey  = [
    1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,
    17,18,19,20,21,22,23,24,25,26,27,28,29,30,31,32,
]
mtls_root_pem  = ""
dao_proposal   = "https://snapshot.org/#/snapshot.dcl.eth/proposal/0xabc"
added_at       = "2026-05-30"

[[peer]]
peer_id        = "peer.example"
catalyst_url   = "https://peer.example/content"
gossip_pubkey  = [
    32,31,30,29,28,27,26,25,24,23,22,21,20,19,18,17,
    16,15,14,13,12,11,10,9,8,7,6,5,4,3,2,1,
]
dao_proposal   = "https://snapshot.org/#/snapshot.dcl.eth/proposal/0xdef"
added_at       = "2026-05-15"
"#;

#[test]
fn peer_file_valid_loads_expected_entries() {
    let path = write_tmp("valid.toml", VALID_TWO_PEERS);
    let reg = FederationRegistry::from_file(&path).expect("valid peer file should load");

    assert!(reg.contains("interconnected.online"));
    assert!(reg.contains("peer.example"));
    assert!(!reg.contains("not-in-list"));

    let mut all: Vec<String> = reg.all().into_iter().map(|p| p.peer_id).collect();
    all.sort();
    assert_eq!(all, vec!["interconnected.online", "peer.example"]);

    let p = reg.get("interconnected.online").unwrap();
    assert_eq!(p.catalyst_url, "https://interconnected.online/content");
    assert_eq!(p.version, 1, "version should default to 1 when omitted");
    assert!(p.dao_proposal.contains("snapshot.dcl.eth"));
    assert_eq!(
        p.worlds_url, "",
        "worlds_url must default to empty so every pre-existing peer file stays valid; \
         empty means 'this peer runs no worlds server', not 'derive one from catalyst_url'"
    );

    let audit = reg.audit();
    assert_eq!(audit.len(), 2);
    let interconnected = audit
        .iter()
        .find(|a| a.peer_id == "interconnected.online")
        .unwrap();
    assert!(interconnected.dao_proposal.contains("0xabc"));
    assert_eq!(interconnected.added_at, "2026-05-30");
}

#[test]
fn peer_file_worlds_url_round_trips_when_present() {
    let body = r#"
[[peer]]
peer_id        = "worldsy.peer"
catalyst_url   = "https://worldsy.peer/content"
worlds_url     = "https://worlds.worldsy.peer"
gossip_pubkey  = [
    1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,
    1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,
]
dao_proposal   = "https://snapshot.org/#/snapshot.dcl.eth/proposal/0xworlds"
added_at       = "2026-06-01"
"#;
    let path = write_tmp("worlds-url.toml", body);
    let reg = FederationRegistry::from_file(&path).expect("worlds_url should parse");
    let p = reg.get("worldsy.peer").unwrap();
    assert_eq!(p.worlds_url, "https://worlds.worldsy.peer");
    assert_eq!(
        p.catalyst_url, "https://worldsy.peer/content",
        "worlds_url must not be conflated with catalyst_url"
    );
}

#[test]
fn peer_file_invalid_toml_rejected_cleanly() {
    let path = write_tmp("broken.toml", "this is not = valid = toml [[");
    let err = FederationRegistry::from_file(&path).expect_err("must reject malformed TOML");
    let msg = format!("{err}");
    assert!(
        msg.contains("peer file"),
        "error should mention peer file: {msg}"
    );
}

#[test]
fn peer_file_missing_dao_proposal_rejected() {
    let body = r#"
[[peer]]
peer_id        = "bad.peer"
catalyst_url   = "https://bad.peer/content"
gossip_pubkey  = [
    0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,
    0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,
]
added_at       = "2026-05-30"
"#;
    let path = write_tmp("missing-dao.toml", body);
    let err = FederationRegistry::from_file(&path).expect_err("missing dao_proposal must reject");
    let msg = format!("{err}");
    assert!(
        msg.contains("dao_proposal"),
        "error should call out dao_proposal: {msg}"
    );
}

#[test]
fn peer_file_missing_added_at_rejected() {
    let body = r#"
[[peer]]
peer_id        = "bad.peer"
catalyst_url   = "https://bad.peer/content"
gossip_pubkey  = [
    0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,
    0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,
]
dao_proposal   = "https://snapshot.org/#/snapshot.dcl.eth/proposal/0xfoo"
"#;
    let path = write_tmp("missing-added-at.toml", body);
    let err = FederationRegistry::from_file(&path).expect_err("missing added_at must reject");
    assert!(format!("{err}").contains("added_at"));
}

/// Two entries naming the same peer are a refusal, not a merge.
///
/// `HashMap::insert` returns the displaced value, and this loop used to discard it, so
/// a file could carry two complete entries -- two DAO proposals, two pinned roots, two
/// hosts -- and boot a registry holding exactly one of them, chosen by document order.
/// In `catalyrst-worlds` that made two admitted peers share one mirror namespace, and
/// whichever polled second wiped the first's rows.
#[test]
fn peer_file_naming_one_peer_twice_is_refused_naming_both_entries() {
    let body = r#"
[[peer]]
peer_id        = "twin.peer"
catalyst_url   = "https://twin.peer/content"
gossip_pubkey  = [
    1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,
    1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,
]
dao_proposal   = "https://snapshot.org/#/snapshot.dcl.eth/proposal/0xaaa"
added_at       = "2026-06-01"

[[peer]]
peer_id        = "Twin.Peer"
catalyst_url   = "https://elsewhere.example/content"
gossip_pubkey  = [
    2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,
    2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,
]
dao_proposal   = "https://snapshot.org/#/snapshot.dcl.eth/proposal/0xbbb"
added_at       = "2026-06-02"
"#;
    let path = write_tmp("case-variant-twins.toml", body);
    let err = FederationRegistry::from_file(&path)
        .expect_err("a file naming one peer twice must be refused, not silently merged");
    let msg = format!("{err}");
    assert!(
        msg.contains("twin.peer") && msg.contains("Twin.Peer"),
        "the refusal must name BOTH entries in the spelling the operator used, so the \
         two offending lines can be found: {msg}"
    );
}

/// An *exact* duplicate is the same defect without the case variance, and was equally
/// silent. It is refused for the same reason.
#[test]
fn peer_file_with_a_verbatim_duplicate_entry_is_refused() {
    let one = r#"
[[peer]]
peer_id        = "dupe.peer"
catalyst_url   = "https://dupe.peer/content"
gossip_pubkey  = [
    3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,
    3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,
]
dao_proposal   = "https://snapshot.org/#/snapshot.dcl.eth/proposal/0xccc"
added_at       = "2026-06-03"
"#;
    let path = write_tmp("verbatim-dupe.toml", &format!("{one}{one}"));
    let err = FederationRegistry::from_file(&path).expect_err("a duplicated entry must be refused");
    assert!(format!("{err}").contains("dupe.peer"));
}

/// Ids are canonicalised once, at parse time. After that the registry holds no raw
/// spelling at all, and lookups fold the needle the same way -- so a caller cannot miss
/// a peer it does hold by writing the id in a different case.
#[test]
fn peer_ids_are_canonicalised_at_parse_time_and_lookups_fold_to_match() {
    let body = r#"
[[peer]]
peer_id        = "  MiXeD.Case.Peer  "
catalyst_url   = "https://mixed.case.peer/content"
gossip_pubkey  = [
    4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,
    4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,
]
dao_proposal   = "https://snapshot.org/#/snapshot.dcl.eth/proposal/0xddd"
added_at       = "2026-06-04"
"#;
    let path = write_tmp("mixed-case.toml", body);
    let reg = FederationRegistry::from_file(&path).expect("a single mixed-case entry is fine");

    let stored: Vec<String> = reg.all().into_iter().map(|p| p.peer_id).collect();
    assert_eq!(
        stored,
        vec!["mixed.case.peer"],
        "the stored id is canonical; the registry keeps no raw spelling for a consumer \
         to re-fold differently"
    );
    assert_eq!(
        reg.audit()[0].peer_id,
        "mixed.case.peer",
        "the audit view reports the canonical id too"
    );

    for spelling in ["mixed.case.peer", "MIXED.CASE.PEER", " MiXeD.Case.Peer "] {
        assert!(reg.contains(spelling), "contains({spelling:?}) must hit");
        assert!(reg.get(spelling).is_some(), "get({spelling:?}) must hit");
    }
    assert!(!reg.contains("other.peer"));

    assert_eq!(
        catalyrst_fed::canonical_peer_id("  MiXeD.Case.Peer  "),
        "mixed.case.peer"
    );
}

#[test]
fn reload_swaps_set_atomically() {
    let path = write_tmp("reload-initial.toml", VALID_TWO_PEERS);
    let reg = FederationRegistry::from_file(&path).unwrap();

    let mut before: Vec<String> = reg.all().into_iter().map(|p| p.peer_id).collect();
    before.sort();
    assert_eq!(before, vec!["interconnected.online", "peer.example"]);

    let new_body = r#"
[[peer]]
peer_id        = "fresh.peer"
catalyst_url   = "https://fresh.peer/content"
gossip_pubkey  = [
    9,9,9,9,9,9,9,9,9,9,9,9,9,9,9,9,
    9,9,9,9,9,9,9,9,9,9,9,9,9,9,9,9,
]
dao_proposal   = "https://snapshot.org/#/snapshot.dcl.eth/proposal/0xfresh"
added_at       = "2026-05-29"
"#;
    let new_path = write_tmp("reload-new.toml", new_body);
    reg.reload(&new_path).unwrap();

    assert_eq!(before, vec!["interconnected.online", "peer.example"]);

    let after: Vec<String> = reg.all().into_iter().map(|p| p.peer_id).collect();
    assert_eq!(after, vec!["fresh.peer"]);
    assert!(!reg.contains("interconnected.online"));
    assert!(reg.contains("fresh.peer"));
}

#[test]
fn reload_failure_leaves_prior_set_intact() {
    let path = write_tmp("reload-keep-initial.toml", VALID_TWO_PEERS);
    let reg = FederationRegistry::from_file(&path).unwrap();

    let bad = write_tmp("reload-broken.toml", "garbage = = =");
    let err = reg.reload(&bad);
    assert!(err.is_err(), "broken reload must return Err");

    assert!(reg.contains("interconnected.online"));
    assert!(reg.contains("peer.example"));
}
