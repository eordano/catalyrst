use anyhow::{bail, Context, Result};
use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use catalyrst_conformance::fixture::{Fixture, RecordedRequest, RecordedResponse};
use chrono::Utc;
use clap::Parser;
use reqwest::{Client, Method};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

#[derive(Parser, Debug)]
#[command(name = "catalyrst-conformance-capture", version)]
#[command(about = "Capture a single HTTP request/response pair to a fixture JSON file.")]
struct Args {
    #[arg(long)]
    peer: String,
    #[arg(long, short = 'o')]
    output: PathBuf,
    #[arg(long)]
    description: Option<String>,
    #[arg(long, value_delimiter = ',')]
    volatile_paths: Vec<String>,
    #[arg(long, default_value_t = 30)]
    timeout_secs: u64,
    method: String,
    path: String,
    body: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    let method_upper = args.method.to_uppercase();
    let method = Method::from_bytes(method_upper.as_bytes())
        .with_context(|| format!("invalid HTTP method: {}", args.method))?;

    if !args.path.starts_with('/') {
        bail!("path must start with '/': got {:?}", args.path);
    }

    let body_value: Option<serde_json::Value> = match &args.body {
        None => None,
        Some(s) => {
            if !matches!(method, Method::POST | Method::PUT | Method::PATCH) {
                bail!(
                    "body supplied but method is {:?}; bodies are only valid for POST/PUT/PATCH",
                    method
                );
            }
            Some(
                serde_json::from_str::<serde_json::Value>(s)
                    .with_context(|| "request body must be valid JSON — multipart/form-data is not supported by capture")?,
            )
        }
    };

    let peer = args.peer.trim_end_matches('/').to_string();
    let url = format!("{}{}", peer, args.path);

    let client = Client::builder()
        .timeout(Duration::from_secs(args.timeout_secs))
        .build()
        .context("building HTTP client")?;

    let mut req = client.request(method.clone(), &url);
    let mut req_headers: BTreeMap<String, String> = BTreeMap::new();
    if let Some(b) = &body_value {
        req = req.json(b);
        req_headers.insert("content-type".to_string(), "application/json".to_string());
    }
    req_headers.insert("accept".to_string(), "application/json".to_string());
    req = req.header("accept", "application/json");

    eprintln!("→ {} {}", method, url);
    let resp = req.send().await.with_context(|| format!("GET {}", url))?;

    let status = resp.status().as_u16();
    let mut resp_headers: BTreeMap<String, String> = BTreeMap::new();
    let mut content_type: Option<String> = None;
    for (k, v) in resp.headers().iter() {
        let key = k.as_str().to_lowercase();
        if let Ok(s) = v.to_str() {
            if key == "content-type" {
                content_type = Some(s.to_string());
            }
            if matches!(
                key.as_str(),
                "content-type" | "content-length" | "etag" | "cache-control"
            ) {
                resp_headers.insert(key, s.to_string());
            }
        }
    }

    let bytes = resp
        .bytes()
        .await
        .with_context(|| format!("reading response body from {}", url))?;

    let is_json = content_type
        .as_deref()
        .map(|ct| ct.to_lowercase().contains("application/json"))
        .unwrap_or(false);

    let (body_json, body_bytes_b64) = if is_json {
        let parsed: serde_json::Value = serde_json::from_slice(&bytes).with_context(|| {
            format!(
                "response advertised content-type {:?} but body did not parse as JSON",
                content_type
            )
        })?;
        (Some(parsed), None)
    } else {
        (None, Some(B64.encode(&bytes)))
    };

    let fixture = Fixture {
        description: args.description.unwrap_or_default(),
        request: RecordedRequest {
            method: method_upper,
            path: args.path.clone(),
            query: BTreeMap::new(),
            headers: req_headers,
            body: body_value,
        },
        response: RecordedResponse {
            status,
            headers: resp_headers,
            body_json,
            body_bytes_b64,
        },
        captured_from: peer,
        captured_at: Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        volatile_paths: args.volatile_paths,
    };

    if let Some(parent) = args.output.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating parent dir {:?}", parent))?;
        }
    }

    let json = serde_json::to_string_pretty(&fixture).context("serialising fixture")?;
    std::fs::write(&args.output, json)
        .with_context(|| format!("writing fixture to {:?}", args.output))?;

    eprintln!(
        "← {} {}  →  wrote {} ({} bytes)",
        status,
        content_type.as_deref().unwrap_or("?"),
        args.output.display(),
        bytes.len()
    );

    if status >= 400 {
        eprintln!("note: captured response is an error ({}).", status);
    }

    Ok(())
}
