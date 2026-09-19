use super::*;
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Action {
    Role { address: String, role: String },
    Remove { address: String },
    Ban { address: String },
    Unban { address: String },
    Bans { offset: u32 },
    Invite { address: String },
    Leave,
}
impl Action {
    pub fn validate(&self) -> ApiResult<()> {
        let address = match self {
            Self::Role { address, role } => {
                if !matches!(role.as_str(), "member" | "moderator") {
                    return Err(bad("Choose member or moderator"));
                }
                Some(address)
            }
            Self::Remove { address }
            | Self::Ban { address }
            | Self::Unban { address }
            | Self::Invite { address } => Some(address),
            Self::Bans { offset } => {
                if *offset > 100_000 {
                    return Err(bad("Invalid page"));
                }
                None
            }
            Self::Leave => None,
        };
        if address.is_some_and(|a| {
            a.len() != 42 || !a.starts_with("0x") || !a[2..].bytes().all(|b| b.is_ascii_hexdigit())
        }) {
            return Err(bad("Invalid member address"));
        }
        Ok(())
    }
    pub fn request(&self, path: &mut String, wallet: &str) -> (&'static str, Option<String>) {
        match self {
            Self::Role { address, role } => {
                path.push_str(&format!("/members/{}", address.to_lowercase()));
                ("PATCH", Some(json!({"role":role}).to_string()))
            }
            Self::Remove { address } => {
                path.push_str(&format!("/members/{}", address.to_lowercase()));
                ("DELETE", None)
            }
            Self::Ban { address } | Self::Unban { address } => {
                path.push_str(&format!("/members/{}/bans", address.to_lowercase()));
                (
                    if matches!(self, Self::Ban { .. }) {
                        "POST"
                    } else {
                        "DELETE"
                    },
                    None,
                )
            }
            Self::Bans { .. } => {
                path.push_str("/bans");
                ("GET", None)
            }
            Self::Invite { address } => {
                path.push_str("/requests");
                (
                    "POST",
                    Some(
                        json!({"type":"invite","targetedAddress":address.to_lowercase()})
                            .to_string(),
                    ),
                )
            }
            Self::Leave => {
                path.push_str(&format!("/members/{wallet}"));
                ("DELETE", None)
            }
        }
    }
}
