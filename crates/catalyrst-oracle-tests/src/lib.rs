#[cfg(test)]
mod tests {
    use base64::Engine;
    use serde::Deserialize;
    use std::path::PathBuf;

    #[derive(Deserialize)]
    #[allow(dead_code)]
    struct AuthChainVector {
        deployment_id: i32,
        entity_type: String,
        entity_id: String,
        deployer_address: String,
        auth_chain: serde_json::Value,
    }

    #[derive(Deserialize)]
    struct EntityVector {
        deployment_id: i32,
        entity_type: String,
        entity_id: String,
        entity_metadata: serde_json::Value,
        entity_pointers: Vec<String>,
        entity_timestamp_ms: i64,
    }

    #[derive(Deserialize)]
    struct HashVector {
        hash: String,
        bytes_base64: String,
        expected_cidv0: String,
        expected_cidv1: String,
    }

    #[derive(Deserialize)]
    struct ContentFileEntry {
        key: String,
        content_hash: String,
    }

    #[derive(Deserialize)]
    #[allow(dead_code)]
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

    fn vectors_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("test-vectors")
    }

    fn load_vectors<T: serde::de::DeserializeOwned>(filename: &str) -> Vec<T> {
        let path = vectors_dir().join(filename);
        let data = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("Failed to read {}: {e}. Run `cargo run -p catalyrst-oracle-tests --bin extract` first.", path.display()));
        serde_json::from_str(&data)
            .unwrap_or_else(|e| panic!("Failed to parse {}: {e}", path.display()))
    }

    #[test]
    #[ignore]
    fn oracle_hashing_cidv0() {
        let vectors: Vec<HashVector> = load_vectors("hashes.json");
        assert!(!vectors.is_empty(), "No hash vectors found");

        for (i, v) in vectors.iter().enumerate() {
            let data = base64::engine::general_purpose::STANDARD
                .decode(&v.bytes_base64)
                .unwrap_or_else(|e| panic!("vector {i}: bad base64: {e}"));

            let got = catalyrst_hashing::hash_bytes(&data);
            assert_eq!(
                got, v.expected_cidv0,
                "CIDv0 mismatch for vector {i} (hash={}): got {got}, expected {}",
                v.hash, v.expected_cidv0
            );
        }

        println!("  OK: {} CIDv0 hashes match", vectors.len());
    }

    #[test]
    #[ignore]
    fn oracle_hashing_cidv1() {
        let vectors: Vec<HashVector> = load_vectors("hashes.json");
        assert!(!vectors.is_empty(), "No hash vectors found");

        for (i, v) in vectors.iter().enumerate() {
            let data = base64::engine::general_purpose::STANDARD
                .decode(&v.bytes_base64)
                .unwrap_or_else(|e| panic!("vector {i}: bad base64: {e}"));

            let got = catalyrst_hashing::hash_bytes_v1(&data);
            assert_eq!(
                got, v.expected_cidv1,
                "CIDv1 mismatch for vector {i} (hash={}): got {got}, expected {}",
                v.hash, v.expected_cidv1
            );
        }

        println!("  OK: {} CIDv1 hashes match", vectors.len());
    }

    #[test]
    #[ignore]
    fn oracle_auth_chain_verification() {
        let vectors: Vec<AuthChainVector> = load_vectors("auth-chains.json");
        assert!(!vectors.is_empty(), "No auth chain vectors found");

        let mut verified = 0;
        let mut skipped_eip1654 = 0;

        for (i, v) in vectors.iter().enumerate() {
            let chain: catalyrst_crypto::AuthChain = serde_json::from_value(v.auth_chain.clone())
                .unwrap_or_else(|e| {
                    panic!(
                        "vector {i} (deployment {}): failed to parse auth chain: {e}",
                        v.deployment_id
                    )
                });

            let has_eip1654 = chain.iter().any(|link| {
                matches!(
                    link.link_type,
                    catalyrst_crypto::AuthLinkType::EcdsaEip1654Ephemeral
                        | catalyrst_crypto::AuthLinkType::EcdsaEip1654SignedEntity
                )
            });

            if has_eip1654 {
                skipped_eip1654 += 1;
                continue;
            }

            let now_ms = earliest_ephemeral_expiration(&chain)
                .map(|exp| exp - 1000)
                .unwrap_or(0);

            let result =
                catalyrst_crypto::verify::verify_auth_chain(&chain, &v.entity_id, Some(now_ms));

            assert!(
                result.is_ok(),
                "Auth chain verification failed for vector {i} \
                 (deployment {}, type={}, entity={}): {:?}",
                v.deployment_id,
                v.entity_type,
                v.entity_id,
                result.unwrap_err()
            );

            verified += 1;
        }

        println!(
            "  OK: {} auth chains verified, {} skipped (EIP-1654)",
            verified, skipped_eip1654
        );
        assert!(
            verified > 0,
            "All auth chains were skipped — no ECDSA chains found"
        );
    }

    #[test]
    #[ignore]
    fn oracle_entity_parsing() {
        let vectors: Vec<EntityVector> = load_vectors("entities.json");
        assert!(!vectors.is_empty(), "No entity vectors found");

        for (i, v) in vectors.iter().enumerate() {
            let entity_json = serde_json::json!({
                "type": v.entity_type,
                "pointers": v.entity_pointers,
                "timestamp": v.entity_timestamp_ms,
                "metadata": v.entity_metadata,
            });

            let entity_bytes = serde_json::to_vec(&entity_json).unwrap();

            let parsed = catalyrst_validator::parse_entity_from_bytes(&entity_bytes, &v.entity_id);

            match parsed {
                Ok(entity) => {
                    assert_eq!(
                        entity.entity_type.as_str(),
                        v.entity_type,
                        "vector {i} (deployment {}): entity type mismatch",
                        v.deployment_id
                    );
                    assert_eq!(
                        entity.id, v.entity_id,
                        "vector {i} (deployment {}): entity id mismatch",
                        v.deployment_id
                    );
                    assert_eq!(
                        entity.timestamp, v.entity_timestamp_ms,
                        "vector {i} (deployment {}): timestamp mismatch",
                        v.deployment_id
                    );
                    let expected_pointers: Vec<String> =
                        v.entity_pointers.iter().map(|p| p.to_lowercase()).collect();
                    assert_eq!(
                        entity.pointers, expected_pointers,
                        "vector {i} (deployment {}): pointers mismatch",
                        v.deployment_id
                    );
                    assert_eq!(
                        entity.metadata.as_ref(),
                        Some(&v.entity_metadata),
                        "vector {i} (deployment {}): metadata mismatch",
                        v.deployment_id
                    );
                }
                Err(e) => {
                    panic!(
                        "vector {i} (deployment {}, type={}): entity parse failed: {e}",
                        v.deployment_id, v.entity_type
                    );
                }
            }
        }

        println!("  OK: {} entities parsed and verified", vectors.len());
    }

    #[test]
    #[ignore]
    fn oracle_storage_hex_prefix() {
        let vectors: Vec<HashVector> = load_vectors("hashes.json");
        assert!(!vectors.is_empty(), "No hash vectors found");

        let content_root = PathBuf::from(
            std::env::var("CATALYRST_ORACLE_CONTENT_ROOT")
                .expect("CATALYRST_ORACLE_CONTENT_ROOT not set"),
        )
        .join("contents");

        for (i, v) in vectors.iter().enumerate() {
            let expected_prefix = compute_hex_prefix(&v.hash);

            let expected_path = content_root.join(&expected_prefix).join(&v.hash);

            assert!(
                expected_path.exists(),
                "Storage oracle: vector {i} (hash={}): expected file at {} but it does not exist. \
                 Computed prefix={expected_prefix}",
                v.hash,
                expected_path.display()
            );
        }

        println!(
            "  OK: {} hashes resolve to correct on-disk paths",
            vectors.len()
        );
    }

    fn earliest_ephemeral_expiration(chain: &catalyrst_crypto::AuthChain) -> Option<i64> {
        let mut earliest: Option<i64> = None;
        for link in chain {
            if matches!(
                link.link_type,
                catalyrst_crypto::AuthLinkType::EcdsaEphemeral
            ) {
                let payload = link.payload.replace('\r', "");
                for line in payload.lines() {
                    if let Some(date_str) = line.strip_prefix("Expiration: ") {
                        if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(date_str) {
                            let ms = dt.timestamp_millis();
                            earliest = Some(earliest.map_or(ms, |e: i64| e.min(ms)));
                        }
                    }
                }
            }
        }
        earliest
    }

    fn compute_hex_prefix(id: &str) -> String {
        use sha1::{Digest, Sha1};
        let mut hasher = Sha1::new();
        hasher.update(id.as_bytes());
        let digest = hasher.finalize();
        format!("{:02x}{:02x}", digest[0], digest[1])
    }

    #[test]
    #[ignore]
    fn oracle_deployment_content_files() {
        let vectors: Vec<DeploymentVector> = load_vectors("deployments.json");
        assert!(!vectors.is_empty(), "No deployment vectors found");

        let content_root = PathBuf::from(
            std::env::var("CATALYRST_ORACLE_CONTENT_ROOT")
                .expect("CATALYRST_ORACLE_CONTENT_ROOT not set"),
        )
        .join("contents");
        let mut total_files = 0;

        for (i, v) in vectors.iter().enumerate() {
            assert!(
                !v.content_files.is_empty(),
                "vector {i} (deployment {}): expected non-empty content_files",
                v.deployment_id
            );

            for cf in &v.content_files {
                let prefix = compute_hex_prefix(&cf.content_hash);
                let file_path = content_root.join(&prefix).join(&cf.content_hash);
                let gzip_path = PathBuf::from(format!("{}.gzip", file_path.display()));

                assert!(
                    file_path.exists() || gzip_path.exists(),
                    "vector {i} (deployment {}): content file {} (key={}) not found at {} or {}.gzip",
                    v.deployment_id,
                    cf.content_hash,
                    cf.key,
                    file_path.display(),
                    file_path.display()
                );

                total_files += 1;
            }
        }

        println!(
            "  OK: {} deployments, {} content files all resolve on disk",
            vectors.len(),
            total_files
        );
    }

    #[test]
    #[ignore]
    fn oracle_hashing_matches_stored_cid() {
        let vectors: Vec<HashVector> = load_vectors("hashes.json");
        assert!(!vectors.is_empty(), "No hash vectors found");

        for (i, v) in vectors.iter().enumerate() {
            let data = base64::engine::general_purpose::STANDARD
                .decode(&v.bytes_base64)
                .unwrap_or_else(|e| panic!("vector {i}: bad base64: {e}"));

            let cidv0 = catalyrst_hashing::hash_bytes(&data);
            let cidv1 = catalyrst_hashing::hash_bytes_v1(&data);

            let matches_v0 = v.hash == cidv0;
            let matches_v1 = v.hash == cidv1;

            assert!(
                matches_v0 || matches_v1,
                "vector {i}: on-disk CID {} does not match computed CIDv0 ({cidv0}) or CIDv1 ({cidv1})",
                v.hash
            );
        }

        println!(
            "  OK: {} on-disk CIDs verified against computed hashes",
            vectors.len()
        );
    }
}
