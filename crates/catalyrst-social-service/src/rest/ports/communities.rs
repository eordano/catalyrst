use chrono::NaiveDateTime;
use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::rest::http::{ApiError, Pagination};

#[derive(Debug, Serialize)]
pub struct CommunityRow {
    pub id: Uuid,
    pub name: String,
    pub description: String,
    #[serde(rename = "ownerAddress")]
    pub owner_address: String,
    pub private: bool,
    pub active: bool,
    pub unlisted: bool,
    #[serde(rename = "createdAt")]
    pub created_at: NaiveDateTime,
    #[serde(rename = "updatedAt")]
    pub updated_at: NaiveDateTime,
}

#[derive(Debug, Serialize)]
pub struct CommunityPublic {
    pub id: Uuid,
    pub name: String,
    pub description: String,
    #[serde(rename = "ownerAddress")]
    pub owner_address: String,
    pub privacy: &'static str,
    pub active: bool,
}

#[derive(Debug, Serialize)]
pub struct CommunityWithUser {
    pub id: Uuid,
    pub name: String,
    pub description: String,
    #[serde(rename = "ownerAddress")]
    pub owner_address: String,
    pub privacy: &'static str,
    pub role: String,
    #[serde(rename = "membersCount")]
    pub members_count: i64,
    pub active: bool,
    #[serde(rename = "isLive")]
    pub is_live: bool,
    #[serde(rename = "voiceChatStatus")]
    pub voice_chat_status: Option<serde_json::Value>,
}

#[derive(Debug, Serialize)]
pub struct MemberCommunity {
    pub id: Uuid,
    pub name: String,
    #[serde(rename = "ownerAddress")]
    pub owner_address: String,
    pub role: String,
    #[serde(rename = "joinedAt")]
    pub joined_at: NaiveDateTime,
}

#[derive(Debug, Serialize)]
pub struct CommunitySearchResult {
    pub id: Uuid,
    pub name: String,
    #[serde(rename = "membersCount")]
    pub members_count: i64,
    pub privacy: &'static str,
}

const MEMBER_EXPR: &str =
    "EXISTS (SELECT 1 FROM community_members m WHERE m.community_id = c.id AND m.member_address = $USER)";

fn default_visibility_clause(
    member_expr: &str,
    has_user: bool,
    only_member_of: bool,
    only_with_active_voice_chat: bool,
) -> Option<String> {
    if !has_user {
        Some("c.unlisted = FALSE AND c.private = FALSE".to_string())
    } else if only_member_of {
        None
    } else if only_with_active_voice_chat {
        Some(format!(
            "({member_expr} OR (c.unlisted = FALSE AND c.private = FALSE))"
        ))
    } else {
        Some("c.unlisted = FALSE".to_string())
    }
}

pub struct CommunitiesComponent {
    pool: PgPool,
}

impl CommunitiesComponent {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn community_exists(&self, id: Uuid, only_public: bool) -> Result<bool, ApiError> {
        let sql = if only_public {
            "SELECT EXISTS (SELECT 1 FROM communities WHERE id = $1 AND active = TRUE AND suspended = FALSE AND private <> TRUE)"
        } else {
            "SELECT EXISTS (SELECT 1 FROM communities WHERE id = $1 AND active = TRUE AND suspended = FALSE)"
        };
        let exists: bool = sqlx::query_scalar(sql)
            .bind(id)
            .fetch_one(&self.pool)
            .await?;
        Ok(exists)
    }

    pub async fn is_private(&self, id: Uuid) -> Result<bool, ApiError> {
        let row: Option<bool> =
            sqlx::query_scalar("SELECT private FROM communities WHERE id = $1 AND active = TRUE")
                .bind(id)
                .fetch_optional(&self.pool)
                .await?;
        Ok(row.unwrap_or(false))
    }

    pub async fn member_role(&self, id: Uuid, address: &str) -> Result<Option<String>, ApiError> {
        let role: Option<String> = sqlx::query_scalar(
            "SELECT role FROM community_members WHERE community_id = $1 AND member_address = $2",
        )
        .bind(id)
        .bind(address.to_lowercase())
        .fetch_optional(&self.pool)
        .await?;
        Ok(role)
    }

    pub async fn get_by_id(
        &self,
        id: Uuid,
        as_user: Option<&str>,
    ) -> Result<Option<serde_json::Value>, ApiError> {
        let row = sqlx::query_as::<_, (Uuid, String, String, String, bool, bool, bool, NaiveDateTime, NaiveDateTime)>(
            "SELECT id, name, description, owner_address, private, active, unlisted, created_at, updated_at \
             FROM communities WHERE id = $1 AND active = true AND suspended = false"
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        let Some((
            id,
            name,
            description,
            owner_address,
            private,
            active,
            unlisted,
            created_at,
            updated_at,
        )) = row
        else {
            return Ok(None);
        };
        let privacy = if private { "private" } else { "public" };
        let visibility = if unlisted { "unlisted" } else { "all" };

        let members_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM community_members WHERE community_id = $1")
                .bind(id)
                .fetch_one(&self.pool)
                .await
                .unwrap_or(0);

        let has_thumbnail: bool = sqlx::query_scalar(
            "SELECT COALESCE(has_thumbnail, FALSE) FROM community_ranking_metrics WHERE community_id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .ok()
        .flatten()
        .unwrap_or(false);

        let mut obj = serde_json::json!({
            "id": id,
            "name": name,
            "description": description,
            "ownerAddress": owner_address,
            "privacy": privacy,
            "visibility": visibility,
            "active": active,
            "unlisted": unlisted,
            "membersCount": members_count,
            "createdAt": created_at,
            "updatedAt": updated_at,
            "isLive": false,
            "voiceChatStatus": serde_json::Value::Null,
            "_hasThumbnail": has_thumbnail,
        });

        let mut role = "none".to_string();
        if let Some(addr) = as_user {
            let db_role: Option<String> = sqlx::query_scalar(
                "SELECT role FROM community_members WHERE community_id = $1 AND member_address = $2",
            )
            .bind(id)
            .bind(addr.to_lowercase())
            .fetch_optional(&self.pool)
            .await?;
            let banned: Option<bool> = sqlx::query_scalar(
                "SELECT active FROM community_bans WHERE community_id = $1 AND banned_address = $2",
            )
            .bind(id)
            .bind(addr.to_lowercase())
            .fetch_optional(&self.pool)
            .await?;
            if let Some(r) = db_role {
                role = r;
            }
            obj["isBanned"] = serde_json::Value::Bool(banned.unwrap_or(false));
        }
        obj["role"] = serde_json::Value::String(role);

        Ok(Some(obj))
    }

    pub async fn list(
        &self,
        pagination: &Pagination,
        search: Option<&str>,
        as_user: Option<&str>,
        only_member_of: bool,
        only_with_active_voice_chat: bool,
        roles: &[String],
    ) -> Result<(Vec<serde_json::Value>, i64), ApiError> {
        let mut where_clauses: Vec<String> = vec![
            "c.active = TRUE".to_string(),
            "c.suspended = FALSE".to_string(),
        ];
        let mut params: Vec<String> = Vec::new();

        if let Some(clause) = default_visibility_clause(
            MEMBER_EXPR,
            as_user.is_some(),
            only_member_of,
            only_with_active_voice_chat,
        ) {
            where_clauses.push(clause);
        }

        if let Some(s) = search {
            params.push(format!("%{}%", s.replace('%', "\\%").replace('_', "\\_")));
            let i = params.len();
            where_clauses.push(format!("c.name ILIKE ${}", i));
        }

        let user_lower = as_user.map(|s| s.to_lowercase());
        let user_param_idx = if let Some(ref u) = user_lower {
            params.push(u.clone());
            Some(params.len())
        } else {
            None
        };

        for clause in where_clauses.iter_mut() {
            if let Some(i) = user_param_idx {
                *clause = clause.replace("$USER", &format!("${}", i));
            } else {
                *clause = clause.replace("$USER", "''");
            }
        }

        if only_member_of {
            if let Some(i) = user_param_idx {
                where_clauses.push(format!(
                    "EXISTS (SELECT 1 FROM community_members m WHERE m.community_id = c.id AND m.member_address = ${})",
                    i
                ));
            } else {
                where_clauses.push("FALSE".to_string());
            }
        }

        if only_with_active_voice_chat {
            where_clauses.push(
                "EXISTS (SELECT 1 FROM community_voice_chats v WHERE v.community_id = c.id)"
                    .to_string(),
            );
        }

        if !roles.is_empty() {
            if let Some(i) = user_param_idx {
                params.push(roles.join(","));
                let r = params.len();
                where_clauses.push(format!(
                    "EXISTS (SELECT 1 FROM community_members m WHERE m.community_id = c.id AND m.member_address = ${} AND m.role = ANY(string_to_array(${}, ',')))",
                    i, r
                ));
            }
        }

        let where_sql = where_clauses.join(" AND ");

        let limit_idx = params.len() + 1;
        let offset_idx = params.len() + 2;

        let role_select = match user_param_idx {
            Some(i) => format!(
                "COALESCE((SELECT m.role FROM community_members m WHERE m.community_id = c.id AND m.member_address = ${}), 'none')",
                i
            ),
            None => "'none'".to_string(),
        };

        let select_sql = format!(
            "SELECT c.id, c.name, c.description, c.owner_address, c.private, c.active, c.unlisted, \
                    c.created_at, c.updated_at, \
                    (SELECT COUNT(*) FROM community_members m WHERE m.community_id = c.id) AS members_count, \
                    {role_select} AS role, \
                    COALESCE((SELECT crm.has_thumbnail FROM community_ranking_metrics crm WHERE crm.community_id = c.id), FALSE) AS has_thumbnail \
             FROM communities c \
             WHERE {where_sql} \
             ORDER BY c.editors_choice DESC, c.ranking_score DESC, c.name ASC \
             LIMIT ${limit_idx} OFFSET ${offset_idx}"
        );

        let mut q = sqlx::query_as::<
            _,
            (
                Uuid,
                String,
                String,
                String,
                bool,
                bool,
                bool,
                NaiveDateTime,
                NaiveDateTime,
                i64,
                String,
                bool,
            ),
        >(sqlx::AssertSqlSafe(select_sql));
        for p in &params {
            q = q.bind(p);
        }
        q = q.bind(pagination.limit).bind(pagination.offset);
        let rows = q.fetch_all(&self.pool).await?;

        let count_sql = format!("SELECT COUNT(*) FROM communities c WHERE {where_sql}");
        let mut cq = sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(count_sql));
        for p in &params {
            cq = cq.bind(p);
        }
        let total = cq.fetch_one(&self.pool).await.unwrap_or(0);

        let results: Vec<serde_json::Value> = rows
            .into_iter()
            .map(
                |(
                    id,
                    name,
                    description,
                    owner_address,
                    private,
                    active,
                    unlisted,
                    created_at,
                    _updated_at,
                    members_count,
                    role,
                    has_thumbnail,
                )| {
                    let privacy = if private { "private" } else { "public" };
                    let visibility = if unlisted { "unlisted" } else { "all" };
                    serde_json::json!({
                        "id": id,
                        "name": name,
                        "description": description,
                        "ownerAddress": owner_address,
                        "privacy": privacy,
                        "visibility": visibility,
                        "role": role,
                        "active": active,
                        "unlisted": unlisted,
                        "membersCount": members_count,
                        "createdAt": created_at,
                        "isLive": false,
                        "friends": serde_json::Value::Array(vec![]),
                        "voiceChatStatus": serde_json::Value::Null,
                        "_hasThumbnail": has_thumbnail,
                    })
                },
            )
            .collect();

        Ok((results, total))
    }

    pub async fn member_communities(
        &self,
        member_address: &str,
        pagination: &Pagination,
        roles: Option<&[&str]>,
        only_public_visible: bool,
    ) -> Result<(Vec<MemberCommunity>, i64), ApiError> {
        let lower = member_address.to_lowercase();
        let mut where_sql = "m.member_address = $1".to_string();
        if let Some(rs) = roles {
            if !rs.is_empty() {
                where_sql.push_str(" AND m.role = ANY($2::text[])");
            }
        }
        if only_public_visible {
            where_sql.push_str(" AND c.private = FALSE AND c.unlisted = FALSE");
        }
        let limit_idx = if roles.map(|r| !r.is_empty()).unwrap_or(false) {
            3
        } else {
            2
        };
        let offset_idx = limit_idx + 1;

        let select_sql = format!(
            "SELECT c.id, c.name, c.owner_address, m.role, m.joined_at \
             FROM community_members m JOIN communities c ON c.id = m.community_id \
             WHERE {where_sql} AND c.active = TRUE \
             ORDER BY m.joined_at DESC LIMIT ${limit_idx} OFFSET ${offset_idx}"
        );
        let count_sql = format!(
            "SELECT COUNT(*) FROM community_members m JOIN communities c ON c.id = m.community_id \
             WHERE {where_sql} AND c.active = TRUE"
        );

        let mut q = sqlx::query_as::<_, (Uuid, String, String, String, NaiveDateTime)>(
            sqlx::AssertSqlSafe(select_sql),
        )
        .bind(&lower);
        let mut cq = sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(count_sql)).bind(&lower);
        if let Some(rs) = roles {
            if !rs.is_empty() {
                let owned: Vec<String> = rs.iter().map(|s| s.to_string()).collect();
                q = q.bind(owned.clone());
                cq = cq.bind(owned);
            }
        }
        q = q.bind(pagination.limit).bind(pagination.offset);
        let rows = q.fetch_all(&self.pool).await?;
        let total = cq.fetch_one(&self.pool).await.unwrap_or(0);

        let results = rows
            .into_iter()
            .map(
                |(id, name, owner_address, role, joined_at)| MemberCommunity {
                    id,
                    name,
                    owner_address,
                    role,
                    joined_at,
                },
            )
            .collect();
        Ok((results, total))
    }

    pub async fn admin_list(
        &self,
        pagination: &Pagination,
        status: &str,
        owner: Option<&str>,
        search: Option<&str>,
    ) -> Result<(Vec<serde_json::Value>, i64), ApiError> {
        let mut where_clauses: Vec<String> = Vec::new();
        let mut params: Vec<String> = Vec::new();

        match status {
            "active" => where_clauses.push("c.active = TRUE AND c.suspended = FALSE".to_string()),
            "suspended" => where_clauses.push("c.suspended = TRUE".to_string()),
            "inactive" => where_clauses.push("c.active = FALSE".to_string()),

            _ => {}
        }

        if let Some(o) = owner {
            params.push(o.to_lowercase());
            let i = params.len();
            where_clauses.push(format!("LOWER(c.owner_address) = ${}", i));
        }

        if let Some(s) = search {
            params.push(format!("%{}%", s.replace('%', "\\%").replace('_', "\\_")));
            let i = params.len();
            where_clauses.push(format!("c.name ILIKE ${}", i));
        }

        let where_sql = if where_clauses.is_empty() {
            "TRUE".to_string()
        } else {
            where_clauses.join(" AND ")
        };

        let limit_idx = params.len() + 1;
        let offset_idx = params.len() + 2;

        let select_sql = format!(
            "SELECT c.id, c.name, c.description, c.owner_address, c.private, c.active, \
                    c.unlisted, c.suspended, c.suspended_at, c.suspended_by, c.suspension_reason, \
                    c.created_at, c.updated_at, \
                    (SELECT COUNT(*) FROM community_members m WHERE m.community_id = c.id) AS members_count \
             FROM communities c \
             WHERE {where_sql} \
             ORDER BY c.created_at DESC \
             LIMIT ${limit_idx} OFFSET ${offset_idx}"
        );

        let mut q = sqlx::query_as::<
            _,
            (
                Uuid,
                String,
                String,
                String,
                bool,
                bool,
                bool,
                bool,
                Option<NaiveDateTime>,
                Option<String>,
                Option<String>,
                NaiveDateTime,
                NaiveDateTime,
                i64,
            ),
        >(sqlx::AssertSqlSafe(select_sql));
        for p in &params {
            q = q.bind(p);
        }
        q = q.bind(pagination.limit).bind(pagination.offset);
        let rows = q.fetch_all(&self.pool).await?;

        let count_sql = format!("SELECT COUNT(*) FROM communities c WHERE {where_sql}");
        let mut cq = sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(count_sql));
        for p in &params {
            cq = cq.bind(p);
        }
        let total = cq.fetch_one(&self.pool).await.unwrap_or(0);

        let results: Vec<serde_json::Value> = rows
            .into_iter()
            .map(
                |(
                    id,
                    name,
                    description,
                    owner_address,
                    private,
                    active,
                    unlisted,
                    suspended,
                    suspended_at,
                    suspended_by,
                    suspension_reason,
                    created_at,
                    updated_at,
                    members_count,
                )| {
                    let privacy = if private { "private" } else { "public" };
                    let visibility = if unlisted { "unlisted" } else { "all" };
                    serde_json::json!({
                        "id": id,
                        "name": name,
                        "description": description,
                        "ownerAddress": owner_address,
                        "privacy": privacy,
                        "visibility": visibility,
                        "active": active,
                        "unlisted": unlisted,
                        "suspended": suspended,
                        "suspendedAt": suspended_at,
                        "suspendedBy": suspended_by,
                        "suspensionReason": suspension_reason,
                        "membersCount": members_count,
                        "createdAt": created_at,
                        "updatedAt": updated_at,
                    })
                },
            )
            .collect();

        Ok((results, total))
    }

    pub async fn set_suspended(
        &self,
        id: Uuid,
        suspended: bool,
        actor: &str,
        reason: Option<&str>,
    ) -> Result<bool, ApiError> {
        let affected = if suspended {
            sqlx::query(
                "UPDATE communities \
                 SET suspended = TRUE, suspended_at = now(), suspended_by = $2, \
                     suspension_reason = $3, updated_at = now() \
                 WHERE id = $1",
            )
            .bind(id)
            .bind(actor)
            .bind(reason)
            .execute(&self.pool)
            .await?
            .rows_affected()
        } else {
            sqlx::query(
                "UPDATE communities \
                 SET suspended = FALSE, suspended_at = NULL, suspended_by = NULL, \
                     suspension_reason = NULL, updated_at = now() \
                 WHERE id = $1",
            )
            .bind(id)
            .execute(&self.pool)
            .await?
            .rows_affected()
        };
        Ok(affected > 0)
    }

    pub async fn visible_communities_by_ids(
        &self,
        community_ids: &[Uuid],
        user_address: &str,
    ) -> Result<Vec<Uuid>, ApiError> {
        if community_ids.is_empty() {
            return Ok(Vec::new());
        }
        let lower = user_address.to_lowercase();
        let rows = sqlx::query_scalar::<_, Uuid>(
            "SELECT DISTINCT c.id \
             FROM communities c \
             LEFT JOIN community_members cm \
               ON c.id = cm.community_id AND cm.member_address = $2 \
             LEFT JOIN community_bans cb \
               ON c.id = cb.community_id AND cb.banned_address = $2 AND cb.active = TRUE \
             WHERE c.id = ANY($1) \
               AND c.active = TRUE \
               AND cb.banned_address IS NULL \
               AND (c.unlisted = FALSE OR cm.member_address IS NOT NULL)",
        )
        .bind(community_ids)
        .bind(&lower)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    pub async fn search_communities(
        &self,
        search: &str,
        user_address: &str,
        limit: i64,
        offset: i64,
    ) -> Result<(Vec<CommunitySearchResult>, i64), ApiError> {
        let lower = user_address.to_lowercase();
        let mut params: Vec<String> = vec![lower];

        let mut where_sql = String::from(
            "c.active = TRUE AND c.suspended = FALSE \
             AND (c.unlisted = FALSE OR EXISTS (\
                 SELECT 1 FROM community_members cm \
                 WHERE cm.community_id = c.id AND cm.member_address = $1))",
        );

        let search = search.trim();
        if !search.is_empty() {
            let esc = search.replace('%', "\\%").replace('_', "\\_");
            params.push(format!("{}%", esc));
            params.push(format!("% {}%", esc));
            let word = params.len();
            let prefix = word - 1;
            where_sql.push_str(&format!(
                " AND (c.name ILIKE ${} OR c.name ILIKE ${})",
                prefix, word
            ));
        }

        let limit_idx = params.len() + 1;
        let offset_idx = params.len() + 2;

        let select_sql = format!(
            "SELECT c.id, c.name, \
                    (SELECT COUNT(*) FROM community_members m WHERE m.community_id = c.id) AS members_count, \
                    c.private \
             FROM communities c \
             WHERE {where_sql} \
             ORDER BY c.name ASC \
             LIMIT ${limit_idx} OFFSET ${offset_idx}"
        );

        let mut q = sqlx::query_as::<_, (Uuid, String, i64, bool)>(sqlx::AssertSqlSafe(select_sql));
        for p in &params {
            q = q.bind(p);
        }
        q = q.bind(limit).bind(offset);
        let rows = q.fetch_all(&self.pool).await?;

        let count_sql = format!("SELECT COUNT(*) FROM communities c WHERE {where_sql}");
        let mut cq = sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(count_sql));
        for p in &params {
            cq = cq.bind(p);
        }
        let total = cq.fetch_one(&self.pool).await.unwrap_or(0);

        let results = rows
            .into_iter()
            .map(|(id, name, members_count, private)| CommunitySearchResult {
                id,
                name,
                members_count,
                privacy: if private { "private" } else { "public" },
            })
            .collect();

        Ok((results, total))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_authenticated_listing_excludes_unlisted() {
        let clause = default_visibility_clause(MEMBER_EXPR, true, false, false)
            .expect("default authed listing appends a visibility clause");
        assert_eq!(clause, "c.unlisted = FALSE");
        assert!(
            !clause.contains("member_address"),
            "default listing must NOT include the member disjunct (would leak unlisted): {clause}"
        );
    }

    #[test]
    fn unauthenticated_listing_is_public_only() {
        let clause = default_visibility_clause(MEMBER_EXPR, false, false, false)
            .expect("unauth listing appends a visibility clause");
        assert_eq!(clause, "c.unlisted = FALSE AND c.private = FALSE");
    }

    #[test]
    fn only_member_of_appends_no_visibility_clause() {
        assert!(default_visibility_clause(MEMBER_EXPR, true, true, false).is_none());
    }

    #[test]
    fn active_voice_chat_keeps_member_or_public_listed_filter() {
        let clause = default_visibility_clause(MEMBER_EXPR, true, false, true)
            .expect("voice-chat listing appends a visibility clause");
        assert_eq!(
            clause,
            format!("({MEMBER_EXPR} OR (c.unlisted = FALSE AND c.private = FALSE))")
        );
    }
}
