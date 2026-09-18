use chrono::{DateTime, Utc};
use moka::future::Cache;
use sqlx::postgres::PgPool;
use sqlx::Row;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use super::types::{
    AchievedTier, Assets, BadgeData, BadgeProgress, LatestAchievedBadge, TierCriteria, TierData,
};
use crate::config::DEFAULT_ASSET_BASE_URL;
use crate::http::errors::ApiError;

/// Walks arbitrarily nested JSON so it need not know the `{"2d":{...},"3d":{...}}` shape;
/// empty strings (the unfilled 3D fields in the seed fixture) never match a non-empty `from`
/// prefix and pass through unchanged.
fn rewrite_base(value: &serde_json::Value, from: &str, to: &str) -> serde_json::Value {
    if from == to {
        return value.clone();
    }
    match value {
        serde_json::Value::String(s) => match s.strip_prefix(from) {
            Some(rest) => serde_json::Value::String(format!("{to}{rest}")),
            None => value.clone(),
        },
        serde_json::Value::Object(map) => serde_json::Value::Object(
            map.iter()
                .map(|(k, v)| (k.clone(), rewrite_base(v, from, to)))
                .collect(),
        ),
        serde_json::Value::Array(arr) => {
            serde_json::Value::Array(arr.iter().map(|v| rewrite_base(v, from, to)).collect())
        }
        other => other.clone(),
    }
}

fn epoch_ms(ts: DateTime<Utc>) -> String {
    ts.timestamp_millis().to_string()
}

const CATALOG_TTL: Duration = Duration::from_secs(300);

pub struct BadgesComponent {
    pool: PgPool,
    public_asset_base_url: String,
    catalog: Cache<(), Arc<Catalog>>,
}

/// Definitions and tiers change only by migration, so every read shares one
/// 300 s snapshot instead of re-reading both tables per request.
struct Catalog {
    defs: Vec<DefRow>,
    tiers: HashMap<String, Vec<TierRow>>,
}

type AchievedMap = HashMap<String, Vec<(String, DateTime<Utc>)>>;

struct DefRow {
    id: String,
    name: String,
    description: Option<String>,
    category: Option<String>,
    is_tier: bool,
    assets: Assets,
}

struct TierRow {
    tier_id: String,
    tier_name: String,
    description: Option<String>,
    assets: Assets,
    criteria_steps: i32,
}

impl BadgesComponent {
    pub fn new(pool: PgPool, public_asset_base_url: String) -> Self {
        Self {
            pool,
            public_asset_base_url,
            catalog: Cache::builder()
                .max_capacity(1)
                .time_to_live(CATALOG_TTL)
                .build(),
        }
    }

    async fn catalog(&self) -> Result<Arc<Catalog>, ApiError> {
        if let Some(c) = self.catalog.get(&()).await {
            return Ok(c);
        }
        let (defs, tiers) = tokio::try_join!(self.load_definitions(), self.load_all_tiers())?;
        let c = Arc::new(Catalog { defs, tiers });
        self.catalog.insert((), c.clone()).await;
        Ok(c)
    }

    /// DB rows stay upstream-parity-faithful (always `badges.decentraland.org`); only the
    /// HTTP response reflects the deployment's self-hosted asset host.
    fn rewrite_assets(&self, assets: Assets) -> Assets {
        rewrite_base(&assets, DEFAULT_ASSET_BASE_URL, &self.public_asset_base_url)
    }

    pub async fn list_categories(&self) -> Result<Vec<String>, ApiError> {
        let rows = sqlx::query(
            "SELECT DISTINCT category FROM badge_definitions \
             WHERE category IS NOT NULL ORDER BY category",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(|r| r.get::<String, _>(0)).collect())
    }

    async fn load_definitions(&self) -> Result<Vec<DefRow>, ApiError> {
        let rows = sqlx::query(
            "SELECT id, name, description, category, is_tier, assets \
             FROM badge_definitions ORDER BY id",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| DefRow {
                id: r.get("id"),
                name: r.get("name"),
                description: r.get("description"),
                category: r.get("category"),
                is_tier: r.get("is_tier"),
                assets: self.rewrite_assets(r.get("assets")),
            })
            .collect())
    }

    async fn load_all_tiers(&self) -> Result<HashMap<String, Vec<TierRow>>, ApiError> {
        let rows = sqlx::query(
            "SELECT badge_id, tier_id, tier_name, description, assets, criteria_steps \
             FROM badge_tiers ORDER BY badge_id, ordinal",
        )
        .fetch_all(&self.pool)
        .await?;
        let mut map: HashMap<String, Vec<TierRow>> = HashMap::new();
        for r in rows {
            let badge_id: String = r.get("badge_id");
            map.entry(badge_id).or_default().push(TierRow {
                tier_id: r.get("tier_id"),
                tier_name: r.get("tier_name"),
                description: r.get("description"),
                assets: self.rewrite_assets(r.get("assets")),
                criteria_steps: r.get("criteria_steps"),
            });
        }
        Ok(map)
    }

    pub async fn list_tiers(&self, badge_id: &str) -> Result<Vec<TierData>, ApiError> {
        let catalog = self.catalog().await?;
        if !catalog.defs.iter().any(|d| d.id == badge_id) {
            return Err(ApiError::not_found("Badge not found"));
        }
        Ok(catalog
            .tiers
            .get(badge_id)
            .map(|ts| {
                ts.iter()
                    .map(|t| TierData {
                        tier_id: t.tier_id.clone(),
                        tier_name: t.tier_name.clone(),
                        description: t.description.clone(),
                        assets: t.assets.clone(),
                        criteria: TierCriteria {
                            steps: t.criteria_steps,
                        },
                    })
                    .collect()
            })
            .unwrap_or_default())
    }

    /// Both per-address tables in one statement; tier rows arrive in
    /// `completed_at` order, which is the order the assemblers rely on.
    async fn load_user_state(
        &self,
        address: &str,
    ) -> Result<(HashMap<String, ProgressRow>, AchievedMap), ApiError> {
        let rows = sqlx::query_as::<
            _,
            (
                String,
                String,
                Option<i32>,
                Option<DateTime<Utc>>,
                Option<String>,
            ),
        >(
            "SELECT 'p' AS kind, badge_id, steps_done, completed_at, NULL::text AS tier_id \
             FROM user_badge_progress WHERE address = $1 \
             UNION ALL \
             SELECT 't', badge_id, NULL::integer, completed_at, tier_id \
             FROM user_achieved_tiers WHERE address = $1 \
             ORDER BY 1, 4",
        )
        .bind(address)
        .fetch_all(&self.pool)
        .await?;
        let mut progress = HashMap::new();
        let mut achieved: AchievedMap = HashMap::new();
        for (kind, badge_id, steps_done, completed_at, tier_id) in rows {
            match (kind.as_str(), tier_id, completed_at) {
                ("t", Some(tier_id), Some(at)) => {
                    achieved.entry(badge_id).or_default().push((tier_id, at))
                }
                ("p", _, completed_at) => {
                    progress.insert(
                        badge_id,
                        ProgressRow {
                            steps_done: steps_done.unwrap_or(0),
                            completed_at,
                        },
                    );
                }
                _ => {}
            }
        }
        Ok((progress, achieved))
    }

    pub async fn user_badges(
        &self,
        address: &str,
        include_not_achieved: bool,
    ) -> Result<(Vec<BadgeData>, Vec<BadgeData>), ApiError> {
        let catalog = self.catalog().await?;
        let (progress, achieved_tiers) = self.load_user_state(address).await?;

        let mut achieved = Vec::new();
        let mut not_achieved = Vec::new();

        for def in &catalog.defs {
            let prog = progress.get(&def.id);
            let badge_tiers = catalog.tiers.get(&def.id);
            let user_tiers = achieved_tiers.get(&def.id);

            let is_achieved = match prog {
                _ if def.is_tier => user_tiers.map(|t| !t.is_empty()).unwrap_or(false),
                Some(p) => p.completed_at.is_some(),
                None => false,
            };

            if !is_achieved && !include_not_achieved {
                continue;
            }

            let badge = self.assemble_badge(def, badge_tiers, prog, user_tiers);
            if is_achieved {
                achieved.push(badge);
            } else {
                not_achieved.push(badge);
            }
        }

        Ok((achieved, not_achieved))
    }

    fn assemble_badge(
        &self,
        def: &DefRow,
        badge_tiers: Option<&Vec<TierRow>>,
        prog: Option<&ProgressRow>,
        user_tiers: Option<&Vec<(String, DateTime<Utc>)>>,
    ) -> BadgeData {
        let steps_done = prog.map(|p| p.steps_done).unwrap_or(0);

        let total_steps_target: i32 = match badge_tiers {
            Some(ts) if def.is_tier => ts.iter().map(|t| t.criteria_steps).max().unwrap_or(0),
            _ => 1,
        };

        let achieved_list: Vec<AchievedTier> = user_tiers
            .map(|ts| {
                ts.iter()
                    .map(|(tier_id, at)| AchievedTier {
                        tier_id: tier_id.clone(),
                        completed_at: Some(at.timestamp_millis()),
                    })
                    .collect()
            })
            .unwrap_or_default();

        let last = user_tiers.and_then(|ts| ts.last());
        let last_tier_def = last.and_then(|(tier_id, _)| {
            badge_tiers.and_then(|defs| defs.iter().find(|t| &t.tier_id == tier_id))
        });
        let last_completed_tier_at = last.map(|(_, at)| at.timestamp_millis());
        let last_completed_tier_name = last_tier_def.map(|t| t.tier_name.clone());
        let last_completed_tier_image = last_tier_def.and_then(|t| tier_image(&t.assets));

        let next_steps_target: Option<i32> = match badge_tiers {
            Some(ts) if def.is_tier => ts
                .iter()
                .map(|t| t.criteria_steps)
                .filter(|&s| s > steps_done)
                .min(),
            _ => {
                if steps_done >= total_steps_target {
                    None
                } else {
                    Some(total_steps_target)
                }
            }
        };

        let completed_at = prog.and_then(|p| p.completed_at.map(epoch_ms));

        BadgeData {
            id: def.id.clone(),
            name: def.name.clone(),
            description: def.description.clone(),
            category: def.category.clone(),
            is_tier: def.is_tier,
            completed_at,
            assets: def.assets.clone(),
            progress: BadgeProgress {
                steps_done,
                next_steps_target,
                total_steps_target,
                last_completed_tier_at,
                last_completed_tier_name,
                last_completed_tier_image,
                achieved_tiers: achieved_list,
            },
        }
    }

    /// One statement: resolves the badge and (for tiered badges) the tier, and
    /// only when that resolves writes the tier, the progress row and the audit
    /// row. `Ok(false)` is "no such badge"; an unresolvable tier keeps its own
    /// error and writes nothing.
    pub async fn grant_badge(
        &self,
        address: &str,
        badge_id: &str,
        tier_id: Option<&str>,
        granted_by: &str,
    ) -> Result<bool, ApiError> {
        let row: Option<(bool, bool)> = sqlx::query_as(
            "WITH def AS ( \
                 SELECT d.is_tier, t.tier_id, t.criteria_steps \
                 FROM badge_definitions d \
                 LEFT JOIN LATERAL ( \
                     SELECT tier_id, criteria_steps FROM badge_tiers \
                     WHERE badge_id = d.id AND ($3::text IS NULL OR tier_id = $3) \
                     ORDER BY ordinal DESC LIMIT 1 \
                 ) t ON TRUE \
                 WHERE d.id = $2 \
             ), ok AS ( \
                 SELECT * FROM def WHERE NOT is_tier OR tier_id IS NOT NULL \
             ), tier AS ( \
                 INSERT INTO user_achieved_tiers \
                   (address, badge_id, tier_id, completed_at, granted_by, granted_at) \
                 SELECT $1, $2, tier_id, now(), $4, now() FROM ok WHERE is_tier \
                 ON CONFLICT (address, badge_id, tier_id) DO UPDATE \
                   SET granted_by = EXCLUDED.granted_by, granted_at = now() \
             ), prog AS ( \
                 INSERT INTO user_badge_progress \
                   (address, badge_id, steps_done, completed_at, last_completed_tier_id, \
                    updated_at, granted_by) \
                 SELECT $1, $2, \
                        CASE WHEN is_tier THEN criteria_steps ELSE 1 END, \
                        now(), \
                        CASE WHEN is_tier THEN tier_id END, \
                        now(), $4 \
                 FROM ok \
                 ON CONFLICT (address, badge_id) DO UPDATE SET \
                   steps_done = GREATEST(user_badge_progress.steps_done, EXCLUDED.steps_done), \
                   completed_at = COALESCE(user_badge_progress.completed_at, EXCLUDED.completed_at), \
                   last_completed_tier_id = COALESCE(EXCLUDED.last_completed_tier_id, \
                                                     user_badge_progress.last_completed_tier_id), \
                   updated_at = now(), \
                   granted_by = EXCLUDED.granted_by \
             ), audit AS ( \
                 INSERT INTO badge_admin_audit (action, address, badge_id, tier_id, actor) \
                 SELECT 'grant', $1, $2, $3, $4 FROM ok \
             ) \
             SELECT is_tier, tier_id IS NOT NULL FROM def",
        )
        .bind(address)
        .bind(badge_id)
        .bind(tier_id)
        .bind(granted_by)
        .fetch_optional(&self.pool)
        .await?;

        match row {
            None => Ok(false),
            Some((true, false)) => Err(match tier_id {
                Some(tid) => ApiError::not_found(format!("no tier '{tid}' on badge '{badge_id}'")),
                None => ApiError::bad_request(format!(
                    "tiered badge '{badge_id}' has no tiers; specify tierId"
                )),
            }),
            Some(_) => Ok(true),
        }
    }

    pub async fn revoke_badge(
        &self,
        address: &str,
        badge_id: &str,
        revoked_by: &str,
    ) -> Result<bool, ApiError> {
        let exists: bool = sqlx::query_scalar(
            "WITH def AS ( \
                 SELECT id FROM badge_definitions WHERE id = $2 \
             ), tiers AS ( \
                 DELETE FROM user_achieved_tiers USING def \
                 WHERE address = $1 AND badge_id = def.id \
             ), prog AS ( \
                 DELETE FROM user_badge_progress USING def \
                 WHERE address = $1 AND badge_id = def.id \
             ), audit AS ( \
                 INSERT INTO badge_admin_audit (action, address, badge_id, actor) \
                 SELECT 'revoke', $1, id, $3 FROM def \
             ) \
             SELECT EXISTS (SELECT 1 FROM def)",
        )
        .bind(address)
        .bind(badge_id)
        .bind(revoked_by)
        .fetch_one(&self.pool)
        .await?;
        Ok(exists)
    }

    pub async fn latest_achieved(
        &self,
        address: &str,
        limit: i64,
    ) -> Result<Vec<LatestAchievedBadge>, ApiError> {
        let catalog = self.catalog().await?;
        let def_by_id: HashMap<&str, &DefRow> =
            catalog.defs.iter().map(|d| (d.id.as_str(), d)).collect();
        let tiers = &catalog.tiers;
        let (progress, achieved_tiers) = self.load_user_state(address).await?;

        let mut rows: Vec<(DateTime<Utc>, LatestAchievedBadge)> = Vec::new();

        for (badge_id, def) in &def_by_id {
            if def.is_tier {
                if let Some(user_tiers) = achieved_tiers.get(*badge_id) {
                    if let Some((tier_id, at)) = user_tiers.last() {
                        let tier_def = tiers
                            .get(*badge_id)
                            .and_then(|defs| defs.iter().find(|t| &t.tier_id == tier_id));
                        rows.push((
                            *at,
                            LatestAchievedBadge {
                                id: def.id.clone(),
                                name: def.name.clone(),
                                tier_name: tier_def.map(|t| t.tier_name.clone()),
                                image: tier_def
                                    .and_then(|t| tier_image(&t.assets))
                                    .or_else(|| tier_image(&def.assets)),
                            },
                        ));
                    }
                }
            } else if let Some(p) = progress.get(*badge_id) {
                if let Some(at) = p.completed_at {
                    rows.push((
                        at,
                        LatestAchievedBadge {
                            id: def.id.clone(),
                            name: def.name.clone(),
                            tier_name: None,
                            image: tier_image(&def.assets),
                        },
                    ));
                }
            }
        }

        rows.sort_by_key(|b| std::cmp::Reverse(b.0));
        Ok(rows
            .into_iter()
            .take(limit.max(0) as usize)
            .map(|(_, b)| b)
            .collect())
    }
}

struct ProgressRow {
    steps_done: i32,
    completed_at: Option<DateTime<Utc>>,
}

fn tier_image(assets: &Assets) -> Option<String> {
    assets
        .get("2d")
        .and_then(|d| d.get("normal"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

#[cfg(test)]
mod rewrite_base_tests {
    use super::rewrite_base;
    use serde_json::json;

    #[test]
    fn swaps_the_host_prefix_on_every_nested_url() {
        let assets = json!({
            "2d": {"normal": "https://badges.decentraland.org/assets/open_for_business/2d/normal.png", "hrm": "", "baseColor": ""},
            "3d": {"normal": "https://badges.decentraland.org/assets/open_for_business/3d/normal.png", "hrm": "", "baseColor": ""}
        });
        let rewritten = rewrite_base(
            &assets,
            "https://badges.decentraland.org",
            "https://badges.interconnected.online",
        );
        assert_eq!(
            rewritten["2d"]["normal"],
            "https://badges.interconnected.online/assets/open_for_business/2d/normal.png"
        );
        assert_eq!(
            rewritten["3d"]["normal"],
            "https://badges.interconnected.online/assets/open_for_business/3d/normal.png"
        );
    }

    #[test]
    fn leaves_empty_strings_untouched() {
        let assets = json!({"2d": {"normal": "", "hrm": "", "baseColor": ""}});
        let rewritten = rewrite_base(
            &assets,
            "https://badges.decentraland.org",
            "https://badges.interconnected.online",
        );
        assert_eq!(rewritten, assets);
    }

    #[test]
    fn is_a_no_op_when_from_and_to_match() {
        let assets = json!({"2d": {"normal": "https://badges.decentraland.org/x.png"}});
        let rewritten = rewrite_base(
            &assets,
            "https://badges.decentraland.org",
            "https://badges.decentraland.org",
        );
        assert_eq!(rewritten, assets);
    }
}
