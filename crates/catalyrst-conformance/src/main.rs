mod bootstrap;
mod capture;
mod checks;
mod diff;
mod fixture;
mod retry;
mod sections;
mod volatility;

use anyhow::Result;
use bootstrap::bootstrap_data;
use clap::Parser;
use colored::Colorize;
use diff::Difference;
use reqwest::Client;
use sections::{run_content_section, run_lambdas_section};
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
        Scoreboard {
            passed: 0,
            failed: 0,
            skipped: 0,
        }
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

enum PairOutcome {
    Got((reqwest::StatusCode, reqwest::StatusCode, String, String)),
    Transient(String),
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
