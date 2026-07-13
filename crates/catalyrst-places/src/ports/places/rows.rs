use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::Row;

pub(super) const PLACE_COLUMNS: &str = r#"
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
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "places/"))]
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
    #[cfg_attr(feature = "ts", ts(type = "string | null"))]
    pub disabled_at: Option<DateTime<Utc>>,
    pub disabled_reason: Option<String>,
    #[cfg_attr(feature = "ts", ts(type = "string | null"))]
    pub created_at: Option<DateTime<Utc>>,
    #[cfg_attr(feature = "ts", ts(type = "string | null"))]
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
    #[cfg_attr(feature = "ts", ts(type = "string | null"))]
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

    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub live: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub connected_addresses: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional, type = "Array<Record<string, unknown>>"))]
    pub realms_detail: Option<Vec<serde_json::Value>>,
}

impl PlaceRow {
    pub fn apply_realms_detail(&mut self, with_realms_detail: bool) {
        if with_realms_detail && !self.world {
            self.realms_detail = Some(Vec::new());
        }
    }
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

    pub owner_filtered: bool,

    pub destinations_mode: bool,

    pub place_user_counts: Vec<(String, i32)>,
    pub world_user_counts: Vec<(String, i32)>,
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
    pub(super) fn column(self) -> &'static str {
        match self {
            Self::LikeScore => "NULLIF(raw->>'like_score','')::float8",
            Self::UpdatedAt => "NULLIF(raw->>'updated_at','')::timestamptz",
            Self::CreatedAt => "NULLIF(raw->>'created_at','')::timestamptz",
            Self::UserVisits => "COALESCE(NULLIF(raw->>'user_visits','')::int, 0)",
            Self::MostActive => "COALESCE(NULLIF(raw->>'user_count','')::int, 0)",
        }
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

pub(super) fn row_to_report(r: sqlx::postgres::PgRow) -> ReportRow {
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

pub(super) fn row_to_poi(r: sqlx::postgres::PgRow) -> PoiRow {
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

pub(super) fn row_to_place(r: sqlx::postgres::PgRow) -> PlaceRow {
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
