use thiserror::Error;

#[derive(Debug, Error)]
pub enum FedError {
    #[error("invalid signature: {0}")]
    InvalidSignature(String),

    #[error("signer mismatch: expected {expected}, recovered {recovered}")]
    SignerMismatch { expected: String, recovered: String },

    #[error("nonce already seen for signer {signer}")]
    DuplicateNonce { signer: String },

    #[error("action timestamp out of window: signed_at={signed_at}, now={now}, skew={skew}s")]
    SkewedTimestamp { signed_at: i64, now: i64, skew: i64 },

    #[error("session delegation expired at {expires_at}, now={now}")]
    SessionExpired { expires_at: u64, now: u64 },

    #[error("session scope mismatch: required {required:?}, have {have:?}")]
    SessionScope { required: String, have: Vec<String> },

    #[error("peer {peer} not in FederationRegistry")]
    UnknownPeer { peer: String },

    #[error("rate limit exceeded for signer {signer}")]
    RateLimited { signer: String },

    #[error("malformed payload: {0}")]
    Malformed(String),

    #[error("transport error: {0}")]
    Transport(String),

    #[error("crypto: {0}")]
    Crypto(#[from] catalyrst_crypto::AuthError),
}
