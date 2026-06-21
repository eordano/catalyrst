use sqlx::{postgres::PgPool, Row};

use crate::http::errors::ApiError;

use super::query::{
    bind_param, build_live_user_count_order, build_order_by, build_where, destinations_order_prefix,
};
use super::rows::{
    row_to_place, row_to_poi, row_to_report, CategoryTarget, PlaceListFilters, PlaceRow,
    PlaceStatusRow, PoiRow, ReportRow, UserInteraction, PLACE_COLUMNS,
};

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
        let row_opt = sqlx::query(sqlx::AssertSqlSafe(sql))
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
        let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(ids)
            .fetch_all(&self.pool)
            .await?;
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
        let search_count = if f.search.is_some() { 1 } else { 0 };
        let live_start = binds.len() + search_count + 1;
        let (live_prefix, live_binds) = build_live_user_count_order(f, live_start);
        let dest_prefix = destinations_order_prefix(f);
        let order_clause = build_order_by(dest_prefix, &live_prefix, &rank_prefix, order, dir);
        let sql = format!(
            r#"
            SELECT {cols}
            FROM place
            WHERE {where_clause}
            ORDER BY {order_clause}
            LIMIT {limit} OFFSET {offset}
            "#,
            cols = PLACE_COLUMNS,
            limit = f.limit.clamp(0, 100),
            offset = f.offset.max(0),
        );
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
        Ok(rows.into_iter().map(row_to_place).collect())
    }

    pub async fn count_list(&self, f: &PlaceListFilters) -> Result<i64, ApiError> {
        if matches!(&f.search, Some(s) if s.len() < 3) {
            return Ok(0);
        }
        let (where_clause, binds) = build_where(f);
        let sql = format!("SELECT count(*)::bigint AS total FROM place WHERE {where_clause}");
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
        let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
            .fetch_all(&self.pool)
            .await?;
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
        let row_opt = sqlx::query(sqlx::AssertSqlSafe(sql))
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
