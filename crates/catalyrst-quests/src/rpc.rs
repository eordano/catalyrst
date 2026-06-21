//! dcl-rpc server lifecycle + signed WebSocket handshake. Mirrors upstream
//! `decentraland/quests` crates/server/src/rpc/mod.rs: each connection's first
//! frame is the signed auth-chain (verified with `verify_handshake`), the
//! recovered address is attached to the transport, and the dcl-rpc server keys
//! the per-connection identity by transport id for the service.

use crate::auth_chain::{verify_handshake, FIVE_MINUTES_SECS};
use crate::context::{Context, SharedContext};
use crate::proto::QuestsServiceRegistration;
use crate::service::QuestsServiceImpl;
use crate::transport::AxumWsTransport;
use anyhow::{anyhow, Result};
use axum::extract::ws::{CloseFrame, Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::IntoResponse;
use chrono::Utc;
use dcl_rpc::server::{RpcServer, ServerEventsSender};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::OnceCell;
use tokio::time::timeout;

/// Holds the live context plus the (lazily started) dcl-rpc server's events
/// sender so the axum WS handler can attach authenticated transports.
pub struct RpcRuntime {
    ctx: SharedContext,
    auth_window_secs: i64,
    rpc_events: OnceCell<ServerEventsSender<AxumWsTransport>>,
}

impl RpcRuntime {
    pub fn new(ctx: SharedContext, auth_window_secs: i64) -> Arc<Self> {
        Arc::new(Self {
            ctx,
            auth_window_secs,
            rpc_events: OnceCell::new(),
        })
    }

    pub fn ctx(&self) -> &SharedContext {
        &self.ctx
    }

    /// Start the dcl-rpc server once (idempotent).
    pub async fn init(self: &Arc<Self>) {
        let ctx = self.ctx.clone();
        let _ = self
            .rpc_events
            .get_or_init(|| async move {
                let (sender, run) = spawn_rpc_server(ctx);
                tokio::spawn(run);
                sender
            })
            .await;
    }

    fn rpc_events(&self) -> Option<&ServerEventsSender<AxumWsTransport>> {
        self.rpc_events.get()
    }

    async fn handle_connection(self: &Arc<Self>, mut socket: WebSocket) -> Result<()> {
        let address = match self.auth_handshake(&mut socket).await {
            Ok(addr) => addr,
            Err(err) => {
                tracing::info!(%err, "quests rpc auth handshake failed");
                let _ = socket
                    .send(Message::Close(Some(CloseFrame {
                        code: 3003,
                        reason: "Unauthorized".into(),
                    })))
                    .await;
                return Ok(());
            }
        };
        tracing::info!(%address, "quests rpc client authenticated");

        let Some(events) = self.rpc_events() else {
            return Err(anyhow!(
                "rpc events sender not initialised; call RpcRuntime::init on boot"
            ));
        };
        let transport = Arc::new(AxumWsTransport::spawn(socket, address));
        events
            .send_attach_transport(transport)
            .map_err(|e| anyhow!("attach transport: {e:?}"))?;
        Ok(())
    }

    async fn auth_handshake(&self, socket: &mut WebSocket) -> Result<String> {
        let window = Duration::from_secs(self.auth_window_secs.max(0) as u64);
        let first = timeout(window, socket.recv())
            .await
            .map_err(|_| anyhow!("auth timeout after {}s", self.auth_window_secs))?
            .ok_or_else(|| anyhow!("socket closed before auth"))?
            .map_err(|e| anyhow!("ws recv error: {e}"))?;

        let text: String = match first {
            Message::Text(s) => s.to_string(),
            Message::Binary(b) => String::from_utf8(b.to_vec())
                .map_err(|_| anyhow!("auth payload was binary but not utf-8 json"))?,
            _ => return Err(anyhow!("expected text/binary frame for auth")),
        };

        let now_secs = Utc::now().timestamp();
        // Upstream signs `get` `/` (authenticate_dcl_user_with_signed_headers("get","/",...)).
        let signer = verify_handshake(&text, "get", "/", FIVE_MINUTES_SECS, now_secs)
            .map_err(|e| anyhow!("handshake: {e}"))?;
        Ok(signer)
    }
}

fn spawn_rpc_server(
    ctx: SharedContext,
) -> (
    ServerEventsSender<AxumWsTransport>,
    impl std::future::Future<Output = ()>,
) {
    let mut server: RpcServer<Context, AxumWsTransport> = RpcServer::create(ctx.clone());
    let sender = server.get_server_events_sender();

    let bind_ctx = ctx.clone();
    server.set_on_transport_connected_handler(move |transport, transport_id| {
        bind_ctx.register_identity(transport_id, transport.address().to_string());
    });
    let forget_ctx = ctx.clone();
    server.set_on_transport_closes_handler(move |_transport, transport_id| {
        forget_ctx.forget_identity(transport_id);
    });

    server.set_module_registrator_handler(|port| {
        QuestsServiceRegistration::register_service(port, QuestsServiceImpl);
    });
    let run = async move {
        server.run().await;
    };
    (sender, run)
}

/// axum WS upgrade endpoint for the dcl-rpc QuestsService transport.
pub async fn ws_upgrade(
    State(rt): State<Arc<RpcRuntime>>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| async move {
        if let Err(err) = rt.handle_connection(socket).await {
            tracing::warn!(error = %err, "quests rpc ws connection ended");
        }
    })
}
