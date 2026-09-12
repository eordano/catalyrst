//! Emitted by the `.proto` codegen -- consumers rarely name these types directly.
use crate::{rpc_protocol::RemoteError, stream_protocol::Generator};
use core::future::Future;
use std::{collections::HashMap, pin::Pin, sync::Arc};

pub struct ProcedureContext<Context> {
    pub server_context: Arc<Context>,
    /// Identifies the transport the call arrived on.
    pub transport_id: u32,
}

pub type Response<T> = Pin<Box<dyn Future<Output = Result<T, RemoteError>> + Send>>;

pub type CommonPayload = Vec<u8>;

pub type UnaryResponse = Response<Vec<u8>>;

pub type UnaryRequestHandler<Context> =
    dyn Fn(CommonPayload, ProcedureContext<Context>) -> UnaryResponse + Send + Sync;

pub type ServerStreamsResponse = Response<Generator<Vec<u8>>>;

pub type ServerStreamsRequestHandler<Context> =
    dyn Fn(CommonPayload, ProcedureContext<Context>) -> ServerStreamsResponse + Send + Sync;

pub type ClientStreamsPayload = Generator<Vec<u8>>;

pub type ClientStreamsResponse = Response<Vec<u8>>;

pub type ClientStreamsRequestHandler<Context> =
    dyn Fn(ClientStreamsPayload, ProcedureContext<Context>) -> ClientStreamsResponse + Send + Sync;

pub type BiStreamsPayload = Generator<Vec<u8>>;

pub type BiStreamsResponse = Response<Generator<Vec<u8>>>;

pub type BiStreamsRequestHandler<Context> =
    dyn Fn(BiStreamsPayload, ProcedureContext<Context>) -> BiStreamsResponse + Send + Sync;

pub enum ProcedureDefinition<Context> {
    Unary(Arc<UnaryRequestHandler<Context>>),
    /// The server sends every payload it has, then closes the stream.
    ServerStreams(Arc<ServerStreamsRequestHandler<Context>>),
    /// The client streams payloads up; the server answers once, at the end.
    ClientStreams(Arc<ClientStreamsRequestHandler<Context>>),
    /// A stream open on both sides.
    BiStreams(Arc<BiStreamsRequestHandler<Context>>),
}

impl<Context> Clone for ProcedureDefinition<Context> {
    fn clone(&self) -> Self {
        match self {
            Self::Unary(procedure) => Self::Unary(procedure.clone()),
            Self::ServerStreams(procedure) => Self::ServerStreams(procedure.clone()),
            Self::ClientStreams(procedure) => Self::ClientStreams(procedure.clone()),
            Self::BiStreams(procedure) => Self::BiStreams(procedure.clone()),
        }
    }
}

pub struct ServiceModuleDefinition<Context> {
    /// Keyed by procedure name.
    procedure_definitions: HashMap<String, ProcedureDefinition<Context>>,
}

impl<Context> ServiceModuleDefinition<Context> {
    pub fn new() -> Self {
        Self {
            procedure_definitions: HashMap::new(),
        }
    }

    pub fn add_unary<
        H: Fn(CommonPayload, ProcedureContext<Context>) -> UnaryResponse + Send + Sync + 'static,
    >(
        &mut self,
        name: &str,
        handler: H,
    ) {
        self.add_definition(name, ProcedureDefinition::Unary(Arc::new(handler)));
    }

    pub fn add_server_streams<
        H: Fn(CommonPayload, ProcedureContext<Context>) -> ServerStreamsResponse
            + Send
            + Sync
            + 'static,
    >(
        &mut self,
        name: &str,
        handler: H,
    ) {
        self.add_definition(name, ProcedureDefinition::ServerStreams(Arc::new(handler)));
    }

    pub fn add_client_streams<
        H: Fn(ClientStreamsPayload, ProcedureContext<Context>) -> ClientStreamsResponse
            + Send
            + Sync
            + 'static,
    >(
        &mut self,
        name: &str,
        handler: H,
    ) {
        self.add_definition(name, ProcedureDefinition::ClientStreams(Arc::new(handler)));
    }

    pub fn add_bidir_streams<
        H: Fn(BiStreamsPayload, ProcedureContext<Context>) -> BiStreamsResponse
            + Send
            + Sync
            + 'static,
    >(
        &mut self,
        name: &str,
        handler: H,
    ) {
        self.add_definition(name, ProcedureDefinition::BiStreams(Arc::new(handler)));
    }

    fn add_definition(&mut self, name: &str, definition: ProcedureDefinition<Context>) {
        self.procedure_definitions
            .insert(name.to_string(), definition);
    }

    pub fn get_definitions(&self) -> &HashMap<String, ProcedureDefinition<Context>> {
        &self.procedure_definitions
    }
}

impl<Context> Default for ServiceModuleDefinition<Context> {
    fn default() -> Self {
        Self::new()
    }
}
