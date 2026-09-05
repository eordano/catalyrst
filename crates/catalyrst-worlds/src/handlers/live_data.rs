use axum::extract::State;
use axum::Json;
use chrono::SecondsFormat;
use serde::Serialize;

use crate::AppState;

/// One world's instantaneous occupancy.
///
/// `worldName` is camelCase on the wire because this endpoint is
/// wire-compatible with Decentraland's worlds-content-server `/live-data`, and
/// a client written against that shape must keep working when pointed here.
#[derive(Debug, Serialize, utoipa::ToSchema)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "worlds/"))]
#[serde(rename_all = "camelCase")]
pub struct WorldOccupancy {
    pub world_name: String,
    /// `i64` on the wire, `number` in TS. The value is a room population, so
    /// the range TS cannot represent is unreachable here.
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub users: i64,
}

/// The payload of `/live-data`.
///
/// Both fields are required and neither is defaulted. A consumer that cannot
/// obtain this must see the request fail rather than receive a zero: a
/// fabricated `totalUsers: 0` is indistinguishable from an empty world, which
/// is the defect `sites/scripts/check-schema-honesty.mts` gates against on the
/// TypeScript side. Keeping the Rust type non-optional is what lets the
/// generated schema stay honest.
#[derive(Debug, Serialize, utoipa::ToSchema)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "worlds/"))]
#[serde(rename_all = "camelCase")]
pub struct LiveDataPayload {
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub total_users: i64,
    pub per_world: Vec<WorldOccupancy>,
}

/// `GET /live-data`.
#[derive(Debug, Serialize, utoipa::ToSchema)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "worlds/"))]
#[serde(rename_all = "camelCase")]
pub struct LiveDataResponse {
    pub data: LiveDataPayload,
    /// RFC3339 with millisecond precision, matching the upstream server.
    pub last_updated: String,
}

#[utoipa::path(
    get,
    path = "/live-data",
    tag = "status",
    responses(
        (status = 200, body = LiveDataResponse)
    )
)]
pub async fn live_data(State(state): State<AppState>) -> Json<LiveDataResponse> {
    let counts = state.presence.world_counts();
    let total: i64 = counts.iter().map(|(_, c)| c).sum();
    let per_world = counts
        .into_iter()
        .map(|(world_name, users)| WorldOccupancy { world_name, users })
        .collect();

    Json(LiveDataResponse {
        data: LiveDataPayload {
            total_users: total,
            per_world,
        },
        last_updated: chrono::Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
    })
}
