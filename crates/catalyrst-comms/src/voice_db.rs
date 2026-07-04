use sqlx::{PgPool, Postgres, Transaction};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VoiceChatUserStatus {
    Connected,

    ConnectionInterrupted,

    Disconnected,

    NotConnected,
}

impl VoiceChatUserStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            VoiceChatUserStatus::Connected => "connected",
            VoiceChatUserStatus::ConnectionInterrupted => "connection_interrupted",
            VoiceChatUserStatus::Disconnected => "disconnected",
            VoiceChatUserStatus::NotConnected => "not_connected",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct VoiceDbConfig {
    pub connection_interrupted_ttl_ms: i64,

    pub initial_connection_ttl_ms: i64,

    pub community_no_moderator_ttl_ms: i64,

    pub expired_batch_size: i64,
}

impl Default for VoiceDbConfig {
    fn default() -> Self {
        Self {
            connection_interrupted_ttl_ms: 300_000,
            initial_connection_ttl_ms: 300_000,
            community_no_moderator_ttl_ms: 300_000,
            expired_batch_size: 50,
        }
    }
}

impl VoiceDbConfig {
    pub fn from_env() -> Self {
        let d = VoiceDbConfig::default();
        Self {
            connection_interrupted_ttl_ms: env_i64(
                "VOICE_CHAT_CONNECTION_INTERRUPTED_TTL",
                d.connection_interrupted_ttl_ms,
            ),
            initial_connection_ttl_ms: env_i64(
                "VOICE_CHAT_INITIAL_CONNECTION_TTL",
                d.initial_connection_ttl_ms,
            ),
            community_no_moderator_ttl_ms: env_i64(
                "COMMUNITY_VOICE_CHAT_NO_MODERATOR_TTL",
                d.community_no_moderator_ttl_ms,
            ),
            expired_batch_size: env_i64("VOICE_CHAT_EXPIRED_BATCH_SIZE", d.expired_batch_size),
        }
    }
}

fn env_i64(key: &str, default: i64) -> i64 {
    std::env::var(key)
        .ok()
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or(default)
}

#[derive(Debug, Clone)]
pub struct VoiceChatUserRow {
    pub address: String,
    pub room_name: String,
    pub status: String,

    pub joined_at: i64,
    pub status_updated_at: i64,
}

#[derive(Clone)]
pub struct VoiceDb {
    pool: PgPool,
    cfg: VoiceDbConfig,
}

impl VoiceDb {
    pub fn new(pool: PgPool, cfg: VoiceDbConfig) -> Self {
        Self { pool, cfg }
    }

    pub fn config(&self) -> VoiceDbConfig {
        self.cfg
    }

    pub async fn create_voice_chat_room(
        &self,
        room_name: &str,
        user_addresses: &[String],
    ) -> Result<(), sqlx::Error> {
        if user_addresses.is_empty() {
            return Ok(());
        }

        for addr in user_addresses {
            sqlx::query(
                "INSERT INTO voice_chat_users (address, room_name, status, joined_at, status_updated_at) \
                 VALUES ($1, $2, $3, now(), now()) \
                 ON CONFLICT (address, room_name) \
                 DO UPDATE SET status = $3, joined_at = now(), status_updated_at = now()",
            )
            .bind(addr)
            .bind(room_name)
            .bind(VoiceChatUserStatus::NotConnected.as_str())
            .execute(&self.pool)
            .await?;
        }
        Ok(())
    }

    pub async fn get_users_in_room(
        &self,
        room_name: &str,
    ) -> Result<Vec<VoiceChatUserRow>, sqlx::Error> {
        let rows: Vec<(String, String, String, i64, i64)> = sqlx::query_as(
            "SELECT address, room_name, status, \
                (extract(epoch FROM joined_at) * 1000)::bigint, \
                (extract(epoch FROM status_updated_at) * 1000)::bigint \
             FROM voice_chat_users WHERE room_name = $1",
        )
        .bind(room_name)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(
                |(address, room_name, status, joined_at, status_updated_at)| VoiceChatUserRow {
                    address,
                    room_name,
                    status,
                    joined_at,
                    status_updated_at,
                },
            )
            .collect())
    }

    pub async fn is_private_room_active(&self, room_name: &str) -> Result<bool, sqlx::Error> {
        let users = self.get_users_in_room(room_name).await?;
        Ok(is_private_room_active(&self.cfg, &users, now_ms()))
    }

    async fn update_user_status(
        &self,
        address: &str,
        room_name: &str,
        status: VoiceChatUserStatus,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE voice_chat_users SET status = $1, status_updated_at = now() \
             WHERE address = $2 AND room_name = $3",
        )
        .bind(status.as_str())
        .bind(address)
        .bind(room_name)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn update_user_status_tx(
        tx: &mut Transaction<'_, Postgres>,
        address: &str,
        room_name: &str,
        status: VoiceChatUserStatus,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE voice_chat_users SET status = $1, status_updated_at = now() \
             WHERE address = $2 AND room_name = $3",
        )
        .bind(status.as_str())
        .bind(address)
        .bind(room_name)
        .execute(&mut **tx)
        .await?;
        Ok(())
    }

    pub async fn update_user_status_as_disconnected(
        &self,
        address: &str,
        room_name: &str,
    ) -> Result<(), sqlx::Error> {
        self.update_user_status(address, room_name, VoiceChatUserStatus::Disconnected)
            .await
    }

    pub async fn update_user_status_as_connection_interrupted(
        &self,
        address: &str,
        room_name: &str,
    ) -> Result<(), sqlx::Error> {
        self.update_user_status(
            address,
            room_name,
            VoiceChatUserStatus::ConnectionInterrupted,
        )
        .await
    }

    pub async fn get_room_user_is_in(&self, address: &str) -> Result<Option<String>, sqlx::Error> {
        let row: Option<(String,)> = sqlx::query_as(
            "SELECT room_name FROM voice_chat_users WHERE address = $1 AND ( \
                status = $2 \
                OR (status = $3 AND status_updated_at > now() - ($5 || ' milliseconds')::interval) \
                OR (status = $4 AND joined_at > now() - ($6 || ' milliseconds')::interval) \
             ) \
             ORDER BY \
                CASE status \
                    WHEN $2 THEN 1 \
                    WHEN $3 THEN 2 \
                    WHEN $4 THEN 3 \
                    ELSE 4 \
                END, \
                status_updated_at DESC \
             LIMIT 1",
        )
        .bind(address)
        .bind(VoiceChatUserStatus::Connected.as_str())
        .bind(VoiceChatUserStatus::ConnectionInterrupted.as_str())
        .bind(VoiceChatUserStatus::NotConnected.as_str())
        .bind(self.cfg.connection_interrupted_ttl_ms.to_string())
        .bind(self.cfg.initial_connection_ttl_ms.to_string())
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|(r,)| r))
    }

    async fn get_room_user_is_in_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        address: &str,
    ) -> Result<Option<String>, sqlx::Error> {
        let row: Option<(String,)> = sqlx::query_as(
            "SELECT room_name FROM voice_chat_users WHERE address = $1 AND ( \
                status = $2 \
                OR (status = $3 AND status_updated_at > now() - ($5 || ' milliseconds')::interval) \
                OR (status = $4 AND joined_at > now() - ($6 || ' milliseconds')::interval) \
             ) \
             ORDER BY \
                CASE status \
                    WHEN $2 THEN 1 \
                    WHEN $3 THEN 2 \
                    WHEN $4 THEN 3 \
                    ELSE 4 \
                END, \
                status_updated_at DESC \
             LIMIT 1",
        )
        .bind(address)
        .bind(VoiceChatUserStatus::Connected.as_str())
        .bind(VoiceChatUserStatus::ConnectionInterrupted.as_str())
        .bind(VoiceChatUserStatus::NotConnected.as_str())
        .bind(self.cfg.connection_interrupted_ttl_ms.to_string())
        .bind(self.cfg.initial_connection_ttl_ms.to_string())
        .fetch_optional(&mut **tx)
        .await?;
        Ok(row.map(|(r,)| r))
    }

    pub async fn join_user_to_room(
        &self,
        address: &str,
        room_name: &str,
    ) -> Result<JoinOutcome, sqlx::Error> {
        let mut tx = self.pool.begin().await?;

        let room_user_is_in = self.get_room_user_is_in_tx(&mut tx, address).await?;

        let Some(old_room) = room_user_is_in else {
            tx.rollback().await?;
            return Err(sqlx::Error::Protocol(format!(
                "User {address} is not in a room"
            )));
        };

        if old_room != room_name {
            Self::update_user_status_tx(
                &mut tx,
                address,
                &old_room,
                VoiceChatUserStatus::Disconnected,
            )
            .await?;
        }

        Self::update_user_status_tx(&mut tx, address, room_name, VoiceChatUserStatus::Connected)
            .await?;

        tx.commit().await?;
        Ok(JoinOutcome { old_room })
    }

    pub async fn delete_private_voice_chat_user_is_or_was_in(
        &self,
        room_name: &str,
        address: &str,
    ) -> Result<Vec<String>, DeleteRoomError> {
        let mut tx = self.pool.begin().await.map_err(DeleteRoomError::Db)?;

        let rows: Vec<(String,)> =
            sqlx::query_as("SELECT address FROM voice_chat_users WHERE room_name = $1")
                .bind(room_name)
                .fetch_all(&mut *tx)
                .await
                .map_err(DeleteRoomError::Db)?;

        let addresses: Vec<String> = rows.into_iter().map(|(a,)| a).collect();
        if addresses.is_empty() || !addresses.iter().any(|a| a == address) {
            tx.rollback().await.map_err(DeleteRoomError::Db)?;
            return Err(DeleteRoomError::RoomDoesNotExist);
        }

        sqlx::query("DELETE FROM voice_chat_users WHERE room_name = $1")
            .bind(room_name)
            .execute(&mut *tx)
            .await
            .map_err(DeleteRoomError::Db)?;

        tx.commit().await.map_err(DeleteRoomError::Db)?;
        Ok(addresses)
    }

    pub async fn delete_private_voice_chat(&self, room_name: &str) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM voice_chat_users WHERE room_name = $1")
            .bind(room_name)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn delete_expired_private_voice_chats(&self) -> Result<Vec<String>, sqlx::Error> {
        let rows: Vec<(String, bool)> = sqlx::query_as(
            "WITH expired_rooms AS ( \
                SELECT \
                    room_name, \
                    bool_or( \
                        status = $1 OR status = $2 \
                    ) AS should_destroy_room \
                FROM voice_chat_users WHERE \
                    (status = $1 AND joined_at <= now() - ($4 || ' milliseconds')::interval) \
                    OR (status = $2 AND status_updated_at <= now() - ($5 || ' milliseconds')::interval) \
                    OR (status = $3) \
                GROUP BY room_name LIMIT $6 \
             ) \
             DELETE FROM voice_chat_users USING expired_rooms \
             WHERE voice_chat_users.room_name = expired_rooms.room_name \
             RETURNING expired_rooms.room_name, expired_rooms.should_destroy_room",
        )
        .bind(VoiceChatUserStatus::NotConnected.as_str())
        .bind(VoiceChatUserStatus::ConnectionInterrupted.as_str())
        .bind(VoiceChatUserStatus::Disconnected.as_str())
        .bind(self.cfg.initial_connection_ttl_ms.to_string())
        .bind(self.cfg.connection_interrupted_ttl_ms.to_string())
        .bind(self.cfg.expired_batch_size)
        .fetch_all(&self.pool)
        .await?;

        let mut out: Vec<String> = Vec::new();
        for (room, should_destroy) in rows {
            if should_destroy && !out.contains(&room) {
                out.push(room);
            }
        }
        Ok(out)
    }

    pub async fn join_user_to_community_room(
        &self,
        user_address: &str,
        room_name: &str,
        is_moderator: bool,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO community_voice_chat_users \
               (address, room_name, is_moderator, status, joined_at, status_updated_at, created_at, sid) \
             VALUES ($1, $2, $3, $4, now(), now(), now(), NULL) \
             ON CONFLICT (address, room_name) DO UPDATE SET \
               status = $4, status_updated_at = now(), is_moderator = $3, sid = NULL",
        )
        .bind(user_address)
        .bind(room_name)
        .bind(is_moderator)
        .bind(VoiceChatUserStatus::NotConnected.as_str())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn update_community_user_status(
        &self,
        user_address: &str,
        room_name: &str,
        status: VoiceChatUserStatus,
        sid: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        match sid {
            Some(sid) => {
                sqlx::query(
                    "UPDATE community_voice_chat_users \
                     SET status = $1, status_updated_at = now(), sid = $4 \
                     WHERE address = $2 AND room_name = $3",
                )
                .bind(status.as_str())
                .bind(user_address)
                .bind(room_name)
                .bind(sid)
                .execute(&self.pool)
                .await?;
            }
            None => {
                sqlx::query(
                    "UPDATE community_voice_chat_users \
                     SET status = $1, status_updated_at = now() \
                     WHERE address = $2 AND room_name = $3",
                )
                .bind(status.as_str())
                .bind(user_address)
                .bind(room_name)
                .execute(&self.pool)
                .await?;
            }
        }
        Ok(())
    }

    pub async fn get_community_users_in_room(
        &self,
        room_name: &str,
    ) -> Result<Vec<CommunityVoiceChatUserRow>, sqlx::Error> {
        type CommunityVoiceChatUserSqlRow =
            (String, String, bool, String, i64, i64, Option<String>);
        let rows: Vec<CommunityVoiceChatUserSqlRow> = sqlx::query_as(
            "SELECT address, room_name, is_moderator, status, \
                (extract(epoch FROM joined_at) * 1000)::bigint, \
                (extract(epoch FROM status_updated_at) * 1000)::bigint, \
                sid \
             FROM community_voice_chat_users WHERE room_name = $1",
        )
        .bind(room_name)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(
                |(address, room_name, is_moderator, status, joined_at, status_updated_at, sid)| {
                    CommunityVoiceChatUserRow {
                        address,
                        room_name,
                        is_moderator,
                        status,
                        joined_at,
                        status_updated_at,
                        sid,
                    }
                },
            )
            .collect())
    }

    pub fn is_active_community_user(&self, user: &CommunityVoiceChatUserRow, now: i64) -> bool {
        is_active_community_user(&self.cfg, user, now)
    }

    pub async fn is_community_room_active(&self, room_name: &str) -> Result<bool, sqlx::Error> {
        let now = now_ms();
        let users = self.get_community_users_in_room(room_name).await?;
        Ok(users
            .iter()
            .any(|u| u.is_moderator && self.is_active_community_user(u, now)))
    }

    pub async fn get_community_voice_chat_participant_count(
        &self,
        room_name: &str,
    ) -> Result<i64, sqlx::Error> {
        let n: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM community_voice_chat_users WHERE room_name = $1",
        )
        .bind(room_name)
        .fetch_one(&self.pool)
        .await?;
        Ok(n)
    }

    pub async fn delete_community_voice_chat(&self, room_name: &str) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM community_voice_chat_users WHERE room_name = $1")
            .bind(room_name)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn delete_expired_community_voice_chats(&self) -> Result<Vec<String>, sqlx::Error> {
        let connected = self.is_connected_sql();
        let query = format!(
            "WITH room_moderator_status AS ( \
                SELECT \
                    room_name, \
                    COUNT(CASE WHEN is_moderator = true THEN 1 END) AS moderator_count, \
                    COUNT(CASE WHEN is_moderator = true AND ({connected}) THEN 1 END) AS active_moderator_count, \
                    MAX(CASE WHEN is_moderator = true THEN (extract(epoch FROM status_updated_at) * 1000)::bigint ELSE 0 END) AS last_moderator_activity \
                FROM community_voice_chat_users \
                GROUP BY room_name \
             ), \
             expired_rooms AS ( \
                SELECT room_name \
                FROM room_moderator_status \
                WHERE ( \
                    moderator_count = 0 \
                ) OR ( \
                    moderator_count > 0 \
                    AND active_moderator_count = 0 \
                    AND last_moderator_activity > 0 \
                    AND last_moderator_activity <= ($1::bigint) \
                ) \
                LIMIT $2 \
             ) \
             DELETE FROM community_voice_chat_users USING expired_rooms \
             WHERE community_voice_chat_users.room_name = expired_rooms.room_name \
             RETURNING expired_rooms.room_name"
        );
        let now = now_ms();
        let rows: Vec<(String,)> = sqlx::query_as(sqlx::AssertSqlSafe(query))
            .bind(now - self.cfg.community_no_moderator_ttl_ms)
            .bind(self.cfg.expired_batch_size)
            .fetch_all(&self.pool)
            .await?;
        let mut out: Vec<String> = Vec::new();
        for (room,) in rows {
            if !out.contains(&room) {
                out.push(room);
            }
        }
        Ok(out)
    }

    pub async fn get_all_active_community_voice_chats(
        &self,
    ) -> Result<Vec<ActiveCommunityVoiceChat>, sqlx::Error> {
        let connected = self.is_connected_sql();
        let prefix = format!("{}-", crate::livekit::COMMUNITY_VOICE_CHAT_ROOM_PREFIX);
        let query = format!(
            "SELECT \
                REPLACE(room_name, $1, '') AS community_id, \
                COUNT(CASE WHEN ({connected}) THEN 1 END) AS participant_count, \
                COUNT(CASE WHEN is_moderator = true AND ({connected}) THEN 1 END) AS moderator_count \
             FROM community_voice_chat_users \
             WHERE room_name LIKE $2 \
             GROUP BY room_name \
             HAVING COUNT(CASE WHEN is_moderator = true AND ({connected}) THEN 1 END) > 0"
        );
        let rows: Vec<(String, i64, i64)> = sqlx::query_as(sqlx::AssertSqlSafe(query))
            .bind(&prefix)
            .bind(format!("{prefix}%"))
            .fetch_all(&self.pool)
            .await?;
        Ok(rows
            .into_iter()
            .map(
                |(community_id, participant_count, moderator_count)| ActiveCommunityVoiceChat {
                    community_id,
                    participant_count,
                    moderator_count,
                },
            )
            .collect())
    }

    pub async fn is_user_in_any_community_voice_chat(
        &self,
        user_address: &str,
    ) -> Result<bool, sqlx::Error> {
        let connected = self.is_connected_sql();
        let query = format!(
            "SELECT EXISTS( \
                SELECT 1 FROM community_voice_chat_users \
                WHERE address = $1 AND ({connected}) \
             )"
        );
        let exists: bool = sqlx::query_scalar(sqlx::AssertSqlSafe(query))
            .bind(user_address.to_lowercase())
            .fetch_one(&self.pool)
            .await?;
        Ok(exists)
    }

    pub async fn get_bulk_community_voice_chat_participant_count(
        &self,
        community_ids: &[String],
    ) -> Result<std::collections::BTreeMap<String, i64>, sqlx::Error> {
        let mut counts: std::collections::BTreeMap<String, i64> = std::collections::BTreeMap::new();
        for id in community_ids {
            counts.insert(id.clone(), 0);
        }
        if community_ids.is_empty() {
            return Ok(counts);
        }
        let room_names: Vec<String> = community_ids
            .iter()
            .map(|id| crate::livekit::community_voice_chat_room_name(id))
            .collect();
        let prefix = format!("{}-", crate::livekit::COMMUNITY_VOICE_CHAT_ROOM_PREFIX);
        let rows: Vec<(String, i64)> = sqlx::query_as(
            "SELECT REPLACE(room_name, $1, '') AS community_id, COUNT(*) \
             FROM community_voice_chat_users \
             WHERE room_name = ANY($2) \
             GROUP BY room_name",
        )
        .bind(&prefix)
        .bind(&room_names)
        .fetch_all(&self.pool)
        .await?;
        for (community_id, count) in rows {
            counts.insert(community_id, count);
        }
        Ok(counts)
    }

    fn is_connected_sql(&self) -> String {
        let now = now_ms();
        let connected = VoiceChatUserStatus::Connected.as_str();
        let interrupted = VoiceChatUserStatus::ConnectionInterrupted.as_str();
        let not_connected = VoiceChatUserStatus::NotConnected.as_str();
        format!(
            "status = '{connected}' \
             OR (status = '{interrupted}' AND (extract(epoch FROM status_updated_at) * 1000)::bigint > {interrupted_bound}) \
             OR (status = '{not_connected}' AND (extract(epoch FROM joined_at) * 1000)::bigint > {initial_bound})",
            interrupted_bound = now - self.cfg.connection_interrupted_ttl_ms,
            initial_bound = now - self.cfg.initial_connection_ttl_ms,
        )
    }
}

#[derive(Debug, Clone)]
pub struct CommunityVoiceChatUserRow {
    pub address: String,
    pub room_name: String,
    pub is_moderator: bool,
    pub status: String,

    pub joined_at: i64,
    pub status_updated_at: i64,
    pub sid: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ActiveCommunityVoiceChat {
    pub community_id: String,
    pub participant_count: i64,
    pub moderator_count: i64,
}

#[derive(Debug, Clone)]
pub struct JoinOutcome {
    pub old_room: String,
}

#[derive(Debug)]
pub enum DeleteRoomError {
    RoomDoesNotExist,
    Db(sqlx::Error),
}

fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn is_active_community_user(
    cfg: &VoiceDbConfig,
    user: &CommunityVoiceChatUserRow,
    now: i64,
) -> bool {
    let status = user.status.as_str();
    status == VoiceChatUserStatus::Connected.as_str()
        || (status == VoiceChatUserStatus::ConnectionInterrupted.as_str()
            && user.status_updated_at + cfg.connection_interrupted_ttl_ms > now)
        || (status == VoiceChatUserStatus::NotConnected.as_str()
            && user.joined_at + cfg.initial_connection_ttl_ms > now)
}

fn is_private_room_active(cfg: &VoiceDbConfig, users: &[VoiceChatUserRow], now: i64) -> bool {
    let interrupted = VoiceChatUserStatus::ConnectionInterrupted.as_str();
    let not_connected = VoiceChatUserStatus::NotConnected.as_str();
    let disconnected = VoiceChatUserStatus::Disconnected.as_str();
    let has_inactive_user = users.iter().any(|user| {
        (user.status == interrupted
            && user.status_updated_at + cfg.connection_interrupted_ttl_ms < now)
            || (user.status == not_connected
                && user.joined_at + cfg.initial_connection_ttl_ms < now)
            || user.status == disconnected
    });
    !has_inactive_user && users.len() >= 2
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_wire_values_match_upstream() {
        assert_eq!(VoiceChatUserStatus::Connected.as_str(), "connected");
        assert_eq!(
            VoiceChatUserStatus::ConnectionInterrupted.as_str(),
            "connection_interrupted"
        );
        assert_eq!(VoiceChatUserStatus::Disconnected.as_str(), "disconnected");
        assert_eq!(VoiceChatUserStatus::NotConnected.as_str(), "not_connected");
    }

    #[test]
    fn default_ttls_match_env_default() {
        let cfg = VoiceDbConfig::default();
        assert_eq!(cfg.connection_interrupted_ttl_ms, 300_000);
        assert_eq!(cfg.initial_connection_ttl_ms, 300_000);
        assert_eq!(cfg.expired_batch_size, 50);
    }

    fn community_user(
        status: VoiceChatUserStatus,
        joined_at: i64,
        updated_at: i64,
    ) -> CommunityVoiceChatUserRow {
        CommunityVoiceChatUserRow {
            address: "0xabc".into(),
            room_name: "voice-chat-community-c1".into(),
            is_moderator: true,
            status: status.as_str().into(),
            joined_at,
            status_updated_at: updated_at,
            sid: None,
        }
    }

    fn private_user(
        status: VoiceChatUserStatus,
        joined_at: i64,
        updated_at: i64,
    ) -> VoiceChatUserRow {
        VoiceChatUserRow {
            address: "0xabc".into(),
            room_name: "voice-chat-private-c1".into(),
            status: status.as_str().into(),
            joined_at,
            status_updated_at: updated_at,
        }
    }

    #[test]
    fn active_community_user_connected_is_always_active() {
        let cfg = VoiceDbConfig::default();
        let now = 10_000_000;

        let u = community_user(VoiceChatUserStatus::Connected, 0, 0);
        assert!(is_active_community_user(&cfg, &u, now));
    }

    #[test]
    fn active_community_user_interrupted_within_ttl() {
        let cfg = VoiceDbConfig::default();
        let now = 10_000_000;

        let inside = community_user(
            VoiceChatUserStatus::ConnectionInterrupted,
            0,
            now - cfg.connection_interrupted_ttl_ms + 1,
        );
        assert!(is_active_community_user(&cfg, &inside, now));

        let at_boundary = community_user(
            VoiceChatUserStatus::ConnectionInterrupted,
            0,
            now - cfg.connection_interrupted_ttl_ms,
        );
        assert!(!is_active_community_user(&cfg, &at_boundary, now));
    }

    #[test]
    fn active_community_user_not_connected_within_initial_ttl() {
        let cfg = VoiceDbConfig::default();
        let now = 10_000_000;
        let inside = community_user(
            VoiceChatUserStatus::NotConnected,
            now - cfg.initial_connection_ttl_ms + 1,
            0,
        );
        assert!(is_active_community_user(&cfg, &inside, now));
        let expired = community_user(
            VoiceChatUserStatus::NotConnected,
            now - cfg.initial_connection_ttl_ms,
            0,
        );
        assert!(!is_active_community_user(&cfg, &expired, now));
    }

    #[test]
    fn disconnected_community_user_is_inactive() {
        let cfg = VoiceDbConfig::default();
        let now = 10_000_000;
        let u = community_user(VoiceChatUserStatus::Disconnected, now, now);
        assert!(!is_active_community_user(&cfg, &u, now));
    }

    #[test]
    fn private_room_active_requires_two_active_users() {
        let cfg = VoiceDbConfig::default();
        let now = 10_000_000;

        let users = vec![
            private_user(VoiceChatUserStatus::Connected, now, now),
            private_user(VoiceChatUserStatus::Connected, now, now),
        ];
        assert!(is_private_room_active(&cfg, &users, now));

        assert!(!is_private_room_active(&cfg, &users[..1], now));
    }

    #[test]
    fn private_room_inactive_when_any_user_timed_out_or_left() {
        let cfg = VoiceDbConfig::default();
        let now = 10_000_000;

        let left = vec![
            private_user(VoiceChatUserStatus::Connected, now, now),
            private_user(VoiceChatUserStatus::Disconnected, now, now),
        ];
        assert!(!is_private_room_active(&cfg, &left, now));

        let stale = vec![
            private_user(VoiceChatUserStatus::Connected, now, now),
            private_user(
                VoiceChatUserStatus::NotConnected,
                now - cfg.initial_connection_ttl_ms - 1,
                now,
            ),
        ];
        assert!(!is_private_room_active(&cfg, &stale, now));

        let interrupted_ok = vec![
            private_user(VoiceChatUserStatus::Connected, now, now),
            private_user(
                VoiceChatUserStatus::ConnectionInterrupted,
                now,
                now - cfg.connection_interrupted_ttl_ms + 1,
            ),
        ];
        assert!(is_private_room_active(&cfg, &interrupted_ok, now));
    }
}
