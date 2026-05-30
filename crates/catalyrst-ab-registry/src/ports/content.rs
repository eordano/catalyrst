use std::collections::HashMap;

use sqlx::{PgPool, Row};

use crate::types::ContentFile;

#[derive(Debug, Clone)]
pub struct ActiveEntity {
    pub deployment_id: i32,
    pub entity_id: String,
    pub entity_type: String,
    pub timestamp: i64,
    pub pointers: Vec<String>,
    pub metadata: serde_json::Value,
    pub deployer_address: Option<String>,
    pub content: Vec<ContentFile>,
}

impl ActiveEntity {
    pub fn world_name(&self) -> Option<&str> {
        self.metadata
            .get("worldConfiguration")
            .and_then(|w| w.get("name"))
            .and_then(|n| n.as_str())
    }

    pub fn is_world(&self) -> bool {
        self.world_name().is_some()
    }
}

#[derive(Clone)]
pub struct ContentComponent {
    pool: PgPool,
}

impl ContentComponent {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    pub async fn resolve_pointers(
        &self,
        pointers: &[String],
    ) -> Result<Vec<ActiveEntity>, sqlx::Error> {
        if pointers.is_empty() {
            return Ok(Vec::new());
        }
        let lowered: Vec<String> = pointers.iter().map(|p| p.to_lowercase()).collect();

        let rows = sqlx::query(
            r#"
            SELECT
                d.id,
                d.entity_id,
                d.entity_type,
                date_part('epoch', d.entity_timestamp) * 1000 AS ts,
                d.entity_pointers,
                d.entity_metadata,
                d.deployer_address
            FROM deployments d
            WHERE d.deleter_deployment IS NULL
              AND (
                    -- entity_pointers are stored already-lowercased (profiles =
                    -- lowercase 0x addresses, scenes = coords), and `lowered` is
                    -- lowercased in Rust above, so a plain && uses the GIN index
                    -- deployments_entity_pointers_index. Wrapping the column in
                    -- ARRAY(SELECT lower(p) FROM unnest(...)) defeated the index
                    -- and forced a full seq scan (~2.8s -> ~3ms).
                    d.entity_pointers && $1
                 OR d.entity_id = ANY($2)
              )
            "#,
        )
        .bind(&lowered)
        .bind(pointers)
        .fetch_all(&self.pool)
        .await?;

        self.hydrate(rows).await
    }

    pub async fn resolve_one(
        &self,
        id: &str,
    ) -> Result<Option<ActiveEntity>, sqlx::Error> {
        let mut all = self.resolve_pointers(std::slice::from_ref(&id.to_string())).await?;

        if let Some(pos) = all.iter().position(|e| e.entity_id == id) {
            return Ok(Some(all.swap_remove(pos)));
        }
        Ok(all.into_iter().next())
    }

    pub async fn active_entities_by_deployer(
        &self,
        deployer: &str,
    ) -> Result<Vec<ActiveEntity>, sqlx::Error> {
        let rows = sqlx::query(
            r#"
            SELECT
                d.id,
                d.entity_id,
                d.entity_type,
                date_part('epoch', d.entity_timestamp) * 1000 AS ts,
                d.entity_pointers,
                d.entity_metadata,
                d.deployer_address
            FROM deployments d
            WHERE d.deleter_deployment IS NULL
              AND lower(d.deployer_address) = lower($1)
            "#,
        )
        .bind(deployer)
        .fetch_all(&self.pool)
        .await?;
        self.hydrate(rows).await
    }

    pub async fn active_entity_ids_of_types(
        &self,
        types: &[&str],
        limit: i64,
    ) -> Result<Vec<String>, sqlx::Error> {
        let types_owned: Vec<String> = types.iter().map(|s| s.to_string()).collect();
        let rows = sqlx::query(
            r#"
            SELECT d.entity_id
            FROM deployments d
            WHERE d.deleter_deployment IS NULL
              AND d.entity_type = ANY($1)
            ORDER BY d.entity_timestamp DESC
            LIMIT $2
            "#,
        )
        .bind(&types_owned)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(|r| r.get::<String, _>("entity_id")).collect())
    }

    pub async fn resolve_profiles(
        &self,
        addresses: &[String],
    ) -> Result<Vec<ActiveEntity>, sqlx::Error> {
        if addresses.is_empty() {
            return Ok(Vec::new());
        }
        let lowered: Vec<String> = addresses.iter().map(|p| p.to_lowercase()).collect();
        let rows = sqlx::query(
            r#"
            SELECT
                d.id,
                d.entity_id,
                d.entity_type,
                date_part('epoch', d.entity_timestamp) * 1000 AS ts,
                d.entity_pointers,
                d.entity_metadata,
                d.deployer_address
            FROM deployments d
            WHERE d.deleter_deployment IS NULL
              AND d.entity_type = 'profile'
              -- plain && uses the GIN index (pointers stored lowercased; `lowered`
              -- lowercased in Rust). The ARRAY(SELECT lower(p)...) wrapper forced
              -- a full seq scan (~2.8s -> ~1.6ms).
              AND d.entity_pointers && $1
            "#,
        )
        .bind(&lowered)
        .fetch_all(&self.pool)
        .await?;
        self.hydrate(rows).await
    }

    async fn hydrate(
        &self,
        rows: Vec<sqlx::postgres::PgRow>,
    ) -> Result<Vec<ActiveEntity>, sqlx::Error> {
        let mut by_entity: HashMap<String, ActiveEntity> = HashMap::new();
        for row in rows {
            let deployment_id: i32 = row.get("id");
            let entity_id: String = row.get("entity_id");
            let ts: f64 = row.try_get("ts").unwrap_or(0.0);
            let timestamp = ts as i64;
            let ent = ActiveEntity {
                deployment_id,
                entity_id: entity_id.clone(),
                entity_type: row.get("entity_type"),
                timestamp,
                pointers: row.try_get("entity_pointers").unwrap_or_default(),
                // entity_metadata is stored DB-side as the wrapper {"v": {...}} (TS
                // catalyst storage format). Serve the unwrapped value — clients read
                // metadata.id/name/data directly; leaking the wrapper renders every
                // wearable DTO empty (avatars go bald). Same unwrap as
                // catalyrst-server entity_cache.rs.
                metadata: row
                    .try_get::<Option<serde_json::Value>, _>("entity_metadata")
                    .ok()
                    .flatten()
                    .map(|m| m.get("v").cloned().unwrap_or(m))
                    .unwrap_or(serde_json::Value::Null),
                deployer_address: row.try_get("deployer_address").ok(),
                content: Vec::new(),
            };
            by_entity
                .entry(entity_id)
                .and_modify(|cur| {
                    if ent.timestamp > cur.timestamp {
                        *cur = ent.clone();
                    }
                })
                .or_insert(ent);
        }

        if by_entity.is_empty() {
            return Ok(Vec::new());
        }

        let dep_ids: Vec<i32> = by_entity.values().map(|e| e.deployment_id).collect();
        let files = sqlx::query(
            "SELECT deployment, key, content_hash FROM content_files WHERE deployment = ANY($1)",
        )
        .bind(&dep_ids)
        .fetch_all(&self.pool)
        .await?;

        let mut by_dep: HashMap<i32, Vec<ContentFile>> = HashMap::new();
        for row in files {
            let dep: i32 = row.get("deployment");
            by_dep.entry(dep).or_default().push(ContentFile {
                file: row.get("key"),
                hash: row.get("content_hash"),
            });
        }
        for ent in by_entity.values_mut() {
            if let Some(mut c) = by_dep.remove(&ent.deployment_id) {
                c.sort_by(|a, b| a.file.cmp(&b.file));
                ent.content = c;
            }
        }

        Ok(by_entity.into_values().collect())
    }
}
