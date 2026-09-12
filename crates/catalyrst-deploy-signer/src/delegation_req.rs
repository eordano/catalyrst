fn is_valid_parcel(parcel: &str) -> bool {
    fn is_coord(s: &str) -> bool {
        let digits = s.strip_prefix('-').unwrap_or(s);
        !digits.is_empty() && digits.len() <= 10 && digits.bytes().all(|b| b.is_ascii_digit())
    }
    parcel
        .split_once(',')
        .is_some_and(|(x, y)| is_coord(x) && is_coord(y))
}

fn is_scene_id(scene_id: &str) -> bool {
    !scene_id.is_empty()
        && scene_id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
}

fn is_world_name(world: &str) -> bool {
    let mut bytes = world.bytes();
    bytes
        .next()
        .is_some_and(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
        && bytes.all(|b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'.' | b'_' | b'-')
        })
}

pub(crate) fn validate_delegation_req(
    world: &str,
    scene_id: &str,
    parcel: &str,
) -> Result<(String, String, String), &'static str> {
    let world = world.to_lowercase();
    if world.is_empty() {
        return Err("world must not be empty");
    }
    if !is_world_name(&world) {
        return Err("world must match [a-z0-9][a-z0-9._-]*");
    }
    if scene_id.is_empty() {
        return Err("sceneId must not be empty");
    }
    if !is_scene_id(scene_id) {
        return Err("sceneId must match [A-Za-z0-9-]+");
    }
    if !is_valid_parcel(parcel) {
        return Err("parcel must be two comma-separated integer coordinates");
    }
    Ok((world, scene_id.to_string(), parcel.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_validation_rejects_bad_input_and_lowercases_the_world() {
        assert_eq!(
            validate_delegation_req("MyWorld.DCL.eth", "bafkreigcene", "10,-25").unwrap(),
            (
                "myworld.dcl.eth".to_string(),
                "bafkreigcene".to_string(),
                "10,-25".to_string()
            )
        );
        assert!(validate_delegation_req("", "scene", "0,0").is_err());
        assert!(validate_delegation_req("   ", "scene", "0,0").is_err());
        assert!(validate_delegation_req("w.dcl.eth", "", "0,0").is_err());
        for bad in ["10", "10,", ",25", "a,b", "10.5,2", "10, 25", "1,2/../x"] {
            assert!(
                validate_delegation_req("w.dcl.eth", "scene", bad).is_err(),
                "parcel {bad:?} must be rejected"
            );
        }
    }

    #[test]
    fn scene_id_accepts_the_identifier_charset() {
        for ok in ["bafkreigcene", "bafkrei-scene", "ABC-123", "0", "-"] {
            assert_eq!(
                validate_delegation_req("w.dcl.eth", ok, "0,0").unwrap().1,
                ok,
                "sceneId {ok:?} must be accepted"
            );
        }
    }

    #[test]
    fn scene_id_with_an_injected_claim_line_is_rejected() {
        for bad in [
            "bafkrei\nParcel: 99,99",
            "bafkrei\r\nWorld: evil.dcl.eth",
            "bafkrei\rExpiration: 2999-01-01T00:00:00Z",
            "bafkrei\n",
            "\nbafkrei",
        ] {
            assert_eq!(
                validate_delegation_req("w.dcl.eth", bad, "0,0"),
                Err("sceneId must match [A-Za-z0-9-]+"),
                "sceneId {bad:?} must be rejected"
            );
        }
    }

    #[test]
    fn scene_id_with_control_or_non_identifier_chars_is_rejected() {
        for bad in [
            "bafkrei scene",
            " bafkrei",
            "bafkrei\tscene",
            "bafkrei\0",
            "bafkrei\u{1b}[0m",
            "bafkrei\u{85}",
            "bafkrei\u{2028}",
            "bafkrei:evil",
            "bafkrei_scene",
            "bafkrei.scene",
            "bafkrei/../x",
            "b\u{e9}",
        ] {
            assert!(
                validate_delegation_req("w.dcl.eth", bad, "0,0").is_err(),
                "sceneId {bad:?} must be rejected"
            );
        }
    }

    #[test]
    fn world_with_a_newline_or_control_char_is_rejected() {
        for bad in [
            "boedo.dcl.eth\nSceneId: evil",
            "boedo.dcl.eth\r\nParcel: 99,99",
            "boedo.dcl.eth\n",
            "\nboedo.dcl.eth",
            "boedo\tdcl.eth",
            "boedo.dcl.eth\0",
            "boedo.dcl.eth\u{1b}",
            "foo bar.dcl.eth",
            " boedo.dcl.eth",
            "boedo:eth",
            ".dcl.eth",
            "-boedo.dcl.eth",
            "b\u{e9}.dcl.eth",
        ] {
            assert!(
                validate_delegation_req(bad, "bafkrei-scene", "0,0").is_err(),
                "world {bad:?} must be rejected"
            );
        }
        for ok in ["boedo.dcl.eth", "Boedo.DCL.eth", "0xab-cd_1.eth", "genesis"] {
            assert_eq!(
                validate_delegation_req(ok, "bafkrei-scene", "0,0")
                    .unwrap()
                    .0,
                ok.to_lowercase(),
                "world {ok:?} must be accepted"
            );
        }
    }
}
