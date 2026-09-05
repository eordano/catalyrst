use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
#[derive(Default)]
pub enum AccessSetting {
    #[serde(rename = "unrestricted")]
    #[default]
    Unrestricted,
    #[serde(rename = "shared-secret")]
    SharedSecret { secret: String },
    #[serde(rename = "nft-ownership")]
    NftOwnership { nft: String },
    #[serde(rename = "allow-list")]
    AllowList {
        #[serde(default)]
        wallets: Vec<String>,
        #[serde(default)]
        communities: Vec<String>,
    },
}

impl AccessSetting {
    pub fn is_shared_secret(&self) -> bool {
        matches!(self, AccessSetting::SharedSecret { .. })
    }

    pub fn to_public_json(&self) -> serde_json::Value {
        match self {
            AccessSetting::Unrestricted => serde_json::json!({ "type": "unrestricted" }),
            AccessSetting::SharedSecret { .. } => {
                serde_json::json!({ "type": "shared-secret" })
            }
            AccessSetting::NftOwnership { nft } => {
                serde_json::json!({ "type": "nft-ownership", "nft": nft })
            }
            AccessSetting::AllowList {
                wallets,
                communities,
            } => serde_json::json!({
                "type": "allow-list",
                "wallets": wallets,
                "communities": communities,
            }),
        }
    }

    pub fn check_access(&self, address: &str, secret: Option<&str>) -> bool {
        match self {
            AccessSetting::Unrestricted => true,
            AccessSetting::SharedSecret { secret: hash } => match secret {
                Some(provided) => bcrypt::verify(provided, hash).unwrap_or(false),
                None => false,
            },
            AccessSetting::NftOwnership { .. } => false,
            AccessSetting::AllowList { wallets, .. } => {
                let addr = address.to_lowercase();
                wallets.iter().any(|w| w.to_lowercase() == addr)
            }
        }
    }
}
