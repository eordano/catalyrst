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

    fn build_where(&self, f: &EventListFilters, binds: &mut Vec<EventBind>) -> String {
        let mut sql = String::from(" WHERE 1=1");
        let now = Utc::now();

        // next_finish_at / next_start_at are extracted by the archive
        // (events-archive.py) into real indexed timestamptz columns, so these
        // filters/sorts are index-backed instead of re-parsing the raw JSONB and
        // casting per row (the text::timestamptz cast is STABLE and can't be
        // indexed as a functional expression).
        let nf = "next_finish_at";
        let ns = "next_start_at";

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

        if let Some(c) = &f.creator {
            let p = next_placeholder(binds, EventBind::Text(c.to_lowercase()));
            sql.push_str(&format!(" AND lower(user_creator) = {}", p));
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

        match f.rejected {
            Some(true) => sql.push_str(" AND COALESCE((raw->>'rejected')::boolean, false) IS TRUE"),
            Some(false) => {
                sql.push_str(" AND COALESCE((raw->>'rejected')::boolean, false) IS FALSE")
            }
            None => sql.push_str(" AND COALESCE((raw->>'rejected')::boolean, false) IS FALSE"),
        }

        if !f.approved_visibility() {
            sql.push_str(" AND approved IS TRUE");
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
        let where_sql = self.build_where(f, &mut binds);

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
            format!(" ORDER BY next_start_at {} NULLS LAST", order_dir)
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
            let cwhere = self.build_where(f, &mut cbinds);
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

    pub async fn attach_connected_addresses(
        &self,
        records: &mut [EventRecord],
    ) -> Result<(), ApiError> {
        if records.is_empty() {
            return Ok(());
        }
        let ids: Vec<String> = records.iter().map(|r| r.id.clone()).collect();
        let rows: Vec<(String, String)> = sqlx::query_as(
            "SELECT event_id, signer FROM event_attendance_local \
             WHERE action = 'going' AND event_id = ANY($1)",
        )
        .bind(&ids)
        .fetch_all(&self.pool)
        .await?;

        use std::collections::HashMap;
        let mut by_event: HashMap<String, Vec<String>> = HashMap::new();
        for (event_id, signer) in rows {
            by_event.entry(event_id).or_default().push(signer);
        }
        for r in records.iter_mut() {
            r.connected_addresses = Some(by_event.remove(&r.id).unwrap_or_default());
        }
        Ok(())
    }

    pub async fn attending(&self, user: &str) -> Result<Vec<EventRecord>, ApiError> {
        let user_lc = user.to_lowercase();
        let rows = sqlx::query_as::<_, EventRow>(
            "SELECT id, name, start_at, finish_at, duration_ms, recurrent, highlighted, trending, \
             approved, attending, community_id, user_creator, coordinates_x, coordinates_y, \
             description, raw FROM event \
             WHERE next_finish_at > now() \
               AND COALESCE((raw->>'rejected')::boolean, false) IS FALSE \
               AND ( \
                 id IN (SELECT event_id FROM event_attendance_local WHERE signer = $1 AND action = 'going') \
                 OR raw->'latest_attendees' ? $1 \
               ) \
             ORDER BY next_start_at ASC NULLS LAST",
        )
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

    pub async fn sitemap_event_ids(&self, page: i64) -> Result<Vec<String>, ApiError> {
        let rows: Vec<(String,)> = sqlx::query_as(
            "SELECT id FROM event WHERE approved IS TRUE \
             ORDER BY (raw->>'created_at')::timestamptz ASC NULLS LAST, id ASC \
             OFFSET $1 LIMIT $2",
        )
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

    /// Persist an admin moderation / create action into the writable
    /// `events_local` overlay table. The archive-owned `event` table is
    /// read-only for this service (the events-archive importer is the only
    /// writer), so admin actions are recorded in the local overlay keyed by
    /// event id. `signed_payload` carries the full action document (action,
    /// edited fields, who/when) and is upserted; the merged document is
    /// returned for the response.
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

    /// Read the local overlay document for an event id, if any.
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
    let next_start_at = parse_dt(raw.get("next_start_at")).or(r.start_at);
    let next_finish_at = parse_dt(raw.get("next_finish_at")).or(r.finish_at);
    let all_day = raw
        .get("all_day")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let world = raw.get("world").and_then(|v| v.as_bool()).unwrap_or(false);
    let duration = r
        .duration_ms
        .or_else(|| raw.get("duration").and_then(|v| v.as_i64()));
    let live = match (next_start_at, duration) {
        (Some(ns), Some(d)) => {
            let now = Utc::now();
            now >= ns && now < ns + chrono::Duration::milliseconds(d)
        }
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
        recurrent_dates: raw
            .get("recurrent_dates")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|x| parse_dt(Some(x))).collect())
            .unwrap_or_default(),
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
