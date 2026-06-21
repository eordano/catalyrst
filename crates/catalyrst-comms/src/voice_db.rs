//! Private (1:1) voice-chat status state machine.
//!
//! Faithful port of the private-voice-chat half of upstream comms-gatekeeper
//! `src/adapters/db/voice-db.ts`. The `voice_chat_users` table tracks, per
//! (address, room_name), a `status` drawn from [`VoiceChatUserStatus`] plus the
//! `joined_at` / `status_updated_at` timestamps that drive the TTL predicates.
//!
//! Upstream stores `joined_at`/`status_updated_at` as epoch-millisecond numbers
//! and compares against `Date.now() - TTL`. catalyrst stores them as Postgres
//! `TIMESTAMP`s and compares against `now() - (ttl_ms || ' milliseconds')`,
//! which is semantically identical.

use sqlx::{PgPool, Postgres, Transaction};

/// Status of a participant in a private voice-chat room. Wire values match
/// upstream `VoiceChatUserStatus` exactly (stored verbatim in the `status`
/// column).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VoiceChatUserStatus {
    /// The user is connected to the room.
    Connected,
    /// The user's connection was interrupted.
    ConnectionInterrupted,
    /// The user left the room voluntarily. This is the default status.
    Disconnected,
    /// Not connected yet.
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

/// TTLs (milliseconds) governing the voice-chat state machines. Defaults mirror
/// upstream `.env.default`: connection-interrupted, initial-connection and
/// community-no-moderator TTLs of 300000ms, expiry batch size 50.
#[derive(Debug, Clone, Copy)]
pub struct VoiceDbConfig {
    /// `VOICE_CHAT_CONNECTION_INTERRUPTED_TTL` — how long an interrupted user is
    /// still considered "in" the room before the room is torn down.
    pub connection_interrupted_ttl_ms: i64,
    /// `VOICE_CHAT_INITIAL_CONNECTION_TTL` — how long a `not_connected` user is
    /// given to connect before the room is torn down.
    pub initial_connection_ttl_ms: i64,
    /// `COMMUNITY_VOICE_CHAT_NO_MODERATOR_TTL` — how long a community room may
    /// run with moderators present but none currently active before the room is
    /// swept by the community expiry job.
    pub community_no_moderator_ttl_ms: i64,
    /// `VOICE_CHAT_EXPIRED_BATCH_SIZE` — max rooms swept per expiry pass.
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
    /// Reads the TTL knobs from the same env vars upstream uses, falling back to
    /// the `.env.default` values when unset.
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

/// A row of `voice_chat_users`.
#[derive(Debug, Clone)]
pub struct VoiceChatUserRow {
    pub address: String,
    pub room_name: String,
    pub status: String,
    /// Milliseconds since unix epoch (extracted from the timestamp column).
    pub joined_at: i64,
    pub status_updated_at: i64,
}

/// Private voice-chat DB component. Holds the pool and the TTL config.
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

    /// Creates a voice chat room and sets the users into the room. The users are
    /// set to **not_connected** (upstream `createVoiceChatRoom`). They flip to
    /// `connected` only once LiveKit fires `participant_joined`.
    pub async fn create_voice_chat_room(
        &self,
        room_name: &str,
        user_addresses: &[String],
    ) -> Result<(), sqlx::Error> {
        if user_addresses.is_empty() {
            return Ok(());
        }
        // Upstream inserts a fresh room; mirror with an ON CONFLICT reset so a
        // re-created room (same id) starts the state machine over.
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

    /// Gets the users in a room (upstream `getUsersInRoom`).
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

    /// Checks if a private room is active. A private room is active if there are
    /// two or more users and none of them have timed out (interrupted past TTL,
    /// not-connected past TTL, or left voluntarily). Faithful port of upstream
    /// `isPrivateRoomActive`.
    pub async fn is_private_room_active(&self, room_name: &str) -> Result<bool, sqlx::Error> {
        let users = self.get_users_in_room(room_name).await?;
        let now = now_ms();
        let interrupted = VoiceChatUserStatus::ConnectionInterrupted.as_str();
        let not_connected = VoiceChatUserStatus::NotConnected.as_str();
        let disconnected = VoiceChatUserStatus::Disconnected.as_str();
        let has_inactive_user = users.iter().any(|user| {
            let interrupted_past_ttl = user.status == interrupted
                && user.status_updated_at + self.cfg.connection_interrupted_ttl_ms < now;
            let not_joined_past_ttl = user.status == not_connected
                && user.joined_at + self.cfg.initial_connection_ttl_ms < now;
            let left_voluntarily = user.status == disconnected;
            interrupted_past_ttl || not_joined_past_ttl || left_voluntarily
        });
        Ok(!has_inactive_user && users.len() >= 2)
    }

    /// Private helper updating one user's status (upstream `_updateUserStatus`).
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

    /// Updates a user to `disconnected` (left voluntarily). Upstream
    /// `updateUserStatusAsDisconnected`.
    pub async fn update_user_status_as_disconnected(
        &self,
        address: &str,
        room_name: &str,
    ) -> Result<(), sqlx::Error> {
        self.update_user_status(address, room_name, VoiceChatUserStatus::Disconnected)
            .await
    }

    /// Updates a user to `connection_interrupted` (abrupt disconnect). Upstream
    /// `updateUserStatusAsConnectionInterrupted`.
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

    /// Gets the room the user is in, considering them "in" a room when
    /// connected, interrupted-within-TTL, or not-connected-within-TTL. The
    /// ORDER BY prefers connected > interrupted > not_connected, then most
    /// recently updated. Faithful port of upstream `_getRoomUserIsIn`.
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

    /// Transactional variant of [`Self::get_room_user_is_in`] used inside
    /// `join_user_to_room`.
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

    /// Joins a user to a room. If they are already in another room, disconnect
    /// them from it. Returns the room the user was in before joining. Faithful
    /// port of upstream `joinUserToRoom` (transactional). Errors if the user is
    /// not already in a room (they must have been inserted at room creation).
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

    /// Deletes a private voice chat by removing all users from the room. If the
    /// given address is or was not in the room, returns
    /// [`DeleteRoomError::RoomDoesNotExist`]. Returns the addresses that were in
    /// the deleted room. Faithful port of `deletePrivateVoiceChatUserIsOrWasIn`.
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

    /// Deletes a private voice chat room from the database without any checks.
    /// Upstream `deletePrivateVoiceChat`.
    pub async fn delete_private_voice_chat(&self, room_name: &str) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM voice_chat_users WHERE room_name = $1")
            .bind(room_name)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Deletes expired private voice chats and returns the names of the rooms
    /// that should be destroyed in LiveKit. A room is expired if any user is
    /// not-connected past the initial TTL, interrupted past the interrupted TTL,
    /// or has left voluntarily. Rooms whose only expiry trigger is a voluntary
    /// `disconnected` (already deleted in LiveKit) are NOT returned for
    /// destruction. Faithful port of `deleteExpiredPrivateVoiceChats`.
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

        // De-dup the rooms that must be destroyed.
        let mut out: Vec<String> = Vec::new();
        for (room, should_destroy) in rows {
            if should_destroy && !out.contains(&room) {
                out.push(room);
            }
        }
        Ok(out)
    }

    // ----------------------------------------------------------------------
    // Community voice-chat state machine
    //
    // Faithful port of the community half of upstream `voice-db.ts`. The
    // `community_voice_chat_users` table tracks, per (address, room_name), an
    // `is_moderator` flag plus the same `status` / `joined_at` /
    // `status_updated_at` triple as the private side. A community room is
    // "active" while it has at least one *active moderator* (connected,
    // interrupted-within-TTL, or not-connected-within-initial-TTL).
    // ----------------------------------------------------------------------

    /// Joins a user to a community voice-chat room as `not_connected` (upstream
    /// `joinUserToCommunityRoom`). They flip to `connected` only once LiveKit
    /// fires `participant_joined`. `is_moderator` is (re)set on conflict.
    pub async fn join_user_to_community_room(
        &self,
        user_address: &str,
        room_name: &str,
        is_moderator: bool,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO community_voice_chat_users \
               (address, room_name, is_moderator, status, joined_at, status_updated_at, created_at) \
             VALUES ($1, $2, $3, $4, now(), now(), now()) \
             ON CONFLICT (address, room_name) DO UPDATE SET \
               status = $4, status_updated_at = now(), is_moderator = $3",
        )
        .bind(user_address)
        .bind(room_name)
        .bind(is_moderator)
        .bind(VoiceChatUserStatus::NotConnected.as_str())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Updates the status of a user in a community room (upstream
    /// `updateCommunityUserStatus`).
    pub async fn update_community_user_status(
        &self,
        user_address: &str,
        room_name: &str,
        status: VoiceChatUserStatus,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE community_voice_chat_users SET status = $1, status_updated_at = now() \
             WHERE address = $2 AND room_name = $3",
        )
        .bind(status.as_str())
        .bind(user_address)
        .bind(room_name)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Gets the users in a community room (upstream `getCommunityUsersInRoom`).
    pub async fn get_community_users_in_room(
        &self,
        room_name: &str,
    ) -> Result<Vec<CommunityVoiceChatUserRow>, sqlx::Error> {
        let rows: Vec<(String, String, bool, String, i64, i64)> = sqlx::query_as(
            "SELECT address, room_name, is_moderator, status, \
                (extract(epoch FROM joined_at) * 1000)::bigint, \
                (extract(epoch FROM status_updated_at) * 1000)::bigint \
             FROM community_voice_chat_users WHERE room_name = $1",
        )
        .bind(room_name)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(
                |(address, room_name, is_moderator, status, joined_at, status_updated_at)| {
                    CommunityVoiceChatUserRow {
                        address,
                        room_name,
                        is_moderator,
                        status,
                        joined_at,
                        status_updated_at,
                    }
                },
            )
            .collect())
    }

    /// Whether a community user is currently active: connected, OR
    /// interrupted-within-TTL, OR not-connected-within-initial-TTL. Faithful
    /// port of `isActiveCommunityUser`.
    pub fn is_active_community_user(&self, user: &CommunityVoiceChatUserRow, now: i64) -> bool {
        let status = user.status.as_str();
        status == VoiceChatUserStatus::Connected.as_str()
            || (status == VoiceChatUserStatus::ConnectionInterrupted.as_str()
                && user.status_updated_at + self.cfg.connection_interrupted_ttl_ms > now)
            || (status == VoiceChatUserStatus::NotConnected.as_str()
                && user.joined_at + self.cfg.initial_connection_ttl_ms > now)
    }

    /// Whether a community room is active — i.e. it has at least one active
    /// moderator. Faithful port of `isCommunityRoomActive`.
    pub async fn is_community_room_active(&self, room_name: &str) -> Result<bool, sqlx::Error> {
        let now = now_ms();
        let users = self.get_community_users_in_room(room_name).await?;
        Ok(users
            .iter()
            .any(|u| u.is_moderator && self.is_active_community_user(u, now)))
    }

    /// Total participant count for a community room (all participants regardless
    /// of status). Upstream `getCommunityVoiceChatParticipantCount`.
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

    /// Deletes a community voice chat room (upstream `deleteCommunityVoiceChat`).
    pub async fn delete_community_voice_chat(&self, room_name: &str) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM community_voice_chat_users WHERE room_name = $1")
            .bind(room_name)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Deletes expired community voice chats and returns the names of the rooms
    /// that were deleted. A room expires when it has no moderators at all, OR it
    /// has moderators but none currently active AND the last moderator activity
    /// was longer ago than `COMMUNITY_VOICE_CHAT_NO_MODERATOR_TTL`. Faithful
    /// port of `deleteExpiredCommunityVoiceChats`.
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
        let rows: Vec<(String,)> = sqlx::query_as(&query)
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

    /// Gets all currently-active community voice chats (rooms with at least one
    /// active moderator), with their active participant + moderator counts.
    /// Faithful port of `getAllActiveCommunityVoiceChats`.
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
        let rows: Vec<(String, i64, i64)> = sqlx::query_as(&query)
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

    /// Whether the given user is currently in any community voice chat (active
    /// status: connected, interrupted-within-TTL, or not-connected-within-TTL).
    /// Faithful port of `isUserInAnyCommunityVoiceChat`.
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
        let exists: bool = sqlx::query_scalar(&query)
            .bind(user_address.to_lowercase())
            .fetch_one(&self.pool)
            .await?;
        Ok(exists)
    }

    /// Per-community participant counts (all participants) for a batch of
    /// communities. Faithful port of `getBulkCommunityVoiceChatParticipantCount`,
    /// keyed by community id. Communities absent from the table map to 0.
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

    /// The `getIsConnectedQuery` predicate: the per-row "active" condition used
    /// across the community counting/expiry queries (connected, OR
    /// interrupted-within-TTL, OR not-connected-within-initial-TTL). Compares
    /// the epoch-ms-extracted timestamp columns against `now_ms()` minus the
    /// TTL, exactly mirroring upstream's `status_updated_at > now - TTL`.
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

/// A row of `community_voice_chat_users`.
#[derive(Debug, Clone)]
pub struct CommunityVoiceChatUserRow {
    pub address: String,
    pub room_name: String,
    pub is_moderator: bool,
    pub status: String,
    /// Milliseconds since unix epoch (extracted from the timestamp column).
    pub joined_at: i64,
    pub status_updated_at: i64,
}

/// An active community voice chat (room with at least one active moderator).
#[derive(Debug, Clone)]
pub struct ActiveCommunityVoiceChat {
    pub community_id: String,
    pub participant_count: i64,
    pub moderator_count: i64,
}

/// Result of [`VoiceDb::join_user_to_room`].
#[derive(Debug, Clone)]
pub struct JoinOutcome {
    pub old_room: String,
}

/// Error from [`VoiceDb::delete_private_voice_chat_user_is_or_was_in`].
#[derive(Debug)]
pub enum DeleteRoomError {
    /// The room (or the given user's membership in it) does not exist.
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
}
