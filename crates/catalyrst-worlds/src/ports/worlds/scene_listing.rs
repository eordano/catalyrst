use sqlx::Row;

use crate::http::ApiError;

use super::types::{WorldSceneRow, WorldsCount};
use super::WorldsComponent;

impl WorldsComponent {
    pub async fn list_scenes_full(&self, world_name: &str) -> Result<Vec<WorldSceneRow>, ApiError> {
        let rows = sqlx::query(
            r#"SELECT world_name, entity_id, deployment_auth_chain, entity, deployer,
                      parcels, size, created_at, updated_at
               FROM world_scenes
               WHERE lower(world_name) = lower($1)
               ORDER BY entity_id"#,
        )
        .bind(world_name)
        .fetch_all(self.pool())
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| WorldSceneRow {
                world_name: r.get("world_name"),
                entity_id: r.get("entity_id"),
                deployment_auth_chain: r.get("deployment_auth_chain"),
                entity: r.get("entity"),
                deployer: r.get("deployer"),
                parcels: r.get("parcels"),
                size: r.get("size"),
                created_at: r.get("created_at"),
                updated_at: r.get("updated_at"),
            })
            .collect())
    }

    pub async fn get_deployed_world_count(&self) -> Result<WorldsCount, ApiError> {
        let rows = sqlx::query(r#"SELECT DISTINCT lower(world_name) AS name FROM world_scenes"#)
            .fetch_all(self.pool())
            .await?;

        let mut count = WorldsCount::default();
        for r in &rows {
            let name: String = r.get("name");
            if name.ends_with(".dcl.eth") {
                count.dcl += 1;
            } else {
                count.ens += 1;
            }
        }
        Ok(count)
    }
}
