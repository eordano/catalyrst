use async_trait::async_trait;

use crate::error::AuthError;

#[async_trait]
pub trait Eip1654Validator: Send + Sync {
    async fn validate_signature(
        &self,
        contract_address: &str,
        hash: &[u8],
        signature: &[u8],
    ) -> Result<bool, AuthError>;
}
