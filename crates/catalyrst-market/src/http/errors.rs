//! Direct port of `marketplace-server/src/logic/http/errors.ts` and `logic/errors.ts`.

use thiserror::Error;

#[derive(Debug, Error)]
#[error("The value of the {parameter} parameter is invalid: {value}")]
pub struct InvalidParameterError {
    pub parameter: String,
    pub value: String,
}

impl InvalidParameterError {
    pub fn new(parameter: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            parameter: parameter.into(),
            value: value.into(),
        }
    }
}

/// `HttpError` from `logic/http/response.ts`. A handler can throw this
/// (Rust: `return Err(...)`) to short-circuit with a custom status code.
#[derive(Debug, Error)]
#[error("{message}")]
pub struct HttpError {
    pub code: u16,
    pub message: String,
}

impl HttpError {
    pub fn new(code: u16, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}
