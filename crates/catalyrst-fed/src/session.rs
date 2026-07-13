use crate::error::FedError;
use crate::sig::{Signed, TypedMessage};
use serde::{Deserialize, Serialize};

pub const MAX_SESSION_LIFETIME_SECS: u64 = 24 * 60 * 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Scope {
    Places,
    Events,
    Communities,
    Friends,
    Messaging,
}

impl Scope {
    pub fn as_str(self) -> &'static str {
        match self {
            Scope::Places => "places",
            Scope::Events => "events",
            Scope::Communities => "communities",
            Scope::Friends => "friends",
            Scope::Messaging => "messaging",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionDelegation {
    pub delegate_pubkey: [u8; 32],
    pub expires_at: u64,
    pub scope: Vec<Scope>,
    pub nonce: [u8; 16],
    pub signed_at: u64,
}

impl TypedMessage for SessionDelegation {
    const PRIMARY_TYPE: &'static str = "SessionDelegation";
    fn encode_struct(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(32 + 8 + 16 + 8 + self.scope.len());
        out.extend_from_slice(&self.delegate_pubkey);
        out.extend_from_slice(&self.expires_at.to_be_bytes());
        for s in &self.scope {
            out.push(*s as u8);
        }
        out.extend_from_slice(&self.nonce);
        out.extend_from_slice(&self.signed_at.to_be_bytes());
        out
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionRevocation {
    pub delegation_hash: [u8; 32],
    pub nonce: [u8; 16],
    pub signed_at: u64,
}

impl TypedMessage for SessionRevocation {
    const PRIMARY_TYPE: &'static str = "SessionRevocation";
    fn encode_struct(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(32 + 16 + 8);
        out.extend_from_slice(&self.delegation_hash);
        out.extend_from_slice(&self.nonce);
        out.extend_from_slice(&self.signed_at.to_be_bytes());
        out
    }
}

pub fn check_delegation(
    delegation: &Signed<SessionDelegation>,
    required_scope: Scope,
    now: u64,
) -> Result<(), FedError> {
    if delegation.message.expires_at <= now {
        return Err(FedError::SessionExpired {
            expires_at: delegation.message.expires_at,
            now,
        });
    }
    if delegation.message.expires_at
        > delegation
            .message
            .signed_at
            .saturating_add(MAX_SESSION_LIFETIME_SECS)
    {
        return Err(FedError::Malformed(
            "delegation lifetime exceeds 24h cap".into(),
        ));
    }
    if !delegation.message.scope.contains(&required_scope) {
        return Err(FedError::SessionScope {
            required: required_scope.as_str().to_string(),
            have: delegation
                .message
                .scope
                .iter()
                .map(|s| s.as_str().to_string())
                .collect(),
        });
    }
    Ok(())
}
