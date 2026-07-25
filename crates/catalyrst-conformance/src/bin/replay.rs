use anyhow::{Context, Result};
use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use catalyrst_conformance::diff::compare_json;
use catalyrst_conformance::fixture::{scrub_volatile_path, Fixture};
use catalyrst_conformance::volatility::Volatility;
use clap::Parser;
use colored::Colorize;
use glob::Pattern;
use reqwest::{Client, Method};
use std::path::{Path, PathBuf};
use std::time::Duration;
use walkdir::WalkDir;

const VOLATILE_HEADERS: &[&str] = &["date", "server", "x-request-id", "x-trace-id", "etag"];

#[derive(Parser, Debug)]
#[command(name = "catalyrst-conformance-replay", version)]
#[command(about = "Replay recorded fixture files against a candidate host and diff the responses.")]
struct Args {
    #[arg(long)]
    candidate: String,
    #[arg(long, default_value = "fixtures/")]
    fixtures: PathBuf,
    #[arg(long)]
    filter: Option<String>,
    #[arg(long, default_value_t = 30)]
    timeout_secs: u64,
    #[arg(long, default_value_t = false)]
    verbose: bool,
}

struct Outcome {
    fixture_path: PathBuf,
    passed: bool,
    notes: Vec<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    let pattern = match &args.filter {
        Some(p) => Some(Pattern::new(p).with_context(|| format!("invalid --filter glob: {p}"))?),
        None => None,
    };

    if !args.fixtures.exists() {
        anyhow::bail!("fixtures dir does not exist: {}", args.fixtures.display());
    }

    let fixtures = collect_fixtures(&args.fixtures, pattern.as_ref())?;
    if fixtures.is_empty() {
        eprintln!(
            "no fixtures found under {} matching filter {:?}",
            args.fixtures.display(),
            args.filter
        );
        return Ok(());
    }

    let client = Client::builder()
        .timeout(Duration::from_secs(args.timeout_secs))
        .build()
        .context("building HTTP client")?;

    let candidate = args.candidate.trim_end_matches('/').to_string();

    println!(
        "Replaying {} fixture(s) against {}",
        fixtures.len(),
        candidate
    );

    let volatility = Volatility::default();

    let mut outcomes: Vec<Outcome> = Vec::new();
    for fx_path in &fixtures {
        let outcome = replay_one(&client, &candidate, fx_path, &volatility, args.verbose).await;
        outcomes.push(outcome);
    }

    let passed = outcomes.iter().filter(|o| o.passed).count();
    let failed = outcomes.len() - passed;

    println!();
    println!(
        "SUMMARY: {}/{} fixtures passed, {} failed",
        passed,
        outcomes.len(),
        failed
    );

    if failed > 0 {
        std::process::exit(1);
    }
    Ok(())
}

fn collect_fixtures(root: &Path, filter: Option<&Pattern>) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    for entry in WalkDir::new(root).follow_links(false) {
        let entry = entry.with_context(|| format!("walking {}", root.display()))?;
        if !entry.file_type().is_file() {
            continue;
        }
        let p = entry.path();
        if p.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        if let Some(pat) = filter {
            let rel = p.strip_prefix(root).unwrap_or(p);
            let rel_str = rel.to_string_lossy();
            if !pat.matches(&rel_str) {
                continue;
            }
        }
        out.push(p.to_path_buf());
    }
    out.sort();
    Ok(out)
}

async fn replay_one(
    client: &Client,
    candidate: &str,
    fx_path: &Path,
    volatility: &Volatility,
    verbose: bool,
) -> Outcome {
    let mut outcome = Outcome {
        fixture_path: fx_path.to_path_buf(),
        passed: false,
        notes: Vec::new(),
    };

    let raw = match std::fs::read_to_string(fx_path) {
        Ok(s) => s,
        Err(e) => {
            outcome.notes.push(format!("read error: {e}"));
            print_outcome(&outcome);
            return outcome;
        }
    };
    let fixture: Fixture = match serde_json::from_str(&raw) {
        Ok(f) => f,
        Err(e) => {
            outcome.notes.push(format!("parse error: {e}"));
            print_outcome(&outcome);
            return outcome;
        }
    };

    let method = match Method::from_bytes(fixture.request.method.as_bytes()) {
        Ok(m) => m,
        Err(e) => {
            outcome
                .notes
                .push(format!("invalid method {:?}: {e}", fixture.request.method));
            print_outcome(&outcome);
            return outcome;
        }
    };

    let url = format!("{}{}", candidate, fixture.request.path);
    let mut req = client.request(method, &url);
    if !fixture.request.query.is_empty() {
        let qp: Vec<(&str, &str)> = fixture
            .request
            .query
            .iter()
            .flat_map(|(k, vs)| vs.iter().map(move |v| (k.as_str(), v.as_str())))
            .collect();
        req = req.query(&qp);
    }
    for (k, v) in &fixture.request.headers {
        if k.eq_ignore_ascii_case("content-type") && fixture.request.body.is_some() {
            continue;
        }
        req = req.header(k, v);
    }
    if let Some(body) = &fixture.request.body {
        req = req.json(body);
    }

    let resp = match req.send().await {
        Ok(r) => r,
        Err(e) => {
            outcome.notes.push(format!("HTTP error: {e}"));
            print_outcome(&outcome);
            return outcome;
        }
    };

    let cand_status = resp.status().as_u16();
    if cand_status != fixture.response.status {
        outcome.notes.push(format!(
            "status: fixture={} candidate={}",
            fixture.response.status, cand_status
        ));
    }

    let cand_headers = resp.headers().clone();
    for (raw_key, expected) in &fixture.response.headers {
        let key_lc = raw_key.to_ascii_lowercase();
        if VOLATILE_HEADERS.contains(&key_lc.as_str()) {
            continue;
        }
        let actual_opt = cand_headers
            .get(raw_key.as_str())
            .and_then(|v| v.to_str().ok());
        let Some(actual) = actual_opt else {
            outcome.notes.push(format!(
                "header {}: fixture={:?} candidate=<missing>",
                raw_key, expected
            ));
            continue;
        };

        let (exp_cmp, act_cmp) = if key_lc == "content-type" {
            (
                expected
                    .split(';')
                    .next()
                    .unwrap_or(expected)
                    .trim()
                    .to_string(),
                actual
                    .split(';')
                    .next()
                    .unwrap_or(actual)
                    .trim()
                    .to_string(),
            )
        } else {
            (expected.trim().to_string(), actual.trim().to_string())
        };
        if exp_cmp != act_cmp {
            outcome.notes.push(format!(
                "header {}: fixture={:?} candidate={:?}",
                raw_key, expected, actual
            ));
        }
    }

    let bytes = match resp.bytes().await {
        Ok(b) => b,
        Err(e) => {
            outcome.notes.push(format!("body read error: {e}"));
            print_outcome(&outcome);
            return outcome;
        }
    };

    if let Some(expected_b64) = &fixture.response.body_bytes_b64 {
        match B64.decode(expected_b64.as_bytes()) {
            Ok(expected) => {
                if expected.as_slice() != bytes.as_ref() {
                    outcome.notes.push(format!(
                        "body bytes differ: fixture={}B candidate={}B",
                        expected.len(),
                        bytes.len()
                    ));
                }
            }
            Err(e) => {
                outcome
                    .notes
                    .push(format!("fixture body_bytes_b64 not valid base64: {e}"));
            }
        }
    } else if let Some(expected_json) = &fixture.response.body_json {
        let mut baseline = expected_json.clone();
        let mut candidate_json: serde_json::Value = match serde_json::from_slice(&bytes) {
            Ok(v) => v,
            Err(e) => {
                outcome
                    .notes
                    .push(format!("candidate body is not valid JSON: {e}"));
                print_outcome(&outcome);
                outcome.passed = outcome.notes.is_empty();
                return outcome;
            }
        };

        for path in &fixture.volatile_paths {
            scrub_volatile_path(&mut baseline, path);
            scrub_volatile_path(&mut candidate_json, path);
        }

        let section = fx_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("fixture");
        let diffs = compare_json(section, "fixture", &baseline, &candidate_json, volatility);
        if !diffs.is_empty() {
            outcome
                .notes
                .push(format!("body diff: {} difference(s)", diffs.len()));
            let limit = if verbose { diffs.len() } else { 5 };
            for d in diffs.iter().take(limit) {
                outcome.notes.push(format!("  {}", d));
            }
            if !verbose && diffs.len() > 5 {
                outcome
                    .notes
                    .push(format!("  ... and {} more", diffs.len() - 5));
            }
        }
    } else {
        outcome
            .notes
            .push("fixture has neither body_json nor body_bytes_b64".to_string());
    }

    outcome.passed = outcome.notes.is_empty();
    print_outcome(&outcome);
    outcome
}

fn print_outcome(o: &Outcome) {
    let label = o.fixture_path.display().to_string();
    if o.passed {
        println!("  {} {}", "✓".green(), label);
    } else {
        println!("  {} {}", "✗".red(), label);
        for n in &o.notes {
            println!("      {}", n);
        }
    }
}
