use axum::extract::{Query, State};
use axum::Json;

use crate::http::response::{ApiError, DataTotal};
use crate::ports::contracts::parse_filters;
use crate::AppState;

pub async fn get_contracts(
    State(state): State<AppState>,
    Query(pairs): Query<Vec<(String, String)>>,
) -> Result<Json<DataTotal<crate::dcl_schemas::Contract>>, ApiError> {
    let filters = parse_filters(&pairs)?;
    let (data, total) = state.contracts.get_contracts(&filters).await?;
    Ok(Json(DataTotal { data, total }))
}
