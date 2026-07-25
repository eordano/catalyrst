use anyhow::{Context, Result};
use base64::Engine;
use serde::{Deserialize, Serialize};
use sqlx::postgres::PgPoolOptions;
use sqlx::Row;
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize)]
struct AuthChainVector {
    deployment_id: i32,
    entity_type: String,
    entity_id: String,
    deployer_address: String,
    auth_chain: serde_json::Value,
}

#[derive(Serialize, Deserialize)]
struct EntityVector {
    deployment_id: i32,
    entity_type: String,
    entity_id: String,
    entity_metadata: serde_json::Value,
    entity_pointers: Vec<String>,
    entity_timestamp_ms: i64,
}

#[derive(Serialize, Deserialize)]
struct HashVector {
    hash: String,
    bytes_base64: String,
    expected_cidv0: String,
    expected_cidv1: String,
}

#[derive(Serialize, Deserialize)]
struct ContentFileEntry {
    key: String,
    content_hash: String,
}

#[derive(Serialize, Deserialize)]
struct DeploymentVector {
    deployment_id: i32,
    entity_type: String,
    entity_id: String,
    deployer_address: String,
    entity_pointers: Vec<String>,
    entity_timestamp_ms: i64,
    auth_chain: serde_json::Value,
    entity_metadata: Option<serde_json::Value>,
    content_files: Vec<ContentFileEntry>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let crate_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let vectors_dir = crate_root.join("test-vectors");
    tokio::fs::create_dir_all(&vectors_dir).await?;

    let content_root = std::env::var("CATALYRST_ORACLE_CONTENT_ROOT")
        .map(PathBuf::from)
        .expect("CATALYRST_ORACLE_CONTENT_ROOT not set");

    let db_url = std::env::var("CATALYRST_ORACLE_DB_URL")
        .expect("CATALYRST_ORACLE_DB_URL not set — point at a populated content DB");
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(&db_url)
        .await
        .context("Failed to connect to postgres")?;

    println!("Connected to postgres. Extracting test vectors...");

    let (auth_chains, entities, deployments) = tokio::try_join!(
        extract_auth_chains(&pool),
        extract_entities(&pool),
        extract_deployments(&pool),
    )?;

    let hashes = extract_hashes(&content_root).await?;

    write_json(&vectors_dir.join("auth-chains.json"), &auth_chains).await?;
    write_json(&vectors_dir.join("entities.json"), &entities).await?;
    write_json(&vectors_dir.join("hashes.json"), &hashes).await?;
    write_json(&vectors_dir.join("deployments.json"), &deployments).await?;

    println!(
        "Done! Wrote {} auth chains, {} entities, {} hashes, {} deployments",
        auth_chains.len(),
        entities.len(),
        hashes.len(),
        deployments.len()
    );

    Ok(())
}

async fn extract_auth_chains(pool: &sqlx::PgPool) -> Result<Vec<AuthChainVector>> {
    let mut vectors = Vec::new();

    let type_counts = [
        ("profile", 20i64),
        ("scene", 15),
        ("wearable", 10),
        ("emote", 2),
        ("store", 2),
        ("outfits", 1),
    ];

    for (etype, count) in &type_counts {
        let rows = sqlx::query(
            "SELECT id, entity_type, entity_id, deployer_address, auth_chain::text \
             FROM deployments \
             WHERE entity_type = $1 AND deleter_deployment IS NULL \
             ORDER BY id DESC \
             LIMIT $2",
        )
        .bind(etype)
        .bind(count)
        .fetch_all(pool)
        .await
        .with_context(|| format!("querying auth chains for {etype}"))?;

        for row in rows {
            let auth_chain_text: String = row.get("auth_chain");
            let auth_chain: serde_json::Value = serde_json::from_str(&auth_chain_text)?;
            vectors.push(AuthChainVector {
                deployment_id: row.get("id"),
                entity_type: row.get("entity_type"),
                entity_id: row.get("entity_id"),
                deployer_address: row.get("deployer_address"),
                auth_chain,
            });
        }
    }

    println!("  Extracted {} auth chains", vectors.len());
    Ok(vectors)
}

async fn extract_entities(pool: &sqlx::PgPool) -> Result<Vec<EntityVector>> {
    let mut vectors = Vec::new();

    let type_counts = [
        ("profile", 8i64),
        ("scene", 5),
        ("wearable", 4),
        ("emote", 2),
        ("store", 1),
    ];

    for (etype, count) in &type_counts {
        let rows = sqlx::query(
            "SELECT id, entity_type, entity_id, entity_pointers, \
                    entity_metadata::text, \
                    (EXTRACT(EPOCH FROM entity_timestamp) * 1000)::float8 AS ts_ms \
             FROM deployments \
             WHERE entity_type = $1 \
               AND entity_metadata IS NOT NULL \
               AND deleter_deployment IS NULL \
             ORDER BY id DESC \
             LIMIT $2",
        )
        .bind(etype)
        .bind(count)
        .fetch_all(pool)
        .await
        .with_context(|| format!("querying entities for {etype}"))?;

        for row in rows {
            let metadata_text: String = row.get("entity_metadata");
            let metadata: serde_json::Value = serde_json::from_str(&metadata_text)?;
            let pointers: Vec<String> = row.get("entity_pointers");
            let ts_ms: f64 = row.get("ts_ms");

            vectors.push(EntityVector {
                deployment_id: row.get("id"),
                entity_type: row.get("entity_type"),
                entity_id: row.get("entity_id"),
                entity_metadata: metadata,
                entity_pointers: pointers,
                entity_timestamp_ms: ts_ms as i64,
            });
        }
    }

    println!("  Extracted {} entities", vectors.len());
    Ok(vectors)
}

async fn extract_deployments(pool: &sqlx::PgPool) -> Result<Vec<DeploymentVector>> {
    let mut vectors = Vec::new();

    let type_counts = [
        ("profile", 3i64),
        ("scene", 3),
        ("wearable", 2),
        ("emote", 1),
        ("store", 1),
    ];

    for (etype, count) in &type_counts {
        let rows = sqlx::query(
            "SELECT d.id, d.entity_type, d.entity_id, d.deployer_address, \
                    d.entity_pointers, d.auth_chain::text, d.entity_metadata::text, \
                    (EXTRACT(EPOCH FROM d.entity_timestamp) * 1000)::float8 AS ts_ms \
             FROM deployments d \
             WHERE d.entity_type = $1 \
               AND d.deleter_deployment IS NULL \
               AND EXISTS (SELECT 1 FROM content_files cf WHERE cf.deployment = d.id) \
             ORDER BY d.id DESC \
             LIMIT $2",
        )
        .bind(etype)
        .bind(count)
        .fetch_all(pool)
        .await
        .with_context(|| format!("querying deployments for {etype}"))?;

        for row in rows {
            let deployment_id: i32 = row.get("id");
            let auth_chain_text: String = row.get("auth_chain");
            let auth_chain: serde_json::Value = serde_json::from_str(&auth_chain_text)?;
            let metadata_text: Option<String> = row.try_get("entity_metadata").ok();
            let metadata: Option<serde_json::Value> = metadata_text
                .map(|t| serde_json::from_str(&t))
                .transpose()?;
            let pointers: Vec<String> = row.get("entity_pointers");
            let ts_ms: f64 = row.get("ts_ms");

            let cf_rows = sqlx::query(
                "SELECT key, content_hash FROM content_files WHERE deployment = $1 ORDER BY key",
            )
            .bind(deployment_id)
            .fetch_all(pool)
            .await?;

            let content_files: Vec<ContentFileEntry> = cf_rows
                .iter()
                .map(|r| ContentFileEntry {
                    key: r.get("key"),
                    content_hash: r.get("content_hash"),
                })
                .collect();

            vectors.push(DeploymentVector {
                deployment_id,
                entity_type: row.get("entity_type"),
                entity_id: row.get("entity_id"),
                deployer_address: row.get("deployer_address"),
                entity_pointers: pointers,
                entity_timestamp_ms: ts_ms as i64,
                auth_chain,
                entity_metadata: metadata,
                content_files,
            });
        }
    }

    println!("  Extracted {} deployments", vectors.len());
    Ok(vectors)
}

async fn extract_hashes(content_root: &Path) -> Result<Vec<HashVector>> {
    let contents_dir = content_root.join("contents");
    let mut vectors = Vec::new();
    let mut shard_dirs: Vec<_> = Vec::new();

    let mut dir = tokio::fs::read_dir(&contents_dir).await?;
    while let Some(entry) = dir.next_entry().await? {
        if entry.file_type().await?.is_dir() {
            shard_dirs.push(entry.path());
        }
    }

    shard_dirs.sort();
    let step = shard_dirs.len().max(1) / 30;

    let mut checked = 0usize;
    'outer: for (i, shard) in shard_dirs.iter().enumerate() {
        if step > 0 && i % step != 0 {
            continue;
        }

        let mut entries = tokio::fs::read_dir(shard).await?;
        while let Some(entry) = entries.next_entry().await? {
            let name = entry.file_name();
            let name_str = name.to_string_lossy().to_string();

            if name_str.ends_with(".gzip") {
                continue;
            }
            if !name_str.starts_with("Qm") && !name_str.starts_with("baf") {
                continue;
            }

            let meta = entry.metadata().await?;
            if !meta.is_file() || meta.len() >= 100_000 || meta.len() == 0 {
                continue;
            }

            let data = tokio::fs::read(entry.path()).await?;

            let cidv0 = catalyrst_hashing::hash_bytes(&data);
            let cidv1 = catalyrst_hashing::hash_bytes_v1(&data);

            let b64 = base64::engine::general_purpose::STANDARD.encode(&data);

            vectors.push(HashVector {
                hash: name_str,
                bytes_base64: b64,
                expected_cidv0: cidv0,
                expected_cidv1: cidv1,
            });

            checked += 1;
            if vectors.len() >= 30 {
                break 'outer;
            }
            if checked >= 2 {
                checked = 0;
                break;
            }
        }
    }

    println!("  Extracted {} hash vectors", vectors.len());
    Ok(vectors)
}

async fn write_json<T: Serialize>(path: &Path, data: &T) -> Result<()> {
    let json = serde_json::to_string_pretty(data)?;
    tokio::fs::write(path, json).await?;
    println!("  Wrote {}", path.display());
    Ok(())
}
