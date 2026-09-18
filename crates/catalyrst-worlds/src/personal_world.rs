use anyhow::{anyhow, Result};
use catalyrst_envcfg::get_u64;

const DCL_ETH_SUFFIX: &str = ".dcl.eth";

pub const DEFAULT_PERSONAL_WORLD_MAX_SIZE_BYTES: i64 = 50 * 1024 * 1024;

fn is_eth_address(label: &str) -> bool {
    label.len() == 42
        && label.starts_with("0x")
        && label[2..].bytes().all(|b| b.is_ascii_hexdigit())
}

/// The world every signed-in account can publish to on this realm without owning a
/// NAME: `<lowercase address>.dcl.eth`. NAME labels are 2-15 alphanumerics, so a
/// 42-character label can never be minted on chain and the owner is the address
/// itself. Accepts the bare label or the full `.dcl.eth` name; `None` for anything else.
pub fn personal_world_owner(world_name: &str) -> Option<String> {
    let lowered = world_name.trim().to_ascii_lowercase();
    let label = lowered.strip_suffix(DCL_ETH_SUFFIX).unwrap_or(&lowered);
    is_eth_address(label).then(|| label.to_string())
}

pub fn personal_world_name(address: &str) -> Option<String> {
    let lowered = address.trim().to_ascii_lowercase();
    is_eth_address(&lowered).then(|| format!("{lowered}{DCL_ETH_SUFFIX}"))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PersonalWorldsPolicy {
    pub max_worlds: u64,
    pub max_size_bytes: i64,
}

#[derive(Debug, PartialEq, Eq)]
pub enum PersonalWorldDeny {
    Disabled,
    RealmFull { max_worlds: u64 },
}

impl PersonalWorldsPolicy {
    pub fn new(max_worlds: u64, max_size_bytes: u64, world_size_cap: i64) -> Result<Self> {
        if max_size_bytes == 0 || max_size_bytes > world_size_cap as u64 {
            return Err(anyhow!(
                "WORLDS_PERSONAL_WORLD_MAX_SIZE_BYTES must be between 1 and {world_size_cap}, got {max_size_bytes}"
            ));
        }
        Ok(Self {
            max_worlds,
            max_size_bytes: max_size_bytes as i64,
        })
    }

    pub fn from_env(world_size_cap: i64) -> Result<Self> {
        Self::new(
            get_u64("WORLDS_PERSONAL_WORLDS_MAX", 0)?,
            get_u64(
                "WORLDS_PERSONAL_WORLD_MAX_SIZE_BYTES",
                DEFAULT_PERSONAL_WORLD_MAX_SIZE_BYTES as u64,
            )?,
            world_size_cap,
        )
    }

    pub fn enabled(&self) -> bool {
        self.max_worlds > 0
    }

    pub fn owner_of(&self, world_name: &str) -> Option<String> {
        if self.enabled() {
            personal_world_owner(world_name)
        } else {
            None
        }
    }

    pub fn size_cap_for(&self, world_name: &str, world_size_cap: i64) -> i64 {
        if self.owner_of(world_name).is_some() {
            self.max_size_bytes
        } else {
            world_size_cap
        }
    }

    pub fn admit(
        &self,
        world_name: &str,
        deployed_worlds: &[String],
    ) -> Result<(), PersonalWorldDeny> {
        if !self.enabled() {
            return Err(PersonalWorldDeny::Disabled);
        }
        let lowered = world_name.trim().to_ascii_lowercase();
        if deployed_worlds.contains(&lowered) {
            return Ok(());
        }
        let hosted = deployed_worlds
            .iter()
            .filter(|n| personal_world_owner(n).is_some())
            .count() as u64;
        if hosted >= self.max_worlds {
            return Err(PersonalWorldDeny::RealmFull {
                max_worlds: self.max_worlds,
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ADDR: &str = "0x4d02aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa7bf2";
    const OTHER: &str = "0x1111111111111111111111111111111111111111";
    const CAP: i64 = 300 * 1024 * 1024;

    fn policy(max_worlds: u64) -> PersonalWorldsPolicy {
        PersonalWorldsPolicy::new(max_worlds, 1024, CAP).unwrap()
    }

    #[test]
    fn the_personal_world_is_owned_by_its_address() {
        assert_eq!(
            personal_world_owner(&format!("{ADDR}.dcl.eth")).as_deref(),
            Some(ADDR)
        );
        assert_eq!(personal_world_owner(ADDR).as_deref(), Some(ADDR));
        assert_eq!(
            personal_world_owner(&format!("{}.DCL.ETH", ADDR.to_ascii_uppercase())).as_deref(),
            Some(ADDR)
        );
    }

    #[test]
    fn a_name_label_is_never_a_personal_world() {
        for name in [
            "boedo.dcl.eth",
            "0x4d02.dcl.eth",
            "0x4d02aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa7bf.dcl.eth",
            "0x4d02aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa7bf2z.dcl.eth",
            "0xzz02aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa7bf2.dcl.eth",
            "",
        ] {
            assert_eq!(personal_world_owner(name), None, "{name:?}");
        }
    }

    #[test]
    fn the_name_round_trips_through_the_owner() {
        let name = personal_world_name(&ADDR.to_ascii_uppercase()).unwrap();
        assert_eq!(name, format!("{ADDR}.dcl.eth"));
        assert_eq!(personal_world_owner(&name).as_deref(), Some(ADDR));
        assert_eq!(personal_world_name("boedo"), None);
    }

    #[test]
    fn the_policy_is_off_unless_the_operator_sets_a_cap() {
        let off = policy(0);
        assert!(!off.enabled());
        assert_eq!(off.owner_of(&format!("{ADDR}.dcl.eth")), None);
        assert_eq!(off.size_cap_for(&format!("{ADDR}.dcl.eth"), CAP), CAP);
        assert_eq!(
            off.admit(&format!("{ADDR}.dcl.eth"), &[]),
            Err(PersonalWorldDeny::Disabled)
        );
        let on = policy(1);
        assert!(on.enabled());
        assert_eq!(
            on.owner_of(&format!("{ADDR}.dcl.eth")).as_deref(),
            Some(ADDR)
        );
        assert_eq!(on.owner_of("boedo.dcl.eth"), None);
    }

    #[test]
    fn the_personal_namespace_gets_the_smaller_size_cap() {
        let on = policy(1);
        assert_eq!(on.size_cap_for(&format!("{ADDR}.dcl.eth"), CAP), 1024);
        assert_eq!(on.size_cap_for("boedo.dcl.eth", CAP), CAP);
        assert!(PersonalWorldsPolicy::new(1, 0, CAP).is_err());
        assert!(PersonalWorldsPolicy::new(1, CAP as u64 + 1, CAP).is_err());
        assert!(PersonalWorldsPolicy::new(1, CAP as u64, CAP).is_ok());
    }

    #[test]
    fn the_realm_cap_counts_only_new_personal_worlds() {
        let on = policy(1);
        let mine = format!("{ADDR}.dcl.eth");
        let theirs = format!("{OTHER}.dcl.eth");
        assert_eq!(on.admit(&mine, &["boedo.dcl.eth".to_string()]), Ok(()));
        assert_eq!(
            on.admit(&mine, std::slice::from_ref(&theirs)),
            Err(PersonalWorldDeny::RealmFull { max_worlds: 1 })
        );
        assert_eq!(on.admit(&mine, &[theirs.clone(), mine.clone()]), Ok(()));
        assert_eq!(
            on.admit(&mine.to_ascii_uppercase(), std::slice::from_ref(&mine)),
            Ok(())
        );
        assert_eq!(policy(2).admit(&mine, &[theirs]), Ok(()));
    }
}
