pub mod parse;
include!(concat!(env!("OUT_DIR"), "/_.rs"));

/// This trait should me implemented by the Error type returned in your server's procedures
///
/// # Example
///
/// ```
/// use catalyrst_drpc::rpc_protocol::{RemoteError, RemoteErrorResponse};
/// pub enum MyEnumOfErrors {
///     EntityNotFound,
///     DbError
/// }
///
/// impl RemoteErrorResponse for MyEnumOfErrors {
///     fn error_code(&self) -> u32 {
///         match self {
///             Self::EntityNotFound => 404,
///             Self::DbError => 500
///         }
///     }
///     
///     fn error_message(&self) -> String {
///         match self {
///             Self::EntityNotFound => "The entity wasn't found".to_string(),
///             Self::DbError => "Internal Server Error".to_string()
///         }
///     }
/// }
///
/// let error: RemoteError = MyEnumOfErrors::EntityNotFound.into();
/// assert_eq!(error.error_code, 404);
/// assert_eq!(error.error_message, "The entity wasn't found")
/// ```
///
///
pub trait RemoteErrorResponse {
    fn error_code(&self) -> u32;
    fn error_message(&self) -> String;
}

impl<T: RemoteErrorResponse> From<T> for RemoteError {
    fn from(value: T) -> Self {
        Self {
            message_identifier: 0,
            error_code: value.error_code(),
            error_message: value.error_message(),
        }
    }
}

/// The `From<T>` conversion above cannot know the `message_number`, so it leaves
/// `message_identifier` at `0` and this fills it in once the number is known.
pub(crate) fn fill_remote_error(remote_error: &mut RemoteError, message_number: u32) {
    remote_error.message_identifier = parse::build_message_identifier(
        RpcMessageTypes::RemoteErrorResponse as u32,
        message_number,
    );
}

pub(crate) fn server_ready_message() -> RpcMessageHeader {
    RpcMessageHeader {
        message_identifier: parse::build_message_identifier(RpcMessageTypes::ServerReady as u32, 0),
    }
}
