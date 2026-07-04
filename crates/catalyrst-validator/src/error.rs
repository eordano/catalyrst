use std::fmt;

#[derive(Debug, Clone)]
pub enum ValidationResponse {
    Ok,
    Failed { errors: Vec<String> },
}

impl ValidationResponse {
    pub const OK: ValidationResponse = ValidationResponse::Ok;

    pub fn is_ok(&self) -> bool {
        matches!(self, ValidationResponse::Ok)
    }

    pub fn failed(errors: impl IntoIterator<Item = String>) -> Self {
        let errors: Vec<String> = errors.into_iter().collect();
        debug_assert!(!errors.is_empty(), "failed() must have at least one error");
        ValidationResponse::Failed { errors }
    }

    pub fn fail(msg: impl Into<String>) -> Self {
        ValidationResponse::Failed {
            errors: vec![msg.into()],
        }
    }

    pub fn from_errors(errors: Vec<String>) -> Self {
        if errors.is_empty() {
            ValidationResponse::Ok
        } else {
            ValidationResponse::Failed { errors }
        }
    }

    pub fn errors(&self) -> Option<&[String]> {
        match self {
            ValidationResponse::Ok => None,
            ValidationResponse::Failed { errors } => Some(errors),
        }
    }
}

impl fmt::Display for ValidationResponse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ValidationResponse::Ok => write!(f, "ok"),
            ValidationResponse::Failed { errors } => {
                write!(f, "validation failed: {}", errors.join("; "))
            }
        }
    }
}

#[derive(Debug, Clone)]
pub enum ServerValidationResult {
    Ok,
    Failed { message: String },
}

impl ServerValidationResult {
    pub fn is_ok(&self) -> bool {
        matches!(self, ServerValidationResult::Ok)
    }

    pub fn fail(msg: impl Into<String>) -> Self {
        ServerValidationResult::Failed {
            message: msg.into(),
        }
    }
}

impl fmt::Display for ServerValidationResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ServerValidationResult::Ok => write!(f, "ok"),
            ServerValidationResult::Failed { message } => {
                write!(f, "server validation failed: {message}")
            }
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ValidatorError {
    #[error("failed to parse entity: {0}")]
    EntityParse(String),

    #[error("blockchain query failed: {0}")]
    BlockchainQuery(String),

    #[error("subgraph query failed: {0}")]
    SubgraphQuery(String),

    #[error("content storage error: {0}")]
    Storage(String),

    #[error("signature validation error: {0}")]
    Signature(String),

    #[error("internal error: {0}")]
    Internal(String),
}

#[derive(Debug, Clone)]
pub struct PermissionResult {
    pub result: bool,
    pub failing: Option<Vec<String>>,
}

impl PermissionResult {
    pub fn ok() -> Self {
        PermissionResult {
            result: true,
            failing: None,
        }
    }

    pub fn denied(failing: Vec<String>) -> Self {
        PermissionResult {
            result: false,
            failing: if failing.is_empty() {
                None
            } else {
                Some(failing)
            },
        }
    }

    pub fn denied_empty() -> Self {
        PermissionResult {
            result: false,
            failing: None,
        }
    }
}
