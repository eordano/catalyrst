use axum::extract::{Query, State};
use axum::Json;
use serde::Serialize;
use std::collections::HashMap;

use crate::http::errors::ApiError;
use crate::http::response::ApiData;
use crate::ports::places::CategoryTarget;
use crate::AppState;

fn category_i18n_en(name: &str) -> Option<&'static str> {
    Some(match name {
        "poi" => "\u{1F4CD} Point of Interest",
        "featured" => "\u{2728} Featured",
        "art" => "\u{1F3A8} Art",
        "game" => "\u{1F3AE} Game",
        "casino" => "\u{2663}\u{FE0F} Casino",
        "social" => "\u{1F465} Social",
        "music" => "\u{1F3B5} Music",
        "fashion" => "\u{1F460} Fashion",
        "crypto" => "\u{1FA99} Crypto",
        "education" => "\u{1F4DA} Education",
        "shop" => "\u{1F6CD}\u{FE0F} Shop",
        "business" => "\u{1F3E2} Business",
        "sports" => "\u{26BD}\u{FE0F} Sports",
        "parkour" => "\u{1F3C3} Parkour",
        _ => return None,
    })
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "places/"))]
pub struct CategoryOut {
    pub name: String,
    pub active: bool,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub count: i64,
    pub i18n: I18n,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "places/"))]
pub struct I18n {
    pub en: Option<String>,
}

#[utoipa::path(
    get,
    path = "/categories",
    tag = "places",
    responses(
        (status = 200, body = ApiData<Vec<CategoryOut>>),
        (status = 500, body = catalyrst_types::ApiErrorBody)
    )
)]
pub async fn get_categories(
    State(state): State<AppState>,
    Query(pairs): Query<HashMap<String, String>>,
) -> Result<Json<ApiData<Vec<CategoryOut>>>, ApiError> {
    let target = CategoryTarget::parse(pairs.get("target").map(|s| s.as_str()));
    let counts = state.places.category_counts(target).await?;
    let data = counts
        .into_iter()
        .map(|(name, count)| {
            let en = category_i18n_en(&name).map(|s| s.to_string());
            CategoryOut {
                i18n: I18n { en },
                active: true,
                count,
                name,
            }
        })
        .collect();
    Ok(Json(ApiData::ok(data)))
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct PlaceCategoriesOut {
    pub categories: Vec<String>,
}

#[utoipa::path(
    get,
    path = "/places/{place_id}/categories",
    tag = "places",
    params(("place_id" = String, Path)),
    responses(
        (status = 200, body = ApiData<PlaceCategoriesOut>),
        (status = 404, body = catalyrst_types::ApiErrorBody),
        (status = 500, body = catalyrst_types::ApiErrorBody)
    )
)]
pub async fn get_place_categories(
    State(state): State<AppState>,
    axum::extract::Path(place_id): axum::extract::Path<String>,
) -> Result<Json<ApiData<PlaceCategoriesOut>>, ApiError> {
    let categories = state.places.categories_for_place(&place_id).await?;
    Ok(Json(ApiData::ok(PlaceCategoriesOut { categories })))
}
