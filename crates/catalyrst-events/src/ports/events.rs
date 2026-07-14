use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::PgPool;

use crate::http::response::ApiError;
use crate::sanitize::sanitize_event_description;
use crate::schemas::EventRecord;

pub struct EventsComponent {
    pool: PgPool,
    rewrite_domain: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct EventListFilters {
    pub limit: i64,
    pub offset: i64,
    pub list: EventListType,
    pub order: SortOrder,
    pub highlighted: Option<bool>,
    pub creator: Option<String>,
    pub world: Option<bool>,
    pub world_names: Vec<String>,
    pub positions: Vec<(i32, i32)>,
    pub estate_id: Option<String>,
    pub community_id: Option<String>,
    pub places_ids: Vec<String>,
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
    pub search: Option<String>,
    pub user: Option<String>,
    pub rejected: Option<bool>,
    pub approved: Option<bool>,
    pub deleted: Option<bool>,
    pub admin: bool,
    pub only_attendee: bool,
    pub owner: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EventListType {
    All,
    #[default]
    Active,
    Live,
    Upcoming,
    Relevance,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SortOrder {
    #[default]
    Asc,
    Desc,
}

#[derive(sqlx::FromRow)]
struct EventRow {
    id: String,
    name: String,
    start_at: Option<DateTime<Utc>>,
    finish_at: Option<DateTime<Utc>>,
    duration_ms: Option<i64>,
    recurrent: bool,
    highlighted: bool,
    trending: bool,
    approved: bool,
    attending: Option<bool>,
    community_id: Option<String>,
    user_creator: Option<String>,
    coordinates_x: Option<i32>,
    coordinates_y: Option<i32>,
    description: Option<String>,
    raw: Value,
    // Only the list query selects the folded `count(*) OVER()`; every other
    // EVENT_COLUMNS query omits the column and decodes with total_count = 0.
    #[sqlx(default)]
    total_count: i64,
}

impl EventsComponent {
    pub fn new(pool: PgPool, rewrite_domain: Option<String>) -> Self {
        Self {
            pool,
            rewrite_domain,
        }
    }

    fn build_where(f: &EventListFilters, binds: &mut Vec<EventBind>) -> String {
        let mut sql = String::from(" WHERE 1=1");
        let now = Utc::now();

        let nf = EFF_NEXT_FINISH_SQL;
        let ns = EFF_NEXT_START_SQL;

        let is_owner = f.owner;
        if is_owner {
            match f.user.as_deref() {
                Some(u) => {
                    let p = next_placeholder(binds, EventBind::Text(u.to_lowercase()));
                    sql.push_str(&format!(" AND lower(user_creator) = {}", p));
                }
                None => sql.push_str(" AND FALSE"),
            }
        }

        match f.list {
            EventListType::All | EventListType::Relevance => {}
            EventListType::Active => {
                let p = next_placeholder(binds, EventBind::Time(now));
                sql.push_str(&format!(" AND {nf} > {p}"));
            }
            EventListType::Live => {
                let p1 = next_placeholder(binds, EventBind::Time(now));
                let p2 = next_placeholder(binds, EventBind::Time(now));
                sql.push_str(&format!(" AND {nf} > {p1} AND {ns} < {p2}"));
            }
            EventListType::Upcoming => {
                let p1 = next_placeholder(binds, EventBind::Time(now));
                let p2 = next_placeholder(binds, EventBind::Time(now));
                sql.push_str(&format!(" AND {nf} > {p1} AND {ns} > {p2}"));
            }
        }

        if !is_owner {
            if let Some(c) = &f.creator {
                let p = next_placeholder(binds, EventBind::Text(c.to_lowercase()));
                sql.push_str(&format!(" AND lower(user_creator) = {}", p));
            }
        }
        if let Some(eid) = &f.estate_id {
            let p = next_placeholder(binds, EventBind::Text(eid.clone()));
            sql.push_str(&format!(" AND raw->>'estate_id' = {}", p));
        }
        if let Some(h) = f.highlighted {
            if h {
                sql.push_str(" AND highlighted IS TRUE");
            }
        }
        if let Some(w) = f.world {
            if w {
                sql.push_str(" AND COALESCE((raw->>'world')::boolean, false) IS TRUE");
            } else {
                sql.push_str(" AND COALESCE((raw->>'world')::boolean, false) IS FALSE");
            }
        }
        if !f.world_names.is_empty() {
            let p = next_placeholder(binds, EventBind::TextArray(f.world_names.clone()));
            sql.push_str(&format!(" AND raw->>'server' = ANY({})", p));
        }
        if !f.positions.is_empty() {
            let mut clauses: Vec<String> = Vec::new();
            for (x, y) in &f.positions {
                let px = next_placeholder(binds, EventBind::Int(*x));
                let py = next_placeholder(binds, EventBind::Int(*y));
                clauses.push(format!(
                    "(coordinates_x = {} AND coordinates_y = {})",
                    px, py
                ));
            }
            sql.push_str(&format!(" AND ({})", clauses.join(" OR ")));
        }

        let has_places = !f.places_ids.is_empty();
        let has_community = f.community_id.is_some();
        if has_places && has_community {
            let pp = next_placeholder(binds, EventBind::TextArray(f.places_ids.clone()));
            let pc = next_placeholder(binds, EventBind::Text(f.community_id.clone().unwrap()));
            sql.push_str(&format!(
                " AND (raw->>'place_id' = ANY({pp}) OR community_id = {pc})"
            ));
        } else if has_places {
            let pp = next_placeholder(binds, EventBind::TextArray(f.places_ids.clone()));
            sql.push_str(&format!(" AND raw->>'place_id' = ANY({pp})"));
        } else if has_community {
            let pc = next_placeholder(binds, EventBind::Text(f.community_id.clone().unwrap()));
            sql.push_str(&format!(" AND community_id = {pc}"));
        }

        if let Some(from) = f.from {
            let p = next_placeholder(binds, EventBind::Time(from));
            sql.push_str(&format!(" AND {ns} >= {p}"));
        }
        if let Some(to) = f.to {
            let p = next_placeholder(binds, EventBind::Time(to));
            sql.push_str(&format!(" AND {ns} < {p}"));
        }

        if let Some(s) = &f.search {
            let p = next_placeholder(binds, EventBind::Text(to_tsquery(s)));
            sql.push_str(&format!(
                " AND ts_rank_cd({tsv}, to_tsquery('english', {p})) > 0",
                tsv = TEXTSEARCH_EXPR
            ));
        }

        if !is_owner {
            match (f.admin, f.rejected) {
                (_, Some(true)) => {
                    sql.push_str(" AND COALESCE((raw->>'rejected')::boolean, false) IS TRUE")
                }
                (_, Some(false)) => {
                    sql.push_str(" AND COALESCE((raw->>'rejected')::boolean, false) IS FALSE")
                }
                (false, None) => {
                    sql.push_str(" AND COALESCE((raw->>'rejected')::boolean, false) IS FALSE")
                }
                (true, None) => {}
            }

            match (f.admin, f.approved) {
                (_, Some(true)) => sql.push_str(" AND approved IS TRUE"),
                (_, Some(false)) => sql.push_str(" AND approved IS FALSE"),
                (false, None) => sql.push_str(" AND approved IS TRUE"),
                (true, None) => {}
            }
        }

        if f.only_attendee {
            if let Some(u) = &f.user {
                let p1 = next_placeholder(binds, EventBind::Text(u.to_lowercase()));
                let p2 = next_placeholder(binds, EventBind::Text(u.to_lowercase()));
                sql.push_str(&format!(
                    " AND (id IN (SELECT event_id FROM event_attendance_local WHERE signer = {p1} AND action = 'going') OR raw->'latest_attendees' ? {p2})"
                ));
            }
        }

        match (f.admin, f.deleted) {
            (true, Some(true)) => sql.push_str(DELETED_ONLY_SQL),
            _ => sql.push_str(NOT_DELETED_SQL),
        }

        sql
    }

    pub async fn list(&self, f: &EventListFilters) -> Result<(Vec<EventRecord>, i64), ApiError> {
        self.query(f, true).await
    }

    pub async fn query(
        &self,
        f: &EventListFilters,
        with_total: bool,
    ) -> Result<(Vec<EventRecord>, i64), ApiError> {
        let mut binds: Vec<EventBind> = Vec::new();
        let sql = build_list_sql(f, with_total, &mut binds);

        let mut q = sqlx::query_as::<_, EventRow>(sqlx::AssertSqlSafe(sql));
        for b in &binds {
            q = bind_one(q, b);
        }
        let rows = q.fetch_all(&self.pool).await?;

        let local_attending = match &f.user {
            Some(u) => self.local_attending_set(u).await?,
            None => Vec::new(),
        };

        // The window aggregate carries the pre-LIMIT/OFFSET total on every returned row, so a
        // non-empty page needs no second query. An empty page cannot report it: when the offset
        // ran past the end (or limit <= 0) the true total is still recoverable only by counting.
        let total = if with_total {
            match rows.first() {
                Some(r) => r.total_count,
                None if f.offset > 0 || f.limit <= 0 => self.count_only(f).await,
                None => 0,
            }
        } else {
            0
        };

        let user = f.user.as_deref();
        let records = rows
            .into_iter()
            .map(|r| event_row_to_record(r, user, &local_attending, self.rewrite_domain.as_deref()))
            .collect();
        Ok((records, total))
    }

    async fn count_only(&self, f: &EventListFilters) -> i64 {
        let mut cbinds: Vec<EventBind> = Vec::new();
        let count_sql = count_only_sql(f, &mut cbinds);
        let mut cq = sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(count_sql));
        for b in &cbinds {
            cq = bind_one_scalar(cq, b);
        }
        cq.fetch_one(&self.pool).await.unwrap_or(0)
    }

    async fn local_attending_set(&self, user: &str) -> Result<Vec<String>, ApiError> {
        let rows: Vec<(String,)> = sqlx::query_as(
            "SELECT event_id FROM event_attendance_local \
             WHERE signer = $1 AND action = 'going'",
        )
        .bind(user.to_lowercase())
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(|(id,)| id).collect())
    }

    pub async fn get(&self, event_id: &str) -> Result<Option<EventRecord>, ApiError> {
        let row = sqlx::query_as::<_, EventRow>(sqlx::AssertSqlSafe(format!(
            "SELECT {EVENT_COLUMNS} FROM event WHERE id = $1"
        )))
        .bind(event_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| event_row_to_record(r, None, &[], self.rewrite_domain.as_deref())))
    }

    pub async fn attending(&self, user: &str) -> Result<Vec<EventRecord>, ApiError> {
        let user_lc = user.to_lowercase();
        let rows = sqlx::query_as::<_, EventRow>(sqlx::AssertSqlSafe(attending_sql()))
            .bind(&user_lc)
            .fetch_all(&self.pool)
            .await?;
        let all_ids: Vec<String> = rows.iter().map(|r| r.id.clone()).collect();
        Ok(rows
            .into_iter()
            .map(|r| {
                event_row_to_record(r, Some(&user_lc), &all_ids, self.rewrite_domain.as_deref())
            })
            .collect())
    }

    pub async fn is_user_attending(&self, event_id: &str, user: &str) -> Result<bool, ApiError> {
        let user_lc = user.to_lowercase();
        let row: Option<(bool,)> = sqlx::query_as(
            "SELECT EXISTS( \
               SELECT 1 FROM event_attendance_local \
               WHERE event_id = $1 AND signer = $2 AND action = 'going' \
             ) OR EXISTS( \
               SELECT 1 FROM event WHERE id = $1 AND raw->'latest_attendees' ? $2 \
             )",
        )
        .bind(event_id)
        .bind(&user_lc)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|(b,)| b).unwrap_or(false))
    }

    pub async fn count_approved(&self) -> Result<i64, ApiError> {
        let row: (i64,) = sqlx::query_as("SELECT count(*) FROM event WHERE approved IS TRUE")
            .fetch_one(&self.pool)
            .await?;
        Ok(row.0)
    }

    pub async fn moderation_pending(&self, limit: i64) -> Result<Vec<EventRecord>, ApiError> {
        let limit = limit.clamp(0, 500);
        let rows = sqlx::query_as::<_, EventRow>(sqlx::AssertSqlSafe(format!(
            "SELECT {EVENT_COLUMNS} FROM event \
             WHERE approved IS NOT TRUE \
                OR COALESCE((raw->>'rejected')::boolean, false) IS TRUE \
             ORDER BY next_start_at DESC NULLS LAST, id ASC \
             LIMIT $1"
        )))
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| event_row_to_record(r, None, &[], self.rewrite_domain.as_deref()))
            .collect())
    }

    pub async fn sitemap_event_ids(&self, page: i64) -> Result<Vec<String>, ApiError> {
        let rows: Vec<(String,)> = sqlx::query_as(SITEMAP_SQL)
            .bind(page * SITEMAP_ITEMS_PER_PAGE)
            .bind(SITEMAP_ITEMS_PER_PAGE)
            .fetch_all(&self.pool)
            .await?;
        Ok(rows.into_iter().map(|(id,)| id).collect())
    }

    pub async fn exists(&self, event_id: &str) -> Result<bool, ApiError> {
        let row: Option<(String,)> = sqlx::query_as("SELECT id FROM event WHERE id = $1")
            .bind(event_id)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.is_some())
    }

    pub async fn upsert_local(
        &self,
        event_id: &str,
        signer: &str,
        payload: Value,
    ) -> Result<Value, ApiError> {
        let signed_at = Utc::now();
        let row: (Value,) = sqlx::query_as(
            "INSERT INTO events_local (id, signer, signed_payload, signed_at) \
             VALUES ($1, $2, $3, $4) \
             ON CONFLICT (id) DO UPDATE \
               SET signer = EXCLUDED.signer, \
                   signed_payload = events_local.signed_payload || EXCLUDED.signed_payload, \
                   signed_at = EXCLUDED.signed_at, \
                   updated_at = now() \
             RETURNING signed_payload",
        )
        .bind(event_id)
        .bind(signer.to_lowercase())
        .bind(payload)
        .bind(signed_at)
        .fetch_one(&self.pool)
        .await?;
        Ok(row.0)
    }

    pub async fn get_local(&self, event_id: &str) -> Result<Option<Value>, ApiError> {
        let row: Option<(Value,)> =
            sqlx::query_as("SELECT signed_payload FROM events_local WHERE id = $1")
                .bind(event_id)
                .fetch_optional(&self.pool)
                .await?;
        Ok(row.map(|(v,)| v))
    }

    pub async fn exists_visible(&self, event_id: &str, signer: &str) -> Result<bool, ApiError> {
        let signer_lc = signer.to_lowercase();
        let row: Option<(bool, Option<String>, Value)> =
            sqlx::query_as("SELECT approved, user_creator, raw FROM event WHERE id = $1")
                .bind(event_id)
                .fetch_optional(&self.pool)
                .await?;
        let Some((approved, user_creator, raw)) = row else {
            return Ok(false);
        };
        if raw_is_soft_deleted(&raw) {
            return Ok(false);
        }
        let rejected = raw
            .get("rejected")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let owner = raw
            .get("user")
            .and_then(|v| v.as_str())
            .map(String::from)
            .or(user_creator)
            .map(|u| u.to_lowercase());
        let is_owner = owner.as_deref() == Some(signer_lc.as_str());
        Ok((approved && !rejected) || is_owner)
    }

    pub async fn get_raw(
        &self,
        event_id: &str,
    ) -> Result<Option<(Value, Option<String>)>, ApiError> {
        let row: Option<(Value, Option<String>)> =
            sqlx::query_as("SELECT raw, user_creator FROM event WHERE id = $1")
                .bind(event_id)
                .fetch_optional(&self.pool)
                .await?;
        Ok(row)
    }

    pub async fn write_event(
        &self,
        id: &str,
        raw: &Value,
        signer: &str,
    ) -> Result<EventRecord, ApiError> {
        let name = raw.get("name").and_then(|v| v.as_str()).unwrap_or("");
        let start_at = raw.get("start_at").and_then(|v| v.as_str());
        let finish_at = raw.get("finish_at").and_then(|v| v.as_str());
        let next_start_at = raw.get("next_start_at").and_then(|v| v.as_str());
        let next_finish_at = raw.get("next_finish_at").and_then(|v| v.as_str());
        let duration = raw.get("duration").and_then(|v| v.as_i64());
        let recurrent = raw
            .get("recurrent")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let highlighted = raw
            .get("highlighted")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let trending = raw
            .get("trending")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let approved = raw
            .get("approved")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let community_id = raw.get("community_id").and_then(|v| v.as_str());
        let user_creator = raw.get("user").and_then(|v| v.as_str());
        let x = raw.get("x").and_then(|v| v.as_i64()).map(|v| v as i32);
        let y = raw.get("y").and_then(|v| v.as_i64()).map(|v| v as i32);
        let description = raw.get("description").and_then(|v| v.as_str());

        sqlx::query(EVENT_WRITE_SQL)
            .bind(id)
            .bind(name)
            .bind(start_at)
            .bind(finish_at)
            .bind(next_start_at)
            .bind(next_finish_at)
            .bind(duration)
            .bind(recurrent)
            .bind(highlighted)
            .bind(trending)
            .bind(approved)
            .bind(community_id)
            .bind(user_creator)
            .bind(x)
            .bind(y)
            .bind(description)
            .bind(raw)
            .execute(&self.pool)
            .await?;

        self.upsert_local(id, signer, raw.clone()).await?;

        self.get(id)
            .await?
            .ok_or_else(|| ApiError::internal("event vanished after write"))
    }
}

const EVENT_WRITE_SQL: &str = r#"
    INSERT INTO event
        (id, name, start_at, finish_at, next_start_at, next_finish_at, duration_ms,
         recurrent, highlighted, trending, approved, community_id,
         user_creator, coordinates_x, coordinates_y, description, raw, fetched_at)
    VALUES
        ($1, $2, $3::timestamptz, $4::timestamptz, $5::timestamptz, $6::timestamptz, $7,
         $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, now())
    ON CONFLICT (id) DO UPDATE SET
        name           = EXCLUDED.name,
        start_at       = EXCLUDED.start_at,
        finish_at      = EXCLUDED.finish_at,
        next_start_at  = EXCLUDED.next_start_at,
        next_finish_at = EXCLUDED.next_finish_at,
        duration_ms    = EXCLUDED.duration_ms,
        recurrent      = EXCLUDED.recurrent,
        highlighted    = EXCLUDED.highlighted,
        trending       = EXCLUDED.trending,
        approved       = EXCLUDED.approved,
        community_id   = EXCLUDED.community_id,
        user_creator   = EXCLUDED.user_creator,
        coordinates_x  = EXCLUDED.coordinates_x,
        coordinates_y  = EXCLUDED.coordinates_y,
        description    = EXCLUDED.description,
        raw            = EXCLUDED.raw,
        fetched_at     = now()
"#;

pub const SITEMAP_ITEMS_PER_PAGE: i64 = 100;

/// Column list shared by every event read query (mirrors `PLACE_COLUMNS` in catalyrst-places).
const EVENT_COLUMNS: &str =
    "id, name, start_at, finish_at, duration_ms, recurrent, highlighted, trending, \
     approved, attending, community_id, user_creator, coordinates_x, coordinates_y, \
     description, raw";

const NOT_DELETED_SQL: &str = " AND (raw->>'deleted_by_user') IS DISTINCT FROM 'true' \
     AND (raw->>'deleted_by_admin') IS DISTINCT FROM 'true'";

const DELETED_ONLY_SQL: &str = " AND ((raw->>'deleted_by_user') = 'true' \
     OR (raw->>'deleted_by_admin') = 'true')";

const EFF_NEXT_START_SQL: &str = "COALESCE((SELECT min((d.value #>> '{}')::timestamptz) \
     FROM jsonb_array_elements(COALESCE(raw->'recurrent_dates', '[]'::jsonb)) d \
     WHERE (d.value #>> '{}')::timestamptz \
         + COALESCE(duration_ms, 0) * interval '1 millisecond' > now()), \
     next_start_at, start_at)";

const EFF_NEXT_FINISH_SQL: &str = "COALESCE((SELECT min((d.value #>> '{}')::timestamptz) \
     FROM jsonb_array_elements(COALESCE(raw->'recurrent_dates', '[]'::jsonb)) d \
     WHERE (d.value #>> '{}')::timestamptz \
         + COALESCE(duration_ms, 0) * interval '1 millisecond' > now()) \
         + COALESCE(duration_ms, 0) * interval '1 millisecond', \
     next_finish_at, finish_at)";

fn next_start_order_by(order: SortOrder) -> String {
    let dir = match order {
        SortOrder::Asc => "ASC",
        SortOrder::Desc => "DESC",
    };
    format!(" ORDER BY {EFF_NEXT_START_SQL} {dir} NULLS LAST, id ASC")
}

fn attending_sql() -> String {
    format!(
        "SELECT {EVENT_COLUMNS} FROM event \
         WHERE {EFF_NEXT_FINISH_SQL} > now() \
           AND COALESCE((raw->>'rejected')::boolean, false) IS FALSE \
           AND (raw->>'deleted_by_user') IS DISTINCT FROM 'true' \
           AND (raw->>'deleted_by_admin') IS DISTINCT FROM 'true' \
           AND ( \
             id IN (SELECT event_id FROM event_attendance_local WHERE signer = $1 AND action = 'going') \
             OR raw->'latest_attendees' ? $1 \
           ) \
         ORDER BY {EFF_NEXT_START_SQL} ASC NULLS LAST, id ASC"
    )
}

const SITEMAP_SQL: &str = "SELECT id FROM event WHERE approved IS TRUE \
     AND (raw->>'deleted_by_user') IS DISTINCT FROM 'true' \
     AND (raw->>'deleted_by_admin') IS DISTINCT FROM 'true' \
     ORDER BY (raw->>'created_at')::timestamptz ASC NULLS LAST, id ASC \
     OFFSET $1 LIMIT $2";

fn raw_is_soft_deleted(raw: &Value) -> bool {
    raw.get("deleted_by_user")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
        || raw
            .get("deleted_by_admin")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
}

const TEXTSEARCH_EXPR: &str = "(setweight(to_tsvector('english', coalesce(name,'')), 'A') || \
     setweight(to_tsvector('english', coalesce(raw->>'user_name','')), 'B') || \
     setweight(to_tsvector('english', coalesce(raw->>'estate_name','')), 'B') || \
     setweight(to_tsvector('english', coalesce(description,'')), 'D'))";

fn to_tsquery(input: &str) -> String {
    let terms: Vec<String> = input
        .split_whitespace()
        .map(|t| {
            t.chars()
                .filter(|c| c.is_alphanumeric())
                .collect::<String>()
        })
        .filter(|t| !t.is_empty())
        .map(|t| format!("{}:*", t))
        .collect();
    terms.join(" & ")
}

enum EventBind {
    Text(String),
    Int(i32),
    Int64(i64),
    Time(DateTime<Utc>),
    TextArray(Vec<String>),
}

fn next_placeholder(binds: &mut Vec<EventBind>, bind: EventBind) -> String {
    binds.push(bind);
    format!("${}", binds.len())
}

/// Assemble the paginated list query. When `with_total`, the pre-LIMIT row count rides along as
/// `count(*) OVER() AS total_count` -- same semantics as a separate `count(*)` over the same
/// WHERE, no extra bind parameters, and it changes ONLY the select list.
fn build_list_sql(f: &EventListFilters, with_total: bool, binds: &mut Vec<EventBind>) -> String {
    let where_sql = EventsComponent::build_where(f, binds);
    let extra = if with_total {
        ", count(*) OVER() AS total_count"
    } else {
        ""
    };

    let order_clause = if let Some(s) = &f.search {
        let dir = if matches!(f.order, SortOrder::Asc) {
            "ASC"
        } else {
            "DESC"
        };
        let p = next_placeholder(binds, EventBind::Text(to_tsquery(s)));
        format!(
            " ORDER BY ts_rank_cd({tsv}, to_tsquery('english', {p})) {dir}, id ASC",
            tsv = TEXTSEARCH_EXPR
        )
    } else {
        next_start_order_by(f.order)
    };

    let lim_p = next_placeholder(binds, EventBind::Int64(f.limit.max(0)));
    let off_p = next_placeholder(binds, EventBind::Int64(f.offset.max(0)));
    format!(
        "SELECT {EVENT_COLUMNS}{extra} FROM event{where_sql}{order_clause} LIMIT {lim_p} OFFSET {off_p}"
    )
}

/// The rare-path count, unchanged from the old two-query design: recovers the true total for an
/// empty page that ran past the offset.
fn count_only_sql(f: &EventListFilters, binds: &mut Vec<EventBind>) -> String {
    let where_sql = EventsComponent::build_where(f, binds);
    format!("SELECT count(*) FROM event{where_sql}")
}

fn bind_one<'q>(
    q: sqlx::query::QueryAs<'q, sqlx::Postgres, EventRow, sqlx::postgres::PgArguments>,
    b: &'q EventBind,
) -> sqlx::query::QueryAs<'q, sqlx::Postgres, EventRow, sqlx::postgres::PgArguments> {
    match b {
        EventBind::Text(s) => q.bind(s),
        EventBind::Int(i) => q.bind(i),
        EventBind::Int64(i) => q.bind(i),
        EventBind::Time(t) => q.bind(t),
        EventBind::TextArray(v) => q.bind(v),
    }
}

fn bind_one_scalar<'q>(
    q: sqlx::query::QueryScalar<'q, sqlx::Postgres, i64, sqlx::postgres::PgArguments>,
    b: &'q EventBind,
) -> sqlx::query::QueryScalar<'q, sqlx::Postgres, i64, sqlx::postgres::PgArguments> {
    match b {
        EventBind::Text(s) => q.bind(s),
        EventBind::Int(i) => q.bind(i),
        EventBind::Int64(i) => q.bind(i),
        EventBind::Time(t) => q.bind(t),
        EventBind::TextArray(v) => q.bind(v),
    }
}

fn rewrite_asset_host(url: &str, domain: &str) -> Option<String> {
    let rest = url.strip_prefix("https://")?;
    let (host, path) = rest.split_once('/')?;
    let bucket = host.strip_suffix(".decentraland.org")?;
    bucket
        .starts_with("events-assets-")
        .then(|| format!("https://{bucket}.{domain}/{path}"))
}

fn rewrite_asset_url(url: &str, domain: Option<&str>) -> String {
    domain
        .and_then(|d| rewrite_asset_host(url, d))
        .unwrap_or_else(|| url.to_string())
}

fn event_row_to_record(
    r: EventRow,
    attending_user: Option<&str>,
    local_attending: &[String],
    rewrite_domain: Option<&str>,
) -> EventRecord {
    let raw = &r.raw;
    let x = r.coordinates_x.unwrap_or(0);
    let y = r.coordinates_y.unwrap_or(0);
    let image = raw
        .get("image")
        .and_then(|v| v.as_str())
        .map(|s| rewrite_asset_url(s, rewrite_domain));
    let image_vertical = raw.get("image_vertical").cloned().map(|v| match v {
        serde_json::Value::String(s) => {
            serde_json::Value::String(rewrite_asset_url(&s, rewrite_domain))
        }
        other => other,
    });
    let server = raw.get("server").and_then(|v| v.as_str()).map(String::from);
    let url = raw.get("url").and_then(|v| v.as_str()).map(String::from);
    let user = raw
        .get("user")
        .and_then(|v| v.as_str())
        .map(String::from)
        .or_else(|| r.user_creator.clone());
    let user_name = raw
        .get("user_name")
        .and_then(|v| v.as_str())
        .map(String::from);
    let estate_id = raw
        .get("estate_id")
        .and_then(|v| v.as_str())
        .map(String::from);
    let scene_name = raw
        .get("scene_name")
        .and_then(|v| v.as_str())
        .map(String::from);
    let estate_name = raw
        .get("estate_name")
        .and_then(|v| v.as_str())
        .map(String::from)
        .or_else(|| scene_name.clone());
    let all_day = raw
        .get("all_day")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let world = raw.get("world").and_then(|v| v.as_bool()).unwrap_or(false);
    let duration = r
        .duration_ms
        .or_else(|| raw.get("duration").and_then(|v| v.as_i64()));
    let recurrent_dates: Vec<DateTime<Utc>> = raw
        .get("recurrent_dates")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|x| parse_dt(Some(x))).collect())
        .unwrap_or_default();
    let occurrence_span = chrono::Duration::milliseconds(duration.unwrap_or(0));
    let now = Utc::now();
    let computed_next = recurrent_dates
        .iter()
        .copied()
        .filter(|d| *d + occurrence_span > now)
        .min();
    let next_start_at = computed_next
        .or_else(|| parse_dt(raw.get("next_start_at")))
        .or(r.start_at);
    let next_finish_at = computed_next
        .map(|d| d + occurrence_span)
        .or_else(|| parse_dt(raw.get("next_finish_at")))
        .or(r.finish_at);
    let live = match (next_start_at, duration) {
        (Some(ns), Some(d)) => now >= ns && now < ns + chrono::Duration::milliseconds(d),
        _ => raw.get("live").and_then(|v| v.as_bool()).unwrap_or(false),
    };
    let attending = match attending_user {
        Some(u) => {
            local_attending.iter().any(|id| id == &r.id)
                || raw
                    .get("latest_attendees")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter().any(|x| {
                            x.as_str()
                                .map(|s| s.eq_ignore_ascii_case(u))
                                .unwrap_or(false)
                        })
                    })
                    .unwrap_or(false)
        }
        None => r.attending.unwrap_or(false),
    };

    EventRecord {
        id: r.id,
        name: r.name,
        image,
        image_vertical,
        description: r
            .description
            .or_else(|| {
                raw.get("description")
                    .and_then(|v| v.as_str())
                    .map(String::from)
            })
            .map(|d| sanitize_event_description(&d)),
        start_at: r.start_at,
        finish_at: r.finish_at,
        next_start_at,
        next_finish_at,
        duration,
        all_day,
        x,
        y,
        server,
        url,
        user,
        user_name,
        estate_id,
        estate_name,
        scene_name,
        approved: r.approved,
        rejected: raw
            .get("rejected")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        highlighted: r.highlighted,
        trending: r.trending,
        world,
        recurrent: r.recurrent,
        recurrent_frequency: raw
            .get("recurrent_frequency")
            .and_then(|v| v.as_str())
            .map(String::from),
        recurrent_weekday_mask: raw
            .get("recurrent_weekday_mask")
            .and_then(|v| v.as_i64())
            .unwrap_or(0),
        recurrent_month_mask: raw
            .get("recurrent_month_mask")
            .and_then(|v| v.as_i64())
            .unwrap_or(0),
        recurrent_interval: raw
            .get("recurrent_interval")
            .and_then(|v| v.as_i64())
            .unwrap_or(0),
        recurrent_setpos: raw.get("recurrent_setpos").and_then(|v| v.as_i64()),
        recurrent_monthday: raw.get("recurrent_monthday").and_then(|v| v.as_i64()),
        recurrent_count: raw.get("recurrent_count").and_then(|v| v.as_i64()),
        recurrent_until: parse_dt(raw.get("recurrent_until")),
        recurrent_dates,
        categories: raw
            .get("categories")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|x| x.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default(),
        schedules: raw
            .get("schedules")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|x| x.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default(),
        total_attendees: raw
            .get("total_attendees")
            .and_then(|v| v.as_i64())
            .unwrap_or(0),
        latest_attendees: raw
            .get("latest_attendees")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|x| x.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default(),
        coordinates: [x, y],
        position: [x, y],
        live,
        attending,
        place_id: raw
            .get("place_id")
            .and_then(|v| v.as_str())
            .map(String::from),
        community_id: r.community_id,
        created_at: parse_dt(raw.get("created_at")),
        updated_at: parse_dt(raw.get("updated_at")),
        approved_by: raw_str(raw, "approved_by"),
        rejected_by: raw_str(raw, "rejected_by"),
        rejection_reason: raw_str(raw, "rejection_reason"),
        deleted_by_user: raw
            .get("deleted_by_user")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        deleted_by_admin: raw
            .get("deleted_by_admin")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        deleted_by: raw_str(raw, "deleted_by"),
        deleted_at: parse_dt(raw.get("deleted_at")),
        deleted_reason: raw_str(raw, "deleted_reason"),
        previous_place_id: raw_str(raw, "previous_place_id"),
        connected_addresses: None,
    }
}

fn raw_str(raw: &Value, key: &str) -> Option<String> {
    raw.get(key).and_then(|v| v.as_str()).map(String::from)
}

fn parse_dt(v: Option<&Value>) -> Option<DateTime<Utc>> {
    let s = v?.as_str()?;
    DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|d| d.with_timezone(&Utc))
}

#[cfg(test)]
#[path = "events_tests.rs"]
mod tests;
