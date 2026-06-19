mod diff;
mod fixture;
mod retry;
mod volatility;

use anyhow::{Context, Result};
use base64::Engine;
use clap::Parser;
use colored::Colorize;
use diff::{compare_json, Difference};
use fixture::{Fixture, RecordedRequest, RecordedResponse};
use reqwest::Client;
use retry::{is_transient_status, parse_retry_after, retry_with_backoff, RetryDecision};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use volatility::Volatility;

#[derive(Parser, Debug)]
#[command(name = "catalyrst-conformance", version)]
#[command(
    about = "Side-by-side conformance / parity tester for two catalyst hosts (any combination of local catalyrst, TS catalyst, public peers)."
)]
struct Args {
    #[arg(long, default_value = "http://127.0.0.1:5140")]
    baseline: String,

    #[arg(long, default_value = "http://127.0.0.1:5141")]
    candidate: String,

    #[arg(long, default_value_t = false)]
    verbose: bool,

    #[arg(long, value_delimiter = ',')]
    only: Option<Vec<String>>,

    #[arg(long, default_value_t = 30)]
    timeout_secs: u64,

    #[arg(long, default_value_t = 0)]
    inter_request_delay_ms: u64,

    #[arg(long)]
    volatility_config: Option<PathBuf>,

    #[arg(long)]
    capture_to: Option<PathBuf>,
}

struct Endpoints {
    baseline_content: String,
    candidate_content: String,
    baseline_lambdas: String,
    candidate_lambdas: String,
    baseline_root: String,
    candidate_root: String,
}

impl Endpoints {
    fn from_args(a: &Args) -> Self {
        let b = a.baseline.trim_end_matches('/').to_string();
        let c = a.candidate.trim_end_matches('/').to_string();
        Self {
            baseline_content: format!("{}/content", b),
            candidate_content: format!("{}/content", c),
            baseline_lambdas: format!("{}/lambdas", b),
            candidate_lambdas: format!("{}/lambdas", c),
            baseline_root: b,
            candidate_root: c,
        }
    }
}

enum Outcome {
    Diffs(Vec<Difference>),
    TransientSkip(String),
    VolatilitySkip,
}

struct Scoreboard {
    passed: u32,
    failed: u32,
    skipped: u32,
}

impl Scoreboard {
    fn new() -> Self {
        Scoreboard { passed: 0, failed: 0, skipped: 0 }
    }

    fn record_outcome(&mut self, outcome: Outcome, label: &str, verbose: bool) {
        match outcome {
            Outcome::Diffs(diffs) => self.record(&diffs, label, verbose),
            Outcome::TransientSkip(reason) => {
                println!(
                    "  {} {} ({}: {})",
                    "~".yellow(),
                    label,
                    "transient-error".yellow(),
                    reason
                );
                self.skipped += 1;
            }
            Outcome::VolatilitySkip => {
                println!(
                    "  {} {} (skipped: state-dependent, see volatility.toml)",
                    "~".yellow(),
                    label,
                );
                self.skipped += 1;
            }
        }
    }

    fn record(&mut self, diffs: &[Difference], label: &str, verbose: bool) {
        if diffs.is_empty() {
            println!("  {} {}", "✓".green(), label);
            self.passed += 1;
        } else {
            println!(
                "  {} {} ({} difference{})",
                "✗".red(),
                label,
                diffs.len(),
                if diffs.len() == 1 { "" } else { "s" }
            );
            self.failed += 1;
            let limit = if verbose { diffs.len() } else { 5 };
            for d in diffs.iter().take(limit) {
                println!("      {}", d);
            }
            if !verbose && diffs.len() > 5 {
                println!("      ... and {} more", diffs.len() - 5);
            }
        }
    }

    fn skip(&mut self, label: &str, reason: &str) {
        println!("  {} {} ({})", "~".yellow(), label, reason);
        self.skipped += 1;
    }

    fn summary(&self) {
        let total = self.passed + self.failed + self.skipped;
        let line = format!(
            "SUMMARY: {}/{} checks passed, {} difference{} found, {} skipped",
            self.passed,
            total,
            self.failed,
            if self.failed == 1 { "" } else { "s" },
            self.skipped,
        );
        if self.failed == 0 {
            println!("\n{}", line.green().bold());
        } else {
            println!("\n{}", line.yellow().bold());
        }
    }
}

struct BootstrapData {
    profile_entity_ids: Vec<String>,
    profile_addresses: Vec<String>,
    scene_pointers: Vec<String>,
    scene_entity_ids: Vec<String>,
    wearable_entity_ids: Vec<String>,
    content_hashes: Vec<String>,
}

struct Ctx {
    client: Client,
    volatility: Volatility,
    inter_delay_ms: u64,
    capture_dir: Option<PathBuf>,
    capture_count: AtomicUsize,
    captured_from: String,
}

impl Ctx {
    async fn sleep_between(&self) {
        if self.inter_delay_ms > 0 {
            tokio::time::sleep(Duration::from_millis(self.inter_delay_ms)).await;
        }
    }

    fn is_capturing(&self) -> bool {
        self.capture_dir.is_some()
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let endpoints = Endpoints::from_args(&args);

    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(args.timeout_secs))
        .build()?;

    let volatility = {
        let path = args.volatility_config.clone().unwrap_or_else(|| {
            let crate_local = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("volatility.toml");
            if crate_local.exists() {
                crate_local
            } else {
                PathBuf::from("volatility.toml")
            }
        });
        Volatility::load_or_default(&path)
    };

    let ctx = Ctx {
        client: client.clone(),
        volatility,
        inter_delay_ms: args.inter_request_delay_ms,
        capture_dir: args.capture_to.clone(),
        capture_count: AtomicUsize::new(0),
        captured_from: endpoints.baseline_root.clone(),
    };

    if let Some(dir) = &ctx.capture_dir {
        std::fs::create_dir_all(dir).ok();
        println!(
            "{} {}\n  Hitting baseline only; candidate ({}) will NOT be contacted.\n",
            "Capture mode →".yellow().bold(),
            dir.display(),
            endpoints.candidate_root,
        );
    }

    let scopes_filter = args.only.clone();
    let should_run = |name: &str| -> bool {
        scopes_filter
            .as_ref()
            .is_none_or(|list| list.iter().any(|s| s.eq_ignore_ascii_case(name)))
    };

    println!(
        "{}\n  baseline:  {}\n  candidate: {}\n",
        "Catalyrst Conformance".bold(),
        endpoints.baseline_root,
        endpoints.candidate_root,
    );

    println!("{}", "Bootstrapping test data from baseline ...".dimmed());
    let bootstrap = bootstrap_data(&ctx, &endpoints.baseline_content).await?;
    println!(
        "  profiles: {} ids, {} addresses;  scenes: {} ids, {} pointers;  wearables: {} ids;  content: {} hashes\n",
        bootstrap.profile_entity_ids.len(),
        bootstrap.profile_addresses.len(),
        bootstrap.scene_entity_ids.len(),
        bootstrap.scene_pointers.len(),
        bootstrap.wearable_entity_ids.len(),
        bootstrap.content_hashes.len(),
    );

    let mut score = Scoreboard::new();

    run_content_section(&ctx, &endpoints, &bootstrap, &mut score, &args, &should_run).await?;
    run_lambdas_section(&ctx, &endpoints, &bootstrap, &mut score, &args, &should_run).await?;

    score.summary();

    if let Some(dir) = &ctx.capture_dir {
        let n = ctx.capture_count.load(Ordering::Relaxed);
        println!(
            "\n{} {} fixture{} to {}\n  Replay with: {} --candidate <host> --fixtures {}",
            "Captured".green().bold(),
            n,
            if n == 1 { "" } else { "s" },
            dir.display(),
            "catalyrst-conformance-replay".cyan(),
            dir.display(),
        );
    }

    if score.failed > 0 {
        std::process::exit(1);
    }
    Ok(())
}

async fn run_content_section(
    ctx: &Ctx,
    ep: &Endpoints,
    bootstrap: &BootstrapData,
    score: &mut Scoreboard,
    args: &Args,
    should_run: &dyn Fn(&str) -> bool,
) -> Result<()> {
    if should_run("about") {
        println!("{}", "=== About ===".bold());
        let outcome =
            test_get_json(ctx, "about", &ep.baseline_root, &ep.candidate_root, "/about").await?;
        score.record_outcome(outcome, "/about (root)", args.verbose);
        ctx.sleep_between().await;
        let outcome2 = test_get_json(
            ctx,
            "about",
            &ep.baseline_content,
            &ep.candidate_content,
            "/about",
        )
        .await?;
        score.record_outcome(outcome2, "/content/about", args.verbose);
        println!();
    }

    if should_run("status") {
        println!("{}", "=== Status ===".bold());
        let outcome = test_get_json(
            ctx,
            "status",
            &ep.baseline_content,
            &ep.candidate_content,
            "/status",
        )
        .await?;
        score.record_outcome(outcome, "/content/status", args.verbose);
        println!();
    }

    if should_run("challenge") {
        println!("{}", "=== Challenge ===".bold());
        let outcome = test_get_json(
            ctx,
            "challenge",
            &ep.baseline_content,
            &ep.candidate_content,
            "/challenge",
        )
        .await?;
        score.record_outcome(outcome, "/content/challenge", args.verbose);
        println!();
    }

    if should_run("snapshots") {
        println!("{}", "=== Snapshots ===".bold());
        let outcome = test_get_json(
            ctx,
            "snapshots",
            &ep.baseline_content,
            &ep.candidate_content,
            "/snapshots",
        )
        .await?;
        score.record_outcome(outcome, "/content/snapshots", args.verbose);
        println!();
    }

    if should_run("failed-deployments") {
        println!("{}", "=== Failed Deployments ===".bold());
        let outcome = test_get_json(
            ctx,
            "failed-deployments",
            &ep.baseline_content,
            &ep.candidate_content,
            "/failed-deployments",
        )
        .await?;
        score.record_outcome(outcome, "/content/failed-deployments", args.verbose);
        println!();
    }

    if should_run("deployments") {
        println!("{}", "=== Deployments ===".bold());
        let cases = [
            (
                "profile, limit=5, DESC",
                "/deployments?entityType=profile&limit=5&sortingOrder=DESC",
            ),
            (
                "scene, limit=5, ASC",
                "/deployments?entityType=scene&limit=5&sortingOrder=ASC",
            ),
            (
                "wearable, limit=3, fields=pointers,content",
                "/deployments?entityType=wearable&limit=3&fields=pointers,content",
            ),
            (
                "emote, limit=3",
                "/deployments?entityType=emote&limit=3&sortingOrder=DESC",
            ),
        ];
        for (label, path) in &cases {
            let outcome = test_get_json(
                ctx,
                "deployments",
                &ep.baseline_content,
                &ep.candidate_content,
                path,
            )
            .await?;
            score.record_outcome(outcome, &format!("Deployments ({})", label), args.verbose);
            ctx.sleep_between().await;
        }
        println!();

        println!("{}", "=== Deployments pagination (3 pages) ===".bold());
        test_pagination(
            ctx,
            &ep.baseline_content,
            &ep.candidate_content,
            score,
            args.verbose,
        )
        .await?;
        println!();
    }

    if should_run("active-entities") {
        println!("{}", "=== Active Entities ===".bold());

        for ptr in ["0,0", "-50,-50", "10,10"] {
            let body = serde_json::json!({ "pointers": [ptr] });
            let outcome = test_post_json(
                ctx,
                "active-entities",
                &ep.baseline_content,
                &ep.candidate_content,
                "/entities/active",
                &body,
            )
            .await?;
            score.record_outcome(
                outcome,
                &format!("Active by pointer {}", ptr),
                args.verbose,
            );
            ctx.sleep_between().await;
        }

        if let Some(eid) = bootstrap.profile_entity_ids.first() {
            let body = serde_json::json!({ "ids": [eid] });
            let outcome = test_post_json(
                ctx,
                "active-entities",
                &ep.baseline_content,
                &ep.candidate_content,
                "/entities/active",
                &body,
            )
            .await?;
            score.record_outcome(
                outcome,
                &format!("Active by ID ({}...)", &eid[..eid.len().min(12)]),
                args.verbose,
            );
        }
        println!();
    }

    if should_run("entities") {
        println!("{}", "=== /entities/{type} ===".bold());
        for ptr in &bootstrap.scene_pointers.iter().take(2).collect::<Vec<_>>() {
            let path = format!("/entities/scene?pointer={}", urlencoding::encode(ptr));
            let outcome = test_get_json(
                ctx,
                "entities",
                &ep.baseline_content,
                &ep.candidate_content,
                &path,
            )
            .await?;
            score.record_outcome(
                outcome,
                &format!("/entities/scene?pointer={}", ptr),
                args.verbose,
            );
            ctx.sleep_between().await;
        }
        if let Some(addr) = bootstrap.profile_addresses.first() {
            let path = format!("/entities/profile?pointer={}", addr);
            let outcome = test_get_json(
                ctx,
                "entities",
                &ep.baseline_content,
                &ep.candidate_content,
                &path,
            )
            .await?;
            score.record_outcome(
                outcome,
                &format!("/entities/profile?pointer={}...", &addr[..addr.len().min(10)]),
                args.verbose,
            );
        }
        println!();
    }

    if should_run("audit") {
        println!("{}", "=== Audit ===".bold());
        if let Some(eid) = bootstrap.profile_entity_ids.first() {
            let path = format!("/audit/profile/{}", eid);
            let outcome = test_get_json(
                ctx,
                "audit",
                &ep.baseline_content,
                &ep.candidate_content,
                &path,
            )
            .await?;
            score.record_outcome(
                outcome,
                &format!("/audit/profile/{}...", &eid[..eid.len().min(12)]),
                args.verbose,
            );
            ctx.sleep_between().await;
        }
        if let Some(eid) = bootstrap.scene_entity_ids.first() {
            let path = format!("/audit/scene/{}", eid);
            let outcome = test_get_json(
                ctx,
                "audit",
                &ep.baseline_content,
                &ep.candidate_content,
                &path,
            )
            .await?;
            score.record_outcome(
                outcome,
                &format!("/audit/scene/{}...", &eid[..eid.len().min(12)]),
                args.verbose,
            );
        }
        println!();
    }

    if should_run("pointer-changes") {
        println!("{}", "=== Pointer Changes ===".bold());
        for (label, path) in [
            (
                "profile, from=1700000000000, limit=10",
                "/pointer-changes?entityType=profile&from=1700000000000&limit=10",
            ),
            (
                "scene, from=1700000000000, limit=10",
                "/pointer-changes?entityType=scene&from=1700000000000&limit=10",
            ),
        ] {
            let outcome = test_get_json(
                ctx,
                "pointer-changes",
                &ep.baseline_content,
                &ep.candidate_content,
                path,
            )
            .await?;
            score.record_outcome(outcome, &format!("pointer-changes ({})", label), args.verbose);
            ctx.sleep_between().await;
        }
        println!();
    }

    if should_run("content") {
        println!("{}", "=== Content bytes ===".bold());
        let hashes_to_test: Vec<&str> = bootstrap
            .content_hashes
            .iter()
            .take(3)
            .map(|s| s.as_str())
            .collect();
        if hashes_to_test.is_empty() {
            score.skip("content bytes", "no content hashes bootstrapped");
        } else {
            for hash in hashes_to_test {
                let outcome =
                    test_content_hash(ctx, &ep.baseline_content, &ep.candidate_content, hash)
                        .await?;
                score.record_outcome(
                    outcome,
                    &format!("Content {}...", &hash[..hash.len().min(12)]),
                    args.verbose,
                );
                ctx.sleep_between().await;
            }
        }
        println!();
    }

    if should_run("available-content") {
        println!("{}", "=== Available Content ===".bold());
        if let Some(hash) = bootstrap.content_hashes.first() {
            let path = format!("/available-content?cid={}", hash);
            let outcome = test_get_json(
                ctx,
                "available-content",
                &ep.baseline_content,
                &ep.candidate_content,
                &path,
            )
            .await?;
            score.record_outcome(
                outcome,
                &format!("Available content ({}...)", &hash[..hash.len().min(12)]),
                args.verbose,
            );
        } else {
            score.skip("available-content", "no content hashes bootstrapped");
        }
        println!();
    }

    if should_run("active-entities-by-hash") {
        println!("{}", "=== Active Entities By Hash ===".bold());
        if let Some(hash) = bootstrap.content_hashes.first() {
            let path = format!("/contents/{}/active-entities", hash);
            let outcome = test_get_json(
                ctx,
                "active-entities-by-hash",
                &ep.baseline_content,
                &ep.candidate_content,
                &path,
            )
            .await?;
            score.record_outcome(
                outcome,
                &format!("Active by hash ({}...)", &hash[..hash.len().min(12)]),
                args.verbose,
            );
        } else {
            score.skip("active-entities-by-hash", "no content hashes bootstrapped");
        }
        println!();
    }

    if should_run("thumbnail") {
        println!("{}", "=== Item thumbnail ===".bold());
        for pointer in [
            "urn:decentraland:matic:collections-v2:0xa8a7a4f1cfedd0c4f51274bc7e1b1f0f7a0adfd5:0",
        ] {
            let path = format!("/queries/items/{}/thumbnail", urlencoding::encode(pointer));
            let outcome =
                test_get_bytes(ctx, &ep.baseline_content, &ep.candidate_content, &path).await?;
            score.record_outcome(outcome, &format!("thumbnail {}", pointer), args.verbose);
            ctx.sleep_between().await;
        }
        println!();
    }

    if should_run("image") {
        println!("{}", "=== Item image ===".bold());
        for pointer in [
            "urn:decentraland:matic:collections-v2:0xa8a7a4f1cfedd0c4f51274bc7e1b1f0f7a0adfd5:0",
        ] {
            let path = format!("/queries/items/{}/image", urlencoding::encode(pointer));
            let outcome =
                test_get_bytes(ctx, &ep.baseline_content, &ep.candidate_content, &path).await?;
            score.record_outcome(outcome, &format!("image {}", pointer), args.verbose);
            ctx.sleep_between().await;
        }
        println!();
    }

    if should_run("erc721") {
        println!("{}", "=== ERC721 entity ===".bold());
        let paths = [
            "/queries/erc721/137/0xa8a7a4f1cfedd0c4f51274bc7e1b1f0f7a0adfd5/0",
            "/queries/erc721/137/0xa8a7a4f1cfedd0c4f51274bc7e1b1f0f7a0adfd5/0/1",
        ];
        for path in paths {
            let outcome = test_get_json(
                ctx,
                "erc721",
                &ep.baseline_content,
                &ep.candidate_content,
                path,
            )
            .await?;
            score.record_outcome(outcome, path, args.verbose);
            ctx.sleep_between().await;
        }
        println!();
    }

    if should_run("entities-by-collection") {
        println!("{}", "=== Entities by collection URN ===".bold());
        let urn = urlencoding::encode(
            "urn:decentraland:matic:collections-v2:0xa8a7a4f1cfedd0c4f51274bc7e1b1f0f7a0adfd5",
        );
        let path = format!("/entities/active/collections/{}", urn);
        let outcome = test_get_json(
            ctx,
            "entities-by-collection",
            &ep.baseline_content,
            &ep.candidate_content,
            &path,
        )
        .await?;
        score.record_outcome(outcome, "entities-by-collection (v2 sample)", args.verbose);
        println!();
    }

    Ok(())
}

async fn run_lambdas_section(
    ctx: &Ctx,
    ep: &Endpoints,
    bootstrap: &BootstrapData,
    score: &mut Scoreboard,
    args: &Args,
    should_run: &dyn Fn(&str) -> bool,
) -> Result<()> {
    if should_run("lambdas-status") {
        println!("{}", "=== /lambdas/status ===".bold());
        let outcome = test_get_json(
            ctx,
            "lambdas-status",
            &ep.baseline_lambdas,
            &ep.candidate_lambdas,
            "/status",
        )
        .await?;
        score.record_outcome(outcome, "/lambdas/status", args.verbose);
        println!();
    }

    if should_run("contracts") {
        println!("{}", "=== /lambdas/contracts/* ===".bold());
        for path in [
            "/contracts/servers",
            "/contracts/pois",
            "/contracts/denylisted-names",
        ] {
            let outcome = test_get_json(
                ctx,
                "contracts",
                &ep.baseline_lambdas,
                &ep.candidate_lambdas,
                path,
            )
            .await?;
            score.record_outcome(outcome, &format!("/lambdas{}", path), args.verbose);
            ctx.sleep_between().await;
        }
        println!();
    }

    if should_run("third-party-integrations") {
        println!("{}", "=== /lambdas/third-party-integrations ===".bold());
        let outcome = test_get_json(
            ctx,
            "third-party-integrations",
            &ep.baseline_lambdas,
            &ep.candidate_lambdas,
            "/third-party-integrations",
        )
        .await?;
        score.record_outcome(outcome, "/lambdas/third-party-integrations", args.verbose);
        println!();
    }

    if should_run("collections") {
        println!("{}", "=== /lambdas/collections (catalog) ===".bold());
        for path in [
            "/collections/wearables?pageSize=5",
            "/collections/emotes?pageSize=5",
        ] {
            let outcome = test_get_json(
                ctx,
                "collections",
                &ep.baseline_lambdas,
                &ep.candidate_lambdas,
                path,
            )
            .await?;
            score.record_outcome(outcome, &format!("/lambdas{}", path), args.verbose);
            ctx.sleep_between().await;
        }
        println!();
    }

    if should_run("nfts-collections") {
        println!("{}", "=== /lambdas/nfts/collections ===".bold());
        let outcome = test_get_json(
            ctx,
            "nfts-collections",
            &ep.baseline_lambdas,
            &ep.candidate_lambdas,
            "/nfts/collections",
        )
        .await?;
        score.record_outcome(outcome, "/lambdas/nfts/collections", args.verbose);
        println!();
    }

    let address_opt = bootstrap.profile_addresses.first().cloned();

    if should_run("profiles") {
        println!("{}", "=== /lambdas/profiles ===".bold());
        if let Some(addr) = &address_opt {
            let outcome_get = test_get_json(
                ctx,
                "profiles",
                &ep.baseline_lambdas,
                &ep.candidate_lambdas,
                &format!("/profiles/{}", addr),
            )
            .await?;
            score.record_outcome(
                outcome_get,
                &format!("GET /lambdas/profiles/{}...", &addr[..addr.len().min(10)]),
                args.verbose,
            );
            ctx.sleep_between().await;

            let outcome_alias = test_get_json(
                ctx,
                "profiles",
                &ep.baseline_lambdas,
                &ep.candidate_lambdas,
                &format!("/profile/{}", addr),
            )
            .await?;
            score.record_outcome(
                outcome_alias,
                &format!("GET /lambdas/profile/{}...", &addr[..addr.len().min(10)]),
                args.verbose,
            );
            ctx.sleep_between().await;

            let body = serde_json::json!({ "ids": [addr] });
            let outcome_post = test_post_json(
                ctx,
                "profiles",
                &ep.baseline_lambdas,
                &ep.candidate_lambdas,
                "/profiles",
                &body,
            )
            .await?;
            score.record_outcome(
                outcome_post,
                "POST /lambdas/profiles (single id)",
                args.verbose,
            );
        } else {
            score.skip("profiles", "no profile addresses bootstrapped");
        }
        println!();
    }

    if should_run("user-items") {
        println!("{}", "=== /lambdas/users/{address}/* ===".bold());
        if let Some(addr) = &address_opt {
            for sub in [
                "/wearables?pageSize=5",
                "/emotes?pageSize=5",
                "/third-party-wearables?pageSize=5",
                "/names?pageSize=5",
                "/lands?pageSize=5",
                "/lands-permissions",
            ] {
                let path = format!("/users/{}{}", addr, sub);
                let outcome = test_get_json(
                    ctx,
                    "user-items",
                    &ep.baseline_lambdas,
                    &ep.candidate_lambdas,
                    &path,
                )
                .await?;
                score.record_outcome(
                    outcome,
                    &format!("/lambdas/users/{}...{}", &addr[..addr.len().min(10)], sub),
                    args.verbose,
                );
                ctx.sleep_between().await;
            }
        } else {
            score.skip("user-items", "no profile addresses bootstrapped");
        }
        println!();
    }

    if should_run("collections-by-owner") {
        println!("{}", "=== /lambdas/collections/*-by-owner ===".bold());
        if let Some(addr) = &address_opt {
            for sub in ["wearables-by-owner", "emotes-by-owner"] {
                let path = format!("/collections/{}/{}", sub, addr);
                let outcome = test_get_json(
                    ctx,
                    "collections-by-owner",
                    &ep.baseline_lambdas,
                    &ep.candidate_lambdas,
                    &path,
                )
                .await?;
                score.record_outcome(outcome, &format!("/lambdas{}", path), args.verbose);
                ctx.sleep_between().await;
            }
        } else {
            score.skip("collections-by-owner", "no profile addresses bootstrapped");
        }
        println!();
    }

    if should_run("explorer") {
        println!("{}", "=== /lambdas/explorer/{address}/* ===".bold());
        if let Some(addr) = &address_opt {
            for sub in [
                "/wearables?pageSize=5",
                "/emotes?pageSize=5",
            ] {
                let path = format!("/explorer/{}{}", addr, sub);
                let outcome = test_get_json(
                    ctx,
                    "explorer",
                    &ep.baseline_lambdas,
                    &ep.candidate_lambdas,
                    &path,
                )
                .await?;
                score.record_outcome(
                    outcome,
                    &format!("/lambdas/explorer/{}...{}", &addr[..addr.len().min(10)], sub),
                    args.verbose,
                );
                ctx.sleep_between().await;
            }
        } else {
            score.skip("explorer", "no profile addresses bootstrapped");
        }
        println!();
    }

    if should_run("parcel") {
        println!("{}", "=== /lambdas/parcels and parcel-permissions ===".bold());
        let parcels = [(0i32, 0i32), (10, 10), (-50, -50)];
        for (x, y) in parcels {
            let outcome = test_get_json(
                ctx,
                "parcel",
                &ep.baseline_lambdas,
                &ep.candidate_lambdas,
                &format!("/parcels/{}/{}/operators", x, y),
            )
            .await?;
            score.record_outcome(
                outcome,
                &format!("/lambdas/parcels/{}/{}/operators", x, y),
                args.verbose,
            );
            ctx.sleep_between().await;
        }
        if let Some(addr) = &address_opt {
            let outcome = test_get_json(
                ctx,
                "parcel",
                &ep.baseline_lambdas,
                &ep.candidate_lambdas,
                &format!("/users/{}/parcels/0/0/permissions", addr),
            )
            .await?;
            score.record_outcome(
                outcome,
                "/lambdas/users/<addr>/parcels/0/0/permissions",
                args.verbose,
            );
        }
        println!();
    }

    if should_run("name-owner") {
        println!("{}", "=== /lambdas/names/{name}/owner ===".bold());
        for name in ["spectabilis", "definitely-not-a-real-name-xxz"] {
            let outcome = test_get_json(
                ctx,
                "name-owner",
                &ep.baseline_lambdas,
                &ep.candidate_lambdas,
                &format!("/names/{}/owner", name),
            )
            .await?;
            score.record_outcome(
                outcome,
                &format!("/lambdas/names/{}/owner", name),
                args.verbose,
            );
            ctx.sleep_between().await;
        }
        println!();
    }

    if should_run("outfits") {
        println!("{}", "=== /lambdas/outfits/{id} ===".bold());
        if let Some(addr) = &address_opt {
            let outcome = test_get_json(
                ctx,
                "outfits",
                &ep.baseline_lambdas,
                &ep.candidate_lambdas,
                &format!("/outfits/{}", addr),
            )
            .await?;
            score.record_outcome(
                outcome,
                &format!("/lambdas/outfits/{}...", &addr[..addr.len().min(10)]),
                args.verbose,
            );
        } else {
            score.skip("outfits", "no profile addresses bootstrapped");
        }
        println!();
    }

    Ok(())
}

async fn bootstrap_data(ctx: &Ctx, baseline_content: &str) -> Result<BootstrapData> {
    let mut profile_entity_ids = Vec::new();
    let mut profile_addresses = Vec::new();
    let mut scene_entity_ids = Vec::new();
    let mut scene_pointers = Vec::new();
    let mut wearable_entity_ids = Vec::new();
    let mut content_hashes = Vec::new();

    let scene_url = format!(
        "{}/deployments?entityType=scene&limit=5&sortingOrder=DESC&fields=pointers,content,entityId,entityType",
        baseline_content
    );
    if let Some(body) = fetch_json_with_retry(ctx, "bootstrap-scenes", &scene_url).await? {
        if let Some(deployments) = body.get("deployments").and_then(|v| v.as_array()) {
            for dep in deployments {
                if let Some(eid) = dep.get("entityId").and_then(|v| v.as_str()) {
                    scene_entity_ids.push(eid.to_string());
                }
                if let Some(ptrs) = dep.get("pointers").and_then(|v| v.as_array()) {
                    for p in ptrs {
                        if let Some(s) = p.as_str() {
                            scene_pointers.push(s.to_string());
                        }
                    }
                }
                if let Some(content) = dep.get("content").and_then(|v| v.as_array()) {
                    for c in content {
                        if let Some(hash) = c.get("hash").and_then(|v| v.as_str()) {
                            content_hashes.push(hash.to_string());
                        }
                    }
                }
            }
        }
    }

    let profile_url = format!(
        "{}/deployments?entityType=profile&limit=5&sortingOrder=DESC&fields=pointers,content,entityId,entityType",
        baseline_content
    );
    if let Some(body) = fetch_json_with_retry(ctx, "bootstrap-profiles", &profile_url).await? {
        harvest_deployments(
            &body,
            &mut profile_entity_ids,
            &mut profile_addresses,
            &mut content_hashes,
        );
    }

    let wearable_url = format!(
        "{}/deployments?entityType=wearable&limit=5&sortingOrder=DESC&fields=entityId,content",
        baseline_content
    );
    if let Some(body) = fetch_json_with_retry(ctx, "bootstrap-wearables", &wearable_url).await? {
        if let Some(deployments) = body.get("deployments").and_then(|v| v.as_array()) {
            for dep in deployments {
                if let Some(eid) = dep.get("entityId").and_then(|v| v.as_str()) {
                    wearable_entity_ids.push(eid.to_string());
                }
                if let Some(content) = dep.get("content").and_then(|v| v.as_array()) {
                    for c in content {
                        if let Some(hash) = c.get("hash").and_then(|v| v.as_str()) {
                            content_hashes.push(hash.to_string());
                        }
                    }
                }
            }
        }
    }

    profile_entity_ids.sort();
    profile_entity_ids.dedup();
    profile_addresses.sort();
    profile_addresses.dedup();
    scene_entity_ids.sort();
    scene_entity_ids.dedup();
    scene_pointers.sort();
    scene_pointers.dedup();
    wearable_entity_ids.sort();
    wearable_entity_ids.dedup();
    content_hashes.sort();
    content_hashes.dedup();

    profile_entity_ids.truncate(5);
    profile_addresses.truncate(5);
    scene_entity_ids.truncate(5);
    scene_pointers.truncate(5);
    wearable_entity_ids.truncate(5);
    content_hashes.truncate(10);

    Ok(BootstrapData {
        profile_entity_ids,
        profile_addresses,
        scene_pointers,
        scene_entity_ids,
        wearable_entity_ids,
        content_hashes,
    })
}

async fn fetch_json_with_retry(ctx: &Ctx, label: &str, url: &str) -> Result<Option<Value>> {
    retry_with_backoff(label, 3, 1000, || async {
        let resp = ctx.client.get(url).send().await
            .with_context(|| format!("GET {}", url))?;
        let status = resp.status();
        if is_transient_status(status) {
            let wait = resp
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(parse_retry_after);
            return Ok(RetryDecision::Retry(wait));
        }
        if !status.is_success() {
            return Ok(RetryDecision::Done(None));
        }
        let body = resp.text().await.context("reading bootstrap body")?;
        let json: Value = serde_json::from_str(&body)
            .with_context(|| format!("parsing bootstrap JSON from {}", url))?;
        Ok(RetryDecision::Done(Some(json)))
    })
    .await
    .map(|opt| opt.flatten())
}

fn harvest_deployments(
    body: &Value,
    entity_ids: &mut Vec<String>,
    addresses: &mut Vec<String>,
    content_hashes: &mut Vec<String>,
) {
    if let Some(deployments) = body.get("deployments").and_then(|v| v.as_array()) {
        for dep in deployments {
            if let Some(eid) = dep.get("entityId").and_then(|v| v.as_str()) {
                entity_ids.push(eid.to_string());
            }
            if let Some(ptrs) = dep.get("pointers").and_then(|v| v.as_array()) {
                for p in ptrs {
                    if let Some(s) = p.as_str() {
                        if s.starts_with("0x") && s.len() == 42 {
                            addresses.push(s.to_string());
                        }
                    }
                }
            }
            if let Some(content) = dep.get("content").and_then(|v| v.as_array()) {
                for c in content {
                    if let Some(hash) = c.get("hash").and_then(|v| v.as_str()) {
                        content_hashes.push(hash.to_string());
                    }
                }
            }
        }
    }
}

async fn test_get_json(
    ctx: &Ctx,
    section: &str,
    baseline_base: &str,
    candidate_base: &str,
    path: &str,
) -> Result<Outcome> {
    if ctx.volatility.ignore_whole(section) {
        return Ok(Outcome::VolatilitySkip);
    }

    let baseline_full = format!("{}{}", baseline_base, path);
    let candidate_full = format!("{}{}", candidate_base, path);

    let pair = retry_pair(ctx, section, &baseline_full, &candidate_full, None).await?;
    let (b_status, c_status, b_body, c_body) = match pair {
        PairOutcome::Got(s) => s,
        PairOutcome::Transient(reason) => return Ok(Outcome::TransientSkip(reason)),
    };

    let mut diffs = Vec::new();

    if b_status != c_status {
        diffs.push(Difference {
            path: "HTTP status".to_string(),
            baseline_value: b_status.to_string(),
            candidate_value: c_status.to_string(),
        });
        return Ok(Outcome::Diffs(diffs));
    }

    if b_body.trim().is_empty() && c_body.trim().is_empty() {
        return Ok(Outcome::Diffs(diffs));
    }
    if b_body.trim().is_empty() || c_body.trim().is_empty() {
        diffs.push(Difference {
            path: "body-presence".to_string(),
            baseline_value: format!("{} bytes", b_body.len()),
            candidate_value: format!("{} bytes", c_body.len()),
        });
        return Ok(Outcome::Diffs(diffs));
    }

    let b_json: Value = match serde_json::from_str(&b_body) {
        Ok(v) => v,
        Err(_) => {
            if b_body == c_body {
                return Ok(Outcome::Diffs(diffs));
            }
            diffs.push(Difference {
                path: "non-JSON-body".to_string(),
                baseline_value: truncate(&b_body),
                candidate_value: truncate(&c_body),
            });
            return Ok(Outcome::Diffs(diffs));
        }
    };
    let c_json: Value = serde_json::from_str(&c_body)
        .context(format!("parsing candidate JSON from {}", path))?;

    diffs.extend(compare_json(section, path, &b_json, &c_json, &ctx.volatility));
    Ok(Outcome::Diffs(diffs))
}

async fn test_post_json(
    ctx: &Ctx,
    section: &str,
    baseline_base: &str,
    candidate_base: &str,
    path: &str,
    body: &Value,
) -> Result<Outcome> {
    if ctx.volatility.ignore_whole(section) {
        return Ok(Outcome::VolatilitySkip);
    }

    let baseline_full = format!("{}{}", baseline_base, path);
    let candidate_full = format!("{}{}", candidate_base, path);

    let pair = retry_pair(ctx, section, &baseline_full, &candidate_full, Some(body)).await?;
    let (b_status, c_status, b_body, c_body) = match pair {
        PairOutcome::Got(s) => s,
        PairOutcome::Transient(reason) => return Ok(Outcome::TransientSkip(reason)),
    };

    let mut diffs = Vec::new();

    if b_status != c_status {
        diffs.push(Difference {
            path: "HTTP status".to_string(),
            baseline_value: b_status.to_string(),
            candidate_value: c_status.to_string(),
        });
        return Ok(Outcome::Diffs(diffs));
    }

    let b_json: Value = serde_json::from_str(&b_body)
        .context(format!("parsing baseline JSON from POST {}", path))?;
    let c_json: Value = serde_json::from_str(&c_body)
        .context(format!("parsing candidate JSON from POST {}", path))?;

    diffs.extend(compare_json(section, path, &b_json, &c_json, &ctx.volatility));
    Ok(Outcome::Diffs(diffs))
}

enum PairOutcome {
    Got((reqwest::StatusCode, reqwest::StatusCode, String, String)),
    Transient(String),
}

async fn retry_pair(
    ctx: &Ctx,
    label: &str,
    baseline_url: &str,
    candidate_url: &str,
    body: Option<&Value>,
) -> Result<PairOutcome> {
    if ctx.is_capturing() {
        return capture_single(ctx, label, baseline_url, body).await;
    }
    let outcome = retry_with_backoff(label, 3, 1000, || async {
        let send_one = |url: &str| {
            let req = if let Some(b) = body {
                ctx.client.post(url).json(b)
            } else {
                ctx.client.get(url)
            };
            req.send()
        };

        let (b_resp, c_resp) = tokio::try_join!(
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

        let b_status = b_resp.status();
        let c_status = c_resp.status();

        if is_transient_status(b_status) || is_transient_status(c_status) {
            let hint = [&b_resp, &c_resp]
                .iter()
                .filter_map(|r| {
                    r.headers()
                        .get("retry-after")
                        .and_then(|v| v.to_str().ok())
                        .and_then(parse_retry_after)
                })
                .max();
            return Ok(RetryDecision::Retry(hint));
        }

        let (b_body, c_body) = tokio::try_join!(
            async { b_resp.text().await.context("reading baseline body") },
            async { c_resp.text().await.context("reading candidate body") },
        )?;

        Ok(RetryDecision::Done((b_status, c_status, b_body, c_body)))
    })
    .await?;

    match outcome {
        Some(tup) => Ok(PairOutcome::Got(tup)),
        None => Ok(PairOutcome::Transient("baseline/candidate kept returning 429/5xx after 3 attempts".to_string())),
    }
}

async fn capture_single(
    ctx: &Ctx,
    label: &str,
    baseline_url: &str,
    body: Option<&Value>,
) -> Result<PairOutcome> {
    let outcome = retry_with_backoff(label, 3, 1000, || async {
        let req = if let Some(b) = body {
            ctx.client.post(baseline_url).json(b)
        } else {
            ctx.client.get(baseline_url)
        };
        let resp = req
            .send()
            .await
            .with_context(|| format!("request {} (capture)", baseline_url))?;
        let status = resp.status();
        if is_transient_status(status) {
            let hint = resp
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(parse_retry_after);
            return Ok(RetryDecision::Retry(hint));
        }
        let headers = collect_response_headers(&resp);
        let bytes = resp.bytes().await.context("reading capture body")?;
        Ok(RetryDecision::Done((status, headers, bytes)))
    })
    .await?;

    let (status, headers, bytes) = match outcome {
        Some(t) => t,
        None => {
            return Ok(PairOutcome::Transient("baseline kept returning 429/5xx after 3 attempts (capture mode)".to_string()));
        }
    };

    let text = String::from_utf8_lossy(&bytes).to_string();
    write_fixture(
        ctx,
        label,
        if body.is_some() { "POST" } else { "GET" },
        baseline_url,
        body,
        status.as_u16(),
        &headers,
        &bytes,
    )?;
    Ok(PairOutcome::Got((status, status, text.clone(), text)))
}

fn collect_response_headers(resp: &reqwest::Response) -> BTreeMap<String, String> {
    let keep = [
        "content-type",
        "content-length",
        "etag",
        "cache-control",
        "last-modified",
    ];
    let mut out = BTreeMap::new();
    for name in keep {
        if let Some(v) = resp.headers().get(name).and_then(|v| v.to_str().ok()) {
            out.insert(name.to_string(), v.to_string());
        }
    }
    out
}

fn url_to_path_and_query(url: &str) -> String {
    if let Some(idx) = url.find("://") {
        let after_scheme = &url[idx + 3..];
        if let Some(slash) = after_scheme.find('/') {
            return after_scheme[slash..].to_string();
        }
    }
    url.to_string()
}

fn split_path_query(s: &str) -> (String, BTreeMap<String, String>) {
    let mut q = BTreeMap::new();
    if let Some(idx) = s.find('?') {
        for kv in s[idx + 1..].split('&') {
            if kv.is_empty() {
                continue;
            }
            let mut it = kv.splitn(2, '=');
            let k = it.next().unwrap_or("").to_string();
            let v = it.next().unwrap_or("").to_string();
            q.insert(k, v);
        }
        (s[..idx].to_string(), q)
    } else {
        (s.to_string(), q)
    }
}

fn fixture_slug_for(method: &str, path_and_query: &str, body: Option<&Value>) -> String {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    method.hash(&mut hasher);
    path_and_query.hash(&mut hasher);
    if let Some(b) = body {
        b.to_string().hash(&mut hasher);
    }
    let h = hasher.finish() & 0xffffffff;
    let (path, _) = split_path_query(path_and_query);
    let slug: String = path
        .trim_start_matches('/')
        .chars()
        .take(60)
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let slug = slug.trim_matches('-');
    if slug.is_empty() {
        format!("call-{:08x}", h)
    } else {
        format!("{}-{:08x}", slug, h)
    }
}

fn fixture_subdir_for(section: &str) -> &'static str {
    match section {
        "lambdas-status" | "contracts" | "third-party-integrations" | "collections"
        | "nfts-collections" | "profiles" | "user-items" | "collections-by-owner" | "explorer"
        | "parcel" | "name-owner" | "outfits" => "lambdas",

        _ => "content",
    }
}

async fn capture_bytes(
    ctx: &Ctx,
    section: &str,
    label: &str,
    method: &str,
    baseline_full: &str,
) -> Result<Outcome> {
    let attempted = retry_with_backoff(label, 3, 1000, || async {
        let resp = ctx
            .client
            .get(baseline_full)
            .send()
            .await
            .with_context(|| format!("GET {} (capture-bytes)", baseline_full))?;
        let status = resp.status();
        if is_transient_status(status) {
            let hint = resp
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(parse_retry_after);
            return Ok(RetryDecision::Retry(hint));
        }
        let headers = collect_response_headers(&resp);
        let bytes = resp.bytes().await.context("reading capture body")?;
        Ok(RetryDecision::Done((status, headers, bytes)))
    })
    .await?;

    let (status, headers, bytes) = match attempted {
        Some(t) => t,
        None => {
            return Ok(Outcome::TransientSkip(format!(
                "{} drained retries (capture mode)",
                label
            )));
        }
    };

    write_fixture(
        ctx,
        section,
        method,
        baseline_full,
        None,
        status.as_u16(),
        &headers,
        &bytes,
    )?;
    Ok(Outcome::Diffs(Vec::new()))
}

#[allow(clippy::too_many_arguments)]
fn write_fixture(
    ctx: &Ctx,
    section: &str,
    method: &str,
    full_url: &str,
    body: Option<&Value>,
    status: u16,
    headers: &BTreeMap<String, String>,
    response_bytes: &[u8],
) -> Result<()> {
    let dir = ctx
        .capture_dir
        .as_ref()
        .expect("capture mode required for write_fixture");
    let pathq = url_to_path_and_query(full_url);
    let (path_only, query) = split_path_query(&pathq);

    let content_type = headers
        .get("content-type")
        .cloned()
        .unwrap_or_default();
    let (body_json, body_bytes_b64) = if content_type.contains("application/json")
        || response_bytes
            .first()
            .map(|c| *c == b'{' || *c == b'[')
            .unwrap_or(false)
    {
        match serde_json::from_slice::<Value>(response_bytes) {
            Ok(v) => (Some(v), None),
            Err(_) => (
                None,
                Some(base64::engine::general_purpose::STANDARD.encode(response_bytes)),
            ),
        }
    } else {
        (
            None,
            Some(base64::engine::general_purpose::STANDARD.encode(response_bytes)),
        )
    };

    let fixture = Fixture {
        description: format!("Captured from baseline: {} {}", method, path_only),
        request: RecordedRequest {
            method: method.to_string(),
            path: path_only.clone(),
            query,
            headers: BTreeMap::new(),
            body: body.cloned(),
        },
        response: RecordedResponse {
            status,
            headers: headers.clone(),
            body_json,
            body_bytes_b64,
        },
        captured_from: ctx.captured_from.clone(),
        captured_at: chrono::Utc::now().to_rfc3339(),
        volatile_paths: Vec::new(),
    };

    let subdir = dir.join(fixture_subdir_for(section));
    std::fs::create_dir_all(&subdir)
        .with_context(|| format!("creating fixture dir {}", subdir.display()))?;
    let slug = fixture_slug_for(method, &pathq, body);
    let outpath = subdir.join(format!("{}.json", slug));
    let json_str = serde_json::to_string_pretty(&fixture)
        .context("serialising fixture")?;
    std::fs::write(&outpath, json_str)
        .with_context(|| format!("writing fixture {}", outpath.display()))?;

    ctx.capture_count.fetch_add(1, Ordering::Relaxed);
    Ok(())
}

async fn test_pagination(
    ctx: &Ctx,
    baseline_base: &str,
    candidate_base: &str,
    score: &mut Scoreboard,
    verbose: bool,
) -> Result<()> {
    let section = "deployments";
    let initial_path = "/deployments?entityType=profile&limit=5&sortingOrder=DESC";

    let mut b_next: Option<String> = Some(format!("{}{}", baseline_base, initial_path));
    let mut c_next: Option<String> = Some(format!("{}{}", candidate_base, initial_path));

    for page in 1..=3 {
        let b_url = match &b_next {
            Some(u) => u.clone(),
            None => {
                score.record(
                    &[Difference {
                        path: "pagination".to_string(),
                        baseline_value: "no next link".to_string(),
                        candidate_value: "n/a".to_string(),
                    }],
                    &format!("Page {}: baseline has no next link", page),
                    verbose,
                );
                return Ok(());
            }
        };
        let c_url = match &c_next {
            Some(u) => u.clone(),
            None => {
                score.record(
                    &[Difference {
                        path: "pagination".to_string(),
                        baseline_value: "n/a".to_string(),
                        candidate_value: "no next link".to_string(),
                    }],
                    &format!("Page {}: candidate has no next link", page),
                    verbose,
                );
                return Ok(());
            }
        };

        let pair = retry_pair(
            ctx,
            &format!("pagination-page-{}", page),
            &b_url,
            &c_url,
            None,
        )
        .await?;
        let (b_status, c_status, b_text, c_text) = match pair {
            PairOutcome::Got(s) => s,
            PairOutcome::Transient(reason) => {
                score.record_outcome(
                    Outcome::TransientSkip(reason),
                    &format!("Page {} (pagination)", page),
                    verbose,
                );
                return Ok(());
            }
        };

        if !b_status.is_success() || !c_status.is_success() {
            score.record(
                &[Difference {
                    path: "HTTP status".to_string(),
                    baseline_value: b_status.to_string(),
                    candidate_value: c_status.to_string(),
                }],
                &format!("Page {}: non-2xx response", page),
                verbose,
            );
            return Ok(());
        }

        let b_body: Value = match serde_json::from_str(&b_text) {
            Ok(v) => v,
            Err(e) => {
                score.record(
                    &[Difference {
                        path: "baseline JSON parse".to_string(),
                        baseline_value: format!("{}: {}", e, truncate(&b_text)),
                        candidate_value: format!("{} bytes", c_text.len()),
                    }],
                    &format!("Page {}: baseline body is not JSON", page),
                    verbose,
                );
                return Ok(());
            }
        };
        let c_body: Value = match serde_json::from_str(&c_text) {
            Ok(v) => v,
            Err(e) => {
                score.record(
                    &[Difference {
                        path: "candidate JSON parse".to_string(),
                        baseline_value: format!("{} bytes", b_text.len()),
                        candidate_value: format!("{}: {}", e, truncate(&c_text)),
                    }],
                    &format!("Page {}: candidate body is not JSON", page),
                    verbose,
                );
                return Ok(());
            }
        };

        let page_label = format!("deployments_page_{}", page);
        let diffs = compare_json(section, &page_label, &b_body, &c_body, &ctx.volatility);

        let b_count = b_body
            .get("deployments")
            .and_then(|v| v.as_array())
            .map(|a| a.len())
            .unwrap_or(0);
        let deployment_diffs: Vec<&Difference> = diffs
            .iter()
            .filter(|d| d.path.starts_with(&format!("{}.deployments", page_label)))
            .collect();
        let match_count = if deployment_diffs.is_empty() {
            b_count
        } else {
            let diffed_indices: std::collections::HashSet<usize> = deployment_diffs
                .iter()
                .filter_map(|d| {
                    let s = &d.path;
                    let bracket = s.find('[')?;
                    let end = s[bracket..].find(']')?;
                    s[bracket + 1..bracket + end].parse::<usize>().ok()
                })
                .collect();
            b_count.saturating_sub(diffed_indices.len())
        };

        score.record(
            &diffs,
            &format!("Page {}: {}/{} match", page, match_count, b_count),
            verbose,
        );

        b_next = extract_next_link(&b_body, baseline_base);
        c_next = extract_next_link(&c_body, candidate_base);
    }

    Ok(())
}

fn extract_next_link(body: &Value, base_url: &str) -> Option<String> {
    let next = body
        .get("pagination")
        .and_then(|p| p.get("next"))
        .and_then(|n| n.as_str())?;

    if next.is_empty() {
        return None;
    }

    if next.starts_with("http") {
        Some(next.to_string())
    } else if next.starts_with('/') {
        let origin = base_url
            .rfind("/content")
            .map(|i| &base_url[..i])
            .unwrap_or(base_url);
        Some(format!("{}{}", origin, next))
    } else {
        Some(format!("{}/{}", base_url, next))
    }
}

async fn test_content_hash(
    ctx: &Ctx,
    baseline_base: &str,
    candidate_base: &str,
    hash: &str,
) -> Result<Outcome> {
    let baseline_full = format!("{}/contents/{}", baseline_base, hash);
    let candidate_full = format!("{}/contents/{}", candidate_base, hash);
    let label = format!("contents/{}", hash);

    if ctx.is_capturing() {
        return capture_bytes(ctx, "content", &label, "GET", &baseline_full).await;
    }

    let attempted = retry_with_backoff(&label, 3, 1000, || async {
        let (b_resp, c_resp) = tokio::try_join!(
            async {
                ctx.client
                    .get(&baseline_full)
                    .send()
                    .await
                    .with_context(|| format!("GET content {} (baseline)", hash))
            },
            async {
                ctx.client
                    .get(&candidate_full)
                    .send()
                    .await
                    .with_context(|| format!("GET content {} (candidate)", hash))
            },
        )?;

        let b_status = b_resp.status();
        let c_status = c_resp.status();

        if is_transient_status(b_status) || is_transient_status(c_status) {
            let hint = [&b_resp, &c_resp]
                .iter()
                .filter_map(|r| {
                    r.headers()
                        .get("retry-after")
                        .and_then(|v| v.to_str().ok())
                        .and_then(parse_retry_after)
                })
                .max();
            return Ok(RetryDecision::Retry(hint));
        }

        Ok(RetryDecision::Done((b_resp, c_resp, b_status, c_status)))
    })
    .await?;

    let (b_resp, c_resp, b_status, c_status) = match attempted {
        Some(t) => t,
        None => {
            return Ok(Outcome::TransientSkip(format!(
                "content hash {}... drained retries",
                &hash[..hash.len().min(12)]
            )))
        }
    };

    let mut diffs = Vec::new();

    if b_status != c_status {
        diffs.push(Difference {
            path: format!("contents/{} HTTP status", hash),
            baseline_value: b_status.to_string(),
            candidate_value: c_status.to_string(),
        });
        return Ok(Outcome::Diffs(diffs));
    }

    let header_path = |name: &str| format!("contents/{} {}", hash, name);

    let read_header = |resp: &reqwest::Response, name: &str| -> String {
        resp.headers()
            .get(name)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string()
    };

    let b_ct = read_header(&b_resp, "content-type");
    let c_ct = read_header(&c_resp, "content-type");
    let b_cl = read_header(&b_resp, "content-length");
    let c_cl = read_header(&c_resp, "content-length");
    let b_etag = read_header(&b_resp, "etag");
    let c_etag = read_header(&c_resp, "etag");

    let (b_bytes, c_bytes) = tokio::try_join!(
        async { b_resp.bytes().await.context("reading baseline content bytes") },
        async { c_resp.bytes().await.context("reading candidate content bytes") },
    )?;

    if b_ct != c_ct {
        diffs.push(Difference {
            path: header_path("Content-Type"),
            baseline_value: b_ct,
            candidate_value: c_ct,
        });
    }
    if b_cl != c_cl {
        diffs.push(Difference {
            path: header_path("Content-Length"),
            baseline_value: b_cl,
            candidate_value: c_cl,
        });
    }
    if !b_etag.is_empty() && !c_etag.is_empty() && b_etag != c_etag {
        diffs.push(Difference {
            path: header_path("ETag"),
            baseline_value: b_etag,
            candidate_value: c_etag,
        });
    }
    if b_bytes != c_bytes {
        diffs.push(Difference {
            path: header_path("body"),
            baseline_value: format!("{} bytes", b_bytes.len()),
            candidate_value: format!("{} bytes", c_bytes.len()),
        });
    }

    Ok(Outcome::Diffs(diffs))
}

async fn test_get_bytes(
    ctx: &Ctx,
    baseline_base: &str,
    candidate_base: &str,
    path: &str,
) -> Result<Outcome> {
    let baseline_full = format!("{}{}", baseline_base, path);
    let candidate_full = format!("{}{}", candidate_base, path);

    if ctx.is_capturing() {
        return capture_bytes(ctx, "content", path, "GET", &baseline_full).await;
    }

    let attempted = retry_with_backoff(path, 3, 1000, || async {
        let (b_resp, c_resp) = tokio::try_join!(
            async {
                ctx.client
                    .get(&baseline_full)
                    .send()
                    .await
                    .with_context(|| format!("GET {} (baseline)", baseline_full))
            },
            async {
                ctx.client
                    .get(&candidate_full)
                    .send()
                    .await
                    .with_context(|| format!("GET {} (candidate)", candidate_full))
            },
        )?;

        let b_status = b_resp.status();
        let c_status = c_resp.status();

        if is_transient_status(b_status) || is_transient_status(c_status) {
            let hint = [&b_resp, &c_resp]
                .iter()
                .filter_map(|r| {
                    r.headers()
                        .get("retry-after")
                        .and_then(|v| v.to_str().ok())
                        .and_then(parse_retry_after)
                })
                .max();
            return Ok(RetryDecision::Retry(hint));
        }

        Ok(RetryDecision::Done((b_resp, c_resp, b_status, c_status)))
    })
    .await?;

    let (b_resp, c_resp, b_status, c_status) = match attempted {
        Some(t) => t,
        None => {
            return Ok(Outcome::TransientSkip(format!(
                "{} drained retries",
                path
            )))
        }
    };

    let mut diffs = Vec::new();

    if b_status != c_status {
        diffs.push(Difference {
            path: format!("{} HTTP status", path),
            baseline_value: b_status.to_string(),
            candidate_value: c_status.to_string(),
        });
        return Ok(Outcome::Diffs(diffs));
    }

    let (b_bytes, c_bytes) = tokio::try_join!(
        async { b_resp.bytes().await.context("reading baseline bytes") },
        async { c_resp.bytes().await.context("reading candidate bytes") },
    )?;

    if b_bytes != c_bytes {
        diffs.push(Difference {
            path: format!("{} body", path),
            baseline_value: format!("{} bytes", b_bytes.len()),
            candidate_value: format!("{} bytes", c_bytes.len()),
        });
    }

    Ok(Outcome::Diffs(diffs))
}

fn truncate(s: &str) -> String {
    if s.len() > 120 {
        format!("{}...", &s[..117])
    } else {
        s.to_string()
    }
}
