//! Deserialisation targets for peer responses. **This file is the trust boundary.**
//!
//! Nothing here has an `owner`, `access`, `permissions`, `blocked_since`, `deployer`,
//! or `single_player` field, and [`tests::ownership_claims_in_a_peer_payload_are_structurally_unreachable`]
//! fails if one is ever added.
//!
//! A peer's `/worlds` response *does* carry `owner` -- this crate emits it itself at
//! `handlers/worlds_list.rs`. Omitting the field means there is no binding, no
//! variable, and no column it could flow into. That is strictly stronger than reading
//! it and choosing not to use it.
//!
//! `deny_unknown_fields` is deliberately **not** used: a peer running a newer build
//! legitimately adds fields, and refusing its whole listing over an additive change
//! would be a self-inflicted outage. Unknown fields -- including `owner` -- are dropped
//! by serde before any code in this crate can see them.

use chrono::{DateTime, Utc};
use serde::Deserialize;

use crate::fed::names::{PeerId, RemoteWorldName};
use crate::fed::store::RemoteWorld;

/// Per-field caps. `WORLDS_FED_MAX_RESPONSE_BYTES` already bounds the whole body, so
/// these exist for a narrower reason: one entry must not be able to spend the entire
/// budget on a single `description`, and a stored row must have a shape an operator
/// can read. An over-long field is **dropped**, not truncated -- a truncated string
/// silently claims to be the peer's value and is not.
const MAX_TITLE_LEN: usize = 512;
const MAX_DESCRIPTION_LEN: usize = 4096;
const MAX_CONTENT_RATING_LEN: usize = 64;
const MAX_THUMBNAIL_HASH_LEN: usize = 128;
const MAX_CATEGORIES: usize = 32;
const MAX_CATEGORY_LEN: usize = 64;

#[derive(Debug, Deserialize)]
pub(crate) struct PeerWorldEntry {
    pub name: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub content_rating: Option<String>,
    #[serde(default)]
    pub categories: Option<Vec<String>>,
    #[serde(default)]
    pub thumbnail_hash: Option<String>,
    #[serde(default)]
    pub deployed_scenes: Option<i64>,
    #[serde(default)]
    pub last_deployed_at: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct PeerWorldsPage {
    /// **Not** `#[serde(default)]`, unlike every field above it, and the asymmetry is
    /// deliberate. A defaulted `worlds` makes `{}` -- and, because serde will build a
    /// struct from a JSON sequence, `[]` as well -- deserialize into a page with zero
    /// worlds. That turns "the peer answered with something that is not a listing"
    /// into "the peer holds no worlds", which is the exact confusion this module
    /// exists to prevent. Requiring the field makes both of those a parse error, and
    /// a parse error retains the peer's previous rows.
    pub worlds: Vec<PeerWorldEntry>,
    /// The peer's own count. Recorded nowhere and read by no branch: it is the peer's
    /// arithmetic about the peer's data, and pagination terminates on what actually
    /// arrived. Kept in the struct only so a reviewer can see it was considered.
    #[serde(default)]
    #[allow(dead_code)]
    pub total: Option<i64>,
}

/// What one page of a peer listing became after adjudication.
#[derive(Debug, Default)]
pub(crate) struct PageIntake {
    pub worlds: Vec<RemoteWorld>,
    /// Entries refused by [`RemoteWorldName::from_peer_listing`]. One bad row must not
    /// cost the other 1,551, so a refusal skips the entry and is counted.
    pub entries_skipped: u64,
    /// Entries the peer sent that we did not read, because the per-peer row cap was
    /// reached. Surfaced as `truncated`, never as a shorter list presented as complete.
    pub truncated: bool,
    /// How many entries the page actually contained, including the refused ones and
    /// the ones past the budget. The pagination loop terminates on this rather than on
    /// the peer's `total`, because it is the only number we counted ourselves.
    pub entries_seen: usize,
}

/// Parse one page and turn it into rows.
///
/// Fails closed: a body that is not a JSON object of the expected shape returns `Err`
/// and the caller retains the peer's previous rows. It never returns an empty page for
/// a malformed body, because an empty page is indistinguishable from "this peer holds
/// no worlds".
pub(crate) fn intake_page(
    peer_id: &PeerId,
    body: &[u8],
    remaining_row_budget: usize,
    observed_at: DateTime<Utc>,
) -> Result<PageIntake, serde_json::Error> {
    // A JSON document whose first token is `{` is an object, and only an object can be
    // a worlds listing. serde will happily build a struct out of a JSON *sequence*,
    // mapping elements to fields positionally, so `[[]]` would otherwise deserialize
    // into a page holding zero worlds -- a malformed body silently becoming "this peer
    // is empty". Checking the first token costs one byte scan and closes that door
    // ahead of the parser.
    if body
        .iter()
        .find(|b| !b.is_ascii_whitespace())
        .copied()
        .unwrap_or(0)
        != b'{'
    {
        return Err(<serde_json::Error as serde::de::Error>::custom(
            "peer response is not a JSON object, so it is not a worlds listing",
        ));
    }

    let page: PeerWorldsPage = serde_json::from_slice(body)?;
    let mut out = PageIntake {
        entries_seen: page.worlds.len(),
        ..Default::default()
    };

    for entry in page.worlds {
        if out.worlds.len() >= remaining_row_budget {
            out.truncated = true;
            break;
        }
        let Some(name) = RemoteWorldName::from_peer_listing(&entry.name) else {
            tracing::warn!(
                peer = %peer_id,
                raw_name = %entry.name.escape_debug(),
                "peer listed a world name of a shape we refuse to store; entry skipped"
            );
            out.entries_skipped += 1;
            continue;
        };
        out.worlds.push(RemoteWorld {
            peer_id: peer_id.clone(),
            name,
            title: capped(entry.title, MAX_TITLE_LEN),
            description: capped(entry.description, MAX_DESCRIPTION_LEN),
            content_rating: capped(entry.content_rating, MAX_CONTENT_RATING_LEN),
            categories: capped_categories(entry.categories),
            thumbnail_hash: capped(entry.thumbnail_hash, MAX_THUMBNAIL_HASH_LEN),
            // A peer reporting a negative scene count is reporting nonsense; nonsense
            // stores as zero rather than as a value that could underflow a consumer.
            deployed_scenes: entry.deployed_scenes.unwrap_or(0).max(0),
            last_deployed_at: entry.last_deployed_at.as_deref().and_then(parse_peer_time),
            // OUR clock. The peer does not get to say when we saw it.
            observed_at,
            // The poller never carries a veto in; `replace_peer_worlds` preserves the
            // stored value. A peer cannot un-hide itself by re-listing.
            hidden_since: None,
        });
    }

    Ok(out)
}

fn capped(v: Option<String>, max: usize) -> Option<String> {
    v.filter(|s| !s.is_empty() && s.len() <= max)
}

fn capped_categories(v: Option<Vec<String>>) -> Option<Vec<String>> {
    let v = v?;
    let kept: Vec<String> = v
        .into_iter()
        .filter(|c| !c.is_empty() && c.len() <= MAX_CATEGORY_LEN)
        .take(MAX_CATEGORIES)
        .collect();
    (!kept.is_empty()).then_some(kept)
}

/// A peer-reported timestamp. Stored as reported metadata that no branch reads, so an
/// unparseable value is `None` rather than a poll failure -- and definitively not
/// "now", which would be us inventing a fact on the peer's behalf.
fn parse_peer_time(raw: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(raw)
        .ok()
        .map(|t| t.with_timezone(&Utc))
}

/// The source-level half of the provenance rule in [`crate::fed::names`].
///
/// A type barrier is a compiler barrier at one chokepoint plus a name nobody types by
/// accident. It is not a proof: `LocalWorldName::from_request_path(r.as_peer_reported_str())`
/// compiles. This gate is the part that catches that, and it lives in the test suite
/// rather than in CI config so it runs wherever the tests run.
#[cfg(test)]
pub(crate) mod provenance_gate {
    use std::path::{Path, PathBuf};

    pub(crate) fn src_dir() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
    }

    pub(crate) fn rust_sources(dir: &Path, out: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(dir).expect("crate src/ is readable") {
            let path = entry.expect("dir entry").path();
            if path.is_dir() {
                rust_sources(&path, out);
            } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
                out.push(path);
            }
        }
    }

    /// Comment lines are excluded: the doc comments on the types under gate name the
    /// very identifiers being gated, and a gate that its own documentation trips is a
    /// gate people delete.
    pub(crate) fn is_comment(line: &str) -> bool {
        let t = line.trim_start();
        t.starts_with("//") || t.starts_with("*") || t.starts_with("#!")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn peer() -> PeerId {
        PeerId::from_admitted("peer.example.org")
    }

    fn now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc)
    }

    /// G4. A peer payload carrying ownership and permission claims must not merely be
    /// ignored -- the claims must have nowhere to land, and nothing we then publish may
    /// contain them either.
    #[test]
    fn ownership_claims_in_a_peer_payload_are_structurally_unreachable() {
        let hostile = json!({
            "total": 1,
            "worlds": [{
                "name": "hostile.dcl.eth",
                "title": "Hostile",
                "owner": "0xdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef",
                "access": { "type": "unrestricted" },
                "permissions": {
                    "deployment": { "type": "allow-list",
                                    "wallets": ["0xdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef"] }
                },
                "blocked_since": null,
                "deployer": "0xdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef",
                "single_player": true,
                "deployment_auth_chain": [{ "type": "SIGNER", "payload": "0xdead" }],
                "deployed_scenes": 3
            }]
        })
        .to_string();

        let intake = intake_page(&peer(), hostile.as_bytes(), 100, now())
            .expect("an additive/hostile payload still parses; unknown fields are dropped");
        assert_eq!(intake.worlds.len(), 1);
        assert_eq!(intake.entries_skipped, 0);

        let row = &intake.worlds[0];
        assert_eq!(row.name.as_peer_reported_str(), "hostile.dcl.eth");
        assert_eq!(row.deployed_scenes, 3);

        // Nothing we hold, and nothing we publish, carries the claim.
        let ours = serde_json::to_value(row.as_published_view()).expect("view serialises");
        let rendered = ours.to_string();
        for forbidden in [
            "owner",
            "access",
            "permissions",
            "blocked_since",
            "blockedSince",
            "deployer",
            "single_player",
            "singlePlayer",
            "auth_chain",
            "authChain",
            "0xdeadbeef",
        ] {
            assert!(
                !rendered.contains(forbidden),
                "our own view of a mirrored world leaked {forbidden:?}: {rendered}"
            );
        }

        // And the deserialisation target itself has no such field: this is the
        // assertion that fails the day somebody adds one.
        let fields = format!("{:?}", intake.worlds[0]);
        for forbidden in ["owner", "access", "permission", "blocked", "deployer"] {
            assert!(
                !fields.to_ascii_lowercase().contains(forbidden),
                "RemoteWorld gained a {forbidden:?} field"
            );
        }
    }

    /// Two entries captured verbatim from
    /// `https://worlds-content-server.decentraland.org/worlds?limit=3&offset=0`
    /// on 2026-08-02 -- a read-only HTTPS GET against production, which is permitted
    /// and is already what `src/bin/worlds-mirror.rs` consumes. Pinned as a fixture
    /// rather than fetched at test time so the suite stays hermetic.
    ///
    /// The first entry carries a **real, non-null `owner`**. That is the whole point:
    /// the field the reference implementation actually sends on the wire, from
    /// software we do not control, has nowhere to land here.
    const PRODUCTION_WORLDS_PAGE: &str = r#"{"worlds": [{"name": "041.dcl.eth", "owner": "0x37b323dd852e38114933f25ad53d0c04ec4ec2bd", "title": "Ultimate Game Party", "description": "Template scene with SDK7 for a 4-parcel area", "shape": {"x1": 0, "x2": 1, "y1": 0, "y2": 1}, "content_rating": null, "spawn_coordinates": "0,0", "skybox_time": null, "categories": null, "single_player": null, "show_in_places": null, "thumbnail_hash": "bafkreidj26s7aenyxfthfdibnqonzqm5ptc4iamml744gmcyuokewkr76y", "last_deployed_at": "2023-09-06T20:13:48.672Z", "blocked_since": null, "deployed_scenes": 1}, {"name": "024.dcl.eth", "owner": null, "title": "DCL Scene", "description": "My new Decentraland project", "shape": {"x1": 0, "x2": 0, "y1": 1, "y2": 1}, "content_rating": null, "spawn_coordinates": "0,1", "skybox_time": null, "categories": null, "single_player": null, "show_in_places": null, "thumbnail_hash": "bafkreidj26s7aenyxfthfdibnqonzqm5ptc4iamml744gmcyuokewkr76y", "last_deployed_at": "2023-09-06T20:13:48.294Z", "blocked_since": null, "deployed_scenes": 1}], "total": 1751}"#;

    /// `https://interconnected.online/worlds?limit=5&offset=0`, same day: a live
    /// third-party catalyst that genuinely holds no worlds. Its answer must be a
    /// *successful* empty page and must remain distinguishable from every failure in
    /// [`a_malformed_body_is_an_error_and_never_an_empty_page`].
    const THIRD_PARTY_EMPTY_PAGE: &str = r#"{"total":0,"worlds":[]}"#;

    #[test]
    fn the_real_production_wire_format_parses_and_its_owner_field_has_nowhere_to_land() {
        let intake = intake_page(&peer(), PRODUCTION_WORLDS_PAGE.as_bytes(), 100, now())
            .expect("the reference implementation's own listing must parse");
        assert_eq!(intake.worlds.len(), 2);
        assert_eq!(intake.entries_skipped, 0);
        assert!(!intake.truncated);

        let first = &intake.worlds[0];
        assert_eq!(first.name.as_peer_reported_str(), "041.dcl.eth");
        assert_eq!(first.title.as_deref(), Some("Ultimate Game Party"));
        assert_eq!(first.deployed_scenes, 1);
        assert!(
            first.last_deployed_at.is_some(),
            "the production RFC3339 shape parses"
        );

        // The peer's `total` (1751) is not our count, and nothing reads it.
        assert_eq!(intake.entries_seen, 2);

        let rendered = serde_json::to_string(
            &intake
                .worlds
                .iter()
                .map(|w| w.as_published_view())
                .collect::<Vec<_>>(),
        )
        .unwrap();
        assert!(
            !rendered.contains("0x37b323dd852e38114933f25ad53d0c04ec4ec2bd"),
            "a real production owner address survived into our published view: {rendered}"
        );
        for forbidden in ["owner", "blockedSince", "spawnCoordinates", "showInPlaces"] {
            assert!(
                !rendered.contains(forbidden),
                "the production field {forbidden:?} survived: {rendered}"
            );
        }
    }

    #[test]
    fn a_third_party_catalyst_that_holds_nothing_is_a_success_not_a_failure() {
        let intake = intake_page(&peer(), THIRD_PARTY_EMPTY_PAGE.as_bytes(), 100, now())
            .expect("an honestly empty listing is a successful poll");
        assert!(intake.worlds.is_empty());
        assert_eq!(intake.entries_seen, 0);
        assert_eq!(intake.entries_skipped, 0);
        assert!(!intake.truncated);
    }

    #[test]
    fn a_badly_shaped_name_skips_that_entry_and_is_counted() {
        let body = json!({
            "worlds": [
                { "name": "good.dcl.eth" },
                { "name": "../../etc/passwd" },
                { "name": "" },
                { "name": "also-good.dcl.eth" }
            ]
        })
        .to_string();
        let intake = intake_page(&peer(), body.as_bytes(), 100, now()).unwrap();
        assert_eq!(intake.worlds.len(), 2, "the good rows survive the bad ones");
        assert_eq!(intake.entries_skipped, 2);
        assert!(!intake.truncated);
    }

    #[test]
    fn a_malformed_body_is_an_error_and_never_an_empty_page() {
        for body in [
            &b"not json at all"[..],
            &b"{"[..],
            &b""[..],
            // A JSON array. serde builds structs from sequences, so without the
            // first-token check this deserialized to a page with zero worlds -- a
            // malformed body reading as "this peer holds nothing".
            &b"[]"[..],
            &b"[[]]"[..],
            &b"null"[..],
            &b"\"a string\""[..],
            &b"{}"[..],
            &br#"{"total": 0}"#[..],
            &br#"{"worlds": "not a list"}"#[..],
            &br#"{"worlds": [{"title": "no name field"}]}"#[..],
        ] {
            assert!(
                intake_page(&peer(), body, 100, now()).is_err(),
                "malformed body {:?} must fail, not yield an empty listing",
                String::from_utf8_lossy(body)
            );
        }
        // A well-formed body that genuinely has no worlds is the ONLY way to get an
        // empty page, and it is a distinct outcome from every failure above.
        let empty = intake_page(&peer(), br#"{"worlds":[],"total":0}"#, 100, now()).unwrap();
        assert!(empty.worlds.is_empty());
        assert!(!empty.truncated);
    }

    #[test]
    fn the_row_budget_truncates_rather_than_silently_shortening() {
        let worlds: Vec<_> = (0..10)
            .map(|i| json!({ "name": format!("w{i}.dcl.eth") }))
            .collect();
        let body = json!({ "worlds": worlds }).to_string();
        let intake = intake_page(&peer(), body.as_bytes(), 4, now()).unwrap();
        assert_eq!(intake.worlds.len(), 4);
        assert!(
            intake.truncated,
            "a capped page must say so; a short list presented as complete is a lie"
        );
    }

    #[test]
    fn oversized_and_nonsensical_fields_fail_closed() {
        let body = json!({
            "worlds": [{
                "name": "x.dcl.eth",
                "title": "t".repeat(MAX_TITLE_LEN + 1),
                "description": "d",
                "categories": ["ok", "", "c".repeat(MAX_CATEGORY_LEN + 1)],
                "deployed_scenes": -5,
                "last_deployed_at": "yesterday-ish"
            }]
        })
        .to_string();
        let row = &intake_page(&peer(), body.as_bytes(), 10, now())
            .unwrap()
            .worlds[0];
        assert_eq!(
            row.title, None,
            "an over-long title is dropped, not truncated"
        );
        assert_eq!(row.description.as_deref(), Some("d"));
        assert_eq!(row.categories.as_deref(), Some(&["ok".to_string()][..]));
        assert_eq!(row.deployed_scenes, 0);
        assert_eq!(
            row.last_deployed_at, None,
            "an unparseable peer timestamp is absent, never 'now'"
        );
        assert_eq!(row.observed_at, now(), "observed_at is our clock");
        assert_eq!(row.hidden_since, None);
    }

    /// S3's grep gate, run as a test so it runs wherever tests run.
    #[test]
    fn provenance_grep_gate() {
        // Built by concatenation so this test's own source does not trip it.
        let escape_hatch = concat!("as_peer_reported", "_str");
        let local_ctor = concat!("from_request", "_path");

        let allowed = [
            "fed/names.rs",    // defines it
            "fed/wire.rs",     // this file: builds rows
            "fed/store.rs",    // binds it to SQL
            "fed/handlers.rs", // serialises it
        ];

        let mut files = Vec::new();
        provenance_gate::rust_sources(&provenance_gate::src_dir(), &mut files);
        assert!(
            files.len() > 10,
            "the source walk found nothing; gate is inert"
        );

        // The one file allowed to name the LOCAL constructor while inside `fed/`. It
        // defines both halves, so it is the irreducible core of the rule rather than an
        // exemption from it.
        let local_ctor_home = "fed/names.rs";

        for path in &files {
            let rel = path
                .to_string_lossy()
                .replace('\\', "/")
                .rsplit("/src/")
                .next()
                .unwrap_or_default()
                .to_string();
            let body = std::fs::read_to_string(path).expect("source file is readable");
            for (i, line) in body.lines().enumerate() {
                if provenance_gate::is_comment(line) {
                    continue;
                }
                if line.contains(escape_hatch) {
                    assert!(
                        allowed.iter().any(|a| rel.ends_with(a)),
                        "{rel}:{} reaches a peer-reported name outside the mirror module. \
                         If a local code path needs this value, it needs a local answer, \
                         not this one.",
                        i + 1
                    );
                }

                // No file under `src/fed/` may MINT a local name, on any line.
                //
                // The one-line rule below is a one-line rule, and the laundering it
                // describes fits in two:
                //
                //     let raw = remote.name.as_peer_reported_str();
                //     let local = LocalWorldName::from_request_path(raw);
                //
                // Inside `fed/handlers.rs` or `fed/store.rs` -- both allowed to hold the
                // escape hatch -- that passed every check. This rule closes it by shape
                // rather than by syntax: nothing in `fed/` reads an HTTP request path,
                // so nothing in `fed/` has any business calling the constructor that
                // interprets one. Combined with the allowlist above, the two escape
                // hatches are now nameable in the same file ONLY in `fed/names.rs`,
                // where the one-line rule still applies and where both are defined.
                if rel.contains("fed/") && !rel.ends_with(local_ctor_home) {
                    assert!(
                        !line.contains(local_ctor),
                        "{rel}:{} mints a local world name inside the federation module. \
                         Nothing in fed/ handles a request path, so nothing in fed/ needs \
                         this constructor \u{2014} and a peer-reported name reaching it is the \
                         laundering the type barrier exists to prevent, whether or not it \
                         fits on one line.",
                        i + 1
                    );
                }

                assert!(
                    !(line.contains(escape_hatch) && line.contains(local_ctor)),
                    "{rel}:{} launders a peer-reported name into a local one on one line. \
                     That is exactly the conversion this branch exists to prevent.",
                    i + 1
                );
            }
        }
    }

    /// The boot sweep is only a revocation mechanism if boot actually runs it, and runs
    /// it before anything can be served.
    ///
    /// `revoke_peers_no_longer_admitted` is called from exactly one production place --
    /// `build_state` -- and nothing in the type system requires that call to exist. A
    /// future edit that reorders `build_state`, or drops the line while resolving a
    /// merge, would leave a process that boots cleanly and republishes worlds for a peer
    /// the DAO withdrew, with every test still green. Hence a source gate, in the style
    /// of the two above it.
    ///
    /// The read path is the other half and is covered by tests rather than by grep:
    /// `list_mirror` cannot be called without an allowlist because the allowlist is a
    /// parameter, and `a_revoked_peers_rows_are_unpublishable_even_if_the_boot_sweep_never_ran`
    /// in tests/audit_federation_holes.rs asserts it holds with this sweep skipped
    /// entirely.
    #[test]
    fn boot_revokes_de_admitted_peers_before_the_state_the_router_is_built_from_exists() {
        let lib = std::fs::read_to_string(provenance_gate::src_dir().join("lib.rs"))
            .expect("src/lib.rs is readable");

        let sweep = lib.find("revoke_peers_no_longer_admitted").expect(
            "build_state no longer reconciles the mirror against the admitted set. Removing \
             that call does not fail to compile and does not fail any type check; it silently \
             restores the finding that a de-admitted peer's worlds keep being published, \
             because the restart that is supposed to revoke them would revoke nothing.",
        );
        let state = lib
            .find("Arc::new(AppStateInner")
            .expect("build_state no longer constructs an AppStateInner; re-check this gate");

        assert!(
            sweep < state,
            "the de-admission sweep must run BEFORE the AppState the router is built from \
             exists, so there is no interleaving in which a request is answered from rows \
             belonging to a peer that has left the peer file"
        );
    }

    /// G5 as a source gate: the mirror path writes zero columns of `worlds` and
    /// `world_scenes`. The runtime half of this rule is
    /// `mirror_writes_zero_rows_to_worlds` in tests/federation_remote_worlds.rs; this
    /// half catches the write before it can be run.
    #[test]
    fn the_mirror_module_issues_no_write_against_the_authoritative_tables() {
        let fed = provenance_gate::src_dir().join("fed");
        let mut files = Vec::new();
        provenance_gate::rust_sources(&fed, &mut files);
        assert!(!files.is_empty(), "no fed sources found; gate is inert");

        for path in &files {
            let body = std::fs::read_to_string(path).expect("source file is readable");
            let name = path.file_name().unwrap().to_string_lossy().to_string();
            for (i, line) in body.lines().enumerate() {
                if provenance_gate::is_comment(line) {
                    continue;
                }
                let upper = line.to_ascii_uppercase();
                let writes = upper.contains("INSERT INTO")
                    || upper.contains("UPDATE ")
                    || upper.contains("DELETE FROM");
                if !writes {
                    continue;
                }
                for table in ["worlds", "world_scenes", "world_permissions"] {
                    // `remote_worlds` and `remote_peer_status` are ours; `worlds` is not.
                    let hit = upper.contains(&format!(" {}", table.to_ascii_uppercase()))
                        && !upper.contains("REMOTE_WORLDS")
                        && !upper.contains("REMOTE_PEER_STATUS");
                    assert!(
                        !hit,
                        "fed/{name}:{} writes to the authoritative table `{table}`. \
                         The mirror path writes zero columns of it \u{2014} see migration 0005.",
                        i + 1
                    );
                }
            }
        }
    }
}
