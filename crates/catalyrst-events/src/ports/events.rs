use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::PgPool;

use crate::http::response::ApiError;
use crate::schemas::EventRecord;

pub struct EventsComponent {
    pool: PgPool,
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
}

impl EventsComponent {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
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
            EventListType::All => {}
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
            match f.rejected {
                Some(true) => {
                    sql.push_str(" AND COALESCE((raw->>'rejected')::boolean, false) IS TRUE")
                }
                Some(false) => {
                    sql.push_str(" AND COALESCE((raw->>'rejected')::boolean, false) IS FALSE")
                }
                None => sql.push_str(" AND COALESCE((raw->>'rejected')::boolean, false) IS FALSE"),
            }

            if !f.approved_visibility() {
                sql.push_str(" AND approved IS TRUE");
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

        sql.push_str(NOT_DELETED_SQL);

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
        let where_sql = Self::build_where(f, &mut binds);

        let base = format!(
            "SELECT id, name, start_at, finish_at, duration_ms, recurrent, highlighted, trending, \
             approved, attending, community_id, user_creator, coordinates_x, coordinates_y, \
             description, raw FROM event{}",
            where_sql
        );

        let order_clause = if let Some(s) = &f.search {
            let dir = if matches!(f.order, SortOrder::Asc) {
                "ASC"
            } else {
                "DESC"
            };
            let p = next_placeholder(&mut binds, EventBind::Text(to_tsquery(s)));
            format!(
                " ORDER BY ts_rank_cd({tsv}, to_tsquery('english', {p})) {dir}",
                tsv = TEXTSEARCH_EXPR
            )
        } else {
            let order_dir = match f.order {
                SortOrder::Asc => "ASC",
                SortOrder::Desc => "DESC",
            };
            format!(" ORDER BY {EFF_NEXT_START_SQL} {order_dir} NULLS LAST")
        };

        let lim_p = next_placeholder(&mut binds, EventBind::Int64(f.limit.max(0)));
        let off_p = next_placeholder(&mut binds, EventBind::Int64(f.offset.max(0)));
        let sql = format!("{base}{order_clause} LIMIT {lim_p} OFFSET {off_p}");

        let mut q = sqlx::query_as::<_, EventRow>(sqlx::AssertSqlSafe(sql));
        for b in &binds {
            q = bind_one(q, b);
        }
        let rows = q.fetch_all(&self.pool).await?;

        let local_attending = match &f.user {
            Some(u) => self.local_attending_set(u).await?,
            None => Vec::new(),
        };

        let total = if with_total {
            let mut cbinds: Vec<EventBind> = Vec::new();
            let cwhere = Self::build_where(f, &mut cbinds);
            let count_sql = format!("SELECT count(*) FROM event{}", cwhere);
            let mut cq = sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(count_sql));
            for b in &cbinds {
                cq = bind_one_scalar(cq, b);
            }
            cq.fetch_one(&self.pool).await.unwrap_or(0)
        } else {
            0
        };

        let user = f.user.as_deref();
        let records = rows
            .into_iter()
            .map(|r| event_row_to_record(r, user, &local_attending))
            .collect();
        Ok((records, total))
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
        let row = sqlx::query_as::<_, EventRow>(
            "SELECT id, name, start_at, finish_at, duration_ms, recurrent, highlighted, trending, \
             approved, attending, community_id, user_creator, coordinates_x, coordinates_y, \
             description, raw FROM event WHERE id = $1",
        )
        .bind(event_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| event_row_to_record(r, None, &[])))
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
            .map(|r| event_row_to_record(r, Some(&user_lc), &all_ids))
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
        let rows = sqlx::query_as::<_, EventRow>(
            "SELECT id, name, start_at, finish_at, duration_ms, recurrent, highlighted, trending, \
             approved, attending, community_id, user_creator, coordinates_x, coordinates_y, \
             description, raw FROM event \
             WHERE approved IS NOT TRUE \
                OR COALESCE((raw->>'rejected')::boolean, false) IS TRUE \
             ORDER BY next_start_at DESC NULLS LAST \
             LIMIT $1",
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| event_row_to_record(r, None, &[]))
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
}

pub const SITEMAP_ITEMS_PER_PAGE: i64 = 100;

const NOT_DELETED_SQL: &str = " AND (raw->>'deleted_by_user') IS DISTINCT FROM 'true' \
     AND (raw->>'deleted_by_admin') IS DISTINCT FROM 'true'";

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

fn attending_sql() -> String {
    format!(
        "SELECT id, name, start_at, finish_at, duration_ms, recurrent, highlighted, trending, \
         approved, attending, community_id, user_creator, coordinates_x, coordinates_y, \
         description, raw FROM event \
         WHERE {EFF_NEXT_FINISH_SQL} > now() \
           AND COALESCE((raw->>'rejected')::boolean, false) IS FALSE \
           AND (raw->>'deleted_by_user') IS DISTINCT FROM 'true' \
           AND (raw->>'deleted_by_admin') IS DISTINCT FROM 'true' \
           AND ( \
             id IN (SELECT event_id FROM event_attendance_local WHERE signer = $1 AND action = 'going') \
             OR raw->'latest_attendees' ? $1 \
           ) \
         ORDER BY {EFF_NEXT_START_SQL} ASC NULLS LAST"
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

impl EventListFilters {
    fn approved_visibility(&self) -> bool {
        false
    }
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

fn event_row_to_record(
    r: EventRow,
    attending_user: Option<&str>,
    local_attending: &[String],
) -> EventRecord {
    let raw = &r.raw;
    let x = r.coordinates_x.unwrap_or(0);
    let y = r.coordinates_y.unwrap_or(0);
    let image = raw.get("image").and_then(|v| v.as_str()).map(String::from);
    let image_vertical = raw.get("image_vertical").cloned();
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
        description: r.description.or_else(|| {
            raw.get("description")
                .and_then(|v| v.as_str())
                .map(String::from)
        }),
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
        connected_addresses: None,
    }
}

fn parse_dt(v: Option<&Value>) -> Option<DateTime<Utc>> {
    let s = v?.as_str()?;
    DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|d| d.with_timezone(&Utc))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const DELETED_USER_CLAUSE: &str = "(raw->>'deleted_by_user') IS DISTINCT FROM 'true'";
    const DELETED_ADMIN_CLAUSE: &str = "(raw->>'deleted_by_admin') IS DISTINCT FROM 'true'";

    #[test]
    fn build_where_excludes_soft_deleted_for_every_list_type() {
        for list in [
            EventListType::All,
            EventListType::Active,
            EventListType::Live,
            EventListType::Upcoming,
        ] {
            let f = EventListFilters {
                list,
                ..Default::default()
            };
            let mut binds = Vec::new();
            let sql = EventsComponent::build_where(&f, &mut binds);
            assert!(
                sql.contains(DELETED_USER_CLAUSE),
                "deleted_by_user guard missing for {list:?}: {sql}"
            );
            assert!(
                sql.contains(DELETED_ADMIN_CLAUSE),
                "deleted_by_admin guard missing for {list:?}: {sql}"
            );
        }
    }

    #[test]
    fn build_where_appends_soft_delete_guard_last() {
        let f = EventListFilters {
            creator: Some("0xABC".into()),
            community_id: Some("c1".into()),
            highlighted: Some(true),
            ..Default::default()
        };
        let mut binds = Vec::new();
        let sql = EventsComponent::build_where(&f, &mut binds);
        assert!(
            sql.ends_with(NOT_DELETED_SQL),
            "soft-delete guard must be the trailing clause: {sql}"
        );
    }

    #[test]
    fn sibling_builders_exclude_soft_deleted() {
        for sql in [attending_sql(), SITEMAP_SQL.to_string()] {
            assert!(
                sql.contains(DELETED_USER_CLAUSE),
                "missing user guard: {sql}"
            );
            assert!(
                sql.contains(DELETED_ADMIN_CLAUSE),
                "missing admin guard: {sql}"
            );
        }
    }

    #[test]
    fn build_where_date_filters_use_recomputed_occurrences() {
        for list in [
            EventListType::Active,
            EventListType::Live,
            EventListType::Upcoming,
        ] {
            let f = EventListFilters {
                list,
                ..Default::default()
            };
            let mut binds = Vec::new();
            let sql = EventsComponent::build_where(&f, &mut binds);
            assert!(
                sql.contains("recurrent_dates"),
                "stale snapshot columns must not gate {list:?}: {sql}"
            );
        }
        assert!(attending_sql().contains("recurrent_dates"));
    }

    fn row_with(raw: Value, start_at: Option<DateTime<Utc>>, duration_ms: Option<i64>) -> EventRow {
        EventRow {
            id: "e1".into(),
            name: "Weekly Show".into(),
            start_at,
            finish_at: start_at
                .map(|s| s + chrono::Duration::milliseconds(duration_ms.unwrap_or(0))),
            duration_ms,
            recurrent: true,
            highlighted: false,
            trending: false,
            approved: true,
            attending: None,
            community_id: None,
            user_creator: None,
            coordinates_x: Some(0),
            coordinates_y: Some(0),
            description: None,
            raw,
        }
    }

    #[test]
    fn record_recomputes_next_occurrence_from_recurrent_dates() {
        let now = Utc::now();
        let past = now - chrono::Duration::days(7);
        let future = now + chrono::Duration::days(1);
        let raw = json!({
            "next_start_at": past.to_rfc3339(),
            "next_finish_at": (past + chrono::Duration::hours(2)).to_rfc3339(),
            "recurrent_dates": [past.to_rfc3339(), future.to_rfc3339()],
        });
        let rec = event_row_to_record(
            row_with(
                raw,
                Some(past - chrono::Duration::days(30)),
                Some(7_200_000),
            ),
            None,
            &[],
        );
        assert_eq!(
            rec.next_start_at.map(|d| d.timestamp()),
            Some(future.timestamp())
        );
        assert_eq!(
            rec.next_finish_at.map(|d| d.timestamp()),
            Some((future + chrono::Duration::hours(2)).timestamp())
        );
        assert!(!rec.live);
    }

    #[test]
    fn record_marks_current_occurrence_live() {
        let now = Utc::now();
        let started = now - chrono::Duration::minutes(30);
        let raw = json!({ "recurrent_dates": [started.to_rfc3339()] });
        let rec = event_row_to_record(row_with(raw, Some(started), Some(7_200_000)), None, &[]);
        assert_eq!(
            rec.next_start_at.map(|d| d.timestamp()),
            Some(started.timestamp())
        );
        assert!(rec.live);
    }

    #[test]
    fn record_falls_back_to_snapshot_when_no_future_occurrence() {
        let now = Utc::now();
        let past = now - chrono::Duration::days(7);
        let raw = json!({
            "next_start_at": past.to_rfc3339(),
            "next_finish_at": (past + chrono::Duration::hours(2)).to_rfc3339(),
            "recurrent_dates": [past.to_rfc3339()],
        });
        let rec = event_row_to_record(row_with(raw, Some(past), Some(7_200_000)), None, &[]);
        assert_eq!(
            rec.next_start_at.map(|d| d.timestamp()),
            Some(past.timestamp())
        );
        assert!(!rec.live);
    }

    const APPROVED_CLAUSE: &str = "approved IS TRUE";
    const REJECTED_FALSE_CLAUSE: &str = "COALESCE((raw->>'rejected')::boolean, false) IS FALSE";

    #[test]
    fn build_where_owner_scopes_to_user_and_drops_status_filters() {
        let f = EventListFilters {
            owner: true,
            user: Some("0xABC".into()),
            ..Default::default()
        };
        let mut binds = Vec::new();
        let sql = EventsComponent::build_where(&f, &mut binds);
        assert!(
            sql.contains("lower(user_creator) = $1"),
            "owner listing must key on the auth user: {sql}"
        );
        assert!(
            !sql.contains(APPROVED_CLAUSE),
            "owner listing must not force approved-only: {sql}"
        );
        assert!(
            !sql.contains(REJECTED_FALSE_CLAUSE),
            "owner listing must not exclude rejected events: {sql}"
        );
        assert!(
            matches!(binds.first(), Some(EventBind::Text(u)) if u.as_str() == "0xabc"),
            "auth user must be bound lower-cased"
        );
    }

    #[test]
    fn build_where_owner_overrides_creator() {
        let f = EventListFilters {
            owner: true,
            user: Some("0xabc".into()),
            creator: Some("0xdef".into()),
            ..Default::default()
        };
        let mut binds = Vec::new();
        let sql = EventsComponent::build_where(&f, &mut binds);
        assert_eq!(
            sql.matches("lower(user_creator)").count(),
            1,
            "creator filter must be suppressed under owner: {sql}"
        );
        assert!(matches!(binds.first(), Some(EventBind::Text(u)) if u.as_str() == "0xabc"));
    }

    #[test]
    fn build_where_non_owner_keeps_status_filters() {
        let f = EventListFilters {
            user: Some("0xabc".into()),
            ..Default::default()
        };
        let mut binds = Vec::new();
        let sql = EventsComponent::build_where(&f, &mut binds);
        assert!(
            sql.contains(APPROVED_CLAUSE),
            "non-owner must force approved: {sql}"
        );
        assert!(
            sql.contains(REJECTED_FALSE_CLAUSE),
            "non-owner must exclude rejected: {sql}"
        );
    }

    #[test]
    fn build_where_owner_without_user_yields_no_rows() {
        let f = EventListFilters {
            owner: true,
            user: None,
            ..Default::default()
        };
        let mut binds = Vec::new();
        let sql = EventsComponent::build_where(&f, &mut binds);
        assert!(
            sql.contains(" AND FALSE"),
            "owner-without-user must match nothing: {sql}"
        );
    }

    #[test]
    fn raw_is_soft_deleted_matches_delete_flags() {
        assert!(raw_is_soft_deleted(&json!({ "deleted_by_user": true })));
        assert!(raw_is_soft_deleted(&json!({ "deleted_by_admin": true })));
        assert!(raw_is_soft_deleted(
            &json!({ "deleted_by_user": false, "deleted_by_admin": true })
        ));
        assert!(!raw_is_soft_deleted(
            &json!({ "deleted_by_user": false, "deleted_by_admin": false })
        ));
        assert!(!raw_is_soft_deleted(&json!({})));
        assert!(!raw_is_soft_deleted(&json!({ "name": "party" })));
    }
}
