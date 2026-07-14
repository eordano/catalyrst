use std::sync::Arc;

use anyhow::{anyhow, bail, Context, Result};
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _;
use chrono::{DateTime, Utc};
use parking_lot::Mutex;

use catalyrst_crypto::Wallet;

pub const STORAGE_DELEGATION_PREFIX: &str = "Decentraland Authoritative Storage Delegation";

pub const MAX_SCOPE_HEADER_LENGTH: usize = 4096;

pub const REFRESH_BUFFER_SECS: i64 = 5 * 60;

pub const RENEWAL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

pub type DelegationSlot = Arc<Mutex<Option<StorageDelegation>>>;

#[derive(Clone)]
pub struct StorageDelegation {
    pub ephemeral: Arc<Wallet>,
    pub ephemeral_address: String,
    pub world: String,
    pub scene_id: String,
    pub parcel: String,
    pub expiration: DateTime<Utc>,

    pub scope_header: String,
}

impl std::fmt::Debug for StorageDelegation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StorageDelegation")
            .field("ephemeral_address", &self.ephemeral_address)
            .field("world", &self.world)
            .field("scene_id", &self.scene_id)
            .field("parcel", &self.parcel)
            .field("expiration", &self.expiration)
            .finish_non_exhaustive()
    }
}

impl StorageDelegation {
    pub fn is_expired(&self, now: DateTime<Utc>) -> bool {
        self.expiration <= now
    }

    pub fn near_expiry(&self, now: DateTime<Utc>) -> bool {
        self.expiration - chrono::Duration::seconds(REFRESH_BUFFER_SECS) <= now
    }
}

fn claim_field(payload: &str, prefix: &str) -> Option<String> {
    payload
        .lines()
        .find_map(|l| l.strip_prefix(prefix))
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

pub fn parse_storage_delegation(encoded: &str) -> Option<StorageDelegation> {
    let decoded = BASE64.decode(encoded.trim()).ok()?;
    let value: serde_json::Value = serde_json::from_slice(&decoded).ok()?;
    let obj = value.as_object()?;
    if obj.get("v")?.as_u64()? != 1 {
        return None;
    }
    let ephemeral = obj.get("ephemeral")?.as_object()?;
    let private_key = ephemeral.get("privateKey")?.as_str()?;
    ephemeral.get("publicKey")?.as_str()?;
    let address = ephemeral.get("address")?.as_str()?;
    let scope = obj.get("scope")?.as_object()?;
    let payload = scope.get("payload")?.as_str()?;
    let signature = scope.get("signature")?.as_str()?;

    if payload.lines().next() != Some(STORAGE_DELEGATION_PREFIX) {
        return None;
    }
    let world = claim_field(payload, "World:")?.to_lowercase();
    let scene_id = claim_field(payload, "SceneId:")?;
    let parcel = claim_field(payload, "Parcel:")?;
    let expiration = DateTime::parse_from_rfc3339(&claim_field(payload, "Expiration:")?)
        .ok()?
        .with_timezone(&Utc);

    let wallet = Wallet::from_hex(private_key).ok()?;
    let ephemeral_address = wallet.address();
    if !ephemeral_address.eq_ignore_ascii_case(address)
        || !ephemeral_address.eq_ignore_ascii_case(&claim_field(payload, "Ephemeral:")?)
    {
        return None;
    }

    let scope_header = BASE64
        .encode(serde_json::json!({ "payload": payload, "signature": signature }).to_string());
    if scope_header.len() > MAX_SCOPE_HEADER_LENGTH {
        return None;
    }

    Some(StorageDelegation {
        ephemeral: Arc::new(wallet),
        ephemeral_address,
        world,
        scene_id,
        parcel,
        expiration,
        scope_header,
    })
}

pub async fn mint_from_minter(
    http: &reqwest::Client,
    minter_url: &str,
    token: Option<&str>,
    world: &str,
    scene_id: &str,
    parcel: &str,
) -> Result<StorageDelegation> {
    let endpoint = format!("{}/delegations", minter_url.trim_end_matches('/'));
    let mut req = http.post(&endpoint).json(&serde_json::json!({
        "world": world,
        "sceneId": scene_id,
        "parcel": parcel,
    }));
    if let Some(token) = token {
        req = req.bearer_auth(token);
    }
    let resp = req
        .send()
        .await
        .context("delegation minter unreachable")?
        .error_for_status()
        .context("delegation minter rejected the request")?;
    let body: serde_json::Value = resp
        .json()
        .await
        .context("delegation minter returned a malformed response")?;
    let encoded = body
        .get("delegation")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("delegation minter response is missing the delegation"))?;
    let delegation = parse_storage_delegation(encoded)
        .ok_or_else(|| anyhow!("delegation minter returned an unparseable delegation"))?;

    if delegation.world != world.to_lowercase()
        || delegation.scene_id != scene_id
        || delegation.parcel != parcel
    {
        bail!("delegation minter answered for a different scene");
    }
    Ok(delegation)
}

#[allow(clippy::too_many_arguments)]
pub async fn renewal_loop(
    http: reqwest::Client,
    minter_url: String,
    token: Option<String>,
    world: String,
    scene_id: String,
    parcel: String,
    slot: DelegationSlot,
    mut rx: tokio::sync::mpsc::UnboundedReceiver<tokio::sync::oneshot::Sender<()>>,
) {
    let mut backoff = std::time::Duration::from_secs(1);
    loop {
        let due = {
            let guard = slot.lock();
            match guard.as_ref() {
                Some(d) => {
                    (d.expiration - chrono::Duration::seconds(REFRESH_BUFFER_SECS) - Utc::now())
                        .to_std()
                        .unwrap_or(std::time::Duration::ZERO)
                }
                None => std::time::Duration::ZERO,
            }
        };
        let sleep_for = if due.is_zero() { backoff } else { due };

        let mut waiters: Vec<tokio::sync::oneshot::Sender<()>> = Vec::new();
        tokio::select! {
            biased;
            req = rx.recv() => match req {
                Some(w) => waiters.push(w),
                None => return,
            },
            _ = tokio::time::sleep(sleep_for) => {}
        }
        while let Ok(w) = rx.try_recv() {
            waiters.push(w);
        }

        let needs = slot
            .lock()
            .as_ref()
            .map(|d| d.near_expiry(Utc::now()))
            .unwrap_or(true);
        if needs {
            match mint_from_minter(
                &http,
                &minter_url,
                token.as_deref(),
                &world,
                &scene_id,
                &parcel,
            )
            .await
            {
                Ok(d) => {
                    *slot.lock() = Some(d);
                    backoff = std::time::Duration::from_secs(1);
                    tracing::info!(world = %world, "storage delegation renewed");
                }
                Err(_) => {
                    backoff = (backoff * 2).min(std::time::Duration::from_secs(300));
                    tracing::warn!(world = %world, "storage delegation renewal failed");
                }
            }
        }
        for w in waiters {
            let _ = w.send(());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use catalyrst_world_storage::delegation::{verify_storage_delegation, StorageDelegationTarget};
    use chrono::Duration;

    const AUTHORITATIVE_KEY: &str =
        "0x4c0883a69102937d6231471b5dbb6204fe5129617082792ae468d01a3f362318";
    const EPHEMERAL_KEY: &str =
        "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80";
    const EPHEMERAL2_KEY: &str =
        "0x59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d";

    const WORLD: &str = "myworld.dcl.eth";
    const SCENE_ID: &str = "bafkreigcene";
    const PARCEL: &str = "10,-25";

    fn claim_payload(
        ephemeral_address: &str,
        world: &str,
        scene_id: &str,
        parcel: &str,
        expiration: &str,
    ) -> String {
        format!(
            "{STORAGE_DELEGATION_PREFIX}\nEphemeral: {ephemeral_address}\nWorld: {world}\nSceneId: {scene_id}\nParcel: {parcel}\nExpiration: {expiration}"
        )
    }

    fn envelope_from_payload(ephemeral_key: &str, payload: &str, signature: &str) -> String {
        let eph = Wallet::from_hex(ephemeral_key).unwrap();
        let env = serde_json::json!({
            "v": 1,
            "ephemeral": {
                "privateKey": ephemeral_key,
                "publicKey": "0x04",
                "address": eph.address(),
            },
            "scope": { "payload": payload, "signature": signature },
        });
        BASE64.encode(env.to_string())
    }

    fn mint(
        ephemeral_key: &str,
        world: &str,
        scene_id: &str,
        parcel: &str,
        ttl_secs: i64,
    ) -> String {
        let authoritative = Wallet::from_hex(AUTHORITATIVE_KEY).unwrap();
        let eph = Wallet::from_hex(ephemeral_key).unwrap();
        let payload = claim_payload(
            &eph.address(),
            world,
            scene_id,
            parcel,
            &(Utc::now() + Duration::seconds(ttl_secs)).to_rfc3339(),
        );
        let signature = authoritative.sign_message(payload.as_bytes()).unwrap();
        envelope_from_payload(ephemeral_key, &payload, &signature)
    }

    #[test]
    fn parses_a_minted_envelope_and_derives_fields_from_the_signed_payload() {
        let d = parse_storage_delegation(&mint(
            EPHEMERAL_KEY,
            "MyWorld.DCL.eth",
            SCENE_ID,
            PARCEL,
            3600,
        ))
        .expect("valid envelope must parse");
        let eph = Wallet::from_hex(EPHEMERAL_KEY).unwrap();
        assert_eq!(d.ephemeral_address, eph.address());
        assert_eq!(d.ephemeral.address(), eph.address());
        assert_eq!(d.world, WORLD, "world is lowercased");
        assert_eq!(d.scene_id, SCENE_ID);
        assert_eq!(d.parcel, PARCEL);
        assert!(!d.is_expired(Utc::now()));
        assert!(!d.near_expiry(Utc::now()));
        assert!(d.near_expiry(Utc::now() + Duration::seconds(3600 - 200)));
        assert!(d.is_expired(Utc::now() + Duration::seconds(3700)));
        assert!(d.scope_header.len() <= MAX_SCOPE_HEADER_LENGTH);
    }

    #[test]
    fn scope_header_verifies_against_the_real_world_storage_verifier() {
        assert_eq!(
            MAX_SCOPE_HEADER_LENGTH,
            catalyrst_world_storage::delegation::MAX_SCOPE_HEADER_LENGTH
        );
        let authoritative = Wallet::from_hex(AUTHORITATIVE_KEY).unwrap();
        let trusted = vec![authoritative.address()];
        let d =
            parse_storage_delegation(&mint(EPHEMERAL_KEY, WORLD, SCENE_ID, PARCEL, 3600)).unwrap();
        let signer = d.ephemeral_address.clone();

        fn target<'a>(
            signer: &'a str,
            world: &'a str,
            scene_id: &'a str,
            parcel: &'a str,
            trusted: &'a [String],
        ) -> StorageDelegationTarget<'a> {
            StorageDelegationTarget {
                signer,
                world,
                scene_id,
                parcel,
                trusted_signers: trusted,
            }
        }

        assert_eq!(
            verify_storage_delegation(
                &d.scope_header,
                &target(&signer, WORLD, SCENE_ID, PARCEL, &trusted)
            ),
            Ok(())
        );
        assert!(verify_storage_delegation(
            &d.scope_header,
            &target(&signer, "other.dcl.eth", SCENE_ID, PARCEL, &trusted)
        )
        .is_err());
        assert!(verify_storage_delegation(
            &d.scope_header,
            &target(&signer, WORLD, "bafkreiother", PARCEL, &trusted)
        )
        .is_err());
        assert!(verify_storage_delegation(
            &d.scope_header,
            &target(&signer, WORLD, SCENE_ID, "0,0", &trusted)
        )
        .is_err());
        assert!(verify_storage_delegation(
            &d.scope_header,
            &target(
                "0x2222222222222222222222222222222222222222",
                WORLD,
                SCENE_ID,
                PARCEL,
                &trusted
            )
        )
        .is_err());

        let rogue = Wallet::from_hex(EPHEMERAL2_KEY).unwrap();
        let eph = Wallet::from_hex(EPHEMERAL_KEY).unwrap();
        let payload = claim_payload(
            &eph.address(),
            WORLD,
            SCENE_ID,
            PARCEL,
            &(Utc::now() + Duration::hours(1)).to_rfc3339(),
        );
        let signature = rogue.sign_message(payload.as_bytes()).unwrap();
        let d =
            parse_storage_delegation(&envelope_from_payload(EPHEMERAL_KEY, &payload, &signature))
                .unwrap();
        assert_eq!(
            verify_storage_delegation(
                &d.scope_header,
                &target(&signer, WORLD, SCENE_ID, PARCEL, &trusted)
            ),
            Err("claim not signed by a trusted authoritative address")
        );
    }

    #[test]
    fn expired_envelope_parses_but_fails_the_verifier_and_is_expired() {
        let authoritative = Wallet::from_hex(AUTHORITATIVE_KEY).unwrap();
        let trusted = vec![authoritative.address()];
        let d =
            parse_storage_delegation(&mint(EPHEMERAL_KEY, WORLD, SCENE_ID, PARCEL, -60)).unwrap();
        assert!(d.is_expired(Utc::now()));
        let signer = d.ephemeral_address.clone();
        assert_eq!(
            verify_storage_delegation(
                &d.scope_header,
                &StorageDelegationTarget {
                    signer: &signer,
                    world: WORLD,
                    scene_id: SCENE_ID,
                    parcel: PARCEL,
                    trusted_signers: &trusted,
                }
            ),
            Err("delegation expired")
        );
    }

    #[test]
    fn rejects_malformed_envelopes() {
        assert!(parse_storage_delegation("!!!not-base64!!!").is_none());
        assert!(parse_storage_delegation(&BASE64.encode("not json")).is_none());
        assert!(parse_storage_delegation(&BASE64.encode("[1,2]")).is_none());

        let eph = Wallet::from_hex(EPHEMERAL_KEY).unwrap();
        let authoritative = Wallet::from_hex(AUTHORITATIVE_KEY).unwrap();
        let exp = (Utc::now() + Duration::hours(1)).to_rfc3339();
        let payload = claim_payload(&eph.address(), WORLD, SCENE_ID, PARCEL, &exp);
        let signature = authoritative.sign_message(payload.as_bytes()).unwrap();

        let cases: Vec<serde_json::Value> = vec![
            serde_json::json!({ "v": 2, "ephemeral": { "privateKey": EPHEMERAL_KEY, "publicKey": "0x04", "address": eph.address() }, "scope": { "payload": payload, "signature": signature } }),
            serde_json::json!({ "ephemeral": { "privateKey": EPHEMERAL_KEY, "publicKey": "0x04", "address": eph.address() }, "scope": { "payload": payload, "signature": signature } }),
            serde_json::json!({ "v": 1, "ephemeral": { "privateKey": 1, "publicKey": "0x04", "address": eph.address() }, "scope": { "payload": payload, "signature": signature } }),
            serde_json::json!({ "v": 1, "ephemeral": { "privateKey": EPHEMERAL_KEY, "publicKey": null, "address": eph.address() }, "scope": { "payload": payload, "signature": signature } }),
            serde_json::json!({ "v": 1, "ephemeral": { "privateKey": EPHEMERAL2_KEY, "publicKey": "0x04", "address": eph.address() }, "scope": { "payload": payload, "signature": signature } }),
            serde_json::json!({ "v": 1, "ephemeral": { "privateKey": EPHEMERAL_KEY, "publicKey": "0x04", "address": eph.address() } }),
            serde_json::json!({ "v": 1, "ephemeral": { "privateKey": EPHEMERAL_KEY, "publicKey": "0x04", "address": eph.address() }, "scope": { "payload": 5, "signature": signature } }),
        ];
        for (i, env) in cases.iter().enumerate() {
            assert!(
                parse_storage_delegation(&BASE64.encode(env.to_string())).is_none(),
                "case {i} must be rejected"
            );
        }

        for bad_payload in [
            format!("wrong prefix\nEphemeral: {}", eph.address()),
            format!(
                "{STORAGE_DELEGATION_PREFIX}\nEphemeral: {}\nWorld: {WORLD}\nSceneId: {SCENE_ID}\nParcel: {PARCEL}",
                eph.address()
            ),
            claim_payload(&eph.address(), WORLD, SCENE_ID, PARCEL, "tomorrow"),
        ] {
            let sig = authoritative.sign_message(bad_payload.as_bytes()).unwrap();
            assert!(
                parse_storage_delegation(&envelope_from_payload(EPHEMERAL_KEY, &bad_payload, &sig))
                    .is_none(),
                "payload {bad_payload:?} must be rejected"
            );
        }

        let other = Wallet::from_hex(EPHEMERAL2_KEY).unwrap();
        let mismatched = claim_payload(&other.address(), WORLD, SCENE_ID, PARCEL, &exp);
        let sig = authoritative.sign_message(mismatched.as_bytes()).unwrap();
        assert!(
            parse_storage_delegation(&envelope_from_payload(EPHEMERAL_KEY, &mismatched, &sig))
                .is_none()
        );
    }

    async fn serve_minter(
        response: serde_json::Value,
    ) -> (String, Arc<std::sync::atomic::AtomicUsize>) {
        let hits = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let hits2 = Arc::clone(&hits);
        let app = axum::Router::new().route(
            "/delegations",
            axum::routing::post(move || {
                let hits = Arc::clone(&hits2);
                let response = response.clone();
                async move {
                    hits.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    axum::Json(response)
                }
            }),
        );
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        (format!("http://{addr}"), hits)
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn renewal_loop_swaps_the_slot_on_demand() {
        let fresh = mint(EPHEMERAL2_KEY, WORLD, SCENE_ID, PARCEL, 3600);
        let (minter, _hits) = serve_minter(serde_json::json!({ "delegation": fresh })).await;

        let near_expiry =
            parse_storage_delegation(&mint(EPHEMERAL_KEY, WORLD, SCENE_ID, PARCEL, 60)).unwrap();
        let old_address = near_expiry.ephemeral_address.clone();
        let slot: DelegationSlot = Arc::new(Mutex::new(Some(near_expiry)));

        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let task = tokio::spawn(renewal_loop(
            reqwest::Client::new(),
            minter,
            None,
            WORLD.to_string(),
            SCENE_ID.to_string(),
            PARCEL.to_string(),
            Arc::clone(&slot),
            rx,
        ));

        let (done_tx, done_rx) = tokio::sync::oneshot::channel();
        tx.send(done_tx).unwrap();
        tokio::time::timeout(RENEWAL_TIMEOUT, done_rx)
            .await
            .expect("renewal must answer within the timeout")
            .unwrap();

        let renewed = slot.lock().clone().expect("slot must stay filled");
        assert_ne!(
            renewed.ephemeral_address, old_address,
            "slot must be swapped"
        );
        assert!(!renewed.near_expiry(Utc::now()));
        task.abort();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn renewal_keeps_the_slot_when_the_minter_rebinds_or_is_down() {
        let rebind = mint(EPHEMERAL2_KEY, WORLD, "bafkreiother", PARCEL, 3600);
        let (minter, _hits) = serve_minter(serde_json::json!({ "delegation": rebind })).await;

        let current =
            parse_storage_delegation(&mint(EPHEMERAL_KEY, WORLD, SCENE_ID, PARCEL, 60)).unwrap();
        let old_address = current.ephemeral_address.clone();
        let slot: DelegationSlot = Arc::new(Mutex::new(Some(current)));

        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let task = tokio::spawn(renewal_loop(
            reqwest::Client::new(),
            minter,
            None,
            WORLD.to_string(),
            SCENE_ID.to_string(),
            PARCEL.to_string(),
            Arc::clone(&slot),
            rx,
        ));
        let (done_tx, done_rx) = tokio::sync::oneshot::channel();
        tx.send(done_tx).unwrap();
        tokio::time::timeout(RENEWAL_TIMEOUT, done_rx)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            slot.lock().clone().unwrap().ephemeral_address,
            old_address,
            "a rebinding minter answer must not replace the slot"
        );
        task.abort();

        let slot2: DelegationSlot = Arc::new(Mutex::new(Some(
            parse_storage_delegation(&mint(EPHEMERAL_KEY, WORLD, SCENE_ID, PARCEL, 60)).unwrap(),
        )));
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let task = tokio::spawn(renewal_loop(
            reqwest::Client::new(),
            "http://127.0.0.1:9".to_string(),
            None,
            WORLD.to_string(),
            SCENE_ID.to_string(),
            PARCEL.to_string(),
            Arc::clone(&slot2),
            rx,
        ));
        let (done_tx, done_rx) = tokio::sync::oneshot::channel();
        tx.send(done_tx).unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(10), done_rx)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(slot2.lock().clone().unwrap().ephemeral_address, old_address);
        task.abort();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn mint_from_minter_rejects_a_rebind() {
        let rebind = mint(EPHEMERAL2_KEY, WORLD, SCENE_ID, "0,0", 3600);
        let (minter, _hits) = serve_minter(serde_json::json!({ "delegation": rebind })).await;
        let err = mint_from_minter(
            &reqwest::Client::new(),
            &minter,
            None,
            WORLD,
            SCENE_ID,
            PARCEL,
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("different scene"), "{err:#}");
    }
}
