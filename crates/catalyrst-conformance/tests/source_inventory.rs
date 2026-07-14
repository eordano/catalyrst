use serde_json::Value;
use std::collections::HashSet;
use std::path::PathBuf;

fn fixture(name: &str) -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures")
        .join("explorer-boot")
        .join(name);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("reading {}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("parsing {}: {e}", path.display()))
}

fn corpus_ids() -> HashSet<String> {
    fixture("corpus.json")["entries"]
        .as_array()
        .expect("corpus.entries")
        .iter()
        .map(|e| {
            format!(
                "{} {}{}",
                e["method"].as_str().unwrap(),
                e["host"].as_str().unwrap(),
                e["path"].as_str().unwrap()
            )
        })
        .collect()
}

fn probe_ids() -> HashSet<String> {
    fixture("synthetic-probes.json")["probes"]
        .as_array()
        .expect("probes")
        .iter()
        .map(|p| p["id"].as_str().unwrap().to_string())
        .collect()
}

fn is_allowed_upstream(host: &str, allow: &HashSet<String>) -> bool {
    host == "__scene__"
        || host.ends_with(".thirdweb.com")
        || host.contains("thirdweb")
        || allow.contains(host)
}

#[test]
fn source_inventory_reconciles() {
    let corpus = corpus_ids();
    let probes = probe_ids();
    let inv = fixture("source-inventory.json");

    let allowed_reasons: HashSet<String> = inv["allowedDeferralReasons"]
        .as_array()
        .expect("allowedDeferralReasons")
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    let allowed_upstream: HashSet<String> = inv["allowedUpstreamHosts"]
        .as_array()
        .expect("allowedUpstreamHosts")
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();

    let shapes = inv["shapes"].as_array().expect("shapes");
    assert_eq!(
        inv["shapeCount"].as_u64().unwrap() as usize,
        shapes.len(),
        "shapeCount must equal shapes.len()"
    );

    let mut errors: Vec<String> = Vec::new();
    for s in shapes {
        let id = s["id"].as_str().unwrap_or("<no-id>");
        let host = s["host"].as_str().unwrap_or("");
        let disp = &s["disposition"];
        match disp["kind"].as_str() {
            Some("corpus") => {
                let r = disp["ref"].as_str().unwrap_or("");
                if !corpus.contains(r) {
                    errors.push(format!("{id}: corpus ref not in corpus.json: {r}"));
                }
            }
            Some("probe") => {
                let r = disp["ref"].as_str().unwrap_or("");
                if !probes.contains(r) {
                    errors.push(format!("{id}: probe ref not in synthetic-probes.json: {r}"));
                }
            }
            Some("browser-only") => {}
            Some("deferred") => {
                let reason = disp["reason"].as_str().unwrap_or("");
                if !allowed_reasons.contains(reason) {
                    errors.push(format!("{id}: deferral reason not allowed: {reason}"));
                }
                if reason == "documented-intentional-upstream-dependency"
                    && !is_allowed_upstream(host, &allowed_upstream)
                {
                    errors.push(format!(
                        "{id}: FIRST-PARTY host classified as upstream (must be corpus/probe/needs-real-wallet/desktop-shell/immutable): {host}"
                    ));
                }
                if let Some(ev) = disp["evidence"].as_str() {
                    if !probes.contains(ev) {
                        errors.push(format!("{id}: evidence probe not found: {ev}"));
                    }
                }
            }
            other => errors.push(format!(
                "{id}: unmapped/invalid disposition kind: {other:?}"
            )),
        }
    }

    assert!(
        errors.is_empty(),
        "source-inventory reconciliation failed ({} problem(s)):\n{}",
        errors.len(),
        errors.join("\n")
    );
}

#[test]
fn route_sweep_has_no_unresolved_gaps() {
    let sweep = fixture("route-sweep.json");
    let inv = fixture("source-inventory.json");

    let mut swept: HashSet<String> = HashSet::new();
    for r in sweep["rows"].as_array().expect("rows") {
        swept.insert(r["id"].as_str().unwrap().to_string());
    }
    let mut missing_from_sweep: Vec<String> = Vec::new();
    for s in inv["shapes"].as_array().unwrap() {
        let d = &s["disposition"];
        if d["kind"] == "deferred" && d["reason"] == "needs-real-wallet" {
            let host = s["host"].as_str().unwrap_or("");
            if host.ends_with(".decentraland.org") && !swept.contains(s["id"].as_str().unwrap()) {
                missing_from_sweep.push(s["id"].as_str().unwrap().to_string());
            }
        }
    }
    assert!(
        missing_from_sweep.is_empty(),
        "first-party needs-real-wallet shapes not covered by route-sweep.json:\n{}",
        missing_from_sweep.join("\n")
    );

    let mut gaps: Vec<String> = Vec::new();
    for r in sweep["rows"].as_array().unwrap() {
        let id = r["id"].as_str().unwrap_or("<no-id>");
        let mapping = r["mapping"].as_str().unwrap_or("");
        let sub_present = r["subdomain"]["routePresent"].as_bool();
        let gw = &r["gateway"];
        let gw_present = gw["routePresent"].as_bool();
        let sub_ok = sub_present == Some(true);
        let gw_ok = mapping != "dual" || gw_present == Some(true);
        if !(sub_ok && gw_ok) {
            gaps.push(format!(
                "{id} [{mapping}] subdomain={:?} gateway={:?}",
                sub_present, gw_present
            ));
        }
    }
    assert!(
        gaps.is_empty(),
        "route-sweep unresolved gaps (each is an exact-path fix item for the conformance lane; may not be waived):\n{}",
        gaps.join("\n")
    );

    let recorded = sweep["gapCount"].as_u64().unwrap_or(u64::MAX);
    let computed = sweep["rows"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["pass"] == false)
        .count() as u64;
    assert_eq!(
        recorded, computed,
        "gapCount must equal the number of failing rows"
    );
}

#[test]
fn reconciliation_has_live_coverage() {
    let inv = fixture("source-inventory.json");
    let shapes = inv["shapes"].as_array().unwrap();
    let count = |kind: &str| {
        shapes
            .iter()
            .filter(|s| s["disposition"]["kind"] == kind)
            .count()
    };
    assert!(count("corpus") >= 20, "expected >=20 corpus-mapped shapes");
    assert!(count("probe") >= 10, "expected >=10 probe-mapped shapes");
    assert!(
        !probe_ids().is_empty() && !corpus_ids().is_empty(),
        "fixtures must be non-empty"
    );
}
