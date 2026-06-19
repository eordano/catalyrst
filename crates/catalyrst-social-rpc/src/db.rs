use chrono::{DateTime, Duration as ChronoDuration, Utc};
use sqlx::{PgPool, Row};
use uuid::Uuid;

#[derive(Clone)]
pub struct Db {
    pool: PgPool,
}

const REQUEST_ADDRESS_SELECT: &str = r#"CASE
    WHEN f.address_requester = fa.acting_user THEN f.address_requested
    ELSE f.address_requester
  END AS address"#;
const SENT_ACTOR_COND: &str = "fa.acting_user = $1";
const RECEIVED_ACTOR_COND: &str =
    "fa.acting_user <> $1 AND ($1 IN (f.address_requester, f.address_requested))";
const BLOCKING_CONDITION: &str = r#"NOT EXISTS (
    SELECT 1 FROM blocks b
    WHERE (b.blocker_address = $1 AND b.blocked_address = CASE
              WHEN f.address_requester = fa.acting_user THEN f.address_requested
              ELSE f.address_requester END)
       OR (b.blocked_address = $1 AND b.blocker_address = CASE
              WHEN f.address_requester = fa.acting_user THEN f.address_requested
              ELSE f.address_requester END)
  )"#;

#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error(transparent)]
    Sqlx(#[from] sqlx::Error),
}

#[derive(Debug, Clone)]
pub struct LastAction {
    pub friendship_id: Uuid,
    pub action: String,
    pub acting_user: String,
    pub is_active: bool,
}

#[derive(Debug, Clone)]
pub struct FriendshipRequestRow {
    pub id: Uuid,
    pub address: String,
    pub timestamp: DateTime<Utc>,
    pub message: Option<String>,
}

#[derive(Debug, Clone)]
pub struct BlockedRow {
    pub address: String,
    pub blocked_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct SocialSettingsRow {
    pub private_messages_privacy: String,
    pub blocked_users_messages_visibility: String,
    pub show_situation_reactions: String,
}

impl Default for SocialSettingsRow {
    fn default() -> Self {
        Self {
            private_messages_privacy: "only_friends".into(),
            blocked_users_messages_visibility: "show_messages".into(),
            show_situation_reactions: "show".into(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct PrivateVoiceChatRow {
    pub id: Uuid,
    pub caller_address: String,
    pub callee_address: String,
}

impl Db {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    pub async fn get_friends(
        &self,
        address: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<String>, DbError> {
        let addr = address.to_lowercase();
        let rows = sqlx::query(
            r#"
            SELECT CASE WHEN address_requester = $1 THEN address_requested
                        ELSE address_requester END AS friend
            FROM friendships
            WHERE is_active = TRUE AND (address_requester = $1 OR address_requested = $1)
            ORDER BY friend
            LIMIT $2 OFFSET $3
            "#,
        )
        .bind(&addr)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(|r| r.get::<String, _>("friend")).collect())
    }

    pub async fn count_friends(&self, address: &str) -> Result<i64, DbError> {
        let addr = address.to_lowercase();
        let row = sqlx::query(
            r#"SELECT COUNT(*) AS n FROM friendships
               WHERE is_active = TRUE AND (address_requester = $1 OR address_requested = $1)"#,
        )
        .bind(&addr)
        .fetch_one(&self.pool)
        .await?;
        Ok(row.get::<i64, _>("n"))
    }

    pub async fn get_mutual_friends(
        &self,
        a: &str,
        b: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<String>, DbError> {
        let a = a.to_lowercase();
        let b = b.to_lowercase();
        let rows = sqlx::query(
            r#"
            SELECT f1.friend FROM (
              SELECT CASE WHEN address_requester = $1 THEN address_requested ELSE address_requester END AS friend
              FROM friendships WHERE is_active AND ($1 IN (address_requester, address_requested))
            ) f1
            JOIN (
              SELECT CASE WHEN address_requester = $2 THEN address_requested ELSE address_requester END AS friend
              FROM friendships WHERE is_active AND ($2 IN (address_requester, address_requested))
            ) f2 ON f1.friend = f2.friend
            WHERE f1.friend NOT IN ($1, $2)
            ORDER BY f1.friend
            LIMIT $3 OFFSET $4
            "#,
        )
        .bind(&a)
        .bind(&b)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(|r| r.get::<String, _>("friend")).collect())
    }

    pub async fn count_mutual_friends(&self, a: &str, b: &str) -> Result<i64, DbError> {
        let a = a.to_lowercase();
        let b = b.to_lowercase();
        let row = sqlx::query(
            r#"
            SELECT COUNT(*) AS n FROM (
              SELECT CASE WHEN address_requester = $1 THEN address_requested ELSE address_requester END AS friend
              FROM friendships WHERE is_active AND ($1 IN (address_requester, address_requested))
            ) f1
            JOIN (
              SELECT CASE WHEN address_requester = $2 THEN address_requested ELSE address_requester END AS friend
              FROM friendships WHERE is_active AND ($2 IN (address_requester, address_requested))
            ) f2 ON f1.friend = f2.friend
            WHERE f1.friend NOT IN ($1, $2)
            "#,
        )
        .bind(&a)
        .bind(&b)
        .fetch_one(&self.pool)
        .await?;
        Ok(row.get::<i64, _>("n"))
    }

    pub async fn get_friendship_requests(
        &self,
        address: &str,
        incoming: bool,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<FriendshipRequestRow>, DbError> {
        let addr = address.to_lowercase();
        let sql = format!(
            r#"
            SELECT fa.id AS id,
                   {address_select},
                   fa.timestamp AS ts,
                   fa.metadata AS metadata
            FROM friendship_actions fa
            JOIN friendships f ON f.id = fa.friendship_id AND f.is_active IS FALSE
            WHERE fa.action = 'request'
              AND {actor_cond}
              AND NOT EXISTS (
                SELECT 1 FROM friendship_actions newer
                WHERE newer.friendship_id = fa.friendship_id
                  AND newer.timestamp > fa.timestamp
              )
              AND {blocking}
            ORDER BY fa.timestamp DESC
            LIMIT $2 OFFSET $3
            "#,
            address_select = REQUEST_ADDRESS_SELECT,
            actor_cond = if incoming { RECEIVED_ACTOR_COND } else { SENT_ACTOR_COND },
            blocking = BLOCKING_CONDITION,
        );
        let rows = sqlx::query(&sql)
            .bind(&addr)
            .bind(limit)
            .bind(offset)
            .fetch_all(&self.pool)
            .await?;
        Ok(rows
            .into_iter()
            .map(|r| {
                let metadata: Option<serde_json::Value> = r.try_get("metadata").ok();
                let message = metadata
                    .as_ref()
                    .and_then(|m| m.get("message"))
                    .and_then(|m| m.as_str())
                    .map(|s| s.to_string());
                FriendshipRequestRow {
                    id: r.get::<Uuid, _>("id"),
                    address: r.get::<String, _>("address"),
                    // social DB timestamps are TIMESTAMP WITHOUT TIME ZONE —
                    // decoding straight to DateTime<Utc> panics (ColumnDecode).
                    timestamp: r.get::<chrono::NaiveDateTime, _>("ts").and_utc(),
                    message,
                }
            })
            .collect())
    }

    pub async fn count_friendship_requests(
        &self,
        address: &str,
        incoming: bool,
    ) -> Result<i64, DbError> {
        let addr = address.to_lowercase();
        let sql = format!(
            r#"
            SELECT COUNT(1) AS n
            FROM friendship_actions fa
            JOIN friendships f ON f.id = fa.friendship_id AND f.is_active IS FALSE
            WHERE fa.action = 'request'
              AND {actor_cond}
              AND NOT EXISTS (
                SELECT 1 FROM friendship_actions newer
                WHERE newer.friendship_id = fa.friendship_id
                  AND newer.timestamp > fa.timestamp
              )
              AND {blocking}
            "#,
            actor_cond = if incoming { RECEIVED_ACTOR_COND } else { SENT_ACTOR_COND },
            blocking = BLOCKING_CONDITION,
        );
        let row = sqlx::query(&sql).bind(&addr).fetch_one(&self.pool).await?;
        Ok(row.get::<i64, _>("n"))
    }

    pub async fn last_friendship_action(
        &self,
        a: &str,
        b: &str,
    ) -> Result<Option<LastAction>, DbError> {
        let a = a.to_lowercase();
        let b = b.to_lowercase();
        let row = sqlx::query(
            r#"
            SELECT f.id AS friendship_id, f.is_active AS is_active,
                   fa.action AS action, fa.acting_user AS acting_user
            FROM friendships f
            LEFT JOIN LATERAL (
              SELECT action, acting_user FROM friendship_actions
              WHERE friendship_id = f.id ORDER BY timestamp DESC LIMIT 1
            ) fa ON TRUE
            WHERE (f.address_requester = $1 AND f.address_requested = $2)
               OR (f.address_requester = $2 AND f.address_requested = $1)
            LIMIT 1
            "#,
        )
        .bind(&a)
        .bind(&b)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| LastAction {
            friendship_id: r.get::<Uuid, _>("friendship_id"),
            action: r.try_get::<String, _>("action").unwrap_or_default(),
            acting_user: r.try_get::<String, _>("acting_user").unwrap_or_default(),
            is_active: r.get::<bool, _>("is_active"),
        }))
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn apply_friendship_action(
        &self,
        acting_user: &str,
        other: &str,
        action: &str,
        is_active: bool,
        existing: Option<Uuid>,
        message: Option<&str>,
    ) -> Result<(Uuid, DateTime<Utc>), DbError> {
        let acting_user = acting_user.to_lowercase();
        let other = other.to_lowercase();
        let mut tx = self.pool.begin().await?;

        let friendship_id = match existing {
            Some(id) => {
                sqlx::query(
                    r#"UPDATE friendships SET is_active = $1, updated_at = now() WHERE id = $2"#,
                )
                .bind(is_active)
                .bind(id)
                .execute(&mut *tx)
                .await?;
                id
            }
            None => {
                let id = Uuid::new_v4();
                let insert = sqlx::query(
                    r#"INSERT INTO friendships (id, address_requester, address_requested, is_active)
                       VALUES ($1, $2, $3, $4)"#,
                )
                .bind(id)
                .bind(&acting_user)
                .bind(&other)
                .bind(is_active)
                .execute(&mut *tx)
                .await;

                match insert {
                    Ok(_) => id,
                    // Concurrency guard: a simultaneous request for the SAME unordered
                    // pair (the classic A->B / B->A race) may have inserted the row
                    // between our caller's `last_friendship_action` lookup and this
                    // INSERT. The `friendships_unordered_pair` unique index turns that
                    // into a unique violation here. Instead of 500ing (and leaving a
                    // duplicate), resolve the row the other request created — locking
                    // it FOR UPDATE — and apply our action to it, so the pair keeps
                    // exactly one friendship row.
                    Err(sqlx::Error::Database(db_err)) if db_err.is_unique_violation() => {
                        let existing_id: Uuid = sqlx::query(
                            r#"SELECT id FROM friendships
                               WHERE (address_requester = $1 AND address_requested = $2)
                                  OR (address_requester = $2 AND address_requested = $1)
                               LIMIT 1
                               FOR UPDATE"#,
                        )
                        .bind(&acting_user)
                        .bind(&other)
                        .fetch_one(&mut *tx)
                        .await?
                        .get::<Uuid, _>("id");

                        sqlx::query(
                            r#"UPDATE friendships SET is_active = $1, updated_at = now() WHERE id = $2"#,
                        )
                        .bind(is_active)
                        .bind(existing_id)
                        .execute(&mut *tx)
                        .await?;
                        existing_id
                    }
                    Err(e) => return Err(e.into()),
                }
            }
        };

        let metadata = message.map(|m| serde_json::json!({ "message": m }));
        let row = sqlx::query(
            r#"INSERT INTO friendship_actions (id, friendship_id, action, acting_user, metadata)
               VALUES ($1, $2, $3, $4, $5) RETURNING timestamp"#,
        )
        .bind(Uuid::new_v4())
        .bind(friendship_id)
        .bind(action)
        .bind(&acting_user)
        .bind(metadata)
        .fetch_one(&mut *tx)
        .await?;
        // TIMESTAMP WITHOUT TIME ZONE column — see note at the "ts" decode.
        let created_at: DateTime<Utc> = row.get::<chrono::NaiveDateTime, _>("timestamp").and_utc();

        tx.commit().await?;
        Ok((friendship_id, created_at))
    }

    pub async fn is_friendship_blocked(&self, a: &str, b: &str) -> Result<bool, DbError> {
        let a = a.to_lowercase();
        let b = b.to_lowercase();
        let row = sqlx::query(
            r#"SELECT EXISTS (
                 SELECT 1 FROM blocks
                 WHERE (blocker_address = $1 AND blocked_address = $2)
                    OR (blocker_address = $2 AND blocked_address = $1)
               ) AS blocked"#,
        )
        .bind(&a)
        .bind(&b)
        .fetch_one(&self.pool)
        .await?;
        Ok(row.get::<bool, _>("blocked"))
    }

    pub async fn is_blocked(&self, blocker: &str, blocked: &str) -> Result<bool, DbError> {
        let row = sqlx::query(
            r#"SELECT 1 FROM blocks WHERE blocker_address = $1 AND blocked_address = $2 LIMIT 1"#,
        )
        .bind(blocker.to_lowercase())
        .bind(blocked.to_lowercase())
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.is_some())
    }

    pub async fn block_user(&self, blocker: &str, blocked: &str) -> Result<(), DbError> {
        sqlx::query(
            r#"INSERT INTO blocks (id, blocker_address, blocked_address)
               VALUES ($1, $2, $3)
               ON CONFLICT (blocker_address, blocked_address) DO NOTHING"#,
        )
        .bind(Uuid::new_v4())
        .bind(blocker.to_lowercase())
        .bind(blocked.to_lowercase())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn unblock_user(&self, blocker: &str, blocked: &str) -> Result<(), DbError> {
        sqlx::query(r#"DELETE FROM blocks WHERE blocker_address = $1 AND blocked_address = $2"#)
            .bind(blocker.to_lowercase())
            .bind(blocked.to_lowercase())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn get_blocked_users(
        &self,
        blocker: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<BlockedRow>, DbError> {
        let rows = sqlx::query(
            r#"SELECT blocked_address, blocked_at FROM blocks
               WHERE blocker_address = $1 ORDER BY blocked_at DESC LIMIT $2 OFFSET $3"#,
        )
        .bind(blocker.to_lowercase())
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| BlockedRow {
                address: r.get::<String, _>("blocked_address"),
                blocked_at: r.get::<chrono::NaiveDateTime, _>("blocked_at").and_utc(),
            })
            .collect())
    }

    pub async fn count_blocked_users(&self, blocker: &str) -> Result<i64, DbError> {
        let row = sqlx::query(r#"SELECT COUNT(*) AS n FROM blocks WHERE blocker_address = $1"#)
            .bind(blocker.to_lowercase())
            .fetch_one(&self.pool)
            .await?;
        Ok(row.get::<i64, _>("n"))
    }

    pub async fn get_blocking_status(
        &self,
        address: &str,
    ) -> Result<(Vec<String>, Vec<String>), DbError> {
        let addr = address.to_lowercase();
        let blocked = sqlx::query(r#"SELECT blocked_address FROM blocks WHERE blocker_address = $1"#)
            .bind(&addr)
            .fetch_all(&self.pool)
            .await?
            .into_iter()
            .map(|r| r.get::<String, _>("blocked_address"))
            .collect();
        let blocked_by =
            sqlx::query(r#"SELECT blocker_address FROM blocks WHERE blocked_address = $1"#)
                .bind(&addr)
                .fetch_all(&self.pool)
                .await?
                .into_iter()
                .map(|r| r.get::<String, _>("blocker_address"))
                .collect();
        Ok((blocked, blocked_by))
    }

    pub async fn get_social_settings(
        &self,
        address: &str,
    ) -> Result<Option<SocialSettingsRow>, DbError> {
        let row = sqlx::query(
            r#"SELECT private_messages_privacy, blocked_users_messages_visibility,
                      show_situation_reactions
               FROM social_settings WHERE address = $1"#,
        )
        .bind(address.to_lowercase())
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| SocialSettingsRow {
            private_messages_privacy: r.get::<String, _>("private_messages_privacy"),
            blocked_users_messages_visibility: r
                .get::<String, _>("blocked_users_messages_visibility"),
            show_situation_reactions: r.get::<String, _>("show_situation_reactions"),
        }))
    }

    pub async fn upsert_social_settings(
        &self,
        address: &str,
        private_messages_privacy: Option<&str>,
        blocked_users_messages_visibility: Option<&str>,
        show_situation_reactions: Option<&str>,
    ) -> Result<SocialSettingsRow, DbError> {
        let addr = address.to_lowercase();

        let row = sqlx::query(
            r#"
            INSERT INTO social_settings (address, private_messages_privacy,
                   blocked_users_messages_visibility, show_situation_reactions)
            VALUES ($1,
                    COALESCE($2, 'only_friends'),
                    COALESCE($3, 'show_messages'),
                    COALESCE($4, 'show'))
            ON CONFLICT (address) DO UPDATE SET
              private_messages_privacy = COALESCE($2, social_settings.private_messages_privacy),
              blocked_users_messages_visibility =
                COALESCE($3, social_settings.blocked_users_messages_visibility),
              show_situation_reactions =
                COALESCE($4, social_settings.show_situation_reactions)
            RETURNING private_messages_privacy, blocked_users_messages_visibility,
                      show_situation_reactions
            "#,
        )
        .bind(&addr)
        .bind(private_messages_privacy)
        .bind(blocked_users_messages_visibility)
        .bind(show_situation_reactions)
        .fetch_one(&self.pool)
        .await?;
        Ok(SocialSettingsRow {
            private_messages_privacy: row.get::<String, _>("private_messages_privacy"),
            blocked_users_messages_visibility: row
                .get::<String, _>("blocked_users_messages_visibility"),
            show_situation_reactions: row.get::<String, _>("show_situation_reactions"),
        })
    }

    pub async fn private_messages_settings(
        &self,
        caller: &str,
        targets: &[String],
    ) -> Result<Vec<(String, String, bool)>, DbError> {
        let caller = caller.to_lowercase();
        let targets: Vec<String> = targets.iter().map(|t| t.to_lowercase()).collect();
        if targets.is_empty() {
            return Ok(Vec::new());
        }

        let privacy_rows = sqlx::query(
            r#"SELECT address, private_messages_privacy
               FROM social_settings WHERE address = ANY($1)"#,
        )
        .bind(&targets)
        .fetch_all(&self.pool)
        .await?;
        let mut privacy: std::collections::HashMap<String, String> = privacy_rows
            .into_iter()
            .map(|r| {
                (
                    r.get::<String, _>("address"),
                    r.get::<String, _>("private_messages_privacy"),
                )
            })
            .collect();

        let friend_rows = sqlx::query(
            r#"
            SELECT CASE WHEN address_requester = $1 THEN address_requested
                        ELSE address_requester END AS friend
            FROM friendships
            WHERE is_active = TRUE
              AND ($1 IN (address_requester, address_requested))
              AND (CASE WHEN address_requester = $1 THEN address_requested
                        ELSE address_requester END) = ANY($2)
              AND NOT EXISTS (
                SELECT 1 FROM blocks b
                WHERE (b.blocker_address = $1 AND b.blocked_address = CASE
                          WHEN address_requester = $1 THEN address_requested
                          ELSE address_requester END)
                   OR (b.blocked_address = $1 AND b.blocker_address = CASE
                          WHEN address_requester = $1 THEN address_requested
                          ELSE address_requester END))
            "#,
        )
        .bind(&caller)
        .bind(&targets)
        .fetch_all(&self.pool)
        .await?;
        let friends: std::collections::HashSet<String> = friend_rows
            .into_iter()
            .map(|r| r.get::<String, _>("friend"))
            .collect();

        Ok(targets
            .into_iter()
            .map(|t| {
                let p = privacy.remove(&t).unwrap_or_else(|| "only_friends".into());
                let is_friend = friends.contains(&t);
                (t, p, is_friend)
            })
            .collect())
    }

    pub async fn start_private_voice_chat(
        &self,
        caller: &str,
        callee: &str,
        ttl_secs: i64,
    ) -> Result<Uuid, DbError> {
        let id = Uuid::new_v4();
        let expires_at = Utc::now() + ChronoDuration::seconds(ttl_secs);
        sqlx::query(
            r#"INSERT INTO private_voice_chats (id, caller_address, callee_address, expires_at)
               VALUES ($1, $2, $3, $4)"#,
        )
        .bind(id)
        .bind(caller.to_lowercase())
        .bind(callee.to_lowercase())
        .bind(expires_at)
        .execute(&self.pool)
        .await?;
        Ok(id)
    }

    pub async fn get_private_voice_chat(
        &self,
        call_id: Uuid,
    ) -> Result<Option<PrivateVoiceChatRow>, DbError> {
        let row = sqlx::query(
            r#"SELECT id, caller_address, callee_address FROM private_voice_chats
               WHERE id = $1 AND expires_at > now()"#,
        )
        .bind(call_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| PrivateVoiceChatRow {
            id: r.get::<Uuid, _>("id"),
            caller_address: r.get::<String, _>("caller_address"),
            callee_address: r.get::<String, _>("callee_address"),
        }))
    }

    pub async fn incoming_private_voice_chat(
        &self,
        callee: &str,
    ) -> Result<Option<PrivateVoiceChatRow>, DbError> {
        let row = sqlx::query(
            r#"SELECT id, caller_address, callee_address FROM private_voice_chats
               WHERE callee_address = $1 AND expires_at > now()
               ORDER BY created_at DESC LIMIT 1"#,
        )
        .bind(callee.to_lowercase())
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| PrivateVoiceChatRow {
            id: r.get::<Uuid, _>("id"),
            caller_address: r.get::<String, _>("caller_address"),
            callee_address: r.get::<String, _>("callee_address"),
        }))
    }

    /// Admin read: every non-expired 1:1 voice chat, newest first. Backs the
    /// admin "active voice calls" view.
    pub async fn list_active_private_voice_chats(
        &self,
        limit: i64,
    ) -> Result<Vec<(Uuid, String, String, DateTime<Utc>, DateTime<Utc>)>, DbError> {
        let rows = sqlx::query(
            r#"SELECT id, caller_address, callee_address, created_at, expires_at
               FROM private_voice_chats
               WHERE expires_at > now()
               ORDER BY created_at DESC
               LIMIT $1"#,
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| {
                (
                    r.get::<Uuid, _>("id"),
                    r.get::<String, _>("caller_address"),
                    r.get::<String, _>("callee_address"),
                    r.get::<chrono::NaiveDateTime, _>("created_at").and_utc(),
                    r.get::<chrono::NaiveDateTime, _>("expires_at").and_utc(),
                )
            })
            .collect())
    }

    /// Admin mutation: reset a user's social settings to the schema defaults by
    /// deleting their row (reads fall back to `SocialSettingsRow::default`).
    /// Returns whether a row existed.
    pub async fn reset_social_settings(&self, address: &str) -> Result<bool, DbError> {
        let res = sqlx::query(r#"DELETE FROM social_settings WHERE address = $1"#)
            .bind(address.to_lowercase())
            .execute(&self.pool)
            .await?;
        Ok(res.rows_affected() > 0)
    }

    pub async fn delete_private_voice_chat(&self, call_id: Uuid) -> Result<(), DbError> {
        sqlx::query(r#"DELETE FROM private_voice_chats WHERE id = $1"#)
            .bind(call_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn community_role(
        &self,
        community_id: &str,
        address: &str,
    ) -> Result<Option<String>, DbError> {
        let cid = match Uuid::parse_str(community_id) {
            Ok(u) => u,
            Err(_) => return Ok(None),
        };
        let row = sqlx::query(
            r#"SELECT role FROM community_members WHERE community_id = $1 AND member_address = $2"#,
        )
        .bind(cid)
        .bind(address.to_lowercase())
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| r.get::<String, _>("role")))
    }

    pub async fn community_name(&self, community_id: &str) -> Result<Option<String>, DbError> {
        let cid = match Uuid::parse_str(community_id) {
            Ok(u) => u,
            Err(_) => return Ok(None),
        };
        let row = sqlx::query(r#"SELECT name FROM communities WHERE id = $1"#)
            .bind(cid)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.map(|r| r.get::<String, _>("name")))
    }

    pub async fn friend_addresses(&self, address: &str) -> Result<Vec<String>, DbError> {
        let addr = address.to_lowercase();
        let rows = sqlx::query(
            r#"SELECT CASE WHEN address_requester = $1 THEN address_requested
                           ELSE address_requester END AS friend
               FROM friendships
               WHERE is_active = TRUE AND ($1 IN (address_requester, address_requested))"#,
        )
        .bind(&addr)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(|r| r.get::<String, _>("friend")).collect())
    }

    pub async fn online_friends(
        &self,
        address: &str,
        online_candidates: &[String],
    ) -> Result<Vec<String>, DbError> {
        if online_candidates.is_empty() {
            return Ok(Vec::new());
        }
        let addr = address.to_lowercase();
        let candidates: Vec<String> =
            online_candidates.iter().map(|c| c.to_lowercase()).collect();
        let rows = sqlx::query(
            r#"
            SELECT DISTINCT CASE WHEN address_requester = $1 THEN address_requested
                                 ELSE address_requester END AS friend
            FROM friendships
            WHERE is_active = TRUE
              AND ($1 IN (address_requester, address_requested))
              AND (CASE WHEN address_requester = $1 THEN address_requested
                        ELSE address_requester END) = ANY($2)
              AND NOT EXISTS (
                SELECT 1 FROM blocks b
                WHERE (b.blocker_address = $1 AND b.blocked_address = CASE
                          WHEN address_requester = $1 THEN address_requested
                          ELSE address_requester END)
                   OR (b.blocked_address = $1 AND b.blocker_address = CASE
                          WHEN address_requester = $1 THEN address_requested
                          ELSE address_requester END))
            "#,
        )
        .bind(&addr)
        .bind(&candidates)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(|r| r.get::<String, _>("friend")).collect())
    }

    pub async fn communities_for_member(&self, address: &str) -> Result<Vec<String>, DbError> {
        let rows = sqlx::query(
            r#"SELECT community_id FROM community_members WHERE member_address = $1"#,
        )
        .bind(address.to_lowercase())
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| r.get::<Uuid, _>("community_id").to_string())
            .collect())
    }

    pub async fn community_member_addresses(
        &self,
        community_id: &str,
    ) -> Result<Vec<String>, DbError> {
        let cid = match Uuid::parse_str(community_id) {
            Ok(u) => u,
            Err(_) => return Ok(Vec::new()),
        };
        let rows = sqlx::query(
            r#"SELECT member_address FROM community_members WHERE community_id = $1"#,
        )
        .bind(cid)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(|r| r.get::<String, _>("member_address")).collect())
    }
}
