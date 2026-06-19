//! MLS happy-path artifact generator for the catalyrst-comms delivery service.
//!
//! The delivery service (`src/handlers/messaging.rs` + `src/mls.rs`) only ever
//! parses the *public framing* of MLS messages — it never joins a group or
//! decrypts. To exercise its happy path end-to-end we need a real RFC 9420
//! client to produce artifacts it will accept: a valid KeyPackage (signed leaf,
//! pinned ciphersuite, credential identity == wallet), an Add commit + Welcome,
//! and an application PrivateMessage carrying the right group_id + epoch.
//!
//! This example is a thin shim over `openmls` 0.6 that does exactly that, in a
//! single process (MLS group secrets must stay in one provider across the
//! create -> add -> message steps), and emits every artifact base64-encoded as
//! one JSON blob on stdout. A Python harness then drives the HTTP flow
//! (signed-fetch authed) with two wallets and verifies each route round-trips.
//!
//! Usage:
//!   cargo run --release -p catalyrst-comms --example mls_client -- \
//!       <walletA-lowercase> <walletB-lowercase> <group_id_hex>
//!
//! The pinned ciphersuite matches `mls::PINNED_CIPHERSUITE`
//! (MLS_128_DHKEMX25519_AES128GCM_SHA256_Ed25519 = 0x0001). The credential is a
//! BasicCredential whose identity is the *lowercase wallet address string bytes*
//! — that is what the server's publish-binding check compares against the
//! signed-fetch signer.

use base64::Engine;
use openmls::prelude::{tls_codec::Serialize as _, *};
use openmls_basic_credential::SignatureKeyPair;
use openmls_rust_crypto::OpenMlsRustCrypto;

const CS: Ciphersuite = Ciphersuite::MLS_128_DHKEMX25519_AES128GCM_SHA256_Ed25519;

fn b64(bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

/// One MLS identity: its own crypto provider (keystore) + signer + credential.
struct Identity {
    provider: OpenMlsRustCrypto,
    signer: SignatureKeyPair,
    credential_with_key: CredentialWithKey,
}

impl Identity {
    fn new(wallet: &str) -> Self {
        let provider = OpenMlsRustCrypto::default();
        // BasicCredential identity = the wallet address bytes (lowercase). The
        // server lowercases + compares this to the authed signer.
        let credential = BasicCredential::new(wallet.as_bytes().to_vec());
        let signer = SignatureKeyPair::new(CS.signature_algorithm())
            .expect("generate signature key pair");
        // Persist the signature key in this provider's store so the group can
        // sign with it later.
        signer
            .store(provider.storage())
            .expect("store signature key");
        let credential_with_key = CredentialWithKey {
            credential: credential.into(),
            signature_key: signer.public().into(),
        };
        Identity {
            provider,
            signer,
            credential_with_key,
        }
    }

    /// Build a fresh one-time KeyPackage and return both the live `KeyPackage`
    /// (for in-process group ops) and the wire `MlsMessage(KeyPackage)` bytes the
    /// directory accepts.
    fn new_key_package(&self) -> (KeyPackage, Vec<u8>) {
        let bundle = KeyPackage::builder()
            .build(CS, &self.provider, &self.signer, self.credential_with_key.clone())
            .expect("build key package");
        let kp = bundle.key_package().clone();
        let msg: MlsMessageOut = kp.clone().into();
        let bytes = msg.tls_serialize_detached().expect("serialize key package");
        (kp, bytes)
    }
}

fn main() {
    let mut args = std::env::args().skip(1);
    let wallet_a = args.next().expect("arg1 = walletA");
    let wallet_b = args.next().expect("arg2 = walletB");
    let group_id_hex = args.next().expect("arg3 = group_id hex");

    let wallet_a = wallet_a.to_lowercase();
    let wallet_b = wallet_b.to_lowercase();
    let gid_bytes = hex::decode(&group_id_hex).expect("group_id must be hex");

    let alice = Identity::new(&wallet_a);
    let bob = Identity::new(&wallet_b);

    // (1) A directory KeyPackage for each wallet (proves publish + claim path,
    // credential-binding to the authed wallet).
    let (_a_kp, a_kp_wire) = alice.new_key_package();
    let (b_kp, b_kp_wire) = bob.new_key_package();

    // (2) A creates the group with the explicit group_id the server will key on,
    // then adds B (Add commit + Welcome). A merges to land in the new epoch.
    let create_config = MlsGroupCreateConfig::builder()
        // PublicMessage handshakes keep the commit framing introspectable, which
        // is all the server parses; PrivateMessage would also be accepted.
        .ciphersuite(CS)
        .use_ratchet_tree_extension(true)
        .build();

    let mut group = MlsGroup::new_with_group_id(
        &alice.provider,
        &alice.signer,
        &create_config,
        GroupId::from_slice(&gid_bytes),
        alice.credential_with_key.clone(),
    )
    .expect("create group");

    let epoch_before = group.epoch().as_u64();

    let (commit_out, welcome_out, _group_info) = group
        .add_members(&alice.provider, &alice.signer, std::slice::from_ref(&b_kp))
        .expect("add member B");
    group
        .merge_pending_commit(&alice.provider)
        .expect("merge pending commit");

    let epoch_after = group.epoch().as_u64();

    let add_commit_wire = commit_out.tls_serialize_detached().expect("serialize commit");
    let welcome_wire = welcome_out.tls_serialize_detached().expect("serialize welcome");

    // (3) Application message from A in the new epoch.
    let app_out = group
        .create_message(&alice.provider, &alice.signer, b"gm from the MLS happy-path harness")
        .expect("create application message");
    let app_wire = app_out.tls_serialize_detached().expect("serialize application message");

    let out = serde_json::json!({
        "ciphersuite": u16::from(CS),
        "group_id_hex": group_id_hex,
        "wallet_a": wallet_a,
        "wallet_b": wallet_b,
        "epoch_before_add": epoch_before,
        "epoch_after_add": epoch_after,
        "a_key_package": b64(&a_kp_wire),
        "b_key_package": b64(&b_kp_wire),
        "add_commit": b64(&add_commit_wire),
        "welcome": b64(&welcome_wire),
        "app_message": b64(&app_wire),
    });
    println!("{}", serde_json::to_string(&out).unwrap());
}
