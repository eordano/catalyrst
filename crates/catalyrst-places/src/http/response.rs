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
