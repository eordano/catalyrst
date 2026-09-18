use chrono::{DateTime, Duration as ChronoDuration, Utc};
use sqlx::{PgPool, Row};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;
use uuid::Uuid;

#[derive(Clone)]
pub struct Db {
    pool: PgPool,
    /// Unix ms before which no private voice chat known to this process can expire; the
    /// periodic sweep issues no DELETE until then.
    voice_due: Arc<AtomicI64>,
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

/// `last_friendship_action` for one pair, plus whether either side blocks the other.
#[derive(Debug, Clone, Default)]
pub struct FriendshipProbe {
    pub last: Option<LastAction>,
    pub blocked: bool,
    pub blocked_by: bool,
}

const LAST_ACTION_SELECT: &str = r#"
SELECT f.id AS friendship_id, f.is_active AS is_active,
       fa.action AS action, fa.acting_user AS acting_user
FROM friendships f
LEFT JOIN LATERAL (
  SELECT action, acting_user FROM friendship_actions
  WHERE friendship_id = f.id ORDER BY timestamp DESC LIMIT 1
) fa ON TRUE
WHERE (f.address_requester = $1 AND f.address_requested = $2)
   OR (f.address_requester = $2 AND f.address_requested = $1)
LIMIT 1"#;

fn last_action_from_row(r: &sqlx::postgres::PgRow) -> LastAction {
    LastAction {
        friendship_id: r.get::<Uuid, _>("friendship_id"),
        action: r.try_get::<String, _>("action").unwrap_or_default(),
        acting_user: r.try_get::<String, _>("acting_user").unwrap_or_default(),
        is_active: r.try_get::<bool, _>("is_active").unwrap_or(false),
    }
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
            private_messages_privacy: "all".into(),
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

/// Everything `start_private_voice_chat` reads before it inserts, in one statement.
#[derive(Debug, Clone)]
pub struct PrivateVoicePreflight {
    pub blocked: bool,
    pub caller_privacy: String,
    pub callee_privacy: String,
    /// `None` when no friendship row exists in either direction.
    pub friends: Option<bool>,
    pub busy: bool,
}

impl Db {
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            voice_due: Arc::new(AtomicI64::new(0)),
        }
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    /// A call expiring at `due_ms` exists: the sweep runs no later than that.
    pub fn note_private_voice_due(&self, due_ms: i64) {
        self.voice_due.fetch_min(due_ms, Ordering::Relaxed);
    }

    pub fn private_voice_sweep_due(&self, now_ms: i64) -> bool {
        self.voice_due.load(Ordering::Relaxed) <= now_ms
    }

    /// Takes the pending due mark before a sweep; calls started meanwhile re-arm it.
    pub fn claim_private_voice_sweep(&self) {
        self.voice_due.store(i64::MAX, Ordering::Relaxed);
    }

    /// Milliseconds, by the database clock, until the earliest call expires (negative when
    /// one already has); `None` when the table is empty.
    pub async fn private_voice_next_expiry_ms(
        &self,
        expiration_ms: i64,
    ) -> Result<Option<i64>, DbError> {
        let remaining: Option<i64> = sqlx::query_scalar(
            r#"SELECT (EXTRACT(EPOCH FROM (MIN(created_at) + ($1 * interval '1 millisecond')
                                          - now()::timestamp)) * 1000)::bigint
               FROM private_voice_chats"#,
        )
        .bind(expiration_ms)
        .fetch_one(&self.pool)
        .await?;
        Ok(remaining)
    }

    /// One page of friends plus the full count (`COUNT(*) OVER ()`, re-counted only when
    /// an offset lands past the end).
    pub async fn get_friends(
        &self,
        address: &str,
        limit: i64,
        offset: i64,
    ) -> Result<(Vec<String>, i64), DbError> {
        let addr = address.to_lowercase();
        let rows = sqlx::query(
            r#"
            SELECT CASE WHEN address_requester = $1 THEN address_requested
                        ELSE address_requester END AS friend,
                   COUNT(*) OVER () AS total
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
        let total = match rows.first() {
            Some(r) => r.get::<i64, _>("total"),
            None if offset > 0 => self.count_friends(&addr).await?,
            None => 0,
        };
        Ok((
            rows.into_iter()
                .map(|r| r.get::<String, _>("friend"))
                .collect(),
            total,
        ))
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
    ) -> Result<(Vec<String>, i64), DbError> {
        let a = a.to_lowercase();
        let b = b.to_lowercase();
        let rows = sqlx::query(
            r#"
            SELECT f1.friend, COUNT(*) OVER () AS total FROM (
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
        let total = match rows.first() {
            Some(r) => r.get::<i64, _>("total"),
            None if offset > 0 => self.count_mutual_friends(&a, &b).await?,
            None => 0,
        };
        Ok((
            rows.into_iter()
                .map(|r| r.get::<String, _>("friend"))
                .collect(),
            total,
        ))
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
    ) -> Result<(Vec<FriendshipRequestRow>, i64), DbError> {
        let addr = address.to_lowercase();
        let sql = format!(
            r#"
            SELECT f.id AS id,
                   {address_select},
                   fa.timestamp AS ts,
                   fa.metadata AS metadata,
                   COUNT(*) OVER () AS total
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
            actor_cond = if incoming {
                RECEIVED_ACTOR_COND
            } else {
                SENT_ACTOR_COND
            },
            blocking = BLOCKING_CONDITION,
        );
        let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(&addr)
            .bind(limit)
            .bind(offset)
            .fetch_all(&self.pool)
            .await?;
        let total = match rows.first() {
            Some(r) => r.get::<i64, _>("total"),
            None if offset > 0 => self.count_friendship_requests(&addr, incoming).await?,
            None => 0,
        };
        let page = rows
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

                    timestamp: r.get::<chrono::NaiveDateTime, _>("ts").and_utc(),
                    message,
                }
            })
            .collect();
        Ok((page, total))
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
            actor_cond = if incoming {
                RECEIVED_ACTOR_COND
            } else {
                SENT_ACTOR_COND
            },
            blocking = BLOCKING_CONDITION,
        );
        let row = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(&addr)
            .fetch_one(&self.pool)
            .await?;
        Ok(row.get::<i64, _>("n"))
    }

    pub async fn last_friendship_action(
        &self,
        a: &str,
        b: &str,
    ) -> Result<Option<LastAction>, DbError> {
        let a = a.to_lowercase();
        let b = b.to_lowercase();
        let row = sqlx::query(LAST_ACTION_SELECT)
            .bind(&a)
            .bind(&b)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.as_ref().map(last_action_from_row))
    }

    /// The pair's friendship row with its latest action plus both block directions, in
    /// one statement.
    pub async fn friendship_probe(
        &self,
        me: &str,
        other: &str,
    ) -> Result<FriendshipProbe, DbError> {
        let me = me.to_lowercase();
        let other = other.to_lowercase();
        let row = sqlx::query(
            r#"
            SELECT f.id AS friendship_id, f.is_active AS is_active,
                   fa.action AS action, fa.acting_user AS acting_user,
                   EXISTS (SELECT 1 FROM blocks
                           WHERE blocker_address = $1 AND blocked_address = $2) AS blocked,
                   EXISTS (SELECT 1 FROM blocks
                           WHERE blocker_address = $2 AND blocked_address = $1) AS blocked_by
            FROM (SELECT 1) AS one
            LEFT JOIN LATERAL (
              SELECT id, is_active FROM friendships
              WHERE (address_requester = $1 AND address_requested = $2)
                 OR (address_requester = $2 AND address_requested = $1)
              LIMIT 1
            ) f ON TRUE
            LEFT JOIN LATERAL (
              SELECT action, acting_user FROM friendship_actions
              WHERE friendship_id = f.id ORDER BY timestamp DESC LIMIT 1
            ) fa ON TRUE
            "#,
        )
        .bind(&me)
        .bind(&other)
        .fetch_one(&self.pool)
        .await?;
        Ok(FriendshipProbe {
            last: row
                .get::<Option<Uuid>, _>("friendship_id")
                .map(|_| last_action_from_row(&row)),
            blocked: row.get::<bool, _>("blocked"),
            blocked_by: row.get::<bool, _>("blocked_by"),
        })
    }

    /// Records the block and returns the pair's latest friendship action in one statement.
    pub async fn block_user_and_last(
        &self,
        blocker: &str,
        blocked: &str,
    ) -> Result<Option<LastAction>, DbError> {
        let sql = format!(
            r#"WITH ins AS (
                 INSERT INTO blocks (id, blocker_address, blocked_address) VALUES ($3, $1, $2)
                 ON CONFLICT (blocker_address, blocked_address) DO NOTHING
               )
               {LAST_ACTION_SELECT}"#
        );
        let row = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(blocker.to_lowercase())
            .bind(blocked.to_lowercase())
            .bind(Uuid::new_v4())
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.as_ref().map(last_action_from_row))
    }

    /// Removes the block and returns the pair's latest friendship action in one statement.
    pub async fn unblock_user_and_last(
        &self,
        blocker: &str,
        blocked: &str,
    ) -> Result<Option<LastAction>, DbError> {
        let sql = format!(
            r#"WITH del AS (
                 DELETE FROM blocks WHERE blocker_address = $1 AND blocked_address = $2
               )
               {LAST_ACTION_SELECT}"#
        );
        let row = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(blocker.to_lowercase())
            .bind(blocked.to_lowercase())
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.as_ref().map(last_action_from_row))
    }

    /// Upserts the friendship row and appends the action in one statement; a concurrent
    /// insert on the same unordered pair resolves through `friendships_unordered_pair`.
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
        let metadata = message.map(|m| serde_json::json!({ "message": m }));
        let row = match existing {
            Some(id) => {
                sqlx::query(
                    r#"WITH f AS (
                         UPDATE friendships SET is_active = $1, updated_at = now()
                         WHERE id = $2 RETURNING id
                       )
                       INSERT INTO friendship_actions (id, friendship_id, action, acting_user, metadata)
                       SELECT $3::uuid, f.id, $4::text, $5::text, $6::json FROM f
                       RETURNING friendship_id, timestamp"#,
                )
                .bind(is_active)
                .bind(id)
                .bind(Uuid::new_v4())
                .bind(action)
                .bind(&acting_user)
                .bind(metadata)
                .fetch_one(&self.pool)
                .await?
            }
            None => {
                sqlx::query(
                    r#"WITH f AS (
                         INSERT INTO friendships (id, address_requester, address_requested, is_active)
                         VALUES ($1, $2, $3, $4)
                         ON CONFLICT ((LEAST(address_requester, address_requested)),
                                      (GREATEST(address_requester, address_requested)))
                         DO UPDATE SET is_active = EXCLUDED.is_active, updated_at = now()
                         RETURNING id
                       )
                       INSERT INTO friendship_actions (id, friendship_id, action, acting_user, metadata)
                       SELECT $5::uuid, f.id, $6::text, $7::text, $8::json FROM f
                       RETURNING friendship_id, timestamp"#,
                )
                .bind(Uuid::new_v4())
                .bind(&acting_user)
                .bind(&other)
                .bind(is_active)
                .bind(Uuid::new_v4())
                .bind(action)
                .bind(&acting_user)
                .bind(metadata)
                .fetch_one(&self.pool)
                .await?
            }
        };
        Ok((
            row.get::<Uuid, _>("friendship_id"),
            row.get::<chrono::NaiveDateTime, _>("timestamp").and_utc(),
        ))
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
    ) -> Result<(Vec<BlockedRow>, i64), DbError> {
        let blocker = blocker.to_lowercase();
        let rows = sqlx::query(
            r#"SELECT blocked_address, blocked_at, COUNT(*) OVER () AS total FROM blocks
               WHERE blocker_address = $1
               ORDER BY blocked_at DESC, blocked_address ASC LIMIT $2 OFFSET $3"#,
        )
        .bind(&blocker)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;
        let total = match rows.first() {
            Some(r) => r.get::<i64, _>("total"),
            None if offset > 0 => self.count_blocked_users(&blocker).await?,
            None => 0,
        };
        let page = rows
            .into_iter()
            .map(|r| BlockedRow {
                address: r.get::<String, _>("blocked_address"),
                blocked_at: r.get::<chrono::NaiveDateTime, _>("blocked_at").and_utc(),
            })
            .collect();
        Ok((page, total))
    }

    pub async fn count_blocked_users(&self, blocker: &str) -> Result<i64, DbError> {
        let row = sqlx::query(r#"SELECT COUNT(*) AS c FROM blocks WHERE blocker_address = $1"#)
            .bind(blocker.to_lowercase())
            .fetch_one(&self.pool)
            .await?;
        Ok(row.get::<i64, _>("c"))
    }

    pub async fn get_blocking_status(
        &self,
        address: &str,
    ) -> Result<(Vec<String>, Vec<String>), DbError> {
        let addr = address.to_lowercase();
        let rows = sqlx::query(
            r#"SELECT blocked_address AS address, TRUE AS outgoing FROM blocks
               WHERE blocker_address = $1
               UNION ALL
               SELECT blocker_address, FALSE FROM blocks WHERE blocked_address = $1"#,
        )
        .bind(&addr)
        .fetch_all(&self.pool)
        .await?;
        let (mut blocked, mut blocked_by) = (Vec::new(), Vec::new());
        for r in rows {
            let address = r.get::<String, _>("address");
            if r.get::<bool, _>("outgoing") {
                blocked.push(address);
            } else {
                blocked_by.push(address);
            }
        }
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

        let rows = sqlx::query(
            r#"
            SELECT t.addr AS address,
                   COALESCE(s.private_messages_privacy, 'all') AS privacy,
                   EXISTS (
                     SELECT 1 FROM friendships f
                     WHERE f.is_active = TRUE
                       AND ((f.address_requester = $1 AND f.address_requested = t.addr)
                         OR (f.address_requester = t.addr AND f.address_requested = $1))
                       AND NOT EXISTS (
                         SELECT 1 FROM blocks b
                         WHERE (b.blocker_address = $1 AND b.blocked_address = t.addr)
                            OR (b.blocked_address = $1 AND b.blocker_address = t.addr))
                   ) AS is_friend
            FROM unnest($2::text[]) WITH ORDINALITY AS t(addr, ord)
            LEFT JOIN social_settings s ON s.address = t.addr
            ORDER BY t.ord
            "#,
        )
        .bind(&caller)
        .bind(&targets)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| {
                (
                    r.get::<String, _>("address"),
                    r.get::<String, _>("privacy"),
                    r.get::<bool, _>("is_friend"),
                )
            })
            .collect())
    }

    pub async fn are_users_being_called_or_calling_someone(
        &self,
        addresses: &[String],
        expiration_ms: i64,
    ) -> Result<bool, DbError> {
        let normalized: Vec<String> = addresses.iter().map(|a| a.to_lowercase()).collect();
        let row = sqlx::query(
            r#"SELECT EXISTS (
                 SELECT 1 FROM private_voice_chats
                 WHERE created_at >= (now()::timestamp - ($2 * interval '1 millisecond'))
                   AND (caller_address = ANY($1) OR callee_address = ANY($1))
               ) AS exists"#,
        )
        .bind(&normalized)
        .bind(expiration_ms)
        .fetch_one(&self.pool)
        .await?;
        Ok(row.get::<bool, _>("exists"))
    }

    pub async fn friendship_is_active(&self, a: &str, b: &str) -> Result<Option<bool>, DbError> {
        let a = a.to_lowercase();
        let b = b.to_lowercase();
        let row = sqlx::query(
            r#"SELECT is_active FROM friendships
               WHERE (address_requester = $1 AND address_requested = $2)
                  OR (address_requester = $2 AND address_requested = $1)
               LIMIT 1"#,
        )
        .bind(&a)
        .bind(&b)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| r.get::<bool, _>("is_active")))
    }

    pub async fn private_voice_preflight(
        &self,
        caller: &str,
        callee: &str,
        expiration_ms: i64,
    ) -> Result<PrivateVoicePreflight, DbError> {
        let caller = caller.to_lowercase();
        let callee = callee.to_lowercase();
        let row = sqlx::query(
            r#"SELECT
                 EXISTS (
                   SELECT 1 FROM blocks
                   WHERE (blocker_address = $1 AND blocked_address = $2)
                      OR (blocker_address = $2 AND blocked_address = $1)
                 ) AS blocked,
                 (SELECT private_messages_privacy FROM social_settings WHERE address = $1)
                   AS caller_privacy,
                 (SELECT private_messages_privacy FROM social_settings WHERE address = $2)
                   AS callee_privacy,
                 (SELECT is_active FROM friendships
                  WHERE (address_requester = $1 AND address_requested = $2)
                     OR (address_requester = $2 AND address_requested = $1)
                  LIMIT 1) AS friends,
                 EXISTS (
                   SELECT 1 FROM private_voice_chats
                   WHERE created_at >= (now()::timestamp - ($3 * interval '1 millisecond'))
                     AND (caller_address IN ($1, $2) OR callee_address IN ($1, $2))
                 ) AS busy"#,
        )
        .bind(&caller)
        .bind(&callee)
        .bind(expiration_ms)
        .fetch_one(&self.pool)
        .await?;
        Ok(PrivateVoicePreflight {
            blocked: row.get::<bool, _>("blocked"),
            caller_privacy: row
                .get::<Option<String>, _>("caller_privacy")
                .unwrap_or_else(|| "all".into()),
            callee_privacy: row
                .get::<Option<String>, _>("callee_privacy")
                .unwrap_or_else(|| "all".into()),
            friends: row.get::<Option<bool>, _>("friends"),
            busy: row.get::<bool, _>("busy"),
        })
    }

    pub async fn start_private_voice_chat(
        &self,
        caller: &str,
        callee: &str,
    ) -> Result<Uuid, DbError> {
        let id = Uuid::new_v4();
        sqlx::query(
            r#"INSERT INTO private_voice_chats (id, caller_address, callee_address)
               VALUES ($1, $2, $3)"#,
        )
        .bind(id)
        .bind(caller.to_lowercase())
        .bind(callee.to_lowercase())
        .execute(&self.pool)
        .await?;
        self.note_private_voice_due(0);
        Ok(id)
    }

    /// `None` if either participant is busy (upstream #450). The plain
    /// [`Self::start_private_voice_chat`] plus a preceding
    /// [`Self::are_users_being_called_or_calling_someone`] leave a TOCTOU window: two
    /// concurrent starts sharing a participant can both pass and both insert. Per-participant
    /// advisory locks taken in **sorted order** serialise every start touching either address;
    /// `pg_advisory_xact_lock` releases on commit/rollback.
    pub async fn start_private_voice_chat_if_free(
        &self,
        caller: &str,
        callee: &str,
        expiration_ms: i64,
    ) -> Result<Option<Uuid>, DbError> {
        let caller = caller.to_lowercase();
        let callee = callee.to_lowercase();
        let mut participants = [caller.clone(), callee.clone()];
        participants.sort();
        let participants = participants.to_vec();

        let mut tx = self.pool.begin().await?;
        // unnest yields the sorted array in order, so the locks are taken in sorted order.
        sqlx::query(
            r#"SELECT pg_advisory_xact_lock(hashtext('private-voice:' || p))
               FROM unnest($1::text[]) AS t(p)"#,
        )
        .bind(&participants)
        .execute(&mut *tx)
        .await?;
        let inserted: Option<Uuid> = sqlx::query_scalar(
            r#"INSERT INTO private_voice_chats (id, caller_address, callee_address)
               SELECT $1::uuid, $2::text, $3::text
               WHERE NOT EXISTS (
                 SELECT 1 FROM private_voice_chats
                 WHERE created_at >= (now()::timestamp - ($5 * interval '1 millisecond'))
                   AND (caller_address = ANY($4) OR callee_address = ANY($4))
               )
               RETURNING id"#,
        )
        .bind(Uuid::new_v4())
        .bind(&caller)
        .bind(&callee)
        .bind(&participants)
        .bind(expiration_ms)
        .fetch_optional(&mut *tx)
        .await?;
        match inserted {
            Some(id) => {
                tx.commit().await?;
                self.note_private_voice_due(Utc::now().timestamp_millis() + expiration_ms);
                Ok(Some(id))
            }
            None => {
                tx.rollback().await?;
                Ok(None)
            }
        }
    }

    pub async fn expire_private_voice_chats(
        &self,
        expiration_ms: i64,
        limit: i64,
    ) -> Result<Vec<(Uuid, String, String)>, DbError> {
        let rows = sqlx::query(
            r#"DELETE FROM private_voice_chats WHERE id IN (
                 SELECT id FROM private_voice_chats
                 WHERE created_at < (now()::timestamp - ($1 * interval '1 millisecond'))
                 ORDER BY created_at ASC
                 LIMIT $2
               )
               RETURNING id, caller_address, callee_address"#,
        )
        .bind(expiration_ms)
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
                )
            })
            .collect())
    }

    pub async fn get_private_voice_chat(
        &self,
        call_id: Uuid,
    ) -> Result<Option<PrivateVoiceChatRow>, DbError> {
        let row = sqlx::query(
            r#"SELECT id, caller_address, callee_address FROM private_voice_chats
               WHERE id = $1"#,
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
               WHERE callee_address = $1
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

    /// The pending call this address is on, on *either* side. Used to end a call when its
    /// party disconnects (upstream #479).
    pub async fn get_private_voice_chat_of_user(
        &self,
        address: &str,
    ) -> Result<Option<PrivateVoiceChatRow>, DbError> {
        let addr = address.to_lowercase();
        let row = sqlx::query(
            r#"SELECT id, caller_address, callee_address FROM private_voice_chats
               WHERE caller_address = $1 OR callee_address = $1
               ORDER BY created_at DESC LIMIT 1"#,
        )
        .bind(&addr)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| PrivateVoiceChatRow {
            id: r.get::<Uuid, _>("id"),
            caller_address: r.get::<String, _>("caller_address"),
            callee_address: r.get::<String, _>("callee_address"),
        }))
    }

    pub async fn list_active_private_voice_chats(
        &self,
        limit: i64,
        expiration_ms: i64,
    ) -> Result<Vec<(Uuid, String, String, DateTime<Utc>, DateTime<Utc>)>, DbError> {
        let rows = sqlx::query(
            r#"SELECT id, caller_address, callee_address, created_at
               FROM private_voice_chats
               WHERE created_at >= (now()::timestamp - ($2 * interval '1 millisecond'))
               ORDER BY created_at DESC
               LIMIT $1"#,
        )
        .bind(limit)
        .bind(expiration_ms)
        .fetch_all(&self.pool)
        .await?;
        let ttl = ChronoDuration::milliseconds(expiration_ms);
        Ok(rows
            .into_iter()
            .map(|r| {
                let created_at = r.get::<chrono::NaiveDateTime, _>("created_at").and_utc();
                (
                    r.get::<Uuid, _>("id"),
                    r.get::<String, _>("caller_address"),
                    r.get::<String, _>("callee_address"),
                    created_at,
                    created_at + ttl,
                )
            })
            .collect())
    }

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

    /// The deleted row, or `None` when no such call existed.
    pub async fn delete_private_voice_chat_returning(
        &self,
        call_id: Uuid,
    ) -> Result<Option<PrivateVoiceChatRow>, DbError> {
        let row = sqlx::query(
            r#"DELETE FROM private_voice_chats WHERE id = $1
               RETURNING id, caller_address, callee_address"#,
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

    /// Deletes the call this address is on, on either side, returning it. The disconnect
    /// half of [`Self::get_private_voice_chat_of_user`].
    pub async fn delete_private_voice_chat_of_user(
        &self,
        address: &str,
    ) -> Result<Option<PrivateVoiceChatRow>, DbError> {
        let row = sqlx::query(
            r#"DELETE FROM private_voice_chats WHERE id = (
                 SELECT id FROM private_voice_chats
                 WHERE caller_address = $1 OR callee_address = $1
                 ORDER BY created_at DESC LIMIT 1
               )
               RETURNING id, caller_address, callee_address"#,
        )
        .bind(address.to_lowercase())
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| PrivateVoiceChatRow {
            id: r.get::<Uuid, _>("id"),
            caller_address: r.get::<String, _>("caller_address"),
            callee_address: r.get::<String, _>("callee_address"),
        }))
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
            r#"SELECT role FROM community_members
               WHERE community_id = $1 AND member_address = $2
                 AND EXISTS (SELECT 1 FROM communities c WHERE c.id = $1 AND c.active = true)"#,
        )
        .bind(cid)
        .bind(address.to_lowercase())
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| r.get::<String, _>("role")))
    }

    /// Live ban status from `community_bans`, the record both the client REST ban and the
    /// federation apply path write. `community_role` alone cannot answer this: a ban deletes
    /// the membership row, so a role captured before the ban is the stale value being raced.
    pub async fn is_member_banned(
        &self,
        community_id: &str,
        address: &str,
    ) -> Result<bool, DbError> {
        let cid = match Uuid::parse_str(community_id) {
            Ok(u) => u,
            Err(_) => return Ok(false),
        };
        let row = sqlx::query(
            r#"SELECT active FROM community_bans WHERE community_id = $1 AND banned_address = $2"#,
        )
        .bind(cid)
        .bind(address.to_lowercase())
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| r.get::<bool, _>("active")).unwrap_or(false))
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

    /// `None` when the community row does not exist. Parses the id as a canonical UUID like
    /// the sibling voice-chat reads in this module, not via the hex helper that
    /// `rest::fed::authority::community_is_private` uses.
    pub async fn community_is_private(&self, community_id: &str) -> Result<Option<bool>, DbError> {
        let cid = match Uuid::parse_str(community_id) {
            Ok(u) => u,
            Err(_) => return Ok(None),
        };
        let row = sqlx::query(r#"SELECT private FROM communities WHERE id = $1"#)
            .bind(cid)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.map(|r| r.get::<bool, _>("private")))
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
        Ok(rows
            .into_iter()
            .map(|r| r.get::<String, _>("friend"))
            .collect())
    }

    /// Active friends with no block in either direction.
    pub async fn friend_addresses_unblocked(&self, address: &str) -> Result<Vec<String>, DbError> {
        let addr = address.to_lowercase();
        let rows = sqlx::query(
            r#"
            SELECT CASE WHEN address_requester = $1 THEN address_requested
                        ELSE address_requester END AS friend
            FROM friendships
            WHERE is_active = TRUE
              AND ($1 IN (address_requester, address_requested))
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
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| r.get::<String, _>("friend"))
            .collect())
    }

    pub async fn communities_for_member(&self, address: &str) -> Result<Vec<String>, DbError> {
        let rows =
            sqlx::query(r#"SELECT community_id FROM community_members WHERE member_address = $1"#)
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
        let rows =
            sqlx::query(r#"SELECT member_address FROM community_members WHERE community_id = $1"#)
                .bind(cid)
                .fetch_all(&self.pool)
                .await?;
        Ok(rows
            .into_iter()
            .map(|r| r.get::<String, _>("member_address"))
            .collect())
    }

    /// `(private, role of a, role of b)` in one read; `None` when the community row is
    /// missing. Roles are `None` for an inactive community, like [`Self::community_role`].
    pub async fn community_voice_roles(
        &self,
        community_id: &str,
        a: &str,
        b: &str,
    ) -> Result<Option<(bool, Option<String>, Option<String>)>, DbError> {
        let cid = match Uuid::parse_str(community_id) {
            Ok(u) => u,
            Err(_) => return Ok(None),
        };
        let row = sqlx::query(
            r#"SELECT c.private,
                      CASE WHEN c.active THEN (
                        SELECT role FROM community_members
                        WHERE community_id = c.id AND member_address = $2) END AS role_a,
                      CASE WHEN c.active THEN (
                        SELECT role FROM community_members
                        WHERE community_id = c.id AND member_address = $3) END AS role_b
               FROM communities c WHERE c.id = $1"#,
        )
        .bind(cid)
        .bind(a.to_lowercase())
        .bind(b.to_lowercase())
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| {
            (
                r.get::<bool, _>("private"),
                r.get::<Option<String>, _>("role_a"),
                r.get::<Option<String>, _>("role_b"),
            )
        }))
    }

    /// `(role, banned)` of one address; `None` when the community row is missing. The role
    /// is `None` for an inactive community, like [`Self::community_role`]; `banned` is the
    /// live `community_bans` status [`Self::is_member_banned`] reads.
    pub async fn community_membership(
        &self,
        community_id: &str,
        address: &str,
    ) -> Result<Option<(Option<String>, bool)>, DbError> {
        let cid = match Uuid::parse_str(community_id) {
            Ok(u) => u,
            Err(_) => return Ok(None),
        };
        let row = sqlx::query(
            r#"SELECT CASE WHEN c.active THEN m.role END AS role,
                      COALESCE(b.active, FALSE) AS banned
               FROM communities c
               LEFT JOIN community_members m
                 ON m.community_id = c.id AND m.member_address = $2
               LEFT JOIN community_bans b
                 ON b.community_id = c.id AND b.banned_address = $2
               WHERE c.id = $1"#,
        )
        .bind(cid)
        .bind(address.to_lowercase())
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| {
            (
                r.get::<Option<String>, _>("role"),
                r.get::<bool, _>("banned"),
            )
        }))
    }

    /// `(name, member addresses)` for a voice fan-out; `None` when the community row is
    /// missing.
    pub async fn community_voice_fanout(
        &self,
        community_id: &str,
    ) -> Result<Option<(String, Vec<String>)>, DbError> {
        let cid = match Uuid::parse_str(community_id) {
            Ok(u) => u,
            Err(_) => return Ok(None),
        };
        let row = sqlx::query(
            r#"SELECT c.name,
                      COALESCE((SELECT array_agg(member_address::text) FROM community_members
                                WHERE community_id = c.id), '{}'::text[]) AS members
               FROM communities c WHERE c.id = $1"#,
        )
        .bind(cid)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| {
            (
                r.get::<String, _>("name"),
                r.get::<Vec<String>, _>("members"),
            )
        }))
    }

    /// `(role of the actor, name, member addresses)` for starting a voice chat; the member
    /// list is only aggregated when the actor holds the owner or moderator role.
    pub async fn community_voice_start(
        &self,
        community_id: &str,
        address: &str,
    ) -> Result<Option<(Option<String>, String, Vec<String>)>, DbError> {
        let cid = match Uuid::parse_str(community_id) {
            Ok(u) => u,
            Err(_) => return Ok(None),
        };
        let row = sqlx::query(
            r#"SELECT c.name,
                      CASE WHEN c.active THEN m.role END AS role,
                      CASE WHEN c.active AND m.role IN ('owner', 'moderator') THEN
                        COALESCE((SELECT array_agg(member_address::text) FROM community_members
                                  WHERE community_id = c.id), '{}'::text[])
                      ELSE '{}'::text[] END AS members
               FROM communities c
               LEFT JOIN community_members m
                 ON m.community_id = c.id AND m.member_address = $2
               WHERE c.id = $1"#,
        )
        .bind(cid)
        .bind(address.to_lowercase())
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| {
            (
                r.get::<Option<String>, _>("role"),
                r.get::<String, _>("name"),
                r.get::<Vec<String>, _>("members"),
            )
        }))
    }

    /// `(community_id, member_address)` for every other member of every community this
    /// address belongs to.
    pub async fn community_co_members(
        &self,
        address: &str,
    ) -> Result<Vec<(String, String)>, DbError> {
        let rows = sqlx::query(
            r#"SELECT m.community_id, m.member_address FROM community_members m
               WHERE m.member_address <> $1
                 AND m.community_id IN (
                   SELECT community_id FROM community_members WHERE member_address = $1)"#,
        )
        .bind(address.to_lowercase())
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| {
                (
                    r.get::<Uuid, _>("community_id").to_string(),
                    r.get::<String, _>("member_address"),
                )
            })
            .collect())
    }
}
