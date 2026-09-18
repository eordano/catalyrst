use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use catalyrst_fed::cache::{cache_get, cache_put, Cached};
use sqlx::{postgres::PgPool, Row};

use crate::http::errors::ApiError;

use super::content_quality::ROAD_POSITIONS_TABLE;
use super::query::{
    bind_param, build_live_user_count_order, build_order_by, build_where, description_plain_sql,
    destinations_highlighted_prefix, destinations_ranking_prefix, order_tail, Bind,
    EXCLUDE_FROM_RANKING_SQL, SHOW_IN_PLACES_SQL,
};
use super::rows::{
    place_columns, row_to_place, row_to_poi, row_to_report, viewer_columns, CategoryTarget,
    PlaceListFilters, PlaceOrderBy, PlaceRow, PlaceStatusRow, PoiRow, ReportRow, UserInteraction,
};
use crate::sanitize::ContentOrigin;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScoreRankingOutcome {
    Stored,
    RefusedCurated,
    NotWritable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReportUploadOutcome {
    Stored,
    NoReportOwnedByReporter,
    PersistenceDisabled,
}

type CategoryCounts = Vec<(String, i64)>;

pub struct PlacesComponent {
    pool: PgPool,
    writer: Option<PgPool>,
    squid: Option<PgPool>,
    squid_schema: String,
    content_origin: Option<ContentOrigin>,
    road_positions: AtomicBool,
    interactions_readable: AtomicBool,
    category_cache: Mutex<HashMap<String, Cached<CategoryCounts>>>,
}

const CATEGORY_CACHE_TTL: Duration = Duration::from_secs(60);

const DB_IDENTITY_SQL: &str =
    "current_database()::text || '|' || COALESCE(inet_server_addr()::text, '') \
     || '|' || COALESCE(inet_server_port()::text, '')";

impl PlacesComponent {
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            writer: None,
            squid: None,
            squid_schema: "squid_marketplace".to_string(),
            content_origin: None,
            road_positions: AtomicBool::new(false),
            interactions_readable: AtomicBool::new(false),
            category_cache: Mutex::new(HashMap::new()),
        }
    }

    /// Thumbnails from this base are exempt from the internal-host image filter.
    pub fn with_content_origin(mut self, base_url: &str) -> Self {
        self.content_origin = ContentOrigin::parse(base_url);
        self
    }

    pub fn content_origin(&self) -> Option<&ContentOrigin> {
        self.content_origin.as_ref()
    }

    pub async fn probe_road_positions(&self) -> Result<bool, ApiError> {
        let readable: bool = sqlx::query_scalar(
            "SELECT COALESCE(has_table_privilege(current_user, to_regclass($1), 'SELECT'), FALSE)",
        )
        .bind(ROAD_POSITIONS_TABLE)
        .fetch_one(&self.pool)
        .await?;
        self.road_positions.store(readable, Ordering::Relaxed);
        Ok(readable)
    }

    fn road_positions_ready(&self) -> bool {
        self.road_positions.load(Ordering::Relaxed)
    }

    /// True when the reader pool is the writer's own primary database and may SELECT the
    /// interaction tables, so reads fold favorites/likes into the page statement.
    pub async fn probe_interactions(&self) -> Result<bool, ApiError> {
        let Some(writer) = self.writer.as_ref() else {
            self.interactions_readable.store(false, Ordering::Relaxed);
            return Ok(false);
        };
        let writer_db: String =
            sqlx::query_scalar(sqlx::AssertSqlSafe(format!("SELECT {DB_IDENTITY_SQL}")))
                .fetch_one(writer)
                .await?;
        let row = sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT ({DB_IDENTITY_SQL}) AS db, \
             NOT pg_is_in_recovery() \
             AND COALESCE(has_table_privilege(current_user, to_regclass('user_favorites'), 'SELECT'), FALSE) \
             AND COALESCE(has_table_privilege(current_user, to_regclass('user_likes'), 'SELECT'), FALSE) \
             AS readable"
        )))
        .fetch_one(&self.pool)
        .await?;
        let readable = row.get::<bool, _>("readable") && row.get::<String, _>("db") == writer_db;
        self.interactions_readable
            .store(readable, Ordering::Relaxed);
        Ok(readable)
    }

    pub fn viewer_in_query(&self) -> bool {
        self.writer.is_some() && self.interactions_readable.load(Ordering::Relaxed)
    }

    fn fold_viewer<'a>(&self, viewer: Option<&'a str>) -> Option<&'a str> {
        viewer.filter(|_| self.viewer_in_query())
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

    pub fn writer_pool(&self) -> Option<&PgPool> {
        self.writer.as_ref()
    }

    fn report_writer(&self) -> Result<&PgPool, ApiError> {
        self.writer
            .as_ref()
            .ok_or_else(|| ApiError::service_unavailable("report persistence not configured"))
    }

    pub(super) fn place_writer(&self) -> Result<&PgPool, ApiError> {
        self.writer
            .as_ref()
            .ok_or_else(|| ApiError::service_unavailable("place writes not configured"))
    }

    fn poi_writer(&self) -> Result<&PgPool, ApiError> {
        self.writer
            .as_ref()
            .ok_or_else(|| ApiError::service_unavailable("poi persistence not configured"))
    }

    pub async fn ensure_local_schema(&self) -> Result<(), ApiError> {
        let Some(writer) = &self.writer else {
            return Ok(());
        };
        for ddl in [
            r#"
            CREATE TABLE IF NOT EXISTS user_favorites (
                "user" text NOT NULL,
                entity_id text NOT NULL,
                user_activity double precision NOT NULL DEFAULT 0,
                created_at timestamptz NOT NULL DEFAULT now(),
                PRIMARY KEY ("user", entity_id)
            )
            "#,
            "CREATE INDEX IF NOT EXISTS user_favorites_entity_idx ON user_favorites (entity_id)",
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
            "CREATE INDEX IF NOT EXISTS user_likes_entity_idx ON user_likes (entity_id)",
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
            "CREATE INDEX IF NOT EXISTS place_reports_local_reporter_idx ON place_reports_local (reporter)",
            "ALTER TABLE place_reports_local ADD COLUMN IF NOT EXISTS status text NOT NULL DEFAULT 'open'",
            "ALTER TABLE place_reports_local ADD COLUMN IF NOT EXISTS resolution text",
            "ALTER TABLE place_reports_local ADD COLUMN IF NOT EXISTS moderator_notes text",
            "ALTER TABLE place_reports_local ADD COLUMN IF NOT EXISTS resolved_by text",
            "ALTER TABLE place_reports_local ADD COLUMN IF NOT EXISTS resolved_at timestamptz",
            "CREATE INDEX IF NOT EXISTS place_reports_local_status_idx ON place_reports_local (status, created_at DESC)",
            "CREATE INDEX IF NOT EXISTS place_reports_local_entity_idx ON place_reports_local (entity_id)",
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
            "CREATE INDEX IF NOT EXISTS idx_sap_signer ON signed_actions_places (signer, action_type, signed_at DESC)",
            "CREATE INDEX IF NOT EXISTS idx_sap_place ON signed_actions_places (place_id, action_type, signed_at DESC)",
            "CREATE INDEX IF NOT EXISTS idx_sap_seq ON signed_actions_places (seq)",
            r#"
            CREATE TABLE IF NOT EXISTS seen_nonces (
                signer     text NOT NULL,
                nonce      text NOT NULL,
                expires_at bigint NOT NULL,
                PRIMARY KEY (signer, nonce)
            )
            "#,
        ] {
            sqlx::query(ddl).execute(writer).await?;
        }
        let nonce_index: Option<String> =
            sqlx::query_scalar("SELECT to_regclass('idx_seen_nonces_expires')::text")
                .fetch_one(writer)
                .await?;
        if nonce_index.is_none() {
            sqlx::query(
                "CREATE INDEX IF NOT EXISTS idx_seen_nonces_expires ON seen_nonces (expires_at)",
            )
            .execute(writer)
            .await?;
        }
        Ok(())
    }

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
        let rows = sqlx::query(
            r#"SELECT entity_id, TRUE AS favorite, NULL::boolean AS "like"
               FROM user_favorites WHERE lower("user") = $1 AND entity_id = ANY($2)
               UNION ALL
               SELECT entity_id, FALSE, "like"
               FROM user_likes WHERE lower("user") = $1 AND entity_id = ANY($2)"#,
        )
        .bind(&user)
        .bind(entity_ids)
        .fetch_all(writer)
        .await
        .ok()?;
        for r in rows {
            let e = map.entry(r.get::<String, _>("entity_id")).or_default();
            if r.get::<bool, _>("favorite") {
                e.user_favorite = true;
                continue;
            }
            match r.get::<Option<bool>, _>("like") {
                Some(true) => e.user_like = true,
                Some(false) => e.user_dislike = true,
                None => {}
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
        // CTE parts share one snapshot, so the count is corrected by the rows this statement changed.
        let sql = if favorite {
            r#"WITH changed AS (
                 INSERT INTO user_favorites ("user", entity_id, created_at)
                 VALUES ($1, $2, now())
                 ON CONFLICT ("user", entity_id) DO NOTHING
                 RETURNING 1
               ), counted AS (
                 SELECT (count(*) + (SELECT count(*) FROM changed))::int AS c
                 FROM user_favorites WHERE entity_id = $2
               ), bumped AS (
                 UPDATE place SET favorites = counted.c FROM counted WHERE place.id = $2
               )
               SELECT c FROM counted"#
        } else {
            r#"WITH changed AS (
                 DELETE FROM user_favorites WHERE lower("user") = $1 AND entity_id = $2
                 RETURNING 1
               ), counted AS (
                 SELECT (count(*) - (SELECT count(*) FROM changed))::int AS c
                 FROM user_favorites WHERE entity_id = $2
               ), bumped AS (
                 UPDATE place SET favorites = counted.c FROM counted WHERE place.id = $2
               )
               SELECT c FROM counted"#
        };
        let count: i32 = sqlx::query_scalar(sql)
            .bind(&user)
            .bind(entity_id)
            .fetch_one(writer)
            .await?;
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
        // `after` is the post-change row set: the snapshot minus this user's row, plus the upserted one.
        let head = match like {
            None => {
                r#"WITH changed AS (
                     DELETE FROM user_likes WHERE lower("user") = $1 AND entity_id = $2
                     RETURNING "like", user_activity
                   ), after AS (
                     SELECT "like", user_activity FROM user_likes
                     WHERE entity_id = $2 AND lower("user") <> $1
                   )"#
            }
            Some(_) => {
                r#"WITH changed AS (
                     INSERT INTO user_likes ("user", entity_id, "like", user_activity, created_at, updated_at)
                     VALUES ($1, $2, $4, $5, now(), now())
                     ON CONFLICT ("user", entity_id)
                     DO UPDATE SET "like" = EXCLUDED."like", user_activity = EXCLUDED.user_activity, updated_at = now()
                     RETURNING "like", user_activity
                   ), after AS (
                     SELECT "like", user_activity FROM user_likes
                     WHERE entity_id = $2 AND lower("user") <> $1
                     UNION ALL
                     SELECT "like", user_activity FROM changed
                   )"#
            }
        };
        let sql = format!("{head}{LIKE_SCORE_TAIL}");
        let mut q = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(&user)
            .bind(entity_id)
            .bind(crate::snapshot::MIN_USER_ACTIVITY);
        if let Some(value) = like {
            q = q.bind(value).bind(user_activity);
        }
        let row = q.fetch_one(writer).await?;
        let (likes, dislikes) = (r_i32(&row, "likes"), r_i32(&row, "dislikes"));
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

    /// Binds the signer to a page query; false means the page is known empty
    /// (only_favorites without a signer, or without any favorites).
    pub async fn scope_to_viewer(
        &self,
        f: &mut PlaceListFilters,
        user: Option<&str>,
        only_favorites: bool,
    ) -> Result<bool, ApiError> {
        f.viewer = user.map(str::to_lowercase);
        if !only_favorites {
            return Ok(true);
        }
        let Some(user) = user else {
            return Ok(false);
        };
        if self.viewer_in_query() {
            f.viewer_favorites_only = true;
            return Ok(true);
        }
        let Some(favorites) = self.favorite_entity_ids(user).await? else {
            return Ok(false);
        };
        if favorites.is_empty() {
            return Ok(false);
        }
        if f.ids.is_empty() {
            f.ids = favorites;
        } else {
            f.ids.retain(|id| favorites.contains(id));
        }
        Ok(!f.ids.is_empty())
    }

    /// One statement per page: rows plus the windowed total, falling back to a COUNT
    /// only when the page ran past the offset.
    pub async fn list_page(&self, f: &PlaceListFilters) -> Result<(Vec<PlaceRow>, i64), ApiError> {
        if matches!(&f.search, Some(s) if s.len() < 3) {
            return Ok((vec![], 0));
        }
        let fold = self.viewer_in_query();
        let (sql, binds, live_binds) = list_sql(f, self.road_positions_ready(), fold, true);
        let mut q = sqlx::query(sqlx::AssertSqlSafe(sql));
        for b in &binds {
            q = bind_param(q, b);
        }
        if let Some(s) = &f.search {
            q = q.bind(s.clone());
        }
        for b in &live_binds {
            q = bind_param(q, b);
        }
        if let (true, Some(v)) = (fold, &f.viewer) {
            q = q.bind(v.clone());
        }
        let rows = q.fetch_all(&self.pool).await?;
        let total = match rows.first() {
            Some(r) => r.try_get::<i64, _>("total_count")?,
            None if f.offset > 0 || f.limit <= 0 => self.count_list(f).await?,
            None => 0,
        };
        let mut data: Vec<PlaceRow> = rows
            .into_iter()
            .map(|r| row_to_place(r, self.content_origin()))
            .collect();
        if !fold {
            self.apply_user_interactions(f.viewer.as_deref(), &mut data)
                .await;
        }
        Ok((data, total))
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
        reporter: &str,
        payload: &serde_json::Value,
    ) -> Result<ReportUploadOutcome, ApiError> {
        let Some(writer) = self.writer.as_ref() else {
            return Ok(ReportUploadOutcome::PersistenceDisabled);
        };
        let updated = sqlx::query(
            r#"UPDATE place_reports_local SET payload = $3
               WHERE filename = $1 AND lower(reporter) = $2"#,
        )
        .bind(filename)
        .bind(reporter.to_lowercase())
        .bind(payload)
        .execute(writer)
        .await?
        .rows_affected();
        if updated == 0 {
            return Ok(ReportUploadOutcome::NoReportOwnedByReporter);
        }
        Ok(ReportUploadOutcome::Stored)
    }

    pub async fn set_highlighted(
        &self,
        entity_id: &str,
        highlighted: bool,
    ) -> Result<u64, ApiError> {
        let writer = self.place_writer()?;
        let updated = sqlx::query("UPDATE place SET highlighted = $2 WHERE id = $1")
            .bind(entity_id)
            .bind(highlighted)
            .execute(writer)
            .await?
            .rows_affected();
        Ok(updated)
    }

    pub async fn set_ranking(
        &self,
        entity_id: &str,
        ranking: Option<f64>,
    ) -> Result<u64, ApiError> {
        let writer = self.place_writer()?;
        let raw_value = match ranking {
            Some(v) => serde_json::Value::from(v),
            None => serde_json::Value::Null,
        };
        let updated = sqlx::query(
            "UPDATE place SET raw = jsonb_set(COALESCE(raw, '{}'::jsonb), '{ranking}', $2, true) WHERE id = $1",
        )
        .bind(entity_id)
        .bind(raw_value)
        .execute(writer)
        .await?
        .rows_affected();
        Ok(updated)
    }

    pub async fn set_ranking_from_score(
        &self,
        entity_id: &str,
        ranking: Option<f64>,
    ) -> Result<ScoreRankingOutcome, ApiError> {
        let writer = self.place_writer()?;
        let raw_value = match ranking {
            Some(v) => serde_json::Value::from(v),
            None => serde_json::Value::Null,
        };
        let sql = format!(
            "UPDATE place SET raw = jsonb_set(COALESCE(raw, '{{}}'::jsonb), '{{ranking}}', $2, true) \
             WHERE id = $1 AND highlighted IS FALSE AND {EXCLUDE_FROM_RANKING_SQL} IS FALSE"
        );
        let updated = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(entity_id)
            .bind(raw_value)
            .execute(writer)
            .await?
            .rows_affected();
        if updated > 0 {
            return Ok(ScoreRankingOutcome::Stored);
        }
        let probe = format!(
            "SELECT (highlighted IS TRUE OR {EXCLUDE_FROM_RANKING_SQL} IS TRUE) AS curated \
             FROM place WHERE id = $1"
        );
        let probed = sqlx::query(sqlx::AssertSqlSafe(probe))
            .bind(entity_id)
            .fetch_optional(writer)
            .await?
            .map(|r| r.try_get::<bool, _>("curated").unwrap_or(false));
        match probed {
            Some(true) => Ok(ScoreRankingOutcome::RefusedCurated),
            Some(false) => Ok(ScoreRankingOutcome::Stored),
            None => Ok(ScoreRankingOutcome::NotWritable),
        }
    }

    pub async fn set_exclude_from_ranking(
        &self,
        entity_id: &str,
        exclude: bool,
    ) -> Result<u64, ApiError> {
        let Some(writer) = self.writer.as_ref() else {
            return Ok(0);
        };
        let updated = sqlx::query(
            "UPDATE place SET raw = CASE WHEN $2 \
             THEN jsonb_set(jsonb_set(COALESCE(raw, '{}'::jsonb), \
                            '{exclude_from_ranking}', 'true'::jsonb, true), \
                            '{ranking}', '0'::jsonb, true) \
             ELSE jsonb_set(COALESCE(raw, '{}'::jsonb), \
                            '{exclude_from_ranking}', 'false'::jsonb, true) END \
             WHERE id = $1",
        )
        .bind(entity_id)
        .bind(exclude)
        .execute(writer)
        .await?
        .rows_affected();
        Ok(updated)
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

    pub async fn list_reports(
        &self,
        status: Option<&str>,
        entity_id: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<ReportRow>, ApiError> {
        let writer = self.report_writer()?;
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

    pub async fn list_reports_page(
        &self,
        status: Option<&str>,
        entity_id: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<(Vec<ReportRow>, i64), ApiError> {
        let writer = self.report_writer()?;
        let offset = offset.max(0);
        let rows = sqlx::query(
            r#"
            SELECT id, entity_id, reporter, signed_url, filename, payload,
                   status, resolution, moderator_notes, resolved_by,
                   resolved_at, created_at, count(*) OVER () AS total_count
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
        .bind(offset)
        .fetch_all(writer)
        .await?;
        let total = match rows.first() {
            Some(r) => r.get::<i64, _>("total_count"),
            None if offset > 0 => self.count_reports(status, entity_id).await?,
            None => 0,
        };
        Ok((rows.into_iter().map(row_to_report).collect(), total))
    }

    pub async fn count_reports(
        &self,
        status: Option<&str>,
        entity_id: Option<&str>,
    ) -> Result<i64, ApiError> {
        let writer = self.report_writer()?;
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

    pub async fn update_report_status(
        &self,
        id: i64,
        status: &str,
        resolution: Option<&str>,
        notes: Option<&str>,
        resolved_by: Option<&str>,
    ) -> Result<Option<ReportRow>, ApiError> {
        let writer = self.report_writer()?;
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

    pub async fn set_disabled(
        &self,
        entity_id: &str,
        disabled: bool,
        reason: Option<&str>,
    ) -> Result<bool, ApiError> {
        let writer = self.place_writer()?;
        let now = chrono::Utc::now().to_rfc3339();
        let reason_value = match (disabled, reason) {
            (true, Some(r)) => serde_json::Value::from(r),
            _ => serde_json::Value::Null,
        };
        let disabled_at_value = if disabled {
            serde_json::Value::from(now.clone())
        } else {
            serde_json::Value::Null
        };
        let updated_at_value = serde_json::Value::from(now);
        let res = sqlx::query(
            r#"
            UPDATE place
            SET disabled = $2,
                raw = jsonb_set(
                          jsonb_set(
                              jsonb_set(COALESCE(raw,'{}'::jsonb), '{disabled_reason}', $3, true),
                              '{disabled_at}', $4, true),
                          '{updated_at}', $5, true)
            WHERE id = $1
            "#,
        )
        .bind(entity_id)
        .bind(disabled)
        .bind(reason_value)
        .bind(disabled_at_value)
        .bind(updated_at_value)
        .execute(writer)
        .await?;
        Ok(res.rows_affected() > 0)
    }

    pub async fn list_pois(&self) -> Result<Vec<PoiRow>, ApiError> {
        let writer = self.poi_writer()?;
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
        let writer = self.poi_writer()?;
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

    pub async fn update_poi(
        &self,
        position: &str,
        entity_id: Option<&str>,
        title: Option<&str>,
        description: Option<&str>,
        enabled: Option<bool>,
    ) -> Result<Option<PoiRow>, ApiError> {
        let writer = self.poi_writer()?;
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
        let writer = self.poi_writer()?;
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
        self.find_by_id_for(place_id, None).await
    }

    /// Lookup with the viewer's favorite/like folded into the statement when the reader
    /// can see the interaction tables; otherwise one follow-up interaction read.
    pub async fn find_by_id_for(
        &self,
        place_id: &str,
        viewer: Option<&str>,
    ) -> Result<Option<PlaceRow>, ApiError> {
        let fold = self.fold_viewer(viewer);
        let sql = format!(
            "SELECT {}{} FROM place_indexed WHERE id = $1",
            place_columns(),
            fold.map(|_| viewer_columns(2)).unwrap_or_default()
        );
        let mut q = sqlx::query(sqlx::AssertSqlSafe(sql)).bind(place_id);
        if let Some(v) = fold {
            q = q.bind(v.to_lowercase());
        }
        let mut row = q
            .fetch_optional(&self.pool)
            .await?
            .map(|r| row_to_place(r, self.content_origin()));
        if let (None, Some(p)) = (fold, row.as_mut()) {
            self.apply_user_interactions(viewer, std::slice::from_mut(p))
                .await;
        }
        Ok(row)
    }

    pub async fn find_by_ids(&self, ids: &[String]) -> Result<Vec<PlaceRow>, ApiError> {
        self.find_by_ids_for(ids, None).await
    }

    pub async fn find_by_ids_for(
        &self,
        ids: &[String],
        viewer: Option<&str>,
    ) -> Result<Vec<PlaceRow>, ApiError> {
        if ids.is_empty() {
            return Ok(vec![]);
        }
        let fold = self.fold_viewer(viewer);
        let sql = format!(
            "SELECT {}{} FROM place_indexed WHERE id = ANY($1)",
            place_columns(),
            fold.map(|_| viewer_columns(2)).unwrap_or_default()
        );
        let mut q = sqlx::query(sqlx::AssertSqlSafe(sql)).bind(ids);
        if let Some(v) = fold {
            q = q.bind(v.to_lowercase());
        }
        let mut rows: Vec<PlaceRow> = q
            .fetch_all(&self.pool)
            .await?
            .into_iter()
            .map(|r| row_to_place(r, self.content_origin()))
            .collect();
        if fold.is_none() {
            self.apply_user_interactions(viewer, &mut rows).await;
        }
        Ok(rows)
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
            SELECT id, disabled, base_position, world, world_name
            FROM place_indexed
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
        let row =
            sqlx::query("SELECT count(*)::bigint AS total FROM place_indexed WHERE id = ANY($1)")
                .bind(ids)
                .fetch_one(&self.pool)
                .await?;
        Ok(row.get::<i64, _>("total"))
    }

    pub async fn find_list(&self, f: &PlaceListFilters) -> Result<Vec<PlaceRow>, ApiError> {
        if matches!(&f.search, Some(s) if s.len() < 3) {
            return Ok(vec![]);
        }
        let (sql, binds, live_binds) = list_sql(f, self.road_positions_ready(), false, false);
        let mut q = sqlx::query(sqlx::AssertSqlSafe(sql));
        for b in &binds {
            q = bind_param(q, b);
        }
        if let Some(s) = &f.search {
            q = q.bind(s.clone());
        }
        for b in &live_binds {
            q = bind_param(q, b);
        }
        let rows = q.fetch_all(&self.pool).await?;
        Ok(rows
            .into_iter()
            .map(|r| row_to_place(r, self.content_origin()))
            .collect())
    }

    pub async fn count_list(&self, f: &PlaceListFilters) -> Result<i64, ApiError> {
        if matches!(&f.search, Some(s) if s.len() < 3) {
            return Ok(0);
        }
        let (where_clause, binds) = build_where(f, self.road_positions_ready());
        let sql =
            format!("SELECT count(*)::bigint AS total FROM place_indexed WHERE {where_clause}");
        let mut q = sqlx::query(sqlx::AssertSqlSafe(sql));
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
        let key = match target {
            CategoryTarget::All => "all",
            CategoryTarget::Places => "places",
            CategoryTarget::Worlds => "worlds",
        };
        if let Some(cached) = cache_get(&self.category_cache, key) {
            return Ok(cached);
        }
        let counts = self.category_counts_uncached(target).await?;
        cache_put(
            &self.category_cache,
            key.to_string(),
            counts.clone(),
            CATEGORY_CACHE_TTL,
        );
        Ok(counts)
    }

    async fn category_counts_uncached(
        &self,
        target: CategoryTarget,
    ) -> Result<Vec<(String, i64)>, ApiError> {
        let world_filter = match target {
            CategoryTarget::Worlds => "AND p.world IS TRUE",
            CategoryTarget::Places => "AND p.world IS FALSE",
            CategoryTarget::All => "",
        };
        let sql = format!(
            r#"
            SELECT cat AS name, count(*)::bigint AS count
            FROM place_indexed p, unnest(p.categories) AS cat
            WHERE p.disabled IS FALSE {world_filter}
            GROUP BY cat
            ORDER BY count DESC, name ASC
            "#,
        );
        let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
            .fetch_all(&self.pool)
            .await?;
        Ok(rows
            .into_iter()
            .map(|r| (r.get::<String, _>("name"), r.get::<i64, _>("count")))
            .collect())
    }

    pub async fn categories_for_place(&self, place_id: &str) -> Result<Vec<String>, ApiError> {
        let row = sqlx::query("SELECT categories FROM place_indexed WHERE id = $1")
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
        self.find_world_by_id_for(world_id, None).await
    }

    pub async fn find_world_by_id_for(
        &self,
        world_id: &str,
        viewer: Option<&str>,
    ) -> Result<Option<PlaceRow>, ApiError> {
        let fold = self.fold_viewer(viewer);
        let sql = format!(
            "SELECT {}{} FROM place_indexed \
             WHERE world IS TRUE \
             AND (id = $1 OR lower(world_name) = lower($1))",
            place_columns(),
            fold.map(|_| viewer_columns(2)).unwrap_or_default()
        );
        let mut q = sqlx::query(sqlx::AssertSqlSafe(sql)).bind(world_id);
        if let Some(v) = fold {
            q = q.bind(v.to_lowercase());
        }
        let mut row = q
            .fetch_optional(&self.pool)
            .await?
            .map(|r| row_to_place(r, self.content_origin()));
        if let (None, Some(p)) = (fold, row.as_mut()) {
            self.apply_user_interactions(viewer, std::slice::from_mut(p))
                .await;
        }
        Ok(row)
    }

    pub async fn world_names(&self) -> Result<Vec<String>, ApiError> {
        let sql = format!(
            "SELECT DISTINCT world_name FROM place_indexed \
             WHERE world IS TRUE AND world_name IS NOT NULL \
             AND {SHOW_IN_PLACES_SQL} IS TRUE ORDER BY 1"
        );
        let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
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
        let rows = match sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(&owner)
            .fetch_all(squid)
            .await
        {
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
                sqlx::query(sqlx::AssertSqlSafe(sql2))
                    .bind(&owner)
                    .fetch_all(squid)
                    .await?
            }
        };
        Ok(rows
            .into_iter()
            .filter_map(|r| r.try_get::<Option<String>, _>("pos").ok().flatten())
            .collect())
    }
}

/// SQL for `find_list`/`list_page`: `binds` go first, then the search term when set, then
/// `live_binds`, then the lowercased viewer when `fold_viewer` and `f.viewer` are set.
/// `with_total` adds `total_count` (a window count, or an uncorrelated COUNT on the
/// like-score path whose candidate cut would undercount).
pub(super) fn list_sql(
    f: &PlaceListFilters,
    road_positions_ready: bool,
    fold_viewer: bool,
    with_total: bool,
) -> (String, Vec<Bind>, Vec<Bind>) {
    let (where_clause, binds) = build_where(f, road_positions_ready);
    let order = f.order_by.column();
    let dir = if f.order_desc { "DESC" } else { "ASC" };
    let rank_prefix = if f.search.is_some() {
        format!(
            "ts_rank_cd(to_tsvector('english', coalesce(title,'') || ' ' || ({plain})), \
             plainto_tsquery('english', ${rank}), 32) DESC, ",
            plain = description_plain_sql(),
            rank = binds.len() + 1,
        )
    } else {
        String::new()
    };
    let search_count = if f.search.is_some() { 1 } else { 0 };
    let live_start = binds.len() + search_count + 1;
    let (live_prefix, live_binds) = build_live_user_count_order(f, live_start);
    let order_clause = build_order_by(
        destinations_highlighted_prefix(f),
        &live_prefix,
        destinations_ranking_prefix(f),
        &rank_prefix,
        order,
        dir,
        order_tail(f),
    );
    let limit = f.limit.clamp(0, 100);
    let offset = f.offset.max(0);
    let like_score_top_n = matches!(f.order_by, PlaceOrderBy::LikeScore)
        && f.order_desc
        && f.search.is_none()
        && !f.destinations_mode;
    let viewer_cols = match (&f.viewer, fold_viewer) {
        (Some(_), true) => viewer_columns(binds.len() + search_count + live_binds.len() + 1),
        _ => String::new(),
    };
    let sql = if like_score_top_n {
        let total = if with_total {
            format!(", (SELECT count(*) FROM place_indexed WHERE {where_clause}) AS total_count")
        } else {
            String::new()
        };
        // Cut each leg of place_indexed to the page under its own partial index, then read only those rows.
        format!(
            r#"
            WITH cand AS (
                (SELECT id FROM place WHERE ({where_clause}) AND world IS FALSE ORDER BY {order_clause} LIMIT {n})
                UNION ALL
                (SELECT id FROM place WHERE ({where_clause}) AND world IS TRUE ORDER BY {order_clause} LIMIT {n})
                UNION ALL
                (SELECT id FROM place_world_local WHERE {where_clause} ORDER BY {order_clause} LIMIT {n})
            )
            SELECT {cols}{viewer_cols}{total}
            FROM place_indexed
            WHERE id = ANY (ARRAY(SELECT id FROM cand)) AND {where_clause}
            ORDER BY {order_clause}
            LIMIT {limit} OFFSET {offset}
            "#,
            cols = place_columns(),
            n = limit + offset,
        )
    } else {
        let total = if with_total {
            ", count(*) OVER () AS total_count"
        } else {
            ""
        };
        format!(
            r#"
            SELECT {cols}{viewer_cols}{total}
            FROM place_indexed
            WHERE {where_clause}
            ORDER BY {order_clause}
            LIMIT {limit} OFFSET {offset}
            "#,
            cols = place_columns(),
        )
    };
    (sql, binds, live_binds)
}

fn r_i32(row: &sqlx::postgres::PgRow, col: &str) -> i32 {
    row.get::<i32, _>(col)
}

const LIKE_SCORE_TAIL: &str = r#", counted AS (
              SELECT
                count(*) filter (where "like") as count_likes,
                count(*) filter (where not "like") as count_dislikes,
                count(*) filter (where user_activity >= $3) as count_active_total,
                count(*) filter (where "like" and user_activity >= $3) as count_active_likes,
                count(*) filter (where not "like" and user_activity >= $3) as count_active_dislikes
              FROM after
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
            ), bumped AS (
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
              WHERE place.id = $2
            )
            SELECT c.count_likes::int AS likes, c.count_dislikes::int AS dislikes FROM computed c"#;

#[cfg(test)]
mod list_sql_tests {
    use super::super::rows::PlaceOrderBy;
    use super::*;

    fn default_list() -> PlaceListFilters {
        PlaceListFilters {
            order_by: PlaceOrderBy::LikeScore,
            order_desc: true,
            limit: 100,
            ..Default::default()
        }
    }

    #[test]
    fn like_score_lists_cut_each_leg_to_the_page_first() {
        let (sql, _, _) = list_sql(&default_list(), true, false, false);
        assert!(sql.contains("WITH cand AS ("), "{sql}");
        assert!(
            sql.contains("FROM place WHERE (") && sql.contains(") AND world IS FALSE ORDER BY"),
            "{sql}"
        );
        assert!(sql.contains(") AND world IS TRUE ORDER BY"), "{sql}");
        assert!(sql.contains("FROM place_world_local WHERE"), "{sql}");
        assert!(
            sql.contains("WHERE id = ANY (ARRAY(SELECT id FROM cand)) AND"),
            "{sql}"
        );
        assert_eq!(sql.matches("LIMIT 100").count(), 4, "{sql}");

        let paged = PlaceListFilters {
            limit: 20,
            offset: 40,
            ..default_list()
        };
        let (sql, _, _) = list_sql(&paged, true, false, false);
        assert_eq!(sql.matches("LIMIT 60)").count(), 3, "{sql}");
        assert!(sql.contains("LIMIT 20 OFFSET 40"), "{sql}");
    }

    #[test]
    fn a_creator_filter_lands_inside_each_leg() {
        let f = PlaceListFilters {
            creator_address: Some("0x17A253C2ac0d5BA92cadBBF665e3390C9913dC5D".into()),
            ..default_list()
        };
        let (sql, binds, live_binds) = list_sql(&f, true, false, false);
        assert!(sql.contains("WITH cand AS ("), "{sql}");
        assert_eq!(
            sql.matches("LOWER(creator_address) = $1").count(),
            4,
            "{sql}"
        );
        assert!(live_binds.is_empty());
        match binds.as_slice() {
            [Bind::Text(addr)] => assert_eq!(addr, "0x17a253c2ac0d5ba92cadbbf665e3390c9913dc5d"),
            other => panic!("expected the lowercased creator as the only bind: {other:?}"),
        }
    }

    #[test]
    fn other_orders_read_place_indexed_directly() {
        for f in [
            PlaceListFilters {
                order_desc: false,
                ..default_list()
            },
            PlaceListFilters {
                search: Some("tower".into()),
                ..default_list()
            },
            PlaceListFilters {
                order_by: PlaceOrderBy::MostActive,
                ..default_list()
            },
            PlaceListFilters {
                destinations_mode: true,
                ..default_list()
            },
        ] {
            let (sql, _, _) = list_sql(&f, true, false, false);
            assert!(!sql.contains("cand"), "{sql}");
            assert!(sql.contains("FROM place_indexed\n"), "{sql}");
        }
    }
}
