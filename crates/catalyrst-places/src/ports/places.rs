use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::{postgres::PgPool, Row};

use crate::http::errors::ApiError;

const PLACE_COLUMNS: &str = r#"
    id, title, description, raw->>'image' AS image,
    creator_address AS owner,
    creator_address,
    COALESCE((SELECT array_agg(p::text) FROM jsonb_array_elements_text(raw->'positions') p), ARRAY[]::text[]) AS positions,
    base_position,
    raw->>'contact_name' AS contact_name,
    raw->>'contact_email' AS contact_email,
    content_rating,
    disabled,
    NULLIF(raw->>'disabled_at','')::timestamptz AS disabled_at,
    raw->>'disabled_reason' AS disabled_reason,
    NULLIF(raw->>'created_at','')::timestamptz AS created_at,
    NULLIF(raw->>'updated_at','')::timestamptz AS updated_at,
    favorites, likes, dislikes, categories,
    COALESCE((SELECT array_agg(t::text) FROM jsonb_array_elements_text(raw->'tags') t), ARRAY[]::text[]) AS tags,
    highlighted,
    raw->>'highlighted_image' AS highlighted_image,
    NULLIF(raw->>'ranking','')::float8 AS ranking,
    raw->>'sdk' AS sdk,
    deployed_at,
    COALESCE((raw->>'world')::bool, false) AS world,
    raw->>'world_name' AS world_name,
    raw->>'world_id' AS world_id,
    COALESCE((raw->>'is_private')::bool, false) AS is_private,
    COALESCE((raw->>'user_favorite')::bool, false) AS user_favorite,
    COALESCE((raw->>'user_like')::bool, false) AS user_like,
    COALESCE((raw->>'user_dislike')::bool, false) AS user_dislike,
    NULLIF(raw->>'user_count','')::int AS user_count,
    COALESCE(NULLIF(raw->>'user_visits','')::int, 0) AS user_visits,
    NULLIF(raw->>'like_rate','')::float8 AS like_rate,
    NULLIF(raw->>'like_score','')::float8 AS like_score
"#;

#[derive(Debug, Clone, Serialize)]
pub struct PlaceRow {
    pub id: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub image: Option<String>,
    pub owner: Option<String>,
    pub positions: Vec<String>,
    pub base_position: String,
    pub contact_name: Option<String>,
    pub contact_email: Option<String>,
    pub content_rating: Option<String>,
    pub disabled: bool,
    pub disabled_at: Option<DateTime<Utc>>,
    pub disabled_reason: Option<String>,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
    pub favorites: i32,
    pub likes: i32,
    pub dislikes: i32,
    pub categories: Vec<String>,
    pub tags: Vec<String>,
    pub highlighted: bool,
    pub highlighted_image: Option<String>,
    pub ranking: Option<f64>,
    pub sdk: Option<String>,
    pub creator_address: Option<String>,
    pub world_id: Option<String>,
    pub deployed_at: Option<DateTime<Utc>>,
    pub world: bool,
    pub world_name: Option<String>,
    pub is_private: bool,
    pub user_favorite: bool,
    pub user_like: bool,
    pub user_dislike: bool,
    pub user_count: Option<i32>,
    pub user_visits: i32,
    pub like_rate: Option<f64>,
    pub like_score: Option<f64>,
    // `live` and `connected_addresses` are NOT part of the base Place wire
    // (upstream `PlaceAttributes` has neither). They are computed on the
    // `/destinations` path only, gated on `with_live_events` /
    // `with_connected_users`, by `destinationsWithAggregates`. Modeled as
    // `Option` + `skip_serializing_if` so they are omitted everywhere else.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub live: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub connected_addresses: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub realms_detail: Option<Vec<serde_json::Value>>,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct UserInteraction {
    pub user_favorite: bool,
    pub user_like: bool,
    pub user_dislike: bool,
}

#[derive(Debug, Default)]
pub struct PlaceListFilters {
    pub limit: i64,
    pub offset: i64,
    pub positions: Vec<String>,
    pub names: Vec<String>,
    pub categories: Vec<String>,
    pub only_highlighted: bool,
    pub search: Option<String>,
    pub creator_address: Option<String>,
    pub sdk: Option<String>,
    pub order_by: PlaceOrderBy,
    pub order_desc: bool,
    pub ids: Vec<String>,
    pub only_worlds: bool,
    pub only_places: bool,
    pub operated_positions: Vec<String>,
}

#[derive(Debug, Clone, Copy, Default)]
pub enum PlaceOrderBy {
    #[default]
    LikeScore,
    UpdatedAt,
    CreatedAt,
    UserVisits,
    MostActive,
}

impl PlaceOrderBy {
    pub fn parse(s: Option<&str>) -> Self {
        match s {
            Some("updated_at") => Self::UpdatedAt,
            Some("created_at") => Self::CreatedAt,
            Some("user_visits") => Self::UserVisits,
            Some("most_active") => Self::MostActive,
            _ => Self::LikeScore,
        }
    }
    fn column(self) -> &'static str {
        match self {
            Self::LikeScore => "NULLIF(raw->>'like_score','')::float8",
            Self::UpdatedAt => "NULLIF(raw->>'updated_at','')::timestamptz",
            Self::CreatedAt => "NULLIF(raw->>'created_at','')::timestamptz",
            Self::UserVisits => "COALESCE(NULLIF(raw->>'user_visits','')::int, 0)",
            Self::MostActive => "COALESCE(NULLIF(raw->>'user_count','')::int, 0)",
        }
    }
}

pub struct PlacesComponent {
    pool: PgPool,
    writer: Option<PgPool>,
    squid: Option<PgPool>,
    squid_schema: String,
}

impl PlacesComponent {
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            writer: None,
            squid: None,
            squid_schema: "squid_marketplace".to_string(),
        }
    }

    pub fn with_squid(mut self, squid: PgPool, schema: String) -> Self {
        self.squid = Some(squid);
        self.squid_schema = schema;
        self
    }

    pub fn with_writer(mut self, writer: PgPool) -> Self {
        self.writer = Some(writer);
        self
    }

    pub fn has_writer(&self) -> bool {
        self.writer.is_some()
    }

    /// The local-interaction writer pool (favorites/likes/reports + the
    /// federation signed-action log live here). `None` when no writer is
    /// configured (reads still work; writes degrade to 503/no-op).
    pub fn writer_pool(&self) -> Option<&PgPool> {
        self.writer.as_ref()
    }

    pub async fn ensure_local_schema(&self) -> Result<(), ApiError> {
        let Some(writer) = &self.writer else {
            return Ok(());
        };
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS user_favorites (
                "user" text NOT NULL,
                entity_id text NOT NULL,
                user_activity double precision NOT NULL DEFAULT 0,
                created_at timestamptz NOT NULL DEFAULT now(),
                PRIMARY KEY ("user", entity_id)
            )
            "#,
        )
        .execute(writer)
        .await?;
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS user_favorites_entity_idx ON user_favorites (entity_id)",
        )
        .execute(writer)
        .await?;
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS user_likes (
                "user" text NOT NULL,
                entity_id text NOT NULL,
                "like" boolean NOT NULL,
                user_activity double precision NOT NULL DEFAULT 0,
                created_at timestamptz NOT NULL DEFAULT now(),
                updated_at timestamptz NOT NULL DEFAULT now(),
                PRIMARY KEY ("user", entity_id)
            )
            "#,
        )
        .execute(writer)
        .await?;
        sqlx::query("CREATE INDEX IF NOT EXISTS user_likes_entity_idx ON user_likes (entity_id)")
            .execute(writer)
            .await?;
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS place_reports_local (
                id bigserial PRIMARY KEY,
                entity_id text,
                reporter text NOT NULL,
                signed_url text NOT NULL,
                filename text NOT NULL,
                payload jsonb NOT NULL DEFAULT '{}'::jsonb,
                created_at timestamptz NOT NULL DEFAULT now()
            )
            "#,
        )
        .execute(writer)
        .await?;
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS place_reports_local_reporter_idx ON place_reports_local (reporter)",
        )
        .execute(writer)
        .await?;

        // Moderation-queue state (admin-console.md §4). Additive columns so the
        // existing report write path is untouched; status defaults to 'open'.
        for ddl in [
            "ALTER TABLE place_reports_local ADD COLUMN IF NOT EXISTS status text NOT NULL DEFAULT 'open'",
            "ALTER TABLE place_reports_local ADD COLUMN IF NOT EXISTS resolution text",
            "ALTER TABLE place_reports_local ADD COLUMN IF NOT EXISTS moderator_notes text",
            "ALTER TABLE place_reports_local ADD COLUMN IF NOT EXISTS resolved_by text",
            "ALTER TABLE place_reports_local ADD COLUMN IF NOT EXISTS resolved_at timestamptz",
        ] {
            sqlx::query(ddl).execute(writer).await?;
        }
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS place_reports_local_status_idx ON place_reports_local (status, created_at DESC)",
        )
        .execute(writer)
        .await?;
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS place_reports_local_entity_idx ON place_reports_local (entity_id)",
        )
        .execute(writer)
        .await?;

        // POI registry (admin-console.md §4: POI CRUD). Owned by this crate's
        // writer DB; positions are catalyst "x,y" base-position strings.
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS pois (
                position    text PRIMARY KEY,
                entity_id   text,
                title       text,
                description text,
                enabled     boolean NOT NULL DEFAULT true,
                created_by  text,
                created_at  timestamptz NOT NULL DEFAULT now(),
                updated_at  timestamptz NOT NULL DEFAULT now()
            )
            "#,
        )
        .execute(writer)
        .await?;

        // Federation signed-action log (docs/federation/places.md §2). The
        // log is the canonical, replicable record of place opinions; the
        // existing user_favorites / user_likes tables are the materialised
        // "current" view (place.md's place_favorites_current /
        // place_vote_score_current, realised here over the legacy schema).
        // origin_peer NULL == produced by a local client.
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS signed_actions_places (
                signature_hash  text PRIMARY KEY,
                signer          text NOT NULL,
                place_id        text NOT NULL,
                action_type     text NOT NULL,
                domain_hash     text NOT NULL DEFAULT '',
                message_payload jsonb NOT NULL,
                signed_at       bigint NOT NULL,
                received_at     bigint NOT NULL,
                origin_peer     text,
                seq             bigserial UNIQUE NOT NULL
            )
            "#,
        )
        .execute(writer)
        .await?;
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_sap_signer ON signed_actions_places (signer, action_type, signed_at DESC)",
        )
        .execute(writer)
        .await?;
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_sap_place ON signed_actions_places (place_id, action_type, signed_at DESC)",
        )
        .execute(writer)
        .await?;
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_sap_seq ON signed_actions_places (seq)")
            .execute(writer)
            .await?;

        // Per-signer replay nonce store (00-primitives.md §2.2). Mirrors the
        // communities crate's seen_nonces table.
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS seen_nonces (
                signer     text NOT NULL,
                nonce      text NOT NULL,
                expires_at bigint NOT NULL,
                PRIMARY KEY (signer, nonce)
            )
            "#,
        )
        .execute(writer)
        .await?;
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_seen_nonces_expires ON seen_nonces (expires_at)",
        )
        .execute(writer)
        .await?;
        Ok(())
    }

    /// Append a signed place opinion to the federation log (idempotent on
    /// signature_hash). Returns false if the row already existed (dedup).
    pub async fn record_signed_action(
        &self,
        signature_hash: &str,
        signer: &str,
        place_id: &str,
        action_type: &str,
        payload: &serde_json::Value,
        signed_at: i64,
        origin_peer: Option<&str>,
    ) -> Result<bool, ApiError> {
        let Some(writer) = self.writer.as_ref() else {
            return Ok(true);
        };
        let now = chrono::Utc::now().timestamp();
        let res = sqlx::query(
            r#"INSERT INTO signed_actions_places
                 (signature_hash, signer, place_id, action_type, message_payload, signed_at, received_at, origin_peer)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8)
               ON CONFLICT (signature_hash) DO NOTHING"#,
        )
        .bind(signature_hash)
        .bind(signer.to_lowercase())
        .bind(place_id)
        .bind(action_type)
        .bind(payload)
        .bind(signed_at)
        .bind(now)
        .bind(origin_peer)
        .execute(writer)
        .await?;
        Ok(res.rows_affected() > 0)
    }

    pub async fn user_interactions(
        &self,
        user: &str,
        entity_ids: &[String],
    ) -> Option<std::collections::HashMap<String, UserInteraction>> {
        let writer = self.writer.as_ref()?;
        if entity_ids.is_empty() {
            return Some(std::collections::HashMap::new());
        }
        let user = user.to_lowercase();
        let mut map: std::collections::HashMap<String, UserInteraction> =
            std::collections::HashMap::new();
        let fav_rows = sqlx::query(
            r#"SELECT entity_id FROM user_favorites WHERE lower("user") = $1 AND entity_id = ANY($2)"#,
        )
        .bind(&user)
        .bind(entity_ids)
        .fetch_all(writer)
        .await
        .ok()?;
        for r in fav_rows {
            map.entry(r.get::<String, _>("entity_id"))
                .or_default()
                .user_favorite = true;
        }
        let like_rows = sqlx::query(
            r#"SELECT entity_id, "like" FROM user_likes WHERE lower("user") = $1 AND entity_id = ANY($2)"#,
        )
        .bind(&user)
        .bind(entity_ids)
        .fetch_all(writer)
        .await
        .ok()?;
        for r in like_rows {
            let e = map.entry(r.get::<String, _>("entity_id")).or_default();
            if r.get::<bool, _>("like") {
                e.user_like = true;
            } else {
                e.user_dislike = true;
            }
        }
        Some(map)
    }

    pub async fn apply_user_interactions(&self, user: Option<&str>, rows: &mut [PlaceRow]) {
        let Some(user) = user else { return };
        if self.writer.is_none() || rows.is_empty() {
            return;
        }
        let ids: Vec<String> = rows.iter().map(|r| r.id.clone()).collect();
        let Some(map) = self.user_interactions(user, &ids).await else {
            return;
        };
        for row in rows.iter_mut() {
            if let Some(i) = map.get(&row.id) {
                row.user_favorite = i.user_favorite;
                row.user_like = i.user_like;
                row.user_dislike = i.user_dislike;
            }
        }
    }

    pub async fn set_favorite(
        &self,
        entity_id: &str,
        user: &str,
        favorite: bool,
        current_count: i32,
        current_favorite: bool,
    ) -> Result<(i32, bool), ApiError> {
        let Some(writer) = self.writer.as_ref() else {
            let count = if favorite == current_favorite {
                current_count
            } else if favorite {
                current_count + 1
            } else {
                (current_count - 1).max(0)
            };
            return Ok((count, favorite));
        };
        let user = user.to_lowercase();
        if favorite {
            sqlx::query(
                r#"INSERT INTO user_favorites ("user", entity_id, created_at)
                   VALUES ($1, $2, now())
                   ON CONFLICT ("user", entity_id) DO NOTHING"#,
            )
            .bind(&user)
            .bind(entity_id)
            .execute(writer)
            .await?;
        } else {
            sqlx::query(
                r#"DELETE FROM user_favorites WHERE lower("user") = $1 AND entity_id = $2"#,
            )
            .bind(&user)
            .bind(entity_id)
            .execute(writer)
            .await?;
        }
        let row = sqlx::query("SELECT count(*)::int AS c FROM user_favorites WHERE entity_id = $1")
            .bind(entity_id)
            .fetch_one(writer)
            .await?;
        let count = row.get::<i32, _>("c");
        let _ = sqlx::query("UPDATE place SET favorites = $2 WHERE id = $1")
            .bind(entity_id)
            .bind(count)
            .execute(writer)
            .await;
        Ok((count, favorite))
    }

    pub async fn set_like(
        &self,
        entity_id: &str,
        user: &str,
        like: Option<bool>,
        user_activity: f64,
        current_likes: i32,
        current_dislikes: i32,
        current_user_like: bool,
        current_user_dislike: bool,
    ) -> Result<(i32, i32, bool, bool), ApiError> {
        let Some(writer) = self.writer.as_ref() else {
            let mut likes = current_likes;
            let mut dislikes = current_dislikes;
            if current_user_like {
                likes = (likes - 1).max(0);
            }
            if current_user_dislike {
                dislikes = (dislikes - 1).max(0);
            }
            let (user_like, user_dislike) = match like {
                Some(true) => {
                    likes += 1;
                    (true, false)
                }
                Some(false) => {
                    dislikes += 1;
                    (false, true)
                }
                None => (false, false),
            };
            return Ok((likes, dislikes, user_like, user_dislike));
        };
        let user = user.to_lowercase();
        match like {
            None => {
                sqlx::query(
                    r#"DELETE FROM user_likes WHERE lower("user") = $1 AND entity_id = $2"#,
                )
                .bind(&user)
                .bind(entity_id)
                .execute(writer)
                .await?;
            }
            Some(value) => {
                // Mirrors UserLikesModel.like: persist the like together with the
                // voter's Snapshot voting power (`user_activity`) so the Wilson
                // ranking below can weight active voters only.
                sqlx::query(
                    r#"INSERT INTO user_likes ("user", entity_id, "like", user_activity, created_at, updated_at)
                       VALUES ($1, $2, $3, $4, now(), now())
                       ON CONFLICT ("user", entity_id)
                       DO UPDATE SET "like" = EXCLUDED."like", user_activity = EXCLUDED.user_activity, updated_at = now()"#,
                )
                .bind(&user)
                .bind(entity_id)
                .bind(value)
                .bind(user_activity)
                .execute(writer)
                .await?;
            }
        }
        // Port of buildUpdateLikesQuery (entityInteractions.ts): recompute
        // likes/dislikes plus the VP-weighted like_rate and Wilson-score
        // like_score in one UPDATE. Only votes with user_activity >=
        // MIN_USER_ACTIVITY (100) count toward the active rate/score.
        //
        // likes/dislikes are real columns; like_rate/like_score are read by the
        // API (and the default like_score sort) from the `raw` JSON blob, so
        // they are written back into `raw` via jsonb_set — like set_ranking —
        // and must round-trip null when there are no active votes.
        let row = sqlx::query(
            r#"
            WITH counted AS (
              SELECT
                count(*) filter (where "like") as count_likes,
                count(*) filter (where not "like") as count_dislikes,
                count(*) filter (where user_activity >= $2) as count_active_total,
                count(*) filter (where "like" and user_activity >= $2) as count_active_likes,
                count(*) filter (where not "like" and user_activity >= $2) as count_active_dislikes
              FROM user_likes
              WHERE entity_id = $1
            ), computed AS (
              SELECT
                count_likes,
                count_dislikes,
                (CASE WHEN count_active_total::float = 0 THEN NULL
                      ELSE count_active_likes / count_active_total::float
                 END) AS like_rate,
                (CASE WHEN (count_active_likes + count_active_dislikes > 0) THEN
                    ((count_active_likes + 1.9208)
                    / (count_active_likes + count_active_dislikes) - 1.96
                    * SQRT((count_active_likes * count_active_dislikes) / (count_active_likes + count_active_dislikes) + 0.9604)
                    / (count_active_likes + count_active_dislikes))
                    / (1 + 3.8416 / (count_active_likes + count_active_dislikes))
                 ELSE NULL END) AS like_score
              FROM counted
            )
            UPDATE place
            SET
              likes = c.count_likes::int,
              dislikes = c.count_dislikes::int,
              raw = jsonb_set(
                      jsonb_set(
                        COALESCE(raw, '{}'::jsonb),
                        '{like_rate}',
                        CASE WHEN c.like_rate IS NULL THEN 'null'::jsonb ELSE to_jsonb(c.like_rate) END,
                        true
                      ),
                      '{like_score}',
                      CASE WHEN c.like_score IS NULL THEN 'null'::jsonb ELSE to_jsonb(c.like_score) END,
                      true
                    )
            FROM computed c
            WHERE id = $1
            RETURNING c.count_likes::int AS likes, c.count_dislikes::int AS dislikes
            "#,
        )
        .bind(entity_id)
        .bind(crate::snapshot::MIN_USER_ACTIVITY)
        .fetch_optional(writer)
        .await?;
        // RETURNING is empty when the place row is absent (e.g. not yet synced
        // locally); fall back to the freshly recomputed counts so the response
        // still reflects the vote.
        let (likes, dislikes) = match row {
            Some(r) => (r.get::<i32, _>("likes"), r.get::<i32, _>("dislikes")),
            None => {
                let r = sqlx::query(
                    r#"SELECT
                         count(*) FILTER (WHERE "like") ::int AS likes,
                         count(*) FILTER (WHERE NOT "like")::int AS dislikes
                       FROM user_likes WHERE entity_id = $1"#,
                )
                .bind(entity_id)
                .fetch_one(writer)
                .await?;
                (r.get::<i32, _>("likes"), r.get::<i32, _>("dislikes"))
            }
        };
        let (user_like, user_dislike) = match like {
            Some(true) => (true, false),
            Some(false) => (false, true),
            None => (false, false),
        };
        Ok((likes, dislikes, user_like, user_dislike))
    }

    pub async fn favorite_entity_ids(&self, user: &str) -> Result<Option<Vec<String>>, ApiError> {
        let Some(writer) = &self.writer else {
            return Ok(None);
        };
        let user = user.to_lowercase();
        let rows = sqlx::query(r#"SELECT entity_id FROM user_favorites WHERE lower("user") = $1"#)
            .bind(&user)
            .fetch_all(writer)
            .await?;
        Ok(Some(
            rows.into_iter()
                .map(|r| r.get::<String, _>("entity_id"))
                .collect(),
        ))
    }

    pub async fn record_report(
        &self,
        entity_id: Option<&str>,
        reporter: &str,
        signed_url: &str,
        filename: &str,
        payload: &serde_json::Value,
    ) -> Result<(), ApiError> {
        let Some(writer) = self.writer.as_ref() else {
            return Ok(());
        };
        sqlx::query(
            r#"INSERT INTO place_reports_local (entity_id, reporter, signed_url, filename, payload)
               VALUES ($1, $2, $3, $4, $5)"#,
        )
        .bind(entity_id)
        .bind(reporter.to_lowercase())
        .bind(signed_url)
        .bind(filename)
        .bind(payload)
        .execute(writer)
        .await?;
        Ok(())
    }

    pub async fn record_report_upload(
        &self,
        filename: &str,
        payload: &serde_json::Value,
    ) -> Result<(), ApiError> {
        let Some(writer) = self.writer.as_ref() else {
            return Ok(());
        };
        sqlx::query(r#"UPDATE place_reports_local SET payload = $2 WHERE filename = $1"#)
            .bind(filename)
            .bind(payload)
            .execute(writer)
            .await?;
        Ok(())
    }

    pub async fn set_highlighted(
        &self,
        entity_id: &str,
        highlighted: bool,
    ) -> Result<(), ApiError> {
        let Some(writer) = self.writer.as_ref() else {
            return Ok(());
        };
        sqlx::query("UPDATE place SET highlighted = $2 WHERE id = $1")
            .bind(entity_id)
            .bind(highlighted)
            .execute(writer)
            .await?;
        Ok(())
    }

    pub async fn set_ranking(&self, entity_id: &str, ranking: Option<f64>) -> Result<(), ApiError> {
        let Some(writer) = self.writer.as_ref() else {
            return Ok(());
        };
        let raw_value = match ranking {
            Some(v) => serde_json::Value::from(v),
            None => serde_json::Value::Null,
        };
        sqlx::query(
            "UPDATE place SET raw = jsonb_set(COALESCE(raw, '{}'::jsonb), '{ranking}', $2, true) WHERE id = $1",
        )
        .bind(entity_id)
        .bind(raw_value)
        .execute(writer)
        .await?;
        Ok(())
    }

    pub async fn set_content_rating(
        &self,
        entity_id: &str,
        content_rating: &str,
    ) -> Result<(), ApiError> {
        let Some(writer) = self.writer.as_ref() else {
            return Ok(());
        };
        sqlx::query("UPDATE place SET content_rating = $2 WHERE id = $1")
            .bind(entity_id)
            .bind(content_rating)
            .execute(writer)
            .await?;
        Ok(())
    }

    /// Moderation queue: list local place reports, newest first, optionally
    /// filtered by status and/or entity_id. Requires the writer pool; returns
    /// 503 when reports are not persisted here.
    pub async fn list_reports(
        &self,
        status: Option<&str>,
        entity_id: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<ReportRow>, ApiError> {
        let writer = self
            .writer
            .as_ref()
            .ok_or_else(|| ApiError::service_unavailable("report persistence not configured"))?;
        let rows = sqlx::query(
            r#"
            SELECT id, entity_id, reporter, signed_url, filename, payload,
                   status, resolution, moderator_notes, resolved_by,
                   resolved_at, created_at
            FROM place_reports_local
            WHERE ($1::text IS NULL OR status = $1)
              AND ($2::text IS NULL OR entity_id = $2)
            ORDER BY created_at DESC, id DESC
            LIMIT $3 OFFSET $4
            "#,
        )
        .bind(status)
        .bind(entity_id)
        .bind(limit.clamp(1, 200))
        .bind(offset.max(0))
        .fetch_all(writer)
        .await?;
        Ok(rows.into_iter().map(row_to_report).collect())
    }

    pub async fn count_reports(
        &self,
        status: Option<&str>,
        entity_id: Option<&str>,
    ) -> Result<i64, ApiError> {
        let writer = self
            .writer
            .as_ref()
            .ok_or_else(|| ApiError::service_unavailable("report persistence not configured"))?;
        let row = sqlx::query(
            r#"SELECT count(*)::bigint AS total FROM place_reports_local
               WHERE ($1::text IS NULL OR status = $1)
                 AND ($2::text IS NULL OR entity_id = $2)"#,
        )
        .bind(status)
        .bind(entity_id)
        .fetch_one(writer)
        .await?;
        Ok(row.get::<i64, _>("total"))
    }

    pub async fn get_report(&self, id: i64) -> Result<Option<ReportRow>, ApiError> {
        let writer = self
            .writer
            .as_ref()
            .ok_or_else(|| ApiError::service_unavailable("report persistence not configured"))?;
        let row = sqlx::query(
            r#"
            SELECT id, entity_id, reporter, signed_url, filename, payload,
                   status, resolution, moderator_notes, resolved_by,
                   resolved_at, created_at
            FROM place_reports_local WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(writer)
        .await?;
        Ok(row.map(row_to_report))
    }

    /// Resolve/dismiss a report. `status` is the new state (e.g. "resolved",
    /// "dismissed", "open"); `resolution`/`notes`/`resolved_by` are optional
    /// audit metadata. Returns the updated row, or `None` if no such report.
    pub async fn update_report_status(
        &self,
        id: i64,
        status: &str,
        resolution: Option<&str>,
        notes: Option<&str>,
        resolved_by: Option<&str>,
    ) -> Result<Option<ReportRow>, ApiError> {
        let writer = self
            .writer
            .as_ref()
            .ok_or_else(|| ApiError::service_unavailable("report persistence not configured"))?;
        let resolved_at_now = !status.eq_ignore_ascii_case("open");
        let row = sqlx::query(
            r#"
            UPDATE place_reports_local
            SET status = $2,
                resolution = COALESCE($3, resolution),
                moderator_notes = COALESCE($4, moderator_notes),
                resolved_by = $5,
                resolved_at = CASE WHEN $6 THEN now() ELSE NULL END
            WHERE id = $1
            RETURNING id, entity_id, reporter, signed_url, filename, payload,
                      status, resolution, moderator_notes, resolved_by,
                      resolved_at, created_at
            "#,
        )
        .bind(id)
        .bind(status)
        .bind(resolution)
        .bind(notes)
        .bind(resolved_by)
        .bind(resolved_at_now)
        .fetch_optional(writer)
        .await?;
        Ok(row.map(row_to_report))
    }

    /// Soft-delete / disable a place: set `disabled` and record the reason +
    /// timestamp in `raw` (mirrors the columns surfaced by `PLACE_COLUMNS`).
    pub async fn set_disabled(
        &self,
        entity_id: &str,
        disabled: bool,
        reason: Option<&str>,
    ) -> Result<bool, ApiError> {
        let writer = self
            .writer
            .as_ref()
            .ok_or_else(|| ApiError::service_unavailable("place writes not configured"))?;
        let reason_value = match (disabled, reason) {
            (true, Some(r)) => serde_json::Value::from(r),
            _ => serde_json::Value::Null,
        };
        let disabled_at_value = if disabled {
            serde_json::Value::from(chrono::Utc::now().to_rfc3339())
        } else {
            serde_json::Value::Null
        };
        let res = sqlx::query(
            r#"
            UPDATE place
            SET disabled = $2,
                raw = jsonb_set(
                          jsonb_set(COALESCE(raw,'{}'::jsonb), '{disabled_reason}', $3, true),
                          '{disabled_at}', $4, true)
            WHERE id = $1
            "#,
        )
        .bind(entity_id)
        .bind(disabled)
        .bind(reason_value)
        .bind(disabled_at_value)
        .execute(writer)
        .await?;
        Ok(res.rows_affected() > 0)
    }

    // ---- POI CRUD (admin-console.md §4) ----

    pub async fn list_pois(&self) -> Result<Vec<PoiRow>, ApiError> {
        let writer = self
            .writer
            .as_ref()
            .ok_or_else(|| ApiError::service_unavailable("poi persistence not configured"))?;
        let rows = sqlx::query(
            r#"SELECT position, entity_id, title, description, enabled,
                      created_by, created_at, updated_at
               FROM pois ORDER BY position ASC"#,
        )
        .fetch_all(writer)
        .await?;
        Ok(rows.into_iter().map(row_to_poi).collect())
    }

    pub async fn upsert_poi(
        &self,
        position: &str,
        entity_id: Option<&str>,
        title: Option<&str>,
        description: Option<&str>,
        enabled: bool,
        created_by: Option<&str>,
    ) -> Result<PoiRow, ApiError> {
        let writer = self
            .writer
            .as_ref()
            .ok_or_else(|| ApiError::service_unavailable("poi persistence not configured"))?;
        let row = sqlx::query(
            r#"
            INSERT INTO pois (position, entity_id, title, description, enabled, created_by)
            VALUES ($1,$2,$3,$4,$5,$6)
            ON CONFLICT (position) DO UPDATE SET
                entity_id = EXCLUDED.entity_id,
                title = EXCLUDED.title,
                description = EXCLUDED.description,
                enabled = EXCLUDED.enabled,
                updated_at = now()
            RETURNING position, entity_id, title, description, enabled,
                      created_by, created_at, updated_at
            "#,
        )
        .bind(position)
        .bind(entity_id)
        .bind(title)
        .bind(description)
        .bind(enabled)
        .bind(created_by)
        .fetch_one(writer)
        .await?;
        Ok(row_to_poi(row))
    }

    /// Partial update of an existing POI. `None` fields are left untouched
    /// (NULL coalesce). Returns the updated row or `None` when absent.
    pub async fn update_poi(
        &self,
        position: &str,
        entity_id: Option<&str>,
        title: Option<&str>,
        description: Option<&str>,
        enabled: Option<bool>,
    ) -> Result<Option<PoiRow>, ApiError> {
        let writer = self
            .writer
            .as_ref()
            .ok_or_else(|| ApiError::service_unavailable("poi persistence not configured"))?;
        let row = sqlx::query(
            r#"
            UPDATE pois SET
                entity_id = COALESCE($2, entity_id),
                title = COALESCE($3, title),
                description = COALESCE($4, description),
                enabled = COALESCE($5, enabled),
                updated_at = now()
            WHERE position = $1
            RETURNING position, entity_id, title, description, enabled,
                      created_by, created_at, updated_at
            "#,
        )
        .bind(position)
        .bind(entity_id)
        .bind(title)
        .bind(description)
        .bind(enabled)
        .fetch_optional(writer)
        .await?;
        Ok(row.map(row_to_poi))
    }

    pub async fn delete_poi(&self, position: &str) -> Result<bool, ApiError> {
        let writer = self
            .writer
            .as_ref()
            .ok_or_else(|| ApiError::service_unavailable("poi persistence not configured"))?;
        let res = sqlx::query("DELETE FROM pois WHERE position = $1")
            .bind(position)
            .execute(writer)
            .await?;
        Ok(res.rows_affected() > 0)
    }

    pub async fn ping(&self) -> Result<(), ApiError> {
        sqlx::query("SELECT 1").fetch_one(&self.pool).await?;
        Ok(())
    }

    pub async fn find_by_id(&self, place_id: &str) -> Result<Option<PlaceRow>, ApiError> {
        let sql = format!("SELECT {PLACE_COLUMNS} FROM place WHERE id = $1");
        let row_opt = sqlx::query(&sql)
            .bind(place_id)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row_opt.map(row_to_place))
    }

    pub async fn find_by_ids(&self, ids: &[String]) -> Result<Vec<PlaceRow>, ApiError> {
        if ids.is_empty() {
            return Ok(vec![]);
        }
        let sql = format!("SELECT {PLACE_COLUMNS} FROM place WHERE id = ANY($1)");
        let rows = sqlx::query(&sql).bind(ids).fetch_all(&self.pool).await?;
        Ok(rows.into_iter().map(row_to_place).collect())
    }

    pub async fn find_by_ids_status(
        &self,
        ids: &[String],
    ) -> Result<Vec<PlaceStatusRow>, ApiError> {
        if ids.is_empty() {
            return Ok(vec![]);
        }
        let rows = sqlx::query(
            r#"
            SELECT id, disabled, base_position,
                   COALESCE((raw->>'world')::bool, false) AS world,
                   raw->>'world_name' AS world_name
            FROM place
            WHERE id = ANY($1)
            "#,
        )
        .bind(ids)
        .fetch_all(&self.pool)
        .await?;
        let out = rows
            .into_iter()
            .map(|r| PlaceStatusRow {
                id: r.get::<String, _>("id"),
                disabled: r.get::<bool, _>("disabled"),
                world: r.get::<bool, _>("world"),
                world_name: r.try_get::<Option<String>, _>("world_name").unwrap_or(None),
                base_position: r.get::<String, _>("base_position"),
            })
            .collect();
        Ok(out)
    }

    pub async fn count_by_ids(&self, ids: &[String]) -> Result<i64, ApiError> {
        if ids.is_empty() {
            return Ok(0);
        }
        let row = sqlx::query("SELECT count(*)::bigint AS total FROM place WHERE id = ANY($1)")
            .bind(ids)
            .fetch_one(&self.pool)
            .await?;
        Ok(row.get::<i64, _>("total"))
    }

    pub async fn find_list(&self, f: &PlaceListFilters) -> Result<Vec<PlaceRow>, ApiError> {
        if matches!(&f.search, Some(s) if s.len() < 3) {
            return Ok(vec![]);
        }
        let (where_clause, binds) = build_where(f);
        let order = f.order_by.column();
        let dir = if f.order_desc { "DESC" } else { "ASC" };
        let rank_prefix = if f.search.is_some() {
            "ts_rank_cd(to_tsvector('english', coalesce(title,'') || ' ' || coalesce(description,'')), \
             plainto_tsquery('english', $rank), 32) DESC, "
                .replace("$rank", &format!("${}", binds.len() + 1))
        } else {
            String::new()
        };
        let sql = format!(
            r#"
            SELECT {cols}
            FROM place
            WHERE {where_clause}
            ORDER BY {rank_prefix}{order} {dir} NULLS LAST, deployed_at DESC
            LIMIT {limit} OFFSET {offset}
            "#,
            cols = PLACE_COLUMNS,
            limit = f.limit.clamp(0, 100),
            offset = f.offset.max(0),
        );
        let mut q = sqlx::query(&sql);
        for b in &binds {
            q = bind_param(q, b);
        }
        if let Some(s) = &f.search {
            q = q.bind(s.clone());
        }
        let rows = q.fetch_all(&self.pool).await?;
        Ok(rows.into_iter().map(row_to_place).collect())
    }

    pub async fn count_list(&self, f: &PlaceListFilters) -> Result<i64, ApiError> {
        if matches!(&f.search, Some(s) if s.len() < 3) {
            return Ok(0);
        }
        let (where_clause, binds) = build_where(f);
        let sql = format!("SELECT count(*)::bigint AS total FROM place WHERE {where_clause}");
        let mut q = sqlx::query(&sql);
        for b in &binds {
            q = bind_param(q, b);
        }
        let row = q.fetch_one(&self.pool).await?;
        Ok(row.get::<i64, _>("total"))
    }

    pub async fn category_counts(
        &self,
        target: CategoryTarget,
    ) -> Result<Vec<(String, i64)>, ApiError> {
        let world_filter = match target {
            CategoryTarget::Worlds => "AND COALESCE((raw->>'world')::bool, false) IS TRUE",
            CategoryTarget::Places => "AND COALESCE((raw->>'world')::bool, false) IS FALSE",
            CategoryTarget::All => "",
        };
        let sql = format!(
            r#"
            SELECT cat AS name, count(*)::bigint AS count
            FROM place p, unnest(p.categories) AS cat
            WHERE p.disabled IS FALSE {world_filter}
            GROUP BY cat
            ORDER BY count DESC, name ASC
            "#,
        );
        let rows = sqlx::query(&sql).fetch_all(&self.pool).await?;
        Ok(rows
            .into_iter()
            .map(|r| (r.get::<String, _>("name"), r.get::<i64, _>("count")))
            .collect())
    }

    pub async fn categories_for_place(&self, place_id: &str) -> Result<Vec<String>, ApiError> {
        let row = sqlx::query("SELECT categories FROM place WHERE id = $1")
            .bind(place_id)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row
            .map(|r| {
                r.try_get::<Vec<String>, _>("categories")
                    .unwrap_or_default()
            })
            .unwrap_or_default())
    }

    pub async fn find_world_by_id(&self, world_id: &str) -> Result<Option<PlaceRow>, ApiError> {
        let sql = format!(
            "SELECT {PLACE_COLUMNS} FROM place \
             WHERE COALESCE((raw->>'world')::bool,false) IS TRUE \
             AND (id = $1 OR lower(raw->>'world_name') = lower($1))"
        );
        let row_opt = sqlx::query(&sql)
            .bind(world_id)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row_opt.map(row_to_place))
    }

    pub async fn world_names(&self) -> Result<Vec<String>, ApiError> {
        let rows = sqlx::query(
            "SELECT DISTINCT raw->>'world_name' AS world_name FROM place \
             WHERE COALESCE((raw->>'world')::bool,false) IS TRUE \
             AND raw->>'world_name' IS NOT NULL ORDER BY 1",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .filter_map(|r| r.try_get::<Option<String>, _>("world_name").ok().flatten())
            .collect())
    }

    pub async fn operated_positions(&self, owner: &str) -> Result<Vec<String>, ApiError> {
        let Some(squid) = &self.squid else {
            return Ok(vec![]);
        };
        let owner = owner.to_lowercase();
        let schema = &self.squid_schema;
        let sql = format!(
            r#"
            SELECT DISTINCT (p.x::text || ',' || p.y::text) AS pos
            FROM {schema}.parcel p
            JOIN {schema}.nft n ON n.id = p.id
            LEFT JOIN {schema}.account a ON a.id = n.owner_id
            WHERE lower(a.address) = $1
               OR lower(n."owner_address") = $1
            "#,
        );
        let rows = match sqlx::query(&sql).bind(&owner).fetch_all(squid).await {
            Ok(r) => r,
            Err(_) => {
                let sql2 = format!(
                    r#"
                    SELECT DISTINCT (p.x::text || ',' || p.y::text) AS pos
                    FROM {schema}.parcel p
                    JOIN {schema}.nft n ON n.id = p.id
                    JOIN {schema}.account a ON a.id = n.owner_id
                    WHERE lower(a.address) = $1
                    "#,
                );
                sqlx::query(&sql2).bind(&owner).fetch_all(squid).await?
            }
        };
        Ok(rows
            .into_iter()
            .filter_map(|r| r.try_get::<Option<String>, _>("pos").ok().flatten())
            .collect())
    }
}

#[derive(Debug, Clone, Copy)]
pub enum CategoryTarget {
    All,
    Places,
    Worlds,
}

impl CategoryTarget {
    pub fn parse(s: Option<&str>) -> Self {
        match s {
            Some("places") => Self::Places,
            Some("worlds") => Self::Worlds,
            _ => Self::All,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ReportRow {
    pub id: i64,
    pub entity_id: Option<String>,
    pub reporter: String,
    pub signed_url: String,
    pub filename: String,
    pub payload: serde_json::Value,
    pub status: String,
    pub resolution: Option<String>,
    pub moderator_notes: Option<String>,
    pub resolved_by: Option<String>,
    pub resolved_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

fn row_to_report(r: sqlx::postgres::PgRow) -> ReportRow {
    ReportRow {
        id: r.get::<i64, _>("id"),
        entity_id: r.try_get::<Option<String>, _>("entity_id").unwrap_or(None),
        reporter: r.get::<String, _>("reporter"),
        signed_url: r.get::<String, _>("signed_url"),
        filename: r.get::<String, _>("filename"),
        payload: r
            .try_get::<serde_json::Value, _>("payload")
            .unwrap_or(serde_json::Value::Null),
        status: r.get::<String, _>("status"),
        resolution: r.try_get::<Option<String>, _>("resolution").unwrap_or(None),
        moderator_notes: r
            .try_get::<Option<String>, _>("moderator_notes")
            .unwrap_or(None),
        resolved_by: r
            .try_get::<Option<String>, _>("resolved_by")
            .unwrap_or(None),
        resolved_at: r
            .try_get::<Option<DateTime<Utc>>, _>("resolved_at")
            .unwrap_or(None),
        created_at: r.get::<DateTime<Utc>, _>("created_at"),
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct PoiRow {
    pub position: String,
    pub entity_id: Option<String>,
    pub title: Option<String>,
    pub description: Option<String>,
    pub enabled: bool,
    pub created_by: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

fn row_to_poi(r: sqlx::postgres::PgRow) -> PoiRow {
    PoiRow {
        position: r.get::<String, _>("position"),
        entity_id: r.try_get::<Option<String>, _>("entity_id").unwrap_or(None),
        title: r.try_get::<Option<String>, _>("title").unwrap_or(None),
        description: r
            .try_get::<Option<String>, _>("description")
            .unwrap_or(None),
        enabled: r.try_get::<bool, _>("enabled").unwrap_or(true),
        created_by: r.try_get::<Option<String>, _>("created_by").unwrap_or(None),
        created_at: r.get::<DateTime<Utc>, _>("created_at"),
        updated_at: r.get::<DateTime<Utc>, _>("updated_at"),
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct PlaceStatusRow {
    pub id: String,
    pub disabled: bool,
    pub world: bool,
    pub world_name: Option<String>,
    pub base_position: String,
}

#[derive(Debug)]
enum Bind {
    Text(String),
    TextArray(Vec<String>),
}

fn build_where(f: &PlaceListFilters) -> (String, Vec<Bind>) {
    let mut clauses: Vec<String> = vec!["disabled IS FALSE".to_string()];
    let mut binds: Vec<Bind> = Vec::new();
    let mut idx = 1;

    if !f.ids.is_empty() {
        clauses.push(format!("id = ANY(${})", idx));
        binds.push(Bind::TextArray(f.ids.clone()));
        idx += 1;
    } else if f.only_worlds {
        clauses.push("COALESCE((raw->>'world')::bool, false) IS TRUE".to_string());
    } else if f.only_places {
        clauses.push("COALESCE((raw->>'world')::bool, false) IS FALSE".to_string());
    }
    if f.only_highlighted {
        clauses.push("highlighted = TRUE".to_string());
    }
    let mut positions: Vec<String> = f.positions.clone();
    positions.extend(f.operated_positions.iter().cloned());
    if !positions.is_empty() {
        clauses.push(format!("raw->'positions' ?| ${}::text[]", idx));
        binds.push(Bind::TextArray(positions));
        idx += 1;
    }
    if !f.names.is_empty() {
        clauses.push(format!("lower(raw->>'world_name') = ANY(${})", idx));
        binds.push(Bind::TextArray(
            f.names.iter().map(|n| n.to_lowercase()).collect(),
        ));
        idx += 1;
    }
    if !f.categories.is_empty() {
        clauses.push(format!("categories && ${}", idx));
        binds.push(Bind::TextArray(f.categories.clone()));
        idx += 1;
    }
    if let Some(addr) = &f.creator_address {
        clauses.push(format!("LOWER(creator_address) = ${}", idx));
        binds.push(Bind::Text(addr.to_lowercase()));
        idx += 1;
    }
    if let Some(sdk) = &f.sdk {
        let null_clause = if sdk == "6" {
            " OR raw->>'sdk' IS NULL"
        } else {
            ""
        };
        clauses.push(format!(
            "(raw->>'sdk' = ${0} OR raw->>'sdk' LIKE ${1}{2})",
            idx,
            idx + 1,
            null_clause
        ));
        binds.push(Bind::Text(sdk.clone()));
        binds.push(Bind::Text(format!("{}.%", sdk)));
        idx += 2;
    }
    if let Some(s) = &f.search {
        clauses.push(format!(
            "(to_tsvector('english', coalesce(title,'') || ' ' || coalesce(description,'')) @@ plainto_tsquery('english', ${0}) \
             OR title ILIKE ${1} OR description ILIKE ${1})",
            idx,
            idx + 1,
        ));
        binds.push(Bind::Text(s.clone()));
        binds.push(Bind::Text(format!("%{}%", s)));
    }
    (clauses.join(" AND "), binds)
}

fn bind_param<'a>(
    q: sqlx::query::Query<'a, sqlx::Postgres, sqlx::postgres::PgArguments>,
    b: &'a Bind,
) -> sqlx::query::Query<'a, sqlx::Postgres, sqlx::postgres::PgArguments> {
    match b {
        Bind::Text(s) => q.bind(s),
        Bind::TextArray(v) => q.bind(v),
    }
}

fn row_to_place(r: sqlx::postgres::PgRow) -> PlaceRow {
    PlaceRow {
        id: r.get::<String, _>("id"),
        title: r.try_get::<Option<String>, _>("title").unwrap_or(None),
        description: r
            .try_get::<Option<String>, _>("description")
            .unwrap_or(None),
        image: r.try_get::<Option<String>, _>("image").unwrap_or(None),
        owner: r.try_get::<Option<String>, _>("owner").unwrap_or(None),
        positions: r.try_get::<Vec<String>, _>("positions").unwrap_or_default(),
        base_position: r.get::<String, _>("base_position"),
        contact_name: r
            .try_get::<Option<String>, _>("contact_name")
            .unwrap_or(None),
        contact_email: r
            .try_get::<Option<String>, _>("contact_email")
            .unwrap_or(None),
        content_rating: r
            .try_get::<Option<String>, _>("content_rating")
            .unwrap_or(None),
        disabled: r.try_get::<bool, _>("disabled").unwrap_or(false),
        disabled_at: r
            .try_get::<Option<DateTime<Utc>>, _>("disabled_at")
            .unwrap_or(None),
        disabled_reason: r
            .try_get::<Option<String>, _>("disabled_reason")
            .unwrap_or(None),
        created_at: r
            .try_get::<Option<DateTime<Utc>>, _>("created_at")
            .unwrap_or(None),
        updated_at: r
            .try_get::<Option<DateTime<Utc>>, _>("updated_at")
            .unwrap_or(None),
        favorites: r.try_get::<i32, _>("favorites").unwrap_or(0),
        likes: r.try_get::<i32, _>("likes").unwrap_or(0),
        dislikes: r.try_get::<i32, _>("dislikes").unwrap_or(0),
        categories: r
            .try_get::<Vec<String>, _>("categories")
            .unwrap_or_default(),
        tags: r.try_get::<Vec<String>, _>("tags").unwrap_or_default(),
        highlighted: r.try_get::<bool, _>("highlighted").unwrap_or(false),
        highlighted_image: r
            .try_get::<Option<String>, _>("highlighted_image")
            .unwrap_or(None),
        ranking: r.try_get::<Option<f64>, _>("ranking").unwrap_or(None),
        sdk: r.try_get::<Option<String>, _>("sdk").unwrap_or(None),
        creator_address: r
            .try_get::<Option<String>, _>("creator_address")
            .unwrap_or(None),
        world_id: r.try_get::<Option<String>, _>("world_id").unwrap_or(None),
        deployed_at: r
            .try_get::<Option<DateTime<Utc>>, _>("deployed_at")
            .unwrap_or(None),
        world: r.try_get::<bool, _>("world").unwrap_or(false),
        world_name: r.try_get::<Option<String>, _>("world_name").unwrap_or(None),
        is_private: r.try_get::<bool, _>("is_private").unwrap_or(false),
        user_favorite: r.try_get::<bool, _>("user_favorite").unwrap_or(false),
        user_like: r.try_get::<bool, _>("user_like").unwrap_or(false),
        user_dislike: r.try_get::<bool, _>("user_dislike").unwrap_or(false),
        user_count: r.try_get::<Option<i32>, _>("user_count").unwrap_or(None),
        user_visits: r.try_get::<i32, _>("user_visits").unwrap_or(0),
        like_rate: r.try_get::<Option<f64>, _>("like_rate").unwrap_or(None),
        like_score: r.try_get::<Option<f64>, _>("like_score").unwrap_or(None),
        live: None,
        connected_addresses: None,
        realms_detail: None,
    }
}

#[cfg(test)]
mod wire_tests {
    use super::*;

    fn sample() -> PlaceRow {
        PlaceRow {
            id: "uuid-1".to_string(),
            title: Some("t".to_string()),
            description: None,
            image: None,
            owner: None,
            positions: vec!["1,2".to_string()],
            base_position: "1,2".to_string(),
            contact_name: None,
            contact_email: None,
            content_rating: None,
            disabled: false,
            disabled_at: None,
            disabled_reason: None,
            created_at: None,
            updated_at: None,
            favorites: 0,
            likes: 0,
            dislikes: 0,
            categories: vec![],
            tags: vec![],
            highlighted: false,
            highlighted_image: None,
            ranking: None,
            sdk: None,
            creator_address: None,
            world_id: None,
            deployed_at: None,
            world: false,
            world_name: None,
            is_private: false,
            user_favorite: false,
            user_like: false,
            user_dislike: false,
            user_count: Some(3),
            user_visits: 0,
            like_rate: None,
            like_score: None,
            live: None,
            connected_addresses: None,
            realms_detail: None,
        }
    }

    /// The base Place wire (GET /places, GET /places/:id) must NOT carry the
    /// invented `featured`/`featured_image` keys, nor the destinations-only
    /// `live`/`connected_addresses`/`realms_detail` keys. Upstream
    /// `PlaceAttributes` has none of these.
    #[test]
    fn base_place_omits_destination_only_fields() {
        let v = serde_json::to_value(sample()).unwrap();
        let obj = v.as_object().unwrap();
        for absent in [
            "featured",
            "featured_image",
            "live",
            "connected_addresses",
            "realms_detail",
        ] {
            assert!(
                !obj.contains_key(absent),
                "{absent} must be omitted on base Place"
            );
        }
        // highlighted (the real source of truth toggled by /featured) is present.
        assert!(obj.contains_key("highlighted"));
    }

    /// When the `/destinations` path enriches a row, the keys appear with the
    /// expected types: `connected_addresses` as a string array, `live` as a
    /// bool, `realms_detail` as an array.
    #[test]
    fn enriched_destination_emits_fields() {
        let mut row = sample();
        row.connected_addresses = Some(vec!["0xaaa".to_string(), "0xbbb".to_string()]);
        row.live = Some(true);
        row.realms_detail = Some(vec![]);
        // finalUserCount: connected (2) is not greater than stored (3), so the
        // caller keeps 3; here we just assert serialization shape.
        let v = serde_json::to_value(&row).unwrap();
        let obj = v.as_object().unwrap();
        assert_eq!(
            obj["connected_addresses"],
            serde_json::json!(["0xaaa", "0xbbb"])
        );
        assert_eq!(obj["live"], serde_json::json!(true));
        assert_eq!(obj["realms_detail"], serde_json::json!([]));
    }
}
