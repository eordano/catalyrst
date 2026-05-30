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
fn peer_file_invalid_toml_rejected_cleanly() {
    let path = write_tmp("broken.toml", "this is not = valid = toml [[");
    let err = FederationRegistry::from_file(&path).expect_err("must reject malformed TOML");
    let msg = format!("{err}");
    assert!(msg.contains("peer file"), "error should mention peer file: {msg}");
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
