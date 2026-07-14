use serde::Serialize;

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ListResponse {
    pub data: Vec<String>,
}

impl ListResponse {
    pub fn new(data: Vec<String>) -> Self {
        Self { data }
    }
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "places/"))]
pub struct ApiData<T> {
    pub ok: bool,
    pub data: T,
}

impl<T: Serialize> ApiData<T> {
    pub fn ok(data: T) -> Self {
        Self { ok: true, data }
    }
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "places/"))]
pub struct ApiDataTotal<T> {
    pub ok: bool,
    pub data: Vec<T>,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub total: i64,
}

impl<T: Serialize> ApiDataTotal<T> {
    pub fn ok(data: Vec<T>, total: i64) -> Self {
        Self {
            ok: true,
            data,
            total,
        }
    }
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "places/"))]
pub struct SignedApiData<T> {
    pub ok: bool,
    pub signature_hash: String,
    pub data: T,
}

impl<T: Serialize> SignedApiData<T> {
    pub fn ok(signature_hash: String, data: T) -> Self {
        Self {
            ok: true,
            signature_hash,
            data,
        }
    }
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "places/"))]
pub struct ApiDataTotalMap<T> {
    pub ok: bool,
    pub data: std::collections::BTreeMap<String, T>,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub total: i64,
}

impl<T: Serialize> ApiDataTotalMap<T> {
    pub fn ok(data: std::collections::BTreeMap<String, T>, total: i64) -> Self {
        Self {
            ok: true,
            data,
            total,
        }
    }
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "places/"))]
pub struct FavoritesResult {
    pub favorites: i32,
    pub user_favorite: bool,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "places/"))]
pub struct LikesResult {
    pub likes: i32,
    pub dislikes: i32,
    pub user_like: bool,
    pub user_dislike: bool,
}
