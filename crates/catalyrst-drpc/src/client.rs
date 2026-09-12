use crate::{
    messages_handlers::ClientMessagesHandler,
    rpc_protocol::{
        parse::{build_message_identifier, parse_header, parse_protocol_message, ParseErrors},
        CreatePort, CreatePortResponse, RemoteError, Request, RequestModule, RequestModuleResponse,
        Response, RpcMessageTypes, StreamMessage,
    },
    stream_protocol::{Generator, StreamProtocol},
    transports::Transport,
};
use prost::Message;
use std::{collections::HashMap, sync::Arc};
use tokio::sync::{oneshot, Mutex};
use tracing::debug;

/// Implemented by the codegen'd client type passed as the [`RpcClientPort::load_module`]
/// generic.
pub trait ServiceClient<T: Transport> {
    fn set_client_module(client_module: RpcClientModule<T>) -> Self;
}

#[derive(Debug)]
pub enum ClientError {
    ProtocolError,
    TransportError,
    /// The procedure id is missing from the client state the server filled in.
    ProcedureNotExists,
    UnexpectedError(String),
    PortNameAlreadyExists,
}

#[derive(Debug)]
pub enum ClientResultError {
    Client(ClientError),
    Remote(RemoteError),
}

pub type ClientResult<T> = Result<T, ClientResultError>;

pub struct RpcClient<T: Transport + 'static> {
    ports: HashMap<String, RpcClientPort<T>>,
    client_request_dispatcher: Arc<ClientRequestDispatcher<T>>,
}

impl<T: Transport + 'static> RpcClient<T> {
    /// Spawns a background task that processes every response from the
    /// [`RpcServer`](crate::server::RpcServer).
    pub async fn new(transport: T) -> ClientResult<Self> {
        let transport = Self::establish_connection(transport).await?;

        let client_request_dispatcher = Arc::new(ClientRequestDispatcher::new(transport));
        client_request_dispatcher.start();

        Ok(Self::from_dispatcher(client_request_dispatcher))
    }

    fn from_dispatcher(client_request_dispatcher: Arc<ClientRequestDispatcher<T>>) -> Self {
        Self {
            ports: HashMap::new(),
            client_request_dispatcher,
        }
    }

    /// Blocks until the server's
    /// [`ServerReady`](`crate::rpc_protocol::RpcMessageTypes::ServerReady`) message arrives.
    async fn establish_connection(transport: T) -> ClientResult<T> {
        match transport.receive().await {
            Ok(data) => {
                let (message_type, _) = parse_header(&data)
                    .ok_or(ClientResultError::Client(ClientError::TransportError))?;
                if matches!(message_type, RpcMessageTypes::ServerReady) {
                    Ok(transport)
                } else {
                    Err(ClientResultError::Client(ClientError::TransportError))
                }
            }
            _ => Err(ClientResultError::Client(ClientError::TransportError)),
        }
    }

    pub async fn create_port(&mut self, port_name: &str) -> ClientResult<&RpcClientPort<T>> {
        if self.ports.contains_key(port_name) {
            return Err(ClientResultError::Client(
                ClientError::PortNameAlreadyExists,
            ));
        }

        let (_message_type, _message_number, create_port_response) = self
            .client_request_dispatcher
            .request::<CreatePortResponse, _, _>(|message_id| CreatePort {
                message_identifier: build_message_identifier(
                    RpcMessageTypes::CreatePort as u32,
                    message_id,
                ),
                port_name: port_name.to_string(),
            })
            .await?;

        let rpc_client_port = RpcClientPort::new(
            port_name,
            create_port_response.port_id,
            self.client_request_dispatcher.clone(),
        );

        let port = self
            .ports
            .entry(port_name.to_string())
            .or_insert(rpc_client_port);

        Ok(port)
    }
}

impl<T: Transport> Drop for RpcClient<T> {
    fn drop(&mut self) {
        self.client_request_dispatcher.stop();
    }
}

pub struct RpcClientPort<T: Transport + 'static> {
    pub port_name: String,
    /// Assigned by the [`RpcServer`](crate::server::RpcServer).
    port_id: u32,
    client_request_dispatcher: Arc<ClientRequestDispatcher<T>>,
}

impl<T: Transport> RpcClientPort<T> {
    fn new(name: &str, id: u32, dispatcher: Arc<ClientRequestDispatcher<T>>) -> Self {
        Self {
            port_name: name.to_string(),
            port_id: id,
            client_request_dispatcher: dispatcher,
        }
    }

    pub fn port_id(&self) -> u32 {
        self.port_id
    }

    pub async fn load_module<S: ServiceClient<T>>(&self, module_name: &str) -> ClientResult<S> {
        let (_message_type, _message_number, request_module_response): (
            u32,
            u32,
            RequestModuleResponse,
        ) = self
            .client_request_dispatcher
            .request(|message_id| RequestModule {
                port_id: self.port_id,
                message_identifier: build_message_identifier(
                    RpcMessageTypes::RequestModule as u32,
                    message_id,
                ),
                module_name: module_name.to_string(),
            })
            .await?;

        let mut procedures = HashMap::new();

        for procedure in request_module_response.procedures {
            let (procedure_name, procedure_id) = (procedure.procedure_name, procedure.procedure_id);

            procedures.insert(procedure_name.to_string(), procedure_id);
        }

        let client_module = RpcClientModule::new(
            module_name,
            request_module_response.port_id,
            procedures,
            self.client_request_dispatcher.clone(),
        );

        let client_service = S::set_client_module(client_module);

        Ok(client_service)
    }
}

pub struct RpcClientModule<T: Transport + 'static> {
    pub module_name: String,
    port_id: u32,
    /// Procedure name -> procedure id assigned by the [`RpcServer`](crate::server::RpcServer).
    procedures: HashMap<String, u32>,
    client_request_dispatcher: Arc<ClientRequestDispatcher<T>>,
}

impl<T: Transport> RpcClientModule<T> {
    fn new(
        name: &str,
        port_id: u32,
        procedures: HashMap<String, u32>,
        dispatcher: Arc<ClientRequestDispatcher<T>>,
    ) -> Self {
        Self {
            module_name: name.to_string(),
            port_id,
            procedures,
            client_request_dispatcher: dispatcher,
        }
    }

    pub async fn call_unary_procedure<ReturnType: Message + Default, M: Message + Default>(
        &self,
        procedure_name: &str,
        payload: M,
    ) -> ClientResult<ReturnType> {
        let response: (u32, u32, Response) = self.call_procedure(procedure_name, payload).await?;

        let returned_type = ReturnType::decode(response.2.payload.as_slice())
            .map_err(|_| ClientResultError::Client(ClientError::ProtocolError))?;

        Ok(returned_type)
    }

    pub async fn call_server_streams_procedure<
        M: Message + Default + 'static,
        ReturnType: Message + Default + 'static,
    >(
        &self,
        procedure_name: &str,
        payload: M,
    ) -> ClientResult<Generator<ReturnType>> {
        let (_message_type, message_number, _procedure_response): (u32, u32, StreamMessage) =
            self.call_procedure(procedure_name, payload).await?;
        let stream_protocol = self
            .client_request_dispatcher
            .stream_server_messages(self.port_id, message_number)
            .await?;

        let generator =
            stream_protocol.to_generator(|item| match ReturnType::decode(item.as_slice()) {
                Ok(item_decoded) => Some(item_decoded),
                Err(_) => {
                    tracing::error!("> RpcClient > call_server_streams_procedure > StreamProtocol::to_generator > Error on decoding the ReturnType");
                    None
                }
            });

        Ok(generator)
    }

    pub async fn call_client_streams_procedure<
        ReturnType: Message + Default,
        M: Message + 'static,
    >(
        &self,
        procedure_name: &str,
        stream: Generator<M>,
    ) -> ClientResult<ReturnType> {
        let client_message_id = self.client_request_dispatcher.next_message_id().await;
        self.client_request_dispatcher
            .send_client_streams(self.port_id, client_message_id, stream)
            .await;

        let procedure_id = self.get_procedure_id(procedure_name)?;
        let (_message_type, _message_number, procedure_response): (u32, u32, Response) = self
            .client_request_dispatcher
            .request(|message_id| Request {
                port_id: self.port_id,
                message_identifier: build_message_identifier(
                    RpcMessageTypes::Request as u32,
                    message_id,
                ),
                procedure_id,
                payload: vec![],
                client_stream: client_message_id,
            })
            .await?;

        let returned_type = ReturnType::decode(procedure_response.payload.as_slice())
            .map_err(|_| ClientResultError::Client(ClientError::ProtocolError))?;

        Ok(returned_type)
    }

    pub async fn call_bidir_streams_procedure<
        M: Message + 'static,
        ReturnType: Message + Default + 'static,
    >(
        &self,
        procedure_name: &str,
        stream: Generator<M>,
    ) -> ClientResult<Generator<ReturnType>> {
        let client_message_id = self.client_request_dispatcher.next_message_id().await;
        self.client_request_dispatcher
            .send_client_streams(self.port_id, client_message_id, stream)
            .await;
        let procedure_id = self.get_procedure_id(procedure_name)?;

        let response: (u32, u32, Response) = self
            .client_request_dispatcher
            .request(|message_id| Request {
                port_id: self.port_id,
                message_identifier: build_message_identifier(
                    RpcMessageTypes::Request as u32,
                    message_id,
                ),
                procedure_id,
                payload: vec![],
                client_stream: client_message_id,
            })
            .await?;

        let stream_protocol = self
            .client_request_dispatcher
            .stream_server_messages(self.port_id, response.1)
            .await?;

        let generator =
            stream_protocol.to_generator(|item| ReturnType::decode(item.as_slice()).ok());

        Ok(generator)
    }

    async fn call_procedure<M: Message, ReturnType: Message + Default>(
        &self,
        procedure_name: &str,
        payload: M,
    ) -> ClientResult<(u32, u32, ReturnType)> {
        let procedure_id = self.get_procedure_id(procedure_name)?;
        let payload = payload.encode_to_vec();
        let response: (u32, u32, ReturnType) = self
            .client_request_dispatcher
            .request(|message_id| Request {
                port_id: self.port_id,
                message_identifier: build_message_identifier(
                    RpcMessageTypes::Request as u32,
                    message_id,
                ),
                procedure_id,
                payload,
                client_stream: 0,
            })
            .await?;

        Ok(response)
    }

    fn get_procedure_id(&self, procedure_name: &str) -> ClientResult<u32> {
        let procedure_id = self.procedures.get(procedure_name);
        if let Some(procedure_id) = procedure_id {
            return Ok(procedure_id.to_owned());
        }
        Err(ClientResultError::Client(ClientError::ProcedureNotExists))
    }
}

/// Shared by `RpcClient`, `RpcClientPort` and `RpcClientModule`, so every request goes out
/// over the one transport.
struct ClientRequestDispatcher<T: Transport + 'static> {
    next_message_id: Mutex<u32>,
    messages_handler: Arc<ClientMessagesHandler<T>>,
}

impl<T: Transport + 'static> ClientRequestDispatcher<T> {
    pub fn new(transport: T) -> Self {
        Self {
            next_message_id: Mutex::new(1),
            messages_handler: Arc::new(ClientMessagesHandler::new(Arc::new(transport))),
        }
    }

    fn start(&self) {
        self.messages_handler.clone().start();
    }

    fn stop(&self) {
        self.messages_handler.stop()
    }

    async fn next_message_id(&self) -> u32 {
        let mut lock = self.next_message_id.lock().await;
        let message_id = *lock;
        *lock += 1;
        message_id
    }

    /// Registers a one-shot listener on the messages handler before the request goes out.
    async fn request<
        ReturnType: Message + Default,
        M: Message + Default,
        Callback: FnOnce(u32) -> M,
    >(
        &self,
        cb: Callback,
    ) -> ClientResult<(u32, u32, ReturnType)> {
        let (payload, current_request_message_id) = {
            let message_id = self.next_message_id().await;
            debug!("Message ID: {}", message_id);
            let payload = cb(message_id);
            (payload, message_id)
        };

        let payload = payload.encode_to_vec();
        self.messages_handler
            .transport
            .send(payload)
            .await
            .map_err(|_| ClientResultError::Client(ClientError::TransportError))?;

        let (tx, rx) = oneshot::channel::<Vec<u8>>();

        self.messages_handler
            .register_one_time_listener(current_request_message_id, tx)
            .await;

        let response = rx
            .await
            .map_err(|_| ClientResultError::Client(ClientError::TransportError))?;

        match parse_protocol_message::<ReturnType>(&response) {
            Ok(result) => Ok(result),
            Err(ParseErrors::IsARemoteError((_, remote_error))) => {
                Err(ClientResultError::Remote(remote_error))
            }
            _ => Err(ClientResultError::Client(ClientError::ProtocolError)),
        }
    }

    /// Registers a listener for every stream message carrying `message_id` and drains it from
    /// a background task.
    async fn stream_server_messages(
        &self,
        port_id: u32,
        message_number: u32,
    ) -> ClientResult<StreamProtocol<T>> {
        let stream_protocol = StreamProtocol::new(
            self.messages_handler.transport.clone(),
            port_id,
            message_number,
        );

        let client_messages_handler_listener_removal = self.messages_handler.clone();
        match stream_protocol
            .start_processing(move || async move {
                client_messages_handler_listener_removal
                    .unregister_listener(message_number)
                    .await;
            })
            .await
        {
            Ok(listener) => {
                self.messages_handler
                    .register_listener(message_number, listener)
                    .await;
                Ok(stream_protocol)
            }
            Err(_) => Err(ClientResultError::Client(ClientError::TransportError)),
        }
    }

    /// Nothing is sent until the server acknowledges the stream opening; the wait and the
    /// sending both run in a background task.
    async fn send_client_streams<M: Message + 'static>(
        &self,
        port_id: u32,
        client_message_id: u32,
        client_stream: Generator<M>,
    ) {
        let (open_resolver, open_promise) = oneshot::channel::<Vec<u8>>();
        self.messages_handler
            .register_one_time_listener(client_message_id, open_resolver)
            .await;

        self.messages_handler
            .clone()
            .await_server_ack_open_and_send_streams(
                open_promise,
                client_stream,
                port_id,
                client_message_id,
            );
    }
}
