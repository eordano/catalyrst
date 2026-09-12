use std::{future::Future, sync::Arc};

use prost::Message;
use tracing::debug;

use tokio::{
    select,
    sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender},
};

use async_channel::{unbounded, Receiver as AsyncChannelReceiver, Sender as AsyncChannelSender};

use tokio_util::sync::CancellationToken;

use crate::{
    rpc_protocol::{parse::build_message_identifier, RpcMessageTypes, StreamMessage},
    transports::{Transport, TransportError},
    CommonError,
};

pub struct StreamProtocol<T: Transport + ?Sized> {
    port_id: u32,
    /// Number of the message that opened the stream.
    message_number: u32,
    last_received_sequence_id: u32,
    is_remote_closed: bool,
    /// Whether the remote half was still open as of the last message.
    was_open: bool,
    generator: (Generator<Vec<u8>>, GeneratorYielder<Vec<u8>>),
    transport: Arc<T>,
    process_cancellation_token: CancellationToken,
}

impl<T: Transport + ?Sized + 'static> StreamProtocol<T> {
    pub(crate) fn new(transport: Arc<T>, port_id: u32, message_number: u32) -> Self {
        Self {
            last_received_sequence_id: 0,
            is_remote_closed: false,
            was_open: false,
            generator: Generator::create(),
            transport,
            message_number,
            port_id,
            process_cancellation_token: CancellationToken::new(),
        }
    }

    async fn next(&mut self) -> Option<Vec<u8>> {
        select! {
            _ = self.process_cancellation_token.cancelled() =>  {
                self.generator.0.close();
                self.is_remote_closed = true;
                None
            }
            message = self.generator.0.next() => {
                match message {
                    Some(msg) => {
                        self.last_received_sequence_id += 1;
                        self.was_open = true;
                        let stream_message = StreamMessage {
                            port_id: self.port_id,
                            sequence_id: self.last_received_sequence_id,
                            message_identifier: build_message_identifier(
                                RpcMessageTypes::StreamAck as u32,
                                self.message_number,
                            ),
                            payload: vec![],
                            closed: false,
                            ack: true,
                        };

                        if let Err(err) = self.transport
                            .send(stream_message.encode_to_vec())
                            .await {
                            tracing::error!("> StreamProtocol > next > Error while sending the ACK StreamMessage through the transport: {err:?}");
                            self.is_remote_closed = true;
                            return None;
                        }

                        Some(msg)
                    }
                    None => {
                        self.is_remote_closed = true;
                        None
                    }
                }
            }
        }
    }

    /// The transform runs in a background task; an item it maps to `None` is dropped.
    pub fn to_generator<
        O: Send + Sync + 'static,
        F: Fn(Vec<u8>) -> Option<O> + Send + Sync + 'static,
    >(
        mut self,
        transformer: F,
    ) -> Generator<O> {
        let (generator, generator_yielder) = Generator::create();
        tokio::spawn(async move {
            while let Some(item) = self.next().await {
                if let Some(item_decoded) = transformer(item) {
                    if generator_yielder.r#yield(item_decoded).await.is_err() {
                        tracing::error!("> StreamProtocol > to_generator > Generator > error r#yield, probably it's closed");
                        tracing::debug!("> StreamProtocol > to_generator > Generator > breaking to stop yielding");
                        break;
                    }
                }
            }
        });

        generator
    }

    async fn acknowledge_open(&self) -> Result<(), TransportError> {
        let stream_message = StreamMessage {
            port_id: self.port_id,
            sequence_id: self.last_received_sequence_id,
            message_identifier: build_message_identifier(
                RpcMessageTypes::StreamAck as u32,
                self.message_number,
            ),
            payload: vec![],
            closed: false,
            ack: true,
        };

        self.transport.send(stream_message.encode_to_vec()).await
    }

    pub fn close(&mut self) {
        self.generator.0.close();
        self.process_cancellation_token.cancel();
    }

    /// The returned listener is meant to be registered in
    /// [`crate::messages_handlers::ServerMessagesHandler`] or
    /// [`crate::messages_handlers::ClientMessagesHandler`], and the callback is what
    /// unregisters it once processing finishes.
    pub(crate) async fn start_processing<
        F: Future + Send,
        Callback: FnOnce() -> F + Send + 'static,
    >(
        &self,
        callback: Callback,
    ) -> Result<AsyncChannelSender<(RpcMessageTypes, u32, StreamMessage)>, CommonError> {
        if self.acknowledge_open().await.is_err() {
            return Err(CommonError::TransportError);
        }
        let token = self.process_cancellation_token.clone();
        let (messages_listener, messages_processor) = unbounded();
        let internal_channel = self.generator.1.clone();
        tokio::spawn(async move {
            let token_cloned = token.clone();
            select! {
                _ = token.cancelled() => {
                    debug!("> StreamProtocol cancelled!");
                    callback().await;
                },
                _ = Self::process_messages(messages_processor, internal_channel, token_cloned) => {
                    callback().await;
                }
            }
        });

        Ok(messages_listener)
    }

    async fn process_messages(
        messages_processor: AsyncChannelReceiver<(RpcMessageTypes, u32, StreamMessage)>,
        internal_channel_sender: GeneratorYielder<Vec<u8>>,
        cancellation_token: CancellationToken,
    ) {
        while let Ok((message_type, _, stream_message)) = messages_processor.recv().await {
            if matches!(message_type, RpcMessageTypes::StreamMessage) {
                if stream_message.closed {
                    cancellation_token.cancel();
                    messages_processor.close();
                } else if internal_channel_sender
                    .r#yield(stream_message.payload)
                    .await
                    .is_err()
                {
                    tracing::error!("> StreamProtocol > process_messages > Error on sending through the Generator, seems to be dropped")
                }
            } else if matches!(message_type, RpcMessageTypes::RemoteErrorResponse) {
                cancellation_token.cancel();
                messages_processor.close();
            }
        }
    }
}

#[derive(Debug)]
pub enum GeneratorError {
    /// The receiving half is gone; stop yielding.
    UnableToInsert,
}

pub struct Generator<M>(UnboundedReceiver<M>);

impl<M: Send + Sync + 'static> Generator<M> {
    pub fn create() -> (Self, GeneratorYielder<M>) {
        let channel = unbounded_channel();
        (Self(channel.1), GeneratorYielder::new(channel.0))
    }

    /// The transform runs in a background task; an item it maps to `None` is dropped.
    pub fn from_generator<
        O: Send + Sync + 'static,
        F: Fn(M) -> Option<O> + Send + Sync + 'static,
    >(
        mut old_generator: Generator<M>,
        transformer: F,
    ) -> Generator<O> {
        let (generator, generator_yielder) = Generator::create();
        tokio::spawn(async move {
            while let Some(item) = old_generator.next().await {
                if let Some(new_item) = transformer(item) {
                    if generator_yielder.r#yield(new_item).await.is_err() {
                        tracing::error!("> Generator > error r#yield, probably it's closed");
                        tracing::debug!("> Generator > breaking to stop yielding");
                        break;
                    }
                }
            }
        });

        generator
    }

    pub async fn next(&mut self) -> Option<M> {
        self.0.recv().await
    }

    pub fn close(&mut self) {
        self.0.close()
    }
}

pub struct GeneratorYielder<M>(UnboundedSender<M>);

impl<M> GeneratorYielder<M> {
    fn new(sender: UnboundedSender<M>) -> Self {
        Self(sender)
    }

    pub async fn r#yield(&self, item: M) -> Result<(), GeneratorError> {
        match self.0.send(item) {
            Ok(_) => Ok(()),
            Err(_) => Err(GeneratorError::UnableToInsert),
        }
    }
}

impl<M> Clone for GeneratorYielder<M> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}
