use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::postgres::PgPool;
use sqlx::Row;

use crate::parse;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "governance/")
)]
pub struct MemberRow {
    pub address: String,
    pub role: String,
    pub fetched_at: String,
}

#[derive(Clone)]
pub struct Store {
    pool: PgPool,
}

impl Store {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    pub async fn upsert_proposals(&self, proposals: &[Value]) -> Result<u64> {
        let mut n = 0u64;
        for p in proposals {
            let Some(id) = parse::opt_str(p, "id") else {
                continue;
            };
            sqlx::query(
                r#"
                INSERT INTO proposals
                    (id, title, description, type, status, "user",
                     snapshot_id, snapshot_space, snapshot_network, snapshot_proposal,
                     discourse_id, discourse_topic_id, discourse_topic_slug,
                     start_at, finish_at, created_at, updated_at,
                     enacted, enacted_by, enacted_description, enacting_tx,
                     passed_by, passed_description, rejected_by, rejected_description,
                     deleted, deleted_by, required_to_pass,
                     vesting_addresses, configuration, raw, fetched_at)
                VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,
                        $18,$19,$20,$21,$22,$23,$24,$25,$26,$27,$28,$29,$30,$31, now())
                ON CONFLICT (id) DO UPDATE SET
                    title=EXCLUDED.title, description=EXCLUDED.description,
                    type=EXCLUDED.type, status=EXCLUDED.status, "user"=EXCLUDED."user",
                    snapshot_id=EXCLUDED.snapshot_id, snapshot_space=EXCLUDED.snapshot_space,
                    snapshot_network=EXCLUDED.snapshot_network,
                    snapshot_proposal=EXCLUDED.snapshot_proposal,
                    discourse_id=EXCLUDED.discourse_id,
                    discourse_topic_id=EXCLUDED.discourse_topic_id,
                    discourse_topic_slug=EXCLUDED.discourse_topic_slug,
                    start_at=EXCLUDED.start_at, finish_at=EXCLUDED.finish_at,
                    created_at=EXCLUDED.created_at, updated_at=EXCLUDED.updated_at,
                    enacted=EXCLUDED.enacted, enacted_by=EXCLUDED.enacted_by,
                    enacted_description=EXCLUDED.enacted_description,
                    enacting_tx=EXCLUDED.enacting_tx,
                    passed_by=EXCLUDED.passed_by, passed_description=EXCLUDED.passed_description,
                    rejected_by=EXCLUDED.rejected_by,
                    rejected_description=EXCLUDED.rejected_description,
                    deleted=EXCLUDED.deleted, deleted_by=EXCLUDED.deleted_by,
                    required_to_pass=EXCLUDED.required_to_pass,
                    vesting_addresses=EXCLUDED.vesting_addresses,
                    configuration=EXCLUDED.configuration,
                    raw=EXCLUDED.raw, fetched_at=now()
                "#,
            )
            .bind(&id)
            .bind(parse::opt_str(p, "title"))
            .bind(parse::opt_str(p, "description"))
            .bind(parse::opt_str(p, "type"))
            .bind(parse::opt_str(p, "status"))
            .bind(parse::opt_str(p, "user"))
            .bind(parse::opt_str(p, "snapshot_id"))
            .bind(parse::opt_str(p, "snapshot_space"))
            .bind(parse::opt_str(p, "snapshot_network"))
            .bind(parse::opt_json(p, "snapshot_proposal"))
            .bind(parse::opt_i32(p, "discourse_id"))
            .bind(parse::opt_i32(p, "discourse_topic_id"))
            .bind(parse::opt_str(p, "discourse_topic_slug"))
            .bind(parse::parse_ts(parse::field(p, "start_at")))
            .bind(parse::parse_ts(parse::field(p, "finish_at")))
            .bind(parse::parse_ts(parse::field(p, "created_at")))
            .bind(parse::parse_ts(parse::field(p, "updated_at")))
            .bind(parse::opt_bool(p, "enacted"))
            .bind(parse::opt_str(p, "enacted_by"))
            .bind(parse::opt_str(p, "enacted_description"))
            .bind(parse::opt_str(p, "enacting_tx"))
            .bind(parse::opt_str(p, "passed_by"))
            .bind(parse::opt_str(p, "passed_description"))
            .bind(parse::opt_str(p, "rejected_by"))
            .bind(parse::opt_str(p, "rejected_description"))
            .bind(parse::opt_bool(p, "deleted"))
            .bind(parse::opt_str(p, "deleted_by"))
            .bind(parse::opt_i32(p, "required_to_pass"))
            .bind(parse::opt_json(p, "vesting_addresses"))
            .bind(parse::opt_json(p, "configuration"))
            .bind(p)
            .execute(&self.pool)
            .await
            .context("upsert proposal")?;
            n += 1;
        }
        Ok(n)
    }

    pub async fn upsert_projects(&self, projects: &[Value]) -> Result<u64> {
        let mut n = 0u64;
        for p in projects {
            let Some(id) = parse::opt_str(p, "id") else {
                continue;
            };
            sqlx::query(
                r#"
                INSERT INTO projects
                    (id, proposal_id, title, status, type, "user",
                     enacting_tx, enacted_description, configuration,
                     vesting_addresses, funding, latest_update,
                     created_at, updated_at, raw, fetched_at)
                VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15, now())
                ON CONFLICT (id) DO UPDATE SET
                    proposal_id=EXCLUDED.proposal_id, title=EXCLUDED.title,
                    status=EXCLUDED.status, type=EXCLUDED.type, "user"=EXCLUDED."user",
                    enacting_tx=EXCLUDED.enacting_tx,
                    enacted_description=EXCLUDED.enacted_description,
                    configuration=EXCLUDED.configuration,
                    vesting_addresses=EXCLUDED.vesting_addresses,
                    funding=EXCLUDED.funding, latest_update=EXCLUDED.latest_update,
                    created_at=EXCLUDED.created_at, updated_at=EXCLUDED.updated_at,
                    raw=EXCLUDED.raw, fetched_at=now()
                "#,
            )
            .bind(&id)
            .bind(parse::opt_str(p, "proposal_id"))
            .bind(parse::opt_str(p, "title"))
            .bind(parse::opt_str(p, "status"))
            .bind(parse::opt_str(p, "type"))
            .bind(parse::opt_str(p, "author"))
            .bind(parse::opt_str(p, "enacting_tx"))
            .bind(parse::opt_str(p, "enacted_description"))
            .bind(parse::opt_json(p, "configuration"))
            .bind(parse::opt_json(p, "vesting_addresses"))
            .bind(parse::opt_json(p, "funding"))
            .bind(parse::opt_json(p, "latest_update"))
            .bind(parse::parse_ts(parse::field(p, "created_at")))
            .bind(parse::parse_ts(parse::field(p, "updated_at")))
            .bind(p)
            .execute(&self.pool)
            .await
            .context("upsert project")?;
            n += 1;
        }
        Ok(n)
    }

    pub async fn upsert_project_updates(&self, updates: &[Value]) -> Result<u64> {
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut n = 0u64;
        for u in updates {
            let Some(id) = parse::opt_str(u, "id") else {
                continue;
            };
            if !seen.insert(id.clone()) {
                continue;
            }
            sqlx::query(
                r#"
                INSERT INTO project_updates
                    (id, project_id, proposal_id, status, health,
                     introduction, highlights, blockers, next_steps,
                     additional_notes, author, due_date, completion_date,
                     discourse_topic_id, discourse_topic_slug,
                     created_at, updated_at, raw, fetched_at)
                VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18, now())
                ON CONFLICT (id) DO UPDATE SET
                    project_id=EXCLUDED.project_id, proposal_id=EXCLUDED.proposal_id,
                    status=EXCLUDED.status, health=EXCLUDED.health,
                    introduction=EXCLUDED.introduction, highlights=EXCLUDED.highlights,
                    blockers=EXCLUDED.blockers, next_steps=EXCLUDED.next_steps,
                    additional_notes=EXCLUDED.additional_notes, author=EXCLUDED.author,
                    due_date=EXCLUDED.due_date, completion_date=EXCLUDED.completion_date,
                    discourse_topic_id=EXCLUDED.discourse_topic_id,
                    discourse_topic_slug=EXCLUDED.discourse_topic_slug,
                    created_at=EXCLUDED.created_at, updated_at=EXCLUDED.updated_at,
                    raw=EXCLUDED.raw, fetched_at=now()
                "#,
            )
            .bind(&id)
            .bind(parse::opt_str(u, "project_id"))
            .bind(parse::opt_str(u, "proposal_id"))
            .bind(parse::opt_str(u, "status"))
            .bind(parse::opt_str(u, "health"))
            .bind(parse::opt_str(u, "introduction"))
            .bind(parse::opt_str(u, "highlights"))
            .bind(parse::opt_str(u, "blockers"))
            .bind(parse::opt_str(u, "next_steps"))
            .bind(parse::opt_str(u, "additional_notes"))
            .bind(parse::opt_str(u, "author"))
            .bind(parse::parse_ts(parse::field(u, "due_date")))
            .bind(parse::parse_ts(parse::field(u, "completion_date")))
            .bind(parse::opt_i32(u, "discourse_topic_id"))
            .bind(parse::opt_str(u, "discourse_topic_slug"))
            .bind(parse::parse_ts(parse::field(u, "created_at")))
            .bind(parse::parse_ts(parse::field(u, "updated_at")))
            .bind(u)
            .execute(&self.pool)
            .await
            .context("upsert project_update")?;
            n += 1;
        }
        Ok(n)
    }

    pub async fn upsert_budgets(&self, budgets: &[Value]) -> Result<u64> {
        let mut n = 0u64;
        for b in budgets {
            let Some(id) = parse::opt_str(b, "id") else {
                continue;
            };
            sqlx::query(
                r#"
                INSERT INTO budgets
                    (id, start_at, finish_at, total, allocated, categories, raw, fetched_at)
                VALUES ($1,$2,$3,$4,$5,$6,$7, now())
                ON CONFLICT (id) DO UPDATE SET
                    start_at=EXCLUDED.start_at, finish_at=EXCLUDED.finish_at,
                    total=EXCLUDED.total, allocated=EXCLUDED.allocated,
                    categories=EXCLUDED.categories, raw=EXCLUDED.raw, fetched_at=now()
                "#,
            )
            .bind(&id)
            .bind(parse::parse_ts(parse::field(b, "start_at")))
            .bind(parse::parse_ts(parse::field(b, "finish_at")))
            .bind(parse::opt_i64(b, "total"))
            .bind(parse::opt_i64(b, "allocated"))
            .bind(parse::opt_json(b, "categories"))
            .bind(b)
            .execute(&self.pool)
            .await
            .context("upsert budget")?;
            n += 1;
        }
        Ok(n)
    }

    pub async fn upsert_vestings(&self, vestings: &[Value]) -> Result<u64> {
        let mut n = 0u64;
        for v in vestings {
            let Some(addr) = parse::opt_str(v, "address") else {
                continue;
            };
            sqlx::query(
                r#"
                INSERT INTO vestings
                    (address, token, status, total, vested, released, releasable,
                     start_at, finish_at, raw, fetched_at)
                VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10, now())
                ON CONFLICT (address) DO UPDATE SET
                    token=EXCLUDED.token, status=EXCLUDED.status,
                    total=EXCLUDED.total, vested=EXCLUDED.vested,
                    released=EXCLUDED.released, releasable=EXCLUDED.releasable,
                    start_at=EXCLUDED.start_at, finish_at=EXCLUDED.finish_at,
                    raw=EXCLUDED.raw, fetched_at=now()
                "#,
            )
            .bind(&addr)
            .bind(parse::opt_str(v, "token"))
            .bind(parse::opt_str(v, "status"))
            .bind(parse::opt_i64(v, "total"))
            .bind(parse::opt_i64(v, "vested"))
            .bind(parse::opt_i64(v, "released"))
            .bind(parse::opt_i64(v, "releasable"))
            .bind(parse::opt_str(v, "start_at").unwrap_or_default())
            .bind(parse::opt_str(v, "finish_at").unwrap_or_default())
            .bind(v)
            .execute(&self.pool)
            .await
            .context("upsert vesting")?;
            n += 1;
        }
        Ok(n)
    }

    pub async fn replace_members(&self, role: &str, addresses: &[String]) -> Result<u64> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("DELETE FROM members WHERE role = $1")
            .bind(role)
            .execute(&mut *tx)
            .await
            .context("delete members")?;
        for addr in addresses {
            sqlx::query(
                "INSERT INTO members (address, role) VALUES ($1, $2) \
                 ON CONFLICT (address, role) DO UPDATE SET fetched_at = now()",
            )
            .bind(addr)
            .bind(role)
            .execute(&mut *tx)
            .await
            .context("insert member")?;
        }
        tx.commit().await?;
        Ok(addresses.len() as u64)
    }

    pub async fn set_sync_state(&self, key: &str, value: &str) -> Result<()> {
        sqlx::query(
            "INSERT INTO sync_state (key, value, updated_at) VALUES ($1, $2, now()) \
             ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value, updated_at = now()",
        )
        .bind(key)
        .bind(value)
        .execute(&self.pool)
        .await
        .context("set sync_state")?;
        Ok(())
    }

    async fn list_raw(&self, sql: &'static str, limit: i64, offset: i64) -> Result<Vec<Value>> {
        let rows = sqlx::query(sql)
            .bind(limit)
            .bind(offset)
            .fetch_all(&self.pool)
            .await
            .context("list raw rows")?;
        Ok(rows.into_iter().map(|r| r.get::<Value, _>("raw")).collect())
    }

    pub async fn list_proposals(
        &self,
        limit: i64,
        offset: i64,
        type_filter: Option<&str>,
        linked_proposal_id: Option<&str>,
        id_filter: Option<&str>,
        status_filter: Option<&str>,
    ) -> Result<Vec<Value>> {
        let mut sql = String::from("SELECT raw FROM proposals WHERE TRUE");
        let mut n = 0;
        if id_filter.is_some() {
            n += 1;
            sql.push_str(&format!(" AND id = ${n}"));
        }
        if type_filter.is_some() {
            n += 1;
            sql.push_str(&format!(" AND type = ${n}"));
        }
        if linked_proposal_id.is_some() {
            n += 1;
            sql.push_str(&format!(" AND configuration->>'linked_proposal_id' = ${n}"));
        }
        if status_filter.is_some() {
            n += 1;
            sql.push_str(&format!(" AND status = ${n}"));
        }
        sql.push_str(&format!(
            " ORDER BY fetched_at DESC LIMIT ${} OFFSET ${}",
            n + 1,
            n + 2
        ));

        let mut q = sqlx::query(sqlx::AssertSqlSafe(sql));
        if let Some(v) = id_filter {
            q = q.bind(v.to_owned());
        }
        if let Some(v) = type_filter {
            q = q.bind(v.to_owned());
        }
        if let Some(v) = linked_proposal_id {
            q = q.bind(v.to_owned());
        }
        if let Some(v) = status_filter {
            q = q.bind(v.to_owned());
        }
        let rows = q
            .bind(limit)
            .bind(offset)
            .fetch_all(&self.pool)
            .await
            .context("list proposals")?;
        Ok(rows.into_iter().map(|r| r.get::<Value, _>("raw")).collect())
    }

    pub async fn list_projects(&self, limit: i64, offset: i64) -> Result<Vec<Value>> {
        self.list_raw(
            "SELECT raw FROM projects ORDER BY fetched_at DESC LIMIT $1 OFFSET $2",
            limit,
            offset,
        )
        .await
    }

    pub async fn get_project_detail(
        &self,
        id: &str,
    ) -> Result<Option<(Value, Option<Value>, Vec<Value>)>> {
        let Some(row) = sqlx::query("SELECT raw, proposal_id FROM projects WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .context("get project")?
        else {
            return Ok(None);
        };
        let project_raw: Value = row.get("raw");
        let proposal_id: Option<String> = row.get("proposal_id");

        let proposal_cfg: Option<Value> = match proposal_id.as_deref() {
            Some(pid) => sqlx::query("SELECT configuration FROM proposals WHERE id = $1")
                .bind(pid)
                .fetch_optional(&self.pool)
                .await
                .context("get proposal configuration")?
                .and_then(|r| r.get::<Option<Value>, _>("configuration")),
            None => None,
        };

        let updates = sqlx::query(
            "SELECT raw FROM project_updates WHERE project_id = $1 \
             ORDER BY created_at ASC NULLS LAST",
        )
        .bind(id)
        .fetch_all(&self.pool)
        .await
        .context("list project updates")?
        .into_iter()
        .map(|r| r.get::<Value, _>("raw"))
        .collect();

        Ok(Some((project_raw, proposal_cfg, updates)))
    }

    pub async fn list_budgets(&self, limit: i64, offset: i64) -> Result<Vec<Value>> {
        self.list_raw(
            "SELECT raw FROM budgets ORDER BY fetched_at DESC LIMIT $1 OFFSET $2",
            limit,
            offset,
        )
        .await
    }

    pub async fn list_vestings(&self, limit: i64, offset: i64) -> Result<Vec<Value>> {
        self.list_raw(
            "SELECT raw FROM vestings ORDER BY fetched_at DESC LIMIT $1 OFFSET $2",
            limit,
            offset,
        )
        .await
    }

    pub async fn list_members(&self, limit: i64, offset: i64) -> Result<Vec<MemberRow>> {
        let rows = sqlx::query(
            "SELECT address, role, fetched_at FROM members \
             ORDER BY role, address LIMIT $1 OFFSET $2",
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .context("list members")?;
        Ok(rows
            .into_iter()
            .map(|r| {
                let fetched: chrono::DateTime<chrono::Utc> = r.get("fetched_at");
                MemberRow {
                    address: r.get::<String, _>("address"),
                    role: r.get::<String, _>("role"),
                    fetched_at: fetched.to_rfc3339(),
                }
            })
            .collect())
    }
}

impl Store {
    pub async fn proposal_refs(&self, id: &str) -> Result<Option<(Option<String>, Option<i64>)>> {
        let row =
            sqlx::query("SELECT snapshot_id, discourse_topic_id FROM proposals WHERE id = $1")
                .bind(id)
                .fetch_optional(&self.pool)
                .await
                .context("proposal refs")?;
        Ok(row.map(|r| {
            let snapshot_id = r.try_get::<Option<String>, _>("snapshot_id").ok().flatten();
            let topic = r
                .try_get::<Option<i32>, _>("discourse_topic_id")
                .ok()
                .flatten()
                .map(i64::from);
            (snapshot_id, topic)
        }))
    }

    pub async fn recent_proposals(
        &self,
        limit: i64,
    ) -> Result<Vec<(String, String, Option<String>, i64)>> {
        let rows = sqlx::query(
            "SELECT id, COALESCE(title, '') AS title, \"user\",
                    COALESCE(EXTRACT(EPOCH FROM created_at), 0)::bigint AS ts
             FROM proposals WHERE created_at IS NOT NULL
             ORDER BY created_at DESC LIMIT $1",
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .context("recent proposals")?;
        Ok(rows
            .iter()
            .map(|r| {
                (
                    r.get::<String, _>("id"),
                    r.get::<String, _>("title"),
                    r.try_get::<Option<String>, _>("user").ok().flatten(),
                    r.get::<i64, _>("ts"),
                )
            })
            .collect())
    }

    pub async fn recently_finished(&self, limit: i64) -> Result<Vec<(String, String, i64)>> {
        let rows = sqlx::query(
            "SELECT id, COALESCE(title, '') AS title,
                    COALESCE(EXTRACT(EPOCH FROM finish_at), 0)::bigint AS ts
             FROM proposals WHERE finish_at IS NOT NULL AND finish_at < now()
             ORDER BY finish_at DESC LIMIT $1",
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .context("recently finished")?;
        Ok(rows
            .iter()
            .map(|r| {
                (
                    r.get::<String, _>("id"),
                    r.get::<String, _>("title"),
                    r.get::<i64, _>("ts"),
                )
            })
            .collect())
    }

    pub async fn recent_updates(&self, limit: i64) -> Result<Vec<(Option<String>, String, i64)>> {
        let rows = sqlx::query(
            "SELECT pu.proposal_id,
                    COALESCE(pr.title, p.title, '') AS title,
                    COALESCE(EXTRACT(EPOCH FROM pu.created_at), 0)::bigint AS ts
             FROM project_updates pu
             LEFT JOIN projects pr ON pr.id = pu.project_id
             LEFT JOIN proposals p ON p.id = pu.proposal_id
             WHERE pu.created_at IS NOT NULL
             ORDER BY pu.created_at DESC LIMIT $1",
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .context("recent updates")?;
        Ok(rows
            .iter()
            .map(|r| {
                (
                    r.try_get::<Option<String>, _>("proposal_id").ok().flatten(),
                    r.get::<String, _>("title"),
                    r.get::<i64, _>("ts"),
                )
            })
            .collect())
    }

    pub async fn titles_by_snapshot_ids(
        &self,
        ids: &[String],
    ) -> Result<Vec<(String, String, String)>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let rows = sqlx::query(
            "SELECT snapshot_id, id, COALESCE(title, '') AS title
             FROM proposals WHERE snapshot_id = ANY($1)",
        )
        .bind(ids)
        .fetch_all(&self.pool)
        .await
        .context("titles by snapshot ids")?;
        Ok(rows
            .iter()
            .filter_map(|r| {
                r.try_get::<Option<String>, _>("snapshot_id")
                    .ok()
                    .flatten()
                    .map(|sid| (sid, r.get::<String, _>("id"), r.get::<String, _>("title")))
            })
            .collect())
    }
}
