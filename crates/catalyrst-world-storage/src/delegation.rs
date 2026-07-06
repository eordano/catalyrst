// Port of upstream src/utils/storage-delegation.ts (ef2939b): world-scoped
// authoritative storage delegations carried in the x-authoritative-scope header.

use chrono::{DateTime, Utc};

pub const AUTHORITATIVE_SCOPE_HEADER: &str = "x-authoritative-scope";

pub const STORAGE_DELEGATION_PREFIX: &str = "Decentraland Authoritative Storage Delegation";

pub const MAX_SCOPE_HEADER_LENGTH: usize = 4096;

const CLAIM_FIELDS: [&str; 5] = ["Ephemeral:", "World:", "SceneId:", "Parcel:", "Expiration:"];

#[derive(Debug, PartialEq)]
struct ParsedClaim {
    ephemeral: String,
    world: String,
    scene_id: String,
    parcel: String,
    expiration: DateTime<Utc>,
}

pub struct StorageDelegationTarget<'a> {
    pub signer: &'a str,
    pub world: &'a str,
    pub scene_id: &'a str,
    pub parcel: &'a str,
    pub trusted_signers: &'a [String],
}

fn parse_claim(payload: &str) -> Option<ParsedClaim> {
    let mut lines = payload.split('\n');
    if lines.next() != Some(STORAGE_DELEGATION_PREFIX) {
        return None;
    }

    // Require EXACTLY the known field lines, each present once — no unknown, extra,
    // or duplicate lines — so minter/verifier format drift fails closed.
    let mut values: [Option<&str>; CLAIM_FIELDS.len()] = [None; CLAIM_FIELDS.len()];
    for line in lines {
        let idx = CLAIM_FIELDS.iter().position(|p| line.starts_with(p))?;
        if values[idx].is_some() {
            return None;
        }
        values[idx] = Some(line[CLAIM_FIELDS[idx].len()..].trim());
    }

    let [ephemeral, world, scene_id, parcel, expiration_iso] =
        values.map(|v| v.filter(|s| !s.is_empty()));
    let expiration = DateTime::parse_from_rfc3339(expiration_iso?).ok()?;

    Some(ParsedClaim {
        ephemeral: ephemeral?.to_lowercase(),
        world: world?.to_lowercase(),
        scene_id: scene_id?.to_string(),
        parcel: parcel?.to_string(),
        expiration: expiration.with_timezone(&Utc),
    })
}

pub fn verify_storage_delegation(
    scope_header: &str,
    target: &StorageDelegationTarget<'_>,
) -> Result<(), &'static str> {
    verify_storage_delegation_at(scope_header, target, Utc::now())
}

fn verify_storage_delegation_at(
    scope_header: &str,
    target: &StorageDelegationTarget<'_>,
    now: DateTime<Utc>,
) -> Result<(), &'static str> {
    if target.trusted_signers.is_empty() {
        return Err("no trusted authoritative signers configured");
    }
    if scope_header.len() > MAX_SCOPE_HEADER_LENGTH {
        return Err("scope header too large");
    }

    let decoded = base64_decode(scope_header.trim()).ok_or("malformed scope header")?;
    let parsed: serde_json::Value =
        serde_json::from_slice(&decoded).map_err(|_| "malformed scope header")?;
    let obj = parsed.as_object().ok_or("malformed scope header")?;

    let (Some(payload), Some(signature)) = (
        obj.get("payload").and_then(|v| v.as_str()),
        obj.get("signature").and_then(|v| v.as_str()),
    ) else {
        return Err("scope missing payload or signature");
    };

    let claim = parse_claim(payload).ok_or("unparseable claim")?;

    if claim.ephemeral != target.signer.to_lowercase() {
        return Err("claim ephemeral does not match request signer");
    }
    if claim.world != target.world.to_lowercase() {
        return Err("claim world does not match target world");
    }
    if claim.scene_id != target.scene_id {
        return Err("claim sceneId does not match target scene");
    }
    if claim.parcel != target.parcel {
        return Err("claim parcel does not match target parcel");
    }
    if claim.expiration <= now {
        return Err("delegation expired");
    }

    // The claim must be personally signed (EOA; EIP-1654 contract wallets are
    // intentionally unsupported) by a trusted authoritative address.
    let recovered = catalyrst_crypto::recover::recover_address(payload.as_bytes(), signature)
        .map_err(|_| "claim not signed by a trusted authoritative address")?;
    if target
        .trusted_signers
        .iter()
        .any(|s| s.eq_ignore_ascii_case(&recovered))
    {
        return Ok(());
    }
    Err("claim not signed by a trusted authoritative address")
}

fn base64_val(b: u8) -> Option<u8> {
    match b {
        b'A'..=b'Z' => Some(b - b'A'),
        b'a'..=b'z' => Some(b - b'a' + 26),
        b'0'..=b'9' => Some(b - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}

fn base64_decode(input: &str) -> Option<Vec<u8>> {
    let data = input.as_bytes();
    let data = match data.iter().position(|&b| b == b'=') {
        Some(pos) if data[pos..].iter().all(|&b| b == b'=') && data.len() - pos <= 2 => {
            &data[..pos]
        }
        Some(_) => return None,
        None => data,
    };
    if data.len() % 4 == 1 {
        return None;
    }
    let mut out = Vec::with_capacity(data.len() * 3 / 4);
    for chunk in data.chunks(4) {
        let mut acc: u32 = 0;
        for &b in chunk {
            acc = (acc << 6) | base64_val(b)? as u32;
        }
        acc <<= 6 * (4 - chunk.len()) as u32;
        let bytes = acc.to_be_bytes();
        out.extend_from_slice(&bytes[1..chunk.len()]);
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use catalyrst_crypto::Wallet;
    use chrono::Duration;

    const AUTHORITATIVE_KEY: &str =
        "0x4c0883a69102937d6231471b5dbb6204fe5129617082792ae468d01a3f362318";
    const OTHER_KEY: &str = "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80";

    const EPHEMERAL: &str = "0x1111111111111111111111111111111111111111";
    const WORLD: &str = "myworld.dcl.eth";
    const SCENE_ID: &str = "bafkreigcene";
    const PARCEL: &str = "10,-25";

    fn now() -> DateTime<Utc> {
        Utc::now()
    }

    fn claim_payload(expiration: DateTime<Utc>) -> String {
        format!(
            "{STORAGE_DELEGATION_PREFIX}\nEphemeral: {EPHEMERAL}\nWorld: {WORLD}\nSceneId: {SCENE_ID}\nParcel: {PARCEL}\nExpiration: {}",
            expiration.to_rfc3339()
        )
    }

    fn scope_header(wallet: &Wallet, payload: &str) -> String {
        let signature = wallet.sign_message(payload.as_bytes()).unwrap();
        let json = serde_json::json!({ "payload": payload, "signature": signature });
        base64_encode(json.to_string().as_bytes())
    }

    fn base64_encode(data: &[u8]) -> String {
        const ALPHABET: &[u8; 64] =
            b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = String::new();
        for chunk in data.chunks(3) {
            let mut acc: u32 = 0;
            for (i, &b) in chunk.iter().enumerate() {
                acc |= (b as u32) << (16 - 8 * i);
            }
            for i in 0..4 {
                if i <= chunk.len() {
                    out.push(ALPHABET[((acc >> (18 - 6 * i)) & 0x3f) as usize] as char);
                } else {
                    out.push('=');
                }
            }
        }
        out
    }

    fn target<'a>(trusted: &'a [String]) -> StorageDelegationTarget<'a> {
        StorageDelegationTarget {
            signer: EPHEMERAL,
            world: WORLD,
            scene_id: SCENE_ID,
            parcel: PARCEL,
            trusted_signers: trusted,
        }
    }

    fn trusted(wallet: &Wallet) -> Vec<String> {
        vec![wallet.address()]
    }

    #[test]
    fn accepts_a_valid_delegation() {
        let wallet = Wallet::from_hex(AUTHORITATIVE_KEY).unwrap();
        let header = scope_header(&wallet, &claim_payload(now() + Duration::hours(1)));
        let t = trusted(&wallet);
        assert_eq!(
            verify_storage_delegation_at(&header, &target(&t), now()),
            Ok(())
        );
    }

    #[test]
    fn rejects_untrusted_signer() {
        let wallet = Wallet::from_hex(OTHER_KEY).unwrap();
        let authoritative = Wallet::from_hex(AUTHORITATIVE_KEY).unwrap();
        let header = scope_header(&wallet, &claim_payload(now() + Duration::hours(1)));
        let t = trusted(&authoritative);
        assert_eq!(
            verify_storage_delegation_at(&header, &target(&t), now()),
            Err("claim not signed by a trusted authoritative address")
        );
    }

    #[test]
    fn rejects_when_no_trusted_signers_configured() {
        let wallet = Wallet::from_hex(AUTHORITATIVE_KEY).unwrap();
        let header = scope_header(&wallet, &claim_payload(now() + Duration::hours(1)));
        assert_eq!(
            verify_storage_delegation_at(&header, &target(&[]), now()),
            Err("no trusted authoritative signers configured")
        );
    }

    #[test]
    fn rejects_wrong_ephemeral_world_scene_and_parcel() {
        let wallet = Wallet::from_hex(AUTHORITATIVE_KEY).unwrap();
        let header = scope_header(&wallet, &claim_payload(now() + Duration::hours(1)));
        let t = trusted(&wallet);

        let mut wrong = target(&t);
        wrong.signer = "0x2222222222222222222222222222222222222222";
        assert_eq!(
            verify_storage_delegation_at(&header, &wrong, now()),
            Err("claim ephemeral does not match request signer")
        );

        let mut wrong = target(&t);
        wrong.world = "other.dcl.eth";
        assert_eq!(
            verify_storage_delegation_at(&header, &wrong, now()),
            Err("claim world does not match target world")
        );

        let mut wrong = target(&t);
        wrong.scene_id = "bafkreiother";
        assert_eq!(
            verify_storage_delegation_at(&header, &wrong, now()),
            Err("claim sceneId does not match target scene")
        );

        let mut wrong = target(&t);
        wrong.parcel = "0,0";
        assert_eq!(
            verify_storage_delegation_at(&header, &wrong, now()),
            Err("claim parcel does not match target parcel")
        );
    }

    #[test]
    fn world_match_is_case_insensitive() {
        let wallet = Wallet::from_hex(AUTHORITATIVE_KEY).unwrap();
        let header = scope_header(&wallet, &claim_payload(now() + Duration::hours(1)));
        let t = trusted(&wallet);
        let mut upper = target(&t);
        upper.world = "MyWorld.dcl.eth";
        assert_eq!(verify_storage_delegation_at(&header, &upper, now()), Ok(()));
    }

    #[test]
    fn rejects_expired_delegation() {
        let wallet = Wallet::from_hex(AUTHORITATIVE_KEY).unwrap();
        let header = scope_header(&wallet, &claim_payload(now() - Duration::minutes(1)));
        let t = trusted(&wallet);
        assert_eq!(
            verify_storage_delegation_at(&header, &target(&t), now()),
            Err("delegation expired")
        );
    }

    #[test]
    fn rejects_oversized_header_before_decoding() {
        let wallet = Wallet::from_hex(AUTHORITATIVE_KEY).unwrap();
        let header = "A".repeat(MAX_SCOPE_HEADER_LENGTH + 1);
        let t = trusted(&wallet);
        assert_eq!(
            verify_storage_delegation_at(&header, &target(&t), now()),
            Err("scope header too large")
        );
    }

    #[test]
    fn rejects_null_and_primitive_json_payloads() {
        let wallet = Wallet::from_hex(AUTHORITATIVE_KEY).unwrap();
        let t = trusted(&wallet);
        for raw in ["null", "42", "\"str\"", "[1,2]"] {
            let header = base64_encode(raw.as_bytes());
            assert_eq!(
                verify_storage_delegation_at(&header, &target(&t), now()),
                Err("malformed scope header"),
                "payload {raw:?} must be rejected"
            );
        }
        let header = base64_encode(br#"{"payload": 1, "signature": "0xff"}"#);
        assert_eq!(
            verify_storage_delegation_at(&header, &target(&t), now()),
            Err("scope missing payload or signature")
        );
        assert_eq!(
            verify_storage_delegation_at("!!!not-base64!!!", &target(&t), now()),
            Err("malformed scope header")
        );
    }

    #[test]
    fn claim_parser_requires_exactly_the_known_fields() {
        let exp = (now() + Duration::hours(1)).to_rfc3339();
        let valid = claim_payload(now() + Duration::hours(1));
        assert!(parse_claim(&valid).is_some());

        assert!(parse_claim("wrong prefix\nEphemeral: 0x1").is_none());

        let missing = format!(
            "{STORAGE_DELEGATION_PREFIX}\nEphemeral: {EPHEMERAL}\nWorld: {WORLD}\nSceneId: {SCENE_ID}\nParcel: {PARCEL}"
        );
        assert!(parse_claim(&missing).is_none(), "missing Expiration");

        let duplicate = format!("{valid}\nWorld: {WORLD}");
        assert!(parse_claim(&duplicate).is_none(), "duplicate field");

        let unknown = format!("{valid}\nExtra: x");
        assert!(parse_claim(&unknown).is_none(), "unknown field");

        let bad_exp = format!(
            "{STORAGE_DELEGATION_PREFIX}\nEphemeral: {EPHEMERAL}\nWorld: {WORLD}\nSceneId: {SCENE_ID}\nParcel: {PARCEL}\nExpiration: tomorrow"
        );
        assert!(parse_claim(&bad_exp).is_none(), "non-RFC3339 expiration");

        let reordered = format!(
            "{STORAGE_DELEGATION_PREFIX}\nExpiration: {exp}\nParcel: {PARCEL}\nSceneId: {SCENE_ID}\nWorld: {WORLD}\nEphemeral: {EPHEMERAL}"
        );
        assert!(parse_claim(&reordered).is_some(), "field order is free");
    }

    #[test]
    fn base64_roundtrip_and_rejection() {
        for data in [
            &b""[..],
            b"a",
            b"ab",
            b"abc",
            b"abcd",
            b"{\"payload\":\"x\"}",
        ] {
            assert_eq!(
                base64_decode(&base64_encode(data)).as_deref(),
                Some(data),
                "roundtrip {data:?}"
            );
        }
        assert_eq!(
            base64_decode("eyJhIjoxfQ").as_deref(),
            Some(&b"{\"a\":1}"[..])
        );
        assert!(base64_decode("a").is_none());
        assert!(base64_decode("ab=c").is_none());
        assert!(base64_decode("a b c").is_none());
    }
}
