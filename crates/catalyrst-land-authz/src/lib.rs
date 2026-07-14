pub mod events;
pub mod indexer;
pub mod resolve;

pub use events::{AuthzEvent, ESTATE_REGISTRY_MAINNET, LAND_REGISTRY_MAINNET};
pub use indexer::{cursor, fold, Indexer};
pub use resolve::{LandAuthzStore, ParcelSubject, UpdatableParcel};

pub const DEFAULT_RPC_URL: &str = "https://rpc.decentraland.org/mainnet";

pub async fn migrate(pool: &sqlx::PgPool) -> Result<(), sqlx::migrate::MigrateError> {
    sqlx::migrate!("./migrations").run(pool).await
}
