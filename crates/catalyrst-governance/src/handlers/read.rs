use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::ports::store::MemberRow;
use crate::rows::{BudgetRow, ProjectRow, ProposalRow};
use crate::AppState;

const DEFAULT_LIMIT: i64 = 100;
const MAX_LIMIT: i64 = 200;

#[derive(Deserialize, Default)]
pub struct Page {
    limit: Option<i64>,
    offset: Option<i64>,

    #[serde(rename = "type")]
    type_filter: Option<String>,

    linked_proposal_id: Option<String>,

    id: Option<String>,

    status: Option<String>,
}

impl Page {
    fn normalized(&self) -> (i64, i64) {
        let limit = self.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
        let offset = self.offset.unwrap_or(0).max(0);
        (limit, offset)
    }
}

#[derive(Serialize)]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "governance/")
)]
pub struct ListEnvelope {
    #[cfg_attr(feature = "ts", ts(type = "Array<Record<string, unknown>>"))]
    pub data: Vec<Value>,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub limit: i64,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub offset: i64,
}

#[derive(Serialize)]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "governance/")
)]
pub struct ProposalsEnvelope {
    pub data: Vec<ProposalRow>,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub limit: i64,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub offset: i64,
}

#[derive(Serialize)]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "governance/")
)]
pub struct ProjectsEnvelope {
    pub data: Vec<ProjectRow>,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub limit: i64,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub offset: i64,
}

#[derive(Serialize)]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "governance/")
)]
pub struct BudgetsEnvelope {
    pub data: Vec<BudgetRow>,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub limit: i64,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub offset: i64,
}

#[derive(Serialize)]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "governance/")
)]
pub struct MembersEnvelope {
    pub data: Vec<MemberRow>,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub limit: i64,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub offset: i64,
}

#[derive(Serialize)]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "governance/")
)]
pub struct ErrorBody {
    pub error: String,
}

fn internal(err: anyhow::Error) -> Response {
    tracing::error!(%err, "governance read query failed");
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ErrorBody {
            error: "Internal Server Error".to_string(),
        }),
    )
        .into_response()
}

fn page_body(rows: Vec<Value>, limit: i64, offset: i64) -> Response {
    Json(ListEnvelope {
        data: rows,
        limit,
        offset,
    })
    .into_response()
}

fn typed_rows<T: DeserializeOwned>(endpoint: &'static str, rows: &[Value]) -> Option<Vec<T>> {
    match rows
        .iter()
        .map(|row| serde_json::from_value(row.clone()))
        .collect::<Result<Vec<T>, _>>()
    {
        Ok(typed) => Some(typed),
        Err(err) => {
            tracing::warn!(
                %err,
                endpoint,
                "raw row no longer matches its typed row DTO; serving raw passthrough"
            );
            None
        }
    }
}

pub async fn proposals(State(state): State<AppState>, Query(page): Query<Page>) -> Response {
    let (limit, offset) = page.normalized();
    match state
        .store
        .list_proposals(
            limit,
            offset,
            page.type_filter.as_deref(),
            page.linked_proposal_id.as_deref(),
            page.id.as_deref(),
            page.status.as_deref(),
        )
        .await
    {
        Ok(rows) => match typed_rows::<ProposalRow>("/proposals", &rows) {
            Some(data) => Json(ProposalsEnvelope {
                data,
                limit,
                offset,
            })
            .into_response(),
            None => page_body(rows, limit, offset),
        },
        Err(e) => internal(e),
    }
}

pub async fn projects(State(state): State<AppState>, Query(page): Query<Page>) -> Response {
    let (limit, offset) = page.normalized();
    match state.store.list_projects(limit, offset).await {
        Ok(rows) => match typed_rows::<ProjectRow>("/projects", &rows) {
            Some(data) => Json(ProjectsEnvelope {
                data,
                limit,
                offset,
            })
            .into_response(),
            None => page_body(rows, limit, offset),
        },
        Err(e) => internal(e),
    }
}

pub async fn project_by_id(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    match state.store.get_project_detail(&id).await {
        Ok(Some((raw, cfg, updates))) => Json(crate::parse::build_project_detail(
            &raw,
            cfg.as_ref(),
            &updates,
        ))
        .into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(ErrorBody {
                error: "Not Found".to_string(),
            }),
        )
            .into_response(),
        Err(e) => internal(e),
    }
}

pub async fn budgets(State(state): State<AppState>, Query(page): Query<Page>) -> Response {
    let (limit, offset) = page.normalized();
    match state.store.list_budgets(limit, offset).await {
        Ok(rows) => match typed_rows::<BudgetRow>("/budgets", &rows) {
            Some(data) => Json(BudgetsEnvelope {
                data,
                limit,
                offset,
            })
            .into_response(),
            None => page_body(rows, limit, offset),
        },
        Err(e) => internal(e),
    }
}

pub async fn vestings(State(state): State<AppState>, Query(page): Query<Page>) -> Response {
    let (limit, offset) = page.normalized();
    match state.store.list_vestings(limit, offset).await {
        Ok(rows) => page_body(rows, limit, offset),
        Err(e) => internal(e),
    }
}

pub async fn members(State(state): State<AppState>, Query(page): Query<Page>) -> Response {
    let (limit, offset) = page.normalized();
    match state.store.list_members(limit, offset).await {
        Ok(data) => Json(MembersEnvelope {
            data,
            limit,
            offset,
        })
        .into_response(),
        Err(e) => internal(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const PROPOSALS_CAPTURE: &str = include_str!("../../testdata/gov-proposals.json");
    const PROJECTS_CAPTURE: &str = include_str!("../../testdata/gov-projects.json");
    const BUDGETS_CAPTURE: &str = include_str!("../../testdata/gov-budgets.json");
    const MEMBERS_CAPTURE: &str = include_str!("../../testdata/gov-members.json");

    fn capture_parts(capture: &str) -> (Vec<Value>, i64, i64) {
        let envelope: Value = serde_json::from_str(capture).expect("capture parses");
        (
            envelope["data"].as_array().expect("data array").clone(),
            envelope["limit"].as_i64().expect("limit"),
            envelope["offset"].as_i64().expect("offset"),
        )
    }

    #[test]
    fn wire_identity_proposals_envelope() {
        let (rows, limit, offset) = capture_parts(PROPOSALS_CAPTURE);
        let data = typed_rows::<ProposalRow>("/proposals", &rows).expect("rows conform");
        let new = ProposalsEnvelope {
            data,
            limit,
            offset,
        };
        let old: Value = serde_json::from_str(PROPOSALS_CAPTURE).unwrap();
        assert_eq!(serde_json::to_value(&new).unwrap(), old);
    }

    #[test]
    fn wire_identity_projects_envelope() {
        let (rows, limit, offset) = capture_parts(PROJECTS_CAPTURE);
        let data = typed_rows::<ProjectRow>("/projects", &rows).expect("rows conform");
        let new = ProjectsEnvelope {
            data,
            limit,
            offset,
        };
        let old: Value = serde_json::from_str(PROJECTS_CAPTURE).unwrap();
        assert_eq!(serde_json::to_value(&new).unwrap(), old);
    }

    #[test]
    fn wire_identity_budgets_envelope() {
        let (rows, limit, offset) = capture_parts(BUDGETS_CAPTURE);
        let data = typed_rows::<BudgetRow>("/budgets", &rows).expect("rows conform");
        let new = BudgetsEnvelope {
            data,
            limit,
            offset,
        };
        let old: Value = serde_json::from_str(BUDGETS_CAPTURE).unwrap();
        assert_eq!(serde_json::to_value(&new).unwrap(), old);
    }

    #[test]
    fn wire_identity_members_envelope() {
        let (rows, limit, offset) = capture_parts(MEMBERS_CAPTURE);
        let data = typed_rows::<MemberRow>("/members", &rows).expect("rows conform");
        let new = MembersEnvelope {
            data,
            limit,
            offset,
        };
        let old: Value = serde_json::from_str(MEMBERS_CAPTURE).unwrap();
        assert_eq!(serde_json::to_value(&new).unwrap(), old);
    }

    #[test]
    fn typed_rows_falls_back_on_nonconforming_row() {
        let (mut rows, _, _) = capture_parts(BUDGETS_CAPTURE);
        rows.push(json!({ "id": "no-required-keys" }));
        assert!(typed_rows::<BudgetRow>("/budgets", &rows).is_none());
    }

    #[test]
    fn page_clamps_limit_and_offset() {
        assert_eq!(
            Page {
                limit: None,
                offset: None,
                ..Default::default()
            }
            .normalized(),
            (100, 0)
        );
        assert_eq!(
            Page {
                limit: Some(500),
                offset: Some(5),
                ..Default::default()
            }
            .normalized(),
            (200, 5)
        );
        assert_eq!(
            Page {
                limit: Some(0),
                offset: Some(-7),
                ..Default::default()
            }
            .normalized(),
            (1, 0)
        );
        assert_eq!(
            Page {
                limit: Some(50),
                offset: Some(50),
                ..Default::default()
            }
            .normalized(),
            (50, 50)
        );
    }
}

use crate::ports::archives;

#[derive(Deserialize, Default)]
pub struct CommentsQuery {
    limit: Option<i64>,
}

#[derive(Deserialize, Default)]
pub struct EngagementQuery {
    days: Option<i64>,
    limit: Option<i64>,
}

#[derive(Deserialize, Default)]
pub struct ActivityQuery {
    limit: Option<i64>,
}

fn not_found() -> Response {
    (
        StatusCode::NOT_FOUND,
        Json(ErrorBody {
            error: "Not Found".to_string(),
        }),
    )
        .into_response()
}

pub async fn proposal_votes(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    let refs = match state.store.proposal_refs(&id).await {
        Ok(Some(r)) => r,
        Ok(None) => return not_found(),
        Err(e) => return internal(e),
    };
    let (Some(pool), Some(snapshot_id)) = (state.archives.snapshot.as_ref(), refs.0) else {
        return Json(archives::empty_votes_payload()).into_response();
    };
    match archives::proposal_votes(pool, &snapshot_id).await {
        Ok(payload) => Json(payload).into_response(),
        Err(e) => internal(e),
    }
}

pub async fn proposal_comments(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(q): Query<CommentsQuery>,
) -> Response {
    let refs = match state.store.proposal_refs(&id).await {
        Ok(Some(r)) => r,
        Ok(None) => return not_found(),
        Err(e) => return internal(e),
    };
    let (Some(pool), Some(topic_id)) = (state.archives.discourse.as_ref(), refs.1) else {
        return Json(archives::CommentsPayload {
            total: 0,
            comments: Vec::new(),
        })
        .into_response();
    };
    let limit = q.limit.unwrap_or(25).clamp(1, 100);
    match archives::comments_by_topic(pool, topic_id, limit).await {
        Ok(payload) => Json(payload).into_response(),
        Err(e) => internal(e),
    }
}

pub async fn engagement(
    State(state): State<AppState>,
    Query(q): Query<EngagementQuery>,
) -> Response {
    let Some(pool) = state.archives.snapshot.as_ref() else {
        return Json(archives::EngagementPayload {
            voters: Vec::new(),
            weekly: Vec::new(),
        })
        .into_response();
    };
    let days = q.days.unwrap_or(30).clamp(1, 365);
    let limit = q.limit.unwrap_or(10).clamp(1, 100);
    match archives::engagement(pool, days, limit).await {
        Ok(payload) => Json(payload).into_response(),
        Err(e) => internal(e),
    }
}

pub async fn activity(State(state): State<AppState>, Query(q): Query<ActivityQuery>) -> Response {
    let limit = q.limit.unwrap_or(30).clamp(1, 100);
    let mut items: Vec<archives::ActivityFeedItem> = Vec::new();

    if let Some(pool) = state.archives.snapshot.as_ref() {
        match archives::recent_votes(pool, limit).await {
            Ok(votes) => {
                let mut ids: Vec<String> = votes.iter().map(|(_, sid, _, _)| sid.clone()).collect();
                ids.sort();
                ids.dedup();
                match state.store.titles_by_snapshot_ids(&ids).await {
                    Ok(titles) => {
                        let map: std::collections::HashMap<&str, (&str, &str)> = titles
                            .iter()
                            .map(|(sid, gid, title)| (sid.as_str(), (gid.as_str(), title.as_str())))
                            .collect();
                        for (voter, sid, _vp, ts) in &votes {
                            if let Some((gid, title)) = map.get(sid.as_str()) {
                                items.push(archives::ActivityFeedItem {
                                    kind: "vote".to_string(),
                                    address: Some(voter.clone()),
                                    title: Some((*title).to_string()),
                                    proposal_id: Some((*gid).to_string()),
                                    ts: *ts,
                                });
                            }
                        }
                    }
                    Err(e) => return internal(e),
                }
            }
            Err(e) => return internal(e),
        }
    }

    match state.store.recent_proposals(limit).await {
        Ok(rows) => {
            for (id, title, user, ts) in rows {
                items.push(archives::ActivityFeedItem {
                    kind: "proposal".to_string(),
                    address: user,
                    title: Some(title),
                    proposal_id: Some(id),
                    ts,
                });
            }
        }
        Err(e) => return internal(e),
    }
    match state.store.recently_finished(limit).await {
        Ok(rows) => {
            for (id, title, ts) in rows {
                items.push(archives::ActivityFeedItem {
                    kind: "finished".to_string(),
                    address: None,
                    title: Some(title),
                    proposal_id: Some(id),
                    ts,
                });
            }
        }
        Err(e) => return internal(e),
    }
    match state.store.recent_updates(limit).await {
        Ok(rows) => {
            for (proposal_id, title, ts) in rows {
                items.push(archives::ActivityFeedItem {
                    kind: "update".to_string(),
                    address: None,
                    title: Some(title),
                    proposal_id,
                    ts,
                });
            }
        }
        Err(e) => return internal(e),
    }

    items.sort_by_key(|x| std::cmp::Reverse(x.ts));
    items.truncate(limit as usize);
    Json(archives::ActivityPayload { items }).into_response()
}
