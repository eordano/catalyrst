use crate::{
    messages_handlers::ServerMessagesHandler,
    rpc_protocol::{
        fill_remote_error,
        parse::{build_message_identifier, parse_header},
        server_ready_message, CreatePort, CreatePortResponse, DestroyPort, ModuleProcedure,
        RemoteError, RemoteErrorResponse, Request, RequestModule, RequestModuleResponse,
        RpcMessageTypes,
    },
    service_module_definition::{ProcedureContext, ProcedureDefinition, ServiceModuleDefinition},
    stream_protocol::StreamProtocol,
    transports::{Transport, TransportError, TransportMessage},
};
use prost::{alloc::vec::Vec, Message};
use std::{collections::HashMap, sync::Arc};
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};
use tracing::{debug, error};

/// Handler that runs each time that a port is created
type PortHandlerFn<Context> = dyn Fn(&mut RpcServerPort<Context>) + Send + Sync + 'static;

type TransportHandler<Transport> = dyn Fn(Arc<Transport>, TransportID) + Send + Sync + 'static;

type OnTransportClosesHandler<Transport> = TransportHandler<Transport>;

type OnTransportConnected<Transport> = TransportHandler<Transport>;

/// Either an error safe to expose to the client, or an internal one.
#[derive(Debug)]
pub enum ServerResultError {
    External(ServerError),
    Internal(ServerInternalError),
}

pub type ServerResult<T> = Result<T, ServerResultError>;

/// Reaches the client, as a [`crate::rpc_protocol::RemoteError`].
#[derive(Debug)]
pub enum ServerError {
    ProtocolError,
    PortNotFound(u32),
    LoadModuleError,
    ModuleNotFound(String),
    ProcedureNotFound(u32),
    /// A [`Transport::send`] failed; the send is worth retrying.
    UnexpectedErrorOnTransport,
}

impl RemoteErrorResponse for ServerError {
    fn error_code(&self) -> u32 {
        match self {
            Self::ProtocolError => 1,
            Self::PortNotFound(_) => 2,
            Self::ModuleNotFound(_) => 3,
            Self::ProcedureNotFound(_) => 4,
            Self::UnexpectedErrorOnTransport => 5,
            Self::LoadModuleError => 0,
        }
    }

    fn error_message(&self) -> String {
        match self {
            Self::ProtocolError => "Error on parsing a message. The content seems to be corrupted and not to meet the protocol requirements".to_string(),
            Self::PortNotFound(id) => format!("The given Port's ID: {id} was not found"),
            Self::LoadModuleError => "Error on loading a module".to_string(),
            Self::ModuleNotFound(module_name) => format!("Module wasn't found on the server, check the name: {module_name}"),
            Self::ProcedureNotFound(id) => format!("Procedure's ID: {id} wasn't found on the server"),
            Self::UnexpectedErrorOnTransport => "Error on the transport while sending the original procedure response".to_string()
        }
    }
}

/// Never reaches the client.
#[derive(Debug)]
pub enum ServerInternalError {
    UnableToNofifyServer,
    TransportError,
    TransportNotAttached,
    InvalidHeader,
    TransportWasClosed,
}

type TransportID = u32;
type PortID = u32;

type TransportEvent<T, M> = (T, M);

enum ServerEvents<T: Transport + ?Sized> {
    AttachTransport(Arc<T>),
    NewTransport(TransportID, Arc<T>),
}

enum TransportNotification<T: Transport + ?Sized> {
    NewMessage(TransportEvent<(Arc<T>, TransportID), TransportMessage>),
    MustAttachTransport(Arc<T>),
    CloseTransport(TransportID),
}

/// Wraps a [`tokio::sync::mpsc::UnboundedSender`] so events can be sent to the server from another thread, e.g. a Websocket listener.
pub struct ServerEventsSender<T: Transport + ?Sized>(UnboundedSender<ServerEvents<T>>);

impl<T: Transport + ?Sized> ServerEventsSender<T> {
    /// `RpcServer::attach_transport` from another thread or background task, e.g. a listener
    /// accepting external connections.
    pub fn send_attach_transport(&self, transport: Arc<T>) -> ServerResult<()> {
        if self
            .0
            .send(ServerEvents::AttachTransport(transport))
            .is_err()
        {
            return Err(ServerResultError::Internal(
                ServerInternalError::UnableToNofifyServer,
            ));
        }
        Ok(())
    }

    fn send_new_transport(&self, id: TransportID, transport: Arc<T>) -> ServerResult<()> {
        if self
            .0
            .send(ServerEvents::NewTransport(id, transport))
            .is_err()
        {
            error!("> RpcServer > Error on notifying the new transport {id}");
            return Err(ServerResultError::Internal(
                ServerInternalError::TransportNotAttached,
            ));
        }
        Ok(())
    }
}

impl<T: Transport + ?Sized> Clone for ServerEventsSender<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

/// A new server needs a transport attached and a port-creation handler set before it can
/// serve anything.
pub struct RpcServer<Context, T: Transport + ?Sized> {
    transports: HashMap<TransportID, Arc<T>>,
    port_creation_handler: Option<Box<PortHandlerFn<Context>>>,
    on_transport_closes_handler: Option<Box<OnTransportClosesHandler<T>>>,
    on_transport_connected_handler: Option<Box<OnTransportConnected<T>>>,
    ports: HashMap<PortID, RpcServerPort<Context>>,
    ports_by_transport_id: HashMap<TransportID, Vec<PortID>>,
    context: Arc<Context>,
    messages_handler: Arc<ServerMessagesHandler>,
    server_events_sender: ServerEventsSender<T>,
    /// An `Option` so it can be taken and moved into the background task.
    server_events_receiver: Option<UnboundedReceiver<ServerEvents<T>>>,
    next_transport_id: u32,
    next_port_id: u32,
}
impl<Context: Send + Sync + 'static, T: Transport + ?Sized + 'static> RpcServer<Context, T> {
    pub fn create(ctx: Context) -> Self {
        let channel = unbounded_channel();
        Self {
            transports: HashMap::new(),
            port_creation_handler: None,
            on_transport_connected_handler: None,
            on_transport_closes_handler: None,
            ports: HashMap::new(),
            ports_by_transport_id: HashMap::new(),
            context: Arc::new(ctx),
            messages_handler: Arc::new(ServerMessagesHandler::new()),
            next_transport_id: 1,
            next_port_id: 1,
            server_events_sender: ServerEventsSender(channel.0),
            server_events_receiver: Some(channel.1),
        }
    }

    pub fn get_server_events_sender(&self) -> ServerEventsSender<T> {
        self.server_events_sender.clone()
    }

    /// Takes `&mut self`, so only the thread owning the [`RpcServer`] can call it; from
    /// anywhere else use [`ServerEventsSender::send_attach_transport`].
    pub async fn attach_transport(&mut self, transport: Arc<T>) -> ServerResult<()> {
        self.new_transport_attached(transport).await
    }

    async fn new_transport_attached(&mut self, transport: Arc<T>) -> ServerResult<()> {
        let current_id = self.next_transport_id;
        if let Err(error) = transport.send(server_ready_message().encode_to_vec()).await {
            error!("> RpcServer > new_transport_attached > Error while sending server ready message: {error:?}");
            if matches!(error, TransportError::Closed) {
                return Err(ServerResultError::Internal(
                    ServerInternalError::TransportError,
                ));
            } else {
                transport.close().await;
                return Err(ServerResultError::Internal(
                    ServerInternalError::TransportError,
                ));
            }
        }
        self.server_events_sender
            .send_new_transport(current_id, transport.clone())?;
        if let Some(handler) = &self.on_transport_connected_handler {
            handler(transport.clone(), current_id);
        }
        self.transports.insert(current_id, transport);
        self.next_transport_id += 1;
        Ok(())
    }

    pub async fn run(&mut self) {
        let (transports_notifier, mut transports_notification_receiver) =
            unbounded_channel::<TransportNotification<T>>();
        self.process_server_events(transports_notifier);
        loop {
            match transports_notification_receiver.recv().await {
                Some(notification) => match notification {
                    TransportNotification::NewMessage(((transport, transport_id), event)) => {
                        match parse_header(&event) {
                            Some((message_type, message_number)) => {
                                match self
                                    .handle_message(
                                        transport_id,
                                        event,
                                        message_type,
                                        message_number,
                                    )
                                    .await
                                {
                                    Ok(_) => debug!("> RpcServer > Transport message handled!"),
                                    Err(server_error) => match server_error {
                                        ServerResultError::External(server_external_error) => {
                                            error!("> RpcServer > Server External Error {server_external_error:?}");
                                            tokio::spawn(async move {
                                                let mut remote_error: RemoteError =
                                                    server_external_error.into();
                                                fill_remote_error(
                                                    &mut remote_error,
                                                    message_number,
                                                );
                                                if transport
                                                    .send(remote_error.encode_to_vec())
                                                    .await
                                                    .is_err()
                                                {
                                                    error!("> RpcServer > Error on sending the a RemoteError to the client {remote_error:?}")
                                                }
                                            });
                                        }
                                        ServerResultError::Internal(server_internal_error) => {
                                            error!("> RpcServer > Server Internal Error: {server_internal_error:?}")
                                        }
                                    },
                                }
                            }
                            None => {
                                error!("> RpcServer > A Invalid Header was sent by the client, message ignored");
                                continue;
                            }
                        }
                    }
                    TransportNotification::MustAttachTransport(transport) => {
                        if let Err(error) = self.new_transport_attached(transport).await {
                            error!("> RpcServer > Error on attaching transport to the server in order to receive message from it: {error:?}");
                            continue;
                        }
                    }
                    TransportNotification::CloseTransport(id) => {
                        if let Some(transport) = self.transports.remove(&id) {
                            if let Some(on_close_handler) = &self.on_transport_closes_handler {
                                on_close_handler(transport, id);
                            }
                            if let Some(port_ids) = self.ports_by_transport_id.remove(&id) {
                                for id in port_ids {
                                    self.ports.remove(&id);
                                }
                            }
                        }
                    }
                },
                None => {
                    error!("> RpcServer > Transport notification receiver error");
                    break;
                }
            }
        }
    }

    /// Once every running transport has finished, this emits `ServerEvents::Terminated`,
    /// which closes both the transports notifier and the events channel.
    fn process_server_events(
        &mut self,
        transports_notifier: UnboundedSender<TransportNotification<T>>,
    ) {
        let mut events_receiver = if let Some(events_receiver) = self.server_events_receiver.take()
        {
            events_receiver
        } else {
            panic!("> RpcServer > process_server_events > misuse of process_server_events, seems to be called more than one time")
        };

        tokio::spawn(async move {
            while let Some(event) = events_receiver.recv().await {
                match event {
                    ServerEvents::NewTransport(id, transport) => {
                        let tx_cloned = transports_notifier.clone();
                        tokio::spawn(async move {
                            loop {
                                match transport.receive().await {
                                    Ok(event) => {
                                        if tx_cloned
                                            .send(TransportNotification::NewMessage((
                                                (transport.clone(), id),
                                                event,
                                            )))
                                            .is_err()
                                        {
                                            error!("> From a Transport > Error while sending new message from transport to server via notifier");
                                            break;
                                        }
                                    }
                                    Err(error) => {
                                        if matches!(error, TransportError::Closed) {
                                            error!(
                                                "> From a Transport > Transport is already closed. Breaking..."
                                            );
                                            if tx_cloned
                                                .send(TransportNotification::CloseTransport(id))
                                                .is_err()
                                            {
                                                error!("> From a Transport > Error while sending new message from transport to server via notifier");
                                                break;
                                            }
                                            break;
                                        }
                                        error!("> From a Transport > Error on receiving {error:?}");
                                    }
                                }
                            }
                        });
                    }
                    ServerEvents::AttachTransport(transport) => {
                        if transports_notifier
                            .send(TransportNotification::MustAttachTransport(transport))
                            .is_err()
                        {
                            error!("> From a Transport > Error while notifying the server to attach a new transport");
                            continue;
                        };
                    }
                }
            }
        });
    }

    /// The handler is expected to register the port's services; a port with none is useless.
    pub fn set_module_registrator_handler<H>(&mut self, handler: H)
    where
        H: Fn(&mut RpcServerPort<Context>) + Send + Sync + 'static,
    {
        self.port_creation_handler = Some(Box::new(handler));
    }

    pub fn set_on_transport_closes_handler<H>(&mut self, handler: H)
    where
        H: Fn(Arc<T>, TransportID) + Send + Sync + 'static,
    {
        self.on_transport_closes_handler = Some(Box::new(handler));
    }

    /// The handler receives the transport ID the server assigned.
    pub fn set_on_transport_connected_handler<H>(&mut self, handler: H)
    where
        H: Fn(Arc<T>, TransportID) + Send + Sync + 'static,
    {
        self.on_transport_connected_handler = Some(Box::new(handler));
    }

    async fn handle_request(
        &self,
        transport: Arc<T>,
        transport_id: TransportID,
        message_number: u32,
        payload: Vec<u8>,
    ) -> ServerResult<()> {
        let request = Request::decode(payload.as_slice())
            .map_err(|_| ServerResultError::External(ServerError::ProtocolError))?;

        match self.ports.get(&request.port_id) {
            Some(port) => {
                let transport_cloned = transport.clone();
                let procedure_handler = port.get_procedure(request.procedure_id)?;
                let procedure_ctx = ProcedureContext {
                    server_context: self.context.clone(),
                    transport_id,
                };

                match procedure_handler {
                    ProcedureDefinition::Unary(procedure_handler) => {
                        self.messages_handler.process_unary_request(
                            transport_cloned,
                            message_number,
                            procedure_handler(request.payload, procedure_ctx),
                        );
                    }
                    ProcedureDefinition::ServerStreams(procedure_handler) => self
                        .messages_handler
                        .clone()
                        .process_server_streams_request(
                            transport_cloned,
                            message_number,
                            request.port_id,
                            procedure_handler(request.payload, procedure_ctx),
                        ),
                    ProcedureDefinition::ClientStreams(procedure_handler) => {
                        let client_stream_id = request.client_stream;
                        let stream_protocol = StreamProtocol::new(
                            transport.clone(),
                            request.port_id,
                            request.client_stream,
                        );

                        let msg_handler = self.messages_handler.clone();
                        match stream_protocol
                            .start_processing(move || async move {
                                msg_handler.unregister_listener(client_stream_id).await
                            })
                            .await
                        {
                            Ok(listener) => {
                                self.messages_handler
                                    .clone()
                                    .process_client_streams_request(
                                        transport_cloned,
                                        message_number,
                                        client_stream_id,
                                        procedure_handler(
                                            stream_protocol.to_generator(Some),
                                            procedure_ctx,
                                        ),
                                        listener,
                                    );
                            }
                            Err(_) => {
                                return Err(ServerResultError::Internal(
                                    ServerInternalError::TransportError,
                                ))
                            }
                        }
                    }
                    ProcedureDefinition::BiStreams(procedure_handler) => {
                        let client_stream_id = request.client_stream;
                        let stream_protocol = StreamProtocol::new(
                            transport.clone(),
                            request.port_id,
                            request.client_stream,
                        );

                        let msg_handler = self.messages_handler.clone();
                        match stream_protocol
                            .start_processing(move || async move {
                                msg_handler.unregister_listener(client_stream_id).await
                            })
                            .await
                        {
                            Ok(listener) => {
                                self.messages_handler.clone().process_bidir_streams_request(
                                    transport_cloned,
                                    message_number,
                                    request.port_id,
                                    client_stream_id,
                                    listener,
                                    procedure_handler(
                                        stream_protocol.to_generator(Some),
                                        procedure_ctx,
                                    ),
                                );
                            }
                            Err(_) => {
                                return Err(ServerResultError::Internal(
                                    ServerInternalError::TransportError,
                                ))
                            }
                        }
                    }
                }

                Ok(())
            }
            _ => Err(ServerResultError::External(ServerError::PortNotFound(
                request.port_id,
            ))),
        }
    }

    async fn handle_request_module(
        &mut self,
        transport: Arc<T>,
        message_number: u32,
        payload: Vec<u8>,
    ) -> ServerResult<()> {
        let request_module = RequestModule::decode(payload.as_slice())
            .map_err(|_| ServerResultError::External(ServerError::ProtocolError))?;
        if let Some(port) = self.ports.get_mut(&request_module.port_id) {
            if let Ok(server_module_declaration) = port.load_module(request_module.module_name) {
                let mut procedures: Vec<ModuleProcedure> = Vec::default();
                for procedure in &server_module_declaration.procedures {
                    let module_procedure = ModuleProcedure {
                        procedure_name: procedure.procedure_name.clone(),
                        procedure_id: procedure.procedure_id,
                    };
                    procedures.push(module_procedure)
                }

                let response = RequestModuleResponse {
                    port_id: request_module.port_id,
                    message_identifier: build_message_identifier(
                        RpcMessageTypes::RequestModuleResponse as u32,
                        message_number,
                    ),
                    procedures,
                };
                let response = response.encode_to_vec();
                transport
                    .send(response)
                    .await
                    .map_err(|_| ServerResultError::Internal(ServerInternalError::TransportError))?
            } else {
                return Err(ServerResultError::External(ServerError::LoadModuleError));
            }
        } else {
            return Err(ServerResultError::External(ServerError::PortNotFound(
                request_module.port_id,
            )));
        }

        Ok(())
    }

    /// Calls the handler registered with
    /// [`RpcServer::set_module_registrator_handler`](#method.set_module_registrator_handler).
    async fn handle_create_port(
        &mut self,
        transport: Arc<T>,
        transport_id: TransportID,
        message_number: u32,
        payload: Vec<u8>,
    ) -> ServerResult<()> {
        let port_id = self.next_port_id;
        let create_port = CreatePort::decode(payload.as_slice())
            .map_err(|_| ServerResultError::External(ServerError::ProtocolError))?;
        let port_name = create_port.port_name;
        let mut port = RpcServerPort::new(port_name.clone());

        if let Some(handler) = &self.port_creation_handler {
            handler(&mut port);
        }

        let response = CreatePortResponse {
            message_identifier: build_message_identifier(
                RpcMessageTypes::CreatePortResponse as u32,
                message_number,
            ),
            port_id,
        };
        let response = response.encode_to_vec();

        transport
            .send(response)
            .await
            .map_err(|_| ServerResultError::Internal(ServerInternalError::TransportError))?;

        self.next_port_id += 1;
        self.ports.insert(port_id, port);
        self.ports_by_transport_id
            .entry(transport_id)
            .and_modify(|ports| ports.push(port_id))
            .or_insert_with(|| vec![port_id]);

        Ok(())
    }

    fn handle_destroy_port(&mut self, payload: Vec<u8>) -> ServerResult<()> {
        let destroy_port = DestroyPort::decode(payload.as_slice())
            .map_err(|_| ServerResultError::External(ServerError::ProtocolError))?;

        self.ports.remove(&destroy_port.port_id);
        Ok(())
    }

    async fn handle_message(
        &mut self,
        transport_id: TransportID,
        payload: Vec<u8>,
        message_type: RpcMessageTypes,
        message_number: u32,
    ) -> ServerResult<()> {
        let transport = self
            .transports
            .get(&transport_id)
            .ok_or(ServerResultError::Internal(
                ServerInternalError::TransportNotAttached,
            ))?
            .clone();
        match message_type {
            RpcMessageTypes::Request => {
                self.handle_request(transport, transport_id, message_number, payload)
                    .await?
            }
            RpcMessageTypes::RequestModule => {
                self.handle_request_module(transport, message_number, payload)
                    .await?
            }
            RpcMessageTypes::CreatePort => {
                self.handle_create_port(transport, transport_id, message_number, payload)
                    .await?
            }
            RpcMessageTypes::DestroyPort => self.handle_destroy_port(payload)?,
            RpcMessageTypes::StreamAck => self
                .messages_handler
                .streams_handler
                .clone()
                .message_acknowledged_by_peer(message_number, payload),
            RpcMessageTypes::StreamMessage => self
                .messages_handler
                .clone()
                .notify_new_client_stream(message_number, payload),
            _ => {
                debug!("Unknown message");
            }
        };

        Ok(())
    }
}

pub struct RpcServerPort<Context> {
    pub name: String,
    /// Registered but not necessarily loaded.
    registered_modules: HashMap<String, ServiceModuleDefinition<Context>>,
    /// A module lands here only once a client asks for it.
    loaded_modules: HashMap<String, ServerModuleDeclaration>,
    procedures: HashMap<u32, ProcedureDefinition<Context>>,
    next_procedure_id: u32,
}

impl<Context> RpcServerPort<Context> {
    fn new(name: String) -> Self {
        RpcServerPort {
            name,
            registered_modules: HashMap::new(),
            loaded_modules: HashMap::new(),
            procedures: HashMap::new(),
            next_procedure_id: 1,
        }
    }

    pub fn register_module(
        &mut self,
        module_name: String,
        service_definition: ServiceModuleDefinition<Context>,
    ) {
        self.registered_modules
            .insert(module_name, service_definition);
    }

    /// Returns the module if already loaded; otherwise loads it from
    /// `registered_modules`.
    fn load_module(&mut self, module_name: String) -> ServerResult<&ServerModuleDeclaration> {
        if self.loaded_modules.contains_key(&module_name) {
            Ok(self
                .loaded_modules
                .get(&module_name)
                .expect("Already checked."))
        } else {
            match self.registered_modules.get(&module_name) {
                None => Err(ServerResultError::External(ServerError::ModuleNotFound(
                    module_name,
                ))),
                Some(module_generator) => {
                    let mut server_module_declaration = ServerModuleDeclaration {
                        procedures: Vec::new(),
                    };

                    let definitions = module_generator.get_definitions();

                    for (procedure_name, procedure_definition) in definitions {
                        let current_id = self.next_procedure_id;
                        self.procedures
                            .insert(current_id, procedure_definition.clone());
                        server_module_declaration
                            .procedures
                            .push(ServerModuleProcedure {
                                procedure_name: procedure_name.clone(),
                                procedure_id: current_id,
                            });
                        self.next_procedure_id += 1;
                    }

                    self.loaded_modules
                        .insert(module_name.clone(), server_module_declaration);

                    let module_definition = self
                        .loaded_modules
                        .get(&module_name)
                        .ok_or(ServerResultError::External(ServerError::LoadModuleError))?;
                    Ok(module_definition)
                }
            }
        }
    }

    fn get_procedure(&self, procedure_id: u32) -> ServerResult<ProcedureDefinition<Context>> {
        match self.procedures.get(&procedure_id) {
            Some(procedure_definition) => Ok(procedure_definition.clone()),
            _ => Err(ServerResultError::External(ServerError::ProcedureNotFound(
                procedure_id,
            ))),
        }
    }
}

#[derive(Debug)]
pub struct ServerModuleProcedure {
    pub procedure_name: String,
    pub procedure_id: u32,
}

pub struct ServerModuleDeclaration {
    pub procedures: Vec<ServerModuleProcedure>,
}
