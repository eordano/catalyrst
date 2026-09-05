use anyhow::{Context, Result};
use colored::Colorize;
use serde::Deserialize;
use std::path::PathBuf;

use crate::diff::Difference;
use crate::retry::{is_transient_status, retry_with_backoff, RetryDecision};
use crate::{Args, Ctx, Outcome, Scoreboard};

const BASELINE_DOMAIN: &str = "decentraland.org";

#[derive(Deserialize)]
struct ExplorerCorpus {
    entries: Vec<CorpusEntry>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CorpusEntry {
    method: String,
    host: String,
    path: String,
    #[serde(default)]
    has_post_data: bool,
    sample_path: String,
}

impl CorpusEntry {
    fn content_addressed(&self) -> bool {
        self.path.contains("{cid}") || self.path.contains("{hash}")
    }
}

pub(crate) fn default_corpus_path() -> Option<PathBuf> {
    let crate_local = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures")
        .join("explorer-boot")
        .join("corpus.json");
    crate_local.exists().then_some(crate_local)
}

#[derive(Deserialize, Default)]
struct AcceptedDivergences {
    entries: Vec<AcceptedEntry>,
}

#[derive(Deserialize)]
struct AcceptedEntry {
    method: String,
    host: String,
    path: String,
    baseline: u16,
    candidate: u16,
    rationale: String,
}

impl AcceptedDivergences {
    fn load_default() -> Self {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("fixtures")
            .join("explorer-boot")
            .join("accepted-divergences.json");
        std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    fn rationale_for(
        &self,
        method: &str,
        host: &str,
        path: &str,
        baseline: u16,
        candidate: u16,
    ) -> Option<&str> {
        self.entries
            .iter()
            .find(|e| {
                e.method == method
                    && e.host == host
                    && e.path == path
                    && e.baseline == baseline
                    && e.candidate == candidate
            })
            .map(|e| e.rationale.as_str())
    }
}

const GATEWAY_PAIRS: &[(&str, &str)] = &[
    ("places", "places"),
    ("api", "api"),
    ("archipelago-ea-stats", "archipelago-ea-stats"),
    ("worlds-content-server", "worlds-content-server"),
    ("asset-bundle-registry", "asset-bundle-registry"),
    ("ab-cdn", "ab-cdn"),
    ("camera-reel-service", "camera-reel-service"),
    ("social-api", "social-api"),
    ("comms-gatekeeper", "comms-gatekeeper"),
    ("notifications", "notifications"),
    ("badges", "badges"),
    ("metamorph-api", "metamorph-api"),
    ("assets-cdn", "assets-cdn"),
    ("credits", "credits"),
    ("realm-provider-ea", "realm-provider-ea"),
    ("auth-api", "auth-api"),
    ("profile-images", "profile-images"),
    ("marketplace-api", "market"),
];

struct ProbeForm {
    label: &'static str,
    baseline_url: String,
    candidate_url: String,
}

fn forms_for(entry: &CorpusEntry, candidate_domain: &str) -> Vec<ProbeForm> {
    let mut forms = Vec::new();
    let baseline_url = format!("https://{}{}", entry.host, entry.sample_path);
    let candidate_host = entry.host.replace(BASELINE_DOMAIN, candidate_domain);

    if entry.host == format!("gateway.{}", BASELINE_DOMAIN) {
        forms.push(ProbeForm {
            label: "gateway",
            baseline_url: baseline_url.clone(),
            candidate_url: format!("https://{}{}", candidate_host, entry.sample_path),
        });
        if let Some((sub, gw)) = GATEWAY_PAIRS.iter().find(|(_, gw)| {
            entry
                .sample_path
                .strip_prefix('/')
                .and_then(|p| p.strip_prefix(gw))
                .is_some_and(|rest| rest.starts_with('/'))
        }) {
            let rest = &entry.sample_path[gw.len() + 1..];
            forms.push(ProbeForm {
                label: "subdomain",
                baseline_url,
                candidate_url: format!("https://{}.{}{}", sub, candidate_domain, rest),
            });
        }
    } else {
        let sub = entry
            .host
            .strip_suffix(&format!(".{}", BASELINE_DOMAIN))
            .unwrap_or("");
        let pair = GATEWAY_PAIRS.iter().find(|(s, _)| *s == sub);
        forms.push(ProbeForm {
            label: if pair.is_some() { "subdomain" } else { "" },
            baseline_url: baseline_url.clone(),
            candidate_url: format!("https://{}{}", candidate_host, entry.sample_path),
        });
        if let Some((_, gw)) = pair {
            forms.push(ProbeForm {
                label: "gateway",
                baseline_url,
                candidate_url: format!(
                    "https://gateway.{}/{}{}",
                    candidate_domain, gw, entry.sample_path
                ),
            });
        }
    }
    forms
}

fn content_type_difference(
    baseline_success: bool,
    baseline_type: &str,
    candidate_type: &str,
) -> Option<Difference> {
    if baseline_success && !baseline_type.is_empty() && baseline_type != candidate_type {
        Some(Difference {
            path: "content-type".to_string(),
            baseline_value: baseline_type.to_string(),
            candidate_value: candidate_type.to_string(),
        })
    } else {
        None
    }
}

pub(crate) async fn run_explorer_corpus_section(
    ctx: &Ctx,
    args: &Args,
    score: &mut Scoreboard,
) -> Result<()> {
    let explicitly_selected = args
        .only
        .as_ref()
        .is_some_and(|list| list.iter().any(|s| s.eq_ignore_ascii_case("explorer")))
        || args.explorer_corpus.is_some();

    if !explicitly_selected {
        return Ok(());
    }

    let corpus_path = args
        .explorer_corpus
        .clone()
        .or_else(default_corpus_path)
        .context(
            "no explorer corpus: pass --explorer-corpus or keep fixtures/explorer-boot/corpus.json",
        )?;

    let corpus: ExplorerCorpus = serde_json::from_str(
        &std::fs::read_to_string(&corpus_path)
            .with_context(|| format!("reading {}", corpus_path.display()))?,
    )
    .with_context(|| format!("parsing {}", corpus_path.display()))?;

    let accepted = AcceptedDivergences::load_default();

    println!("{}", "=== Explorer boot corpus ===".bold());
    println!(
        "  {} entries from {}; candidate domain: {}",
        corpus.entries.len(),
        corpus_path.display(),
        args.explorer_candidate_domain,
    );

    for entry in &corpus.entries {
        let base_label = format!("{} {}{}", entry.method, entry.host, entry.path);

        if entry.host != BASELINE_DOMAIN && !entry.host.ends_with(&format!(".{}", BASELINE_DOMAIN))
        {
            score.skip(&base_label, "external host, out of conformance scope");
            continue;
        }
        if entry.has_post_data {
            score.skip(&base_label, "signed POST, needs an identity auth chain");
            continue;
        }
        if entry.method != "GET" && entry.method != "HEAD" {
            score.skip(&base_label, "unsupported method");
            continue;
        }

        for form in forms_for(entry, &args.explorer_candidate_domain) {
            let label = if form.label.is_empty() {
                base_label.clone()
            } else {
                format!("{} [{}]", base_label, form.label)
            };

            let fetched = match fetch_pair(
                ctx,
                &label,
                &entry.method,
                &form.baseline_url,
                &form.candidate_url,
            )
            .await
            {
                Ok(f) => f,
                Err(e) => {
                    score.record_outcome(
                        Outcome::TransientSkip(format!("request failed: {e:#}")),
                        &label,
                        args.verbose,
                    );
                    continue;
                }
            };

            match fetched {
                PairResult::Transient(reason) => {
                    score.record_outcome(Outcome::TransientSkip(reason), &label, args.verbose)
                }
                PairResult::Got {
                    baseline_status,
                    candidate_status,
                    baseline_type,
                    candidate_type,
                } => {
                    if baseline_status != candidate_status {
                        if entry.content_addressed()
                            && baseline_status.is_success()
                            && candidate_status.as_u16() == 404
                        {
                            score.skip(&label, "content not baked on candidate (CID-addressed)");
                        } else if let Some(rationale) = accepted.rationale_for(
                            &entry.method,
                            &entry.host,
                            &entry.path,
                            baseline_status.as_u16(),
                            candidate_status.as_u16(),
                        ) {
                            score.accept(&label, rationale);
                        } else {
                            score.record_outcome(
                                Outcome::Diffs(vec![Difference {
                                    path: "HTTP status".to_string(),
                                    baseline_value: baseline_status.to_string(),
                                    candidate_value: candidate_status.to_string(),
                                }]),
                                &label,
                                args.verbose,
                            );
                        }
                    } else {
                        let diffs = content_type_difference(
                            baseline_status.is_success(),
                            &baseline_type,
                            &candidate_type,
                        )
                        .into_iter()
                        .collect();
                        score.record_outcome(Outcome::Diffs(diffs), &label, args.verbose);
                    }
                }
            }
            ctx.sleep_between().await;
        }
    }

    run_capability_checks(ctx, args, score).await;

    Ok(())
}

async fn get_json(
    ctx: &Ctx,
    url: &str,
) -> Result<(reqwest::StatusCode, Option<serde_json::Value>), String> {
    let resp = ctx
        .client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("request {url}: {e}"))?;
    let status = resp.status();
    let body = resp.json::<serde_json::Value>().await.ok();
    Ok((status, body))
}

fn required_item_key_diffs(
    body: &serde_json::Value,
    top_key: &str,
    required_item_keys: &[&str],
) -> Vec<Difference> {
    let mut diffs = Vec::new();
    let Some(items) = body.get(top_key).and_then(|d| d.as_array()) else {
        diffs.push(Difference {
            path: format!("top-level '{top_key}' array"),
            baseline_value: "present".to_string(),
            candidate_value: "absent".to_string(),
        });
        return diffs;
    };
    if let Some(first) = items.first() {
        let missing: Vec<&str> = required_item_keys
            .iter()
            .copied()
            .filter(|k| first.get(*k).is_none())
            .collect();
        if !missing.is_empty() {
            diffs.push(Difference {
                path: "required item keys".to_string(),
                baseline_value: required_item_keys.join(","),
                candidate_value: format!("missing: {}", missing.join(",")),
            });
        }
    }
    diffs
}

async fn run_capability_checks(ctx: &Ctx, args: &Args, score: &mut Scoreboard) {
    println!("{}", "=== Gateway capability checks ===".bold());
    let d = &args.explorer_candidate_domain;

    struct JsonCheck {
        label: String,
        baseline_url: String,
        candidate_url: String,
        top_key: &'static str,
        required_item_keys: &'static [&'static str],
    }
    const CATALOG_KEYS: &[&str] = &["id", "name", "category", "contractAddress", "rarity", "urn"];
    const PACK_KEYS: &[&str] = &["id", "usd", "credits", "order"];
    let checks = [
        JsonCheck {
            label: format!("GET marketplace-api.{d}/v1/catalog [subdomain]"),
            baseline_url: "https://marketplace-api.decentraland.org/v1/catalog?first=1".into(),
            candidate_url: format!("https://marketplace-api.{d}/v1/catalog?first=1"),
            top_key: "data",
            required_item_keys: CATALOG_KEYS,
        },
        JsonCheck {
            label: format!("GET gateway.{d}/market/v1/catalog [gateway]"),
            baseline_url: "https://marketplace-api.decentraland.org/v1/catalog?first=1".into(),
            candidate_url: format!("https://gateway.{d}/market/v1/catalog?first=1"),
            top_key: "data",
            required_item_keys: CATALOG_KEYS,
        },
        JsonCheck {
            label: format!("GET marketplace-api.{d}/v2/catalog [unity path]"),
            baseline_url: "https://marketplace-api.decentraland.org/v2/catalog?first=34".into(),
            candidate_url: format!("https://marketplace-api.{d}/v2/catalog?first=34"),
            top_key: "data",
            required_item_keys: CATALOG_KEYS,
        },
        JsonCheck {
            label: format!("GET credits.{d}/credits/packs [subdomain, unity path]"),
            baseline_url: "https://credits.decentraland.org/credits/packs".into(),
            candidate_url: format!("https://credits.{d}/credits/packs"),
            top_key: "packs",
            required_item_keys: PACK_KEYS,
        },
        JsonCheck {
            label: format!("GET gateway.{d}/credits/credits/packs [gateway]"),
            baseline_url: "https://credits.decentraland.org/credits/packs".into(),
            candidate_url: format!("https://gateway.{d}/credits/credits/packs"),
            top_key: "packs",
            required_item_keys: PACK_KEYS,
        },
    ];
    for check in checks {
        let baseline = get_json(ctx, &check.baseline_url).await;
        let candidate = get_json(ctx, &check.candidate_url).await;
        let ((bs, bb), (cs, cb)) = match (baseline, candidate) {
            (Ok(b), Ok(c)) => (b, c),
            (Err(e), _) | (_, Err(e)) => {
                score.record_outcome(Outcome::TransientSkip(e), &check.label, args.verbose);
                continue;
            }
        };
        if bs != cs {
            score.record_outcome(
                Outcome::Diffs(vec![Difference {
                    path: "HTTP status".to_string(),
                    baseline_value: bs.to_string(),
                    candidate_value: cs.to_string(),
                }]),
                &check.label,
                args.verbose,
            );
            continue;
        }
        let mut diffs = Vec::new();
        match (bb, cb) {
            (Some(baseline_body), Some(candidate_body)) => {
                diffs.extend(catalog_shape_sanity(&baseline_body, check.top_key));
                diffs.extend(required_item_key_diffs(
                    &candidate_body,
                    check.top_key,
                    check.required_item_keys,
                ));
            }
            (Some(_), None) => diffs.push(Difference {
                path: "body".to_string(),
                baseline_value: "JSON".to_string(),
                candidate_value: "not JSON".to_string(),
            }),
            _ => {}
        }
        score.record_outcome(Outcome::Diffs(diffs), &check.label, args.verbose);
        ctx.sleep_between().await;
    }

    let label = format!("GET renderer-artifacts.{d}/sdk7-adaption-layer/main/index.js");
    let baseline_url =
        "https://renderer-artifacts.decentraland.org/sdk7-adaption-layer/main/index.js";
    let candidate_url = format!("https://renderer-artifacts.{d}/sdk7-adaption-layer/main/index.js");
    match fetch_pair(ctx, &label, "HEAD", baseline_url, &candidate_url).await {
        Err(e) => score.record_outcome(
            Outcome::TransientSkip(format!("request failed: {e:#}")),
            &label,
            args.verbose,
        ),
        Ok(PairResult::Transient(reason)) => {
            score.record_outcome(Outcome::TransientSkip(reason), &label, args.verbose)
        }
        Ok(PairResult::Got {
            baseline_status,
            candidate_status,
            baseline_type,
            candidate_type,
        }) => {
            let mut diffs = Vec::new();
            if baseline_status != candidate_status {
                diffs.push(Difference {
                    path: "HTTP status".to_string(),
                    baseline_value: baseline_status.to_string(),
                    candidate_value: candidate_status.to_string(),
                });
            } else if let Some(d) = content_type_difference(
                baseline_status.is_success(),
                &baseline_type,
                &candidate_type,
            ) {
                diffs.push(d);
            }
            score.record_outcome(Outcome::Diffs(diffs), &label, args.verbose);
        }
    }
}

fn catalog_shape_sanity(baseline_body: &serde_json::Value, top_key: &str) -> Vec<Difference> {
    if baseline_body
        .get(top_key)
        .and_then(|d| d.as_array())
        .is_none()
    {
        vec![Difference {
            path: format!("baseline top-level '{top_key}' array"),
            baseline_value: "absent (baseline shape moved; update the check)".to_string(),
            candidate_value: String::new(),
        }]
    } else {
        Vec::new()
    }
}

enum PairResult {
    Got {
        baseline_status: reqwest::StatusCode,
        candidate_status: reqwest::StatusCode,
        baseline_type: String,
        candidate_type: String,
    },
    Transient(String),
}

async fn fetch_pair(
    ctx: &Ctx,
    label: &str,
    method: &str,
    baseline_url: &str,
    candidate_url: &str,
) -> Result<PairResult> {
    let outcome = retry_with_backoff(label, 3, 1000, || async {
        let send_one = |url: &str| {
            let req = if method == "HEAD" {
                ctx.client.head(url)
            } else {
                ctx.client.get(url)
            };
            req.send()
        };

        let (baseline, candidate) = tokio::try_join!(
            async {
                send_one(baseline_url)
                    .await
                    .with_context(|| format!("request {} (baseline)", baseline_url))
            },
            async {
                send_one(candidate_url)
                    .await
                    .with_context(|| format!("request {} (candidate)", candidate_url))
            },
        )?;

        if is_transient_status(baseline.status()) || is_transient_status(candidate.status()) {
            return Ok(RetryDecision::Retry {
                after: None,
                rate_limited: baseline.status().as_u16() == 429
                    || candidate.status().as_u16() == 429,
            });
        }

        let media_type = |r: &reqwest::Response| {
            r.headers()
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .map(|v| {
                    v.split(';')
                        .next()
                        .unwrap_or("")
                        .trim()
                        .to_ascii_lowercase()
                })
                .unwrap_or_default()
        };

        Ok(RetryDecision::Done(PairResult::Got {
            baseline_status: baseline.status(),
            candidate_status: candidate.status(),
            baseline_type: media_type(&baseline),
            candidate_type: media_type(&candidate),
        }))
    })
    .await?;

    Ok(outcome.unwrap_or_else(|| {
        PairResult::Transient("transient statuses persisted across retries".to_string())
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_type_absent_on_baseline_is_candidate_value_add() {
        assert!(content_type_difference(true, "", "application/json").is_none());
    }

    #[test]
    fn content_type_mismatch_with_both_present_is_a_difference() {
        let d = content_type_difference(true, "text/plain", "application/json").unwrap();
        assert_eq!(d.baseline_value, "text/plain");
        assert_eq!(d.candidate_value, "application/json");
    }

    #[test]
    fn content_type_ignored_on_non_success() {
        assert!(content_type_difference(false, "text/plain", "application/json").is_none());
    }

    fn ledger() -> AcceptedDivergences {
        serde_json::from_str(
            r#"{"entries":[{"method":"GET","host":"social-api.decentraland.org",
                "path":"/v{n}/mutes","baseline":400,"candidate":401,
                "rationale":"signed-fetch refusals answer 401"}]}"#,
        )
        .unwrap()
    }

    #[test]
    fn ledger_matches_only_the_recorded_status_pair() {
        let l = ledger();
        assert!(l
            .rationale_for(
                "GET",
                "social-api.decentraland.org",
                "/v{n}/mutes",
                400,
                401
            )
            .is_some());
        assert!(l
            .rationale_for(
                "GET",
                "social-api.decentraland.org",
                "/v{n}/mutes",
                400,
                404
            )
            .is_none());
        assert!(l
            .rationale_for("GET", "social-api.decentraland.org", "/v1/other", 400, 401)
            .is_none());
    }

    fn entry(host: &str, sample: &str) -> CorpusEntry {
        CorpusEntry {
            method: "GET".to_string(),
            host: host.to_string(),
            path: sample.split('?').next().unwrap().to_string(),
            has_post_data: false,
            sample_path: sample.to_string(),
        }
    }

    #[test]
    fn subdomain_rows_with_a_gateway_mapping_probe_both_forms() {
        let e = entry(
            "social-api.decentraland.org",
            "/v1/mutes?limit=100&offset=0",
        );
        let f = forms_for(&e, "interconnected.online");
        assert_eq!(f.len(), 2);
        assert_eq!(f[0].label, "subdomain");
        assert_eq!(
            f[0].candidate_url,
            "https://social-api.interconnected.online/v1/mutes?limit=100&offset=0"
        );
        assert_eq!(f[1].label, "gateway");
        assert_eq!(
            f[1].candidate_url,
            "https://gateway.interconnected.online/social-api/v1/mutes?limit=100&offset=0"
        );
        for form in &f {
            assert_eq!(
                form.baseline_url,
                "https://social-api.decentraland.org/v1/mutes?limit=100&offset=0"
            );
        }
    }

    #[test]
    fn gateway_rows_derive_the_subdomain_form() {
        let e = entry("gateway.decentraland.org", "/credits/users/0xa/credits");
        let f = forms_for(&e, "interconnected.online");
        assert_eq!(f.len(), 2);
        assert_eq!(f[0].label, "gateway");
        assert_eq!(
            f[0].candidate_url,
            "https://gateway.interconnected.online/credits/users/0xa/credits"
        );
        assert_eq!(f[1].label, "subdomain");
        assert_eq!(
            f[1].candidate_url,
            "https://credits.interconnected.online/users/0xa/credits"
        );
    }

    #[test]
    fn marketplace_api_maps_to_the_market_gateway_prefix() {
        let e = entry("marketplace-api.decentraland.org", "/v2/catalog?first=34");
        let f = forms_for(&e, "interconnected.online");
        assert_eq!(f.len(), 2);
        assert_eq!(
            f[1].candidate_url,
            "https://gateway.interconnected.online/market/v2/catalog?first=34"
        );
    }

    #[test]
    fn unmapped_hosts_probe_a_single_unlabeled_form() {
        let e = entry("marketing-files.decentraland.org", "/uploads/banner.png");
        let f = forms_for(&e, "interconnected.online");
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].label, "");
        assert_eq!(
            f[0].candidate_url,
            "https://marketing-files.interconnected.online/uploads/banner.png"
        );
    }

    #[test]
    fn required_item_keys_accept_value_add_and_flag_missing() {
        let ok = serde_json::json!({"data":[{"id":"9","name":"y","extra":true}]});
        assert!(required_item_key_diffs(&ok, "data", &["id", "name"]).is_empty());

        let missing = serde_json::json!({"data":[{"id":"9"}]});
        let d = required_item_key_diffs(&missing, "data", &["id", "name"]);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].candidate_value, "missing: name");

        let wrong_top = serde_json::json!({"items":[]});
        assert!(!required_item_key_diffs(&wrong_top, "data", &["id"]).is_empty());

        let empty = serde_json::json!({"data":[]});
        assert!(required_item_key_diffs(&empty, "data", &["id"]).is_empty());
    }

    #[test]
    fn baseline_shape_drift_is_reported_not_masked() {
        let moved = serde_json::json!({"results":[]});
        assert!(!catalog_shape_sanity(&moved, "data").is_empty());
        let fine = serde_json::json!({"data":[]});
        assert!(catalog_shape_sanity(&fine, "data").is_empty());
    }

    #[test]
    fn shipped_ledger_parses_and_every_entry_names_a_corpus_shape() {
        let ledger = AcceptedDivergences::load_default();
        let corpus: ExplorerCorpus = serde_json::from_str(
            &std::fs::read_to_string(default_corpus_path().expect("corpus fixture present"))
                .unwrap(),
        )
        .unwrap();
        for e in &ledger.entries {
            assert!(
                corpus
                    .entries
                    .iter()
                    .any(|c| c.method == e.method && c.host == e.host && c.path == e.path),
                "accepted-divergences entry {} {}{} matches no corpus entry",
                e.method,
                e.host,
                e.path
            );
            assert!(!e.rationale.trim().is_empty());
        }
    }
}
