use crate::rpc::config::Config;
use crate::rpc::context::{Context, SharedContext};
use crate::rpc::db::Db;
use crate::rpc::profiles::Profiles;
use crate::rpc::transport::AxumWsTransport;
use dcl_rpc::server::{RpcServer, ServerEventsSender};
use std::sync::Arc;
use tokio::sync::OnceCell;

pub struct AppStateInner {
    pub cfg: Config,
    pub ctx: SharedContext,
    rpc_events: OnceCell<ServerEventsSender<AxumWsTransport>>,
}

impl AppStateInner {
    pub fn new(cfg: Config, db: Db, profiles: Profiles) -> Self {
        let ctx = Context::new(cfg.clone(), db, profiles);
        Self {
            cfg,
            ctx,
            rpc_events: OnceCell::new(),
        }
    }

    pub async fn init_rpc(self: &Arc<Self>) {
        let ctx = self.ctx.clone();
        let _ = self
            .rpc_events
            .get_or_init(|| async move {
                let (sender, server_handle) = spawn_rpc_server(ctx);
                tokio::spawn(server_handle);
                sender
            })
            .await;
        self.spawn_voice_expiry_job();
    }

    fn spawn_voice_expiry_job(self: &Arc<Self>) {
        let ctx = self.ctx.clone();
        let expiration_ms = self.cfg.private_voice_chat_expiration_ms;
        let batch_size = self.cfg.private_voice_chat_expiration_batch_size;
        let interval_ms = self.cfg.private_voice_chat_job_interval_ms.max(1);
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(std::time::Duration::from_millis(interval_ms));
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                ticker.tick().await;
                ctx.expire_private_voice_chats(expiration_ms, batch_size)
                    .await;
            }
        });
    }

    pub fn rpc_events(&self) -> Option<&ServerEventsSender<AxumWsTransport>> {
        self.rpc_events.get()
    }
}

fn spawn_rpc_server(
    ctx: SharedContext,
) -> (
    ServerEventsSender<AxumWsTransport>,
    impl std::future::Future<Output = ()>,
) {
    use crate::rpc::proto::v2::SocialServiceRegistration;
    use crate::rpc::service::SocialServiceImpl;

    let mut server: RpcServer<Context, AxumWsTransport> = RpcServer::create(ctx.clone());

    let sender = server.get_server_events_sender();

    let bind_ctx = ctx.clone();
    server.set_on_transport_connected_handler(move |transport, transport_id| {
        bind_ctx.connection_opened();
        let address = transport.address().to_string();
        bind_ctx.register_identity(transport_id, address.clone());
        bind_ctx.register_kill_handle(transport_id, transport.kill_handle());
        if bind_ctx.mark_online(&address) {
            let fan_ctx = bind_ctx.clone();
            tokio::spawn(async move {
                fan_ctx
                    .fan_connectivity(&address, crate::rpc::proto::v2::ConnectivityStatus::Online)
                    .await;
            });
        }
    });
    let forget_ctx = ctx.clone();
    server.set_on_transport_closes_handler(move |transport, transport_id| {
        forget_ctx.connection_closed();
        forget_ctx.forget_identity(transport_id);
        let address = transport.address().to_string();
        if forget_ctx.mark_offline(&address) {
            let fan_ctx = forget_ctx.clone();
            tokio::spawn(async move {
                fan_ctx
                    .fan_connectivity(&address, crate::rpc::proto::v2::ConnectivityStatus::Offline)
                    .await;
            });
        }
    });

    server.set_module_registrator_handler(|port| {
        SocialServiceRegistration::register_service(port, SocialServiceImpl);
    });
    let run = async move {
        server.run().await;
    };
    (sender, run)
}

pub type AppState = Arc<AppStateInner>;
