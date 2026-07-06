use sqlx::PgPool;

pub struct UpsertPlayerConnection {
    pub address: String,
    pub ip_address: Option<String>,
    pub device_id: Option<String>,
}

#[derive(Clone)]
pub struct PlayerConnectionComponent {
    pool: PgPool,
}

impl PlayerConnectionComponent {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn upsert(&self, input: UpsertPlayerConnection) -> Result<(), sqlx::Error> {
        let ip_address = input.ip_address.filter(|s| !s.is_empty());
        let device_id = input.device_id.filter(|s| !s.is_empty());
        let now = now_ms();
        sqlx::query(
            "INSERT INTO player_connection_info (address, ip_address, device_id, created_at, updated_at) \
             VALUES ($1, $2, $3, $4, $4) \
             ON CONFLICT (address) DO UPDATE SET \
               ip_address = COALESCE(EXCLUDED.ip_address, player_connection_info.ip_address), \
               device_id = COALESCE(EXCLUDED.device_id, player_connection_info.device_id), \
               updated_at = EXCLUDED.updated_at",
        )
        .bind(&input.address)
        .bind(&ip_address)
        .bind(&device_id)
        .bind(now)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn get_device_id(&self, address: &str) -> Result<Option<String>, sqlx::Error> {
        let row: Option<(Option<String>,)> =
            sqlx::query_as("SELECT device_id FROM player_connection_info WHERE address = $1")
                .bind(address)
                .fetch_optional(&self.pool)
                .await?;
        Ok(row.and_then(|(d,)| d).filter(|s| !s.is_empty()))
    }
}

fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
