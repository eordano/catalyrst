use axum::http::{HeaderMap, HeaderName, HeaderValue};
use catalyrst_builder::auth_chain::build_payload;
use catalyrst_builder::handlers::curation::authorize_admin;
use ethers_signers::{LocalWallet, Signer};

const PATH: &str = "/v1/collections/curation";

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
    fn byte(&mut self) -> u8 {
        (self.next() & 0xff) as u8
    }
}

fn header_name(i: usize) -> HeaderName {
    const NAMES: [&str; 12] = [
        "x-identity-auth-chain-0",
        "x-identity-auth-chain-1",
        "x-identity-auth-chain-2",
        "x-identity-auth-chain-3",
        "x-identity-auth-chain-4",
        "x-identity-auth-chain-5",
        "x-identity-auth-chain-6",
        "x-identity-auth-chain-7",
        "x-identity-auth-chain-8",
        "x-identity-auth-chain-9",
        "x-identity-auth-chain-10",
        "x-identity-auth-chain-11",
    ];
    HeaderName::from_static(NAMES[i])
}

fn put(h: &mut HeaderMap, name: HeaderName, raw: &[u8]) {
    if let Ok(v) = HeaderValue::from_bytes(raw) {
        h.insert(name, v);
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}

async fn valid_headers() -> (Vec<(HeaderName, Vec<u8>)>, String) {
    let root: LocalWallet = "ac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80"
        .parse()
        .unwrap();
    let root_addr = format!("{:#x}", root.address());
    let ephemeral: LocalWallet = "59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d"
        .parse()
        .unwrap();
    let ephemeral_addr = format!("{:#x}", ephemeral.address());
    let ep = format!(
        "Decentraland Login\nEphemeral address: {}\nExpiration: 2099-01-01T00:00:00.000Z",
        ephemeral_addr
    );
    let ep_sig = format!("0x{}", root.sign_message(ep.as_bytes()).await.unwrap());
    let ts = now_ms().to_string();
    let canonical = build_payload("get", PATH, &ts, "{}");
    let ent_sig = format!(
        "0x{}",
        ephemeral.sign_message(canonical.as_bytes()).await.unwrap()
    );
    let link = |k: &str, p: &str, s: &str| {
        serde_json::json!({ "type": k, "payload": p, "signature": s })
            .to_string()
            .into_bytes()
    };
    let headers = vec![
        (header_name(0), link("SIGNER", &root_addr, "")),
        (header_name(1), link("ECDSA_EPHEMERAL", &ep, &ep_sig)),
        (
            header_name(2),
            link("ECDSA_SIGNED_ENTITY", &canonical, &ent_sig),
        ),
        (
            HeaderName::from_static("x-identity-timestamp"),
            ts.into_bytes(),
        ),
    ];
    (headers, root_addr.to_lowercase())
}

#[tokio::test]
async fn fuzz_curation_gate_never_panics_and_never_authorizes_garbage() {
    let (seed_valid, signer) = valid_headers().await;
    let admins = [signer];

    {
        let mut h = HeaderMap::new();
        for (n, v) in &seed_valid {
            put(&mut h, n.clone(), v);
        }
        assert!(
            authorize_admin(None, &admins, &h, "get", PATH).is_ok(),
            "seed chain must authorize, else the fuzz negative is vacuous"
        );
    }

    let mut rng = Rng(0x1234_5678_9abc_def0);
    let iterations = 100_000;

    for i in 0..iterations {
        let mut h = HeaderMap::new();
        match rng.below(3) {
            0 => {
                let links = rng.below(6);
                for l in 0..links {
                    let len = rng.below(48);
                    let raw: Vec<u8> = (0..len).map(|_| rng.byte()).collect();
                    put(&mut h, header_name(l), &raw);
                }
                let tslen = rng.below(20);
                let ts: Vec<u8> = (0..tslen).map(|_| rng.byte()).collect();
                put(&mut h, HeaderName::from_static("x-identity-timestamp"), &ts);
                let md: Vec<u8> = (0..rng.below(32)).map(|_| rng.byte()).collect();
                put(&mut h, HeaderName::from_static("x-identity-metadata"), &md);
                let p: Vec<u8> = (0..rng.below(32)).map(|_| rng.byte()).collect();
                put(&mut h, HeaderName::from_static("x-original-path"), &p);
            }
            1 => {
                let kinds = ["SIGNER", "ECDSA_EPHEMERAL", "ECDSA_SIGNED_ENTITY", "BOGUS"];
                let n = 1 + rng.below(4);
                for l in 0..n {
                    let kind = kinds[rng.below(kinds.len())];
                    let payload: String = (0..rng.below(40))
                        .map(|_| (0x30 + rng.byte() % 0x40) as char)
                        .collect();
                    let sig: String = format!("0x{:x}", rng.next());
                    let raw = serde_json::json!({
                        "type": kind, "payload": payload, "signature": sig
                    })
                    .to_string();
                    put(&mut h, header_name(l), raw.as_bytes());
                }
                put(
                    &mut h,
                    HeaderName::from_static("x-identity-timestamp"),
                    now_ms().to_string().as_bytes(),
                );
            }
            _ => {
                for (n, v) in &seed_valid {
                    let mut bytes = v.clone();
                    let flips = 1 + rng.below(3);
                    for _ in 0..flips {
                        if !bytes.is_empty() {
                            let idx = rng.below(bytes.len());
                            bytes[idx] ^= 1 << rng.below(8);
                        }
                    }
                    put(&mut h, n.clone(), &bytes);
                }
            }
        }

        let decision = authorize_admin(None, &admins, &h, "get", PATH);
        assert!(
            decision.is_err(),
            "iteration {i} authorized a fuzzed/garbage chain via the signature branch"
        );
    }
}
