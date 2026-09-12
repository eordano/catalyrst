use super::{fill_remote_error, RemoteError, RpcMessageHeader, RpcMessageTypes};
use prost::Message;

/// Bits 1..28 hold the sequential message number (analogous to JSON-RPC 2), bits 28..32 the
/// message type.
pub fn build_message_identifier(message_type: u32, message_number: u32) -> u32 {
    ((message_type & 0xf) << 27) | (message_number & 0x07ffffff)
}

/// Returns `(message_type, message_number)` -- the inverse of [`build_message_identifier`].
pub fn parse_message_identifier(value: u32) -> (u32, u32) {
    ((value >> 27) & 0xf, value & 0x07ffffff)
}

pub fn parse_header(data: &[u8]) -> Option<(RpcMessageTypes, u32)> {
    let message_header = RpcMessageHeader::decode(data).ok()?;
    let (message_type, message_number) =
        parse_message_identifier(message_header.message_identifier);
    let rpc_message_type = RpcMessageTypes::try_from(message_type as i32).ok()?;
    Some((rpc_message_type, message_number))
}

#[derive(Debug)]
pub enum ParseErrors {
    IsARemoteError((u32, RemoteError)),
    DecodingFailed,
    /// [`RpcMessageTypes::Empty`] and [`RpcMessageTypes::ServerReady`] carry no body: they
    /// arrive as a bare [`RpcMessageHeader`].
    NotMessageType,
    /// The bytes do not even decode into a [`RpcMessageHeader`].
    InvalidHeader,
}

type ParseMessageResult<R> = Result<(u32, u32, R), ParseErrors>;

/// Decodes `data` into `R`, returning `(message_type, message_number, R)`.
///
/// A [`RpcMessageTypes::RemoteErrorResponse`] on the wire comes back as
/// [`ParseErrors::IsARemoteError`], not as a decode failure.
pub fn parse_protocol_message<R: Message + Default>(data: &[u8]) -> ParseMessageResult<R> {
    let (message_type, message_number) = match parse_header(data) {
        Some(header) => header,
        None => return Err(ParseErrors::InvalidHeader),
    };

    if matches!(message_type, RpcMessageTypes::RemoteErrorResponse) {
        match RemoteError::decode(data) {
            Ok(remote_error) => {
                return Err(ParseErrors::IsARemoteError((message_number, remote_error)))
            }
            Err(_) => {
                let mut remote_error_default = RemoteError::default();
                fill_remote_error(&mut remote_error_default, message_number);

                return Err(ParseErrors::IsARemoteError((
                    message_number,
                    remote_error_default,
                )));
            }
        }
    }

    if matches!(
        message_type,
        RpcMessageTypes::Empty | RpcMessageTypes::ServerReady
    ) {
        return Err(ParseErrors::NotMessageType);
    }

    let message = R::decode(data);

    match message {
        Ok(message) => Ok((message_type as u32, message_number, message)),
        Err(_) => Err(ParseErrors::DecodingFailed),
    }
}

#[cfg(test)]
mod tests {
    use crate::rpc_protocol::*;
    use prost::Message;

    use super::{build_message_identifier, parse_protocol_message};

    #[test]
    fn test_parse_protocol_message() {
        let port = CreatePort {
            message_identifier: build_message_identifier(RpcMessageTypes::CreatePort as u32, 1),
            port_name: "port_name".to_string(),
        };

        let vec = port.encode_to_vec();

        let parse_back = parse_protocol_message::<CreatePortResponse>(&vec).unwrap();

        assert_eq!(parse_back.0, RpcMessageTypes::CreatePort as u32);
        assert_eq!(parse_back.2.port_id, 0);
    }

    #[test]
    fn test_remote_error_in_parse_protocol_message() {
        let remote_error = RemoteError {
            message_identifier: build_message_identifier(
                RpcMessageTypes::RemoteErrorResponse as u32,
                1,
            ),
            error_code: 400,
            error_message: "Bad request error".to_string(),
        };
        let vec = remote_error.encode_to_vec();

        let parse_back = parse_protocol_message::<CreatePortResponse>(&vec).unwrap_err();
        match parse_back {
            parse::ParseErrors::IsARemoteError((message_number, remote_error_produced)) => {
                assert_eq!(message_number, 1);
                assert_eq!(remote_error, remote_error_produced);
            }
            _ => panic!(),
        }
    }
}
