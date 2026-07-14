use std::collections::HashMap;

pub use crate::profiles::{NameColor, ProfileInfo, ProfilesCache as ProfilesComponent};

impl crate::profiles::ProfilesCache {
    pub async fn get_owner_names(&self, addresses: &[String]) -> HashMap<String, String> {
        self.get_profiles(addresses)
            .await
            .into_iter()
            .map(|(addr, info)| (addr, info.name))
            .collect()
    }

    pub async fn has_owned_name(&self, address: &str) -> Option<bool> {
        let pool = self.pool.as_ref()?;
        let addr = address.to_lowercase();
        let row: Option<(bool,)> = sqlx::query_as(
            "SELECT COALESCE((d.entity_metadata::jsonb #>> '{v,avatars,0,hasClaimedName}')::bool, false) \
             FROM deployments d \
             WHERE d.entity_type = 'profile' \
               AND d.deleter_deployment IS NULL \
               AND d.entity_pointers && ARRAY[$1]::text[] \
             LIMIT 1",
        )
        .bind(&addr)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten();
        Some(row.map(|(c,)| c).unwrap_or(false))
    }
}
