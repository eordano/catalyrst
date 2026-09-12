//! Decentraland RPC, vendored from `dcl-rpc` 2.3.6 (decentraland/rpc-rust).
//!
//! Vendored rather than depended on because the published crate pins prost
//! 0.11 and tokio-tungstenite 0.24, which forced this workspace to carry a
//! second copy of prost/prost-build/prost-types and of tungstenite alongside
//! the 0.14 / 0.30 everything else uses. Changes against upstream 2.3.6:
//!
//! - prost 0.11 -> 0.14, tokio-tungstenite 0.24 -> 0.30 (one version each,
//!   workspace-wide).
//! - `log` -> `tracing`. Nothing in this workspace installs a log-to-tracing
//!   bridge, so upstream's diagnostics were being written to a facade with no
//!   logger behind it and silently dropped.
//! - Dropped the warp transport, the `tungstenite-native-tls` feature, and the
//!   unused `bytes` / `futures-channel` dependencies. `futures-util` is now
//!   optional behind `tungstenite`, the only module that uses it.
//! - The service generator no longer `println!`s every generated service into
//!   the consumer's build output.
//!
//! Protocol shape: a [`RpcClient`](crate::client::RpcClient) connects to a
//! [`RpcServer`](crate::server::RpcServer) over any
//! [`Transport`](crate::transports::Transport), framing protobuf messages. A client creates
//! ports, each mirrored by a [`RpcServerPort`](crate::server::RpcServerPort); a port owns the
//! modules (a `service` in the `.proto`) registered for it, and every procedure of a server
//! shares one `Arc<Context>`. Closing a port leaves both the client and the server alive.
//!
//! Examples: <https://github.com/decentraland/rpc-rust/tree/main/examples>.

#[cfg(feature = "client")]
pub mod client;
#[cfg(feature = "codegen")]
pub mod codegen;
pub mod messages_handlers;
pub mod rpc_protocol;
#[cfg(feature = "server")]
pub mod server;
#[cfg(feature = "server")]
pub mod service_module_definition;
pub mod stream_protocol;
pub mod transports;

#[derive(Debug)]
pub enum CommonError {
    ProtocolError,
    TransportError,
    TransportNotAttached,
    UnexpectedError(String),
    TransportWasClosed,
}
