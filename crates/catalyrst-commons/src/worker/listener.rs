use std::time::Duration;

use sqlx::postgres::PgListener;
use sqlx::PgPool;
use tokio::task::JoinHandle;

const RECONNECT_DELAY: Duration = Duration::from_secs(5);

/// `on_notify` fires on connect and on every disconnect, not only on a delivered
/// notification: while the listener was down any number of writes may have landed
/// unseen, so the only safe assumption on either edge is that everything the caller
/// derives from the database is stale.
pub fn spawn_invalidation_listener<F>(
    pool: PgPool,
    channel: impl Into<String>,
    on_notify: F,
) -> JoinHandle<()>
where
    F: Fn() + Send + 'static,
{
    let channel = channel.into();
    tokio::spawn(async move {
        loop {
            match PgListener::connect_with(&pool).await {
                Ok(mut listener) => {
                    on_notify();
                    if let Err(err) = listener.listen(&channel).await {
                        tracing::warn!(channel = %channel, %err, "LISTEN failed; retrying");
                        tokio::time::sleep(RECONNECT_DELAY).await;
                        continue;
                    }
                    tracing::info!(channel = %channel, "invalidation listener up");
                    loop {
                        match listener.recv().await {
                            Ok(_) => on_notify(),
                            Err(err) => {
                                tracing::warn!(channel = %channel, %err, "listener dropped; reconnecting");
                                on_notify();
                                break;
                            }
                        }
                    }
                }
                Err(err) => {
                    tracing::warn!(channel = %channel, %err, "listener connect failed; retrying");
                }
            }
            tokio::time::sleep(RECONNECT_DELAY).await;
        }
    })
}
