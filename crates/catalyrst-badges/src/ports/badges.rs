use chrono::{DateTime, Utc};
use sqlx::postgres::PgPool;
use sqlx::Row;
use std::collections::HashMap;

use super::types::{
    AchievedTier, Assets, BadgeData, BadgeProgress, LatestAchievedBadge, TierCriteria, TierData,
};
use crate::http::errors::ApiError;

fn epoch_ms(ts: DateTime<Utc>) -> String {
    ts.timestamp_millis().to_string()
}

pub struct BadgesComponent {
    pool: PgPool,
}

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
    assets: Assets,
    criteria_steps: i32,
}

impl BadgesComponent {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
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
                assets: r.get("assets"),
            })
            .collect())
    }

    async fn load_all_tiers(&self) -> Result<HashMap<String, Vec<TierRow>>, ApiError> {
        let rows = sqlx::query(
            "SELECT badge_id, tier_id, tier_name, assets, criteria_steps \
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
                assets: r.get("assets"),
                criteria_steps: r.get("criteria_steps"),
            });
        }
        Ok(map)
    }

    pub async fn list_tiers(&self, badge_id: &str) -> Result<Vec<TierData>, ApiError> {
        let exists =
            sqlx::query("SELECT 1 FROM badge_definitions WHERE id = $1 AND is_tier = true")
                .bind(badge_id)
                .fetch_optional(&self.pool)
                .await?;
        if exists.is_none() {
            return Err(ApiError::not_found(format!(
                "no tiered badge found with id: {badge_id}"
            )));
        }
        let rows = sqlx::query(
            "SELECT tier_id, tier_name, description, assets, criteria_steps \
             FROM badge_tiers WHERE badge_id = $1 ORDER BY ordinal",
        )
        .bind(badge_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| TierData {
                tier_id: r.get("tier_id"),
                tier_name: r.get("tier_name"),
                description: r.get("description"),
                assets: r.get("assets"),
                criteria: TierCriteria {
                    steps: r.get("criteria_steps"),
                },
            })
            .collect())
    }

    async fn load_progress(
        &self,
        address: &str,
    ) -> Result<HashMap<String, ProgressRow>, ApiError> {
        let rows = sqlx::query(
            "SELECT badge_id, steps_done, completed_at \
             FROM user_badge_progress WHERE address = $1",
        )
        .bind(address)
        .fetch_all(&self.pool)
        .await?;
        let mut map = HashMap::new();
        for r in rows {
            let badge_id: String = r.get("badge_id");
            map.insert(
                badge_id,
                ProgressRow {
                    steps_done: r.get("steps_done"),
                    completed_at: r.get("completed_at"),
                },
            );
        }
        Ok(map)
    }

    async fn load_achieved_tiers(
        &self,
        address: &str,
    ) -> Result<HashMap<String, Vec<(String, DateTime<Utc>)>>, ApiError> {
        let rows = sqlx::query(
            "SELECT badge_id, tier_id, completed_at FROM user_achieved_tiers \
             WHERE address = $1 ORDER BY completed_at",
        )
        .bind(address)
        .fetch_all(&self.pool)
        .await?;
        let mut map: HashMap<String, Vec<(String, DateTime<Utc>)>> = HashMap::new();
        for r in rows {
            let badge_id: String = r.get("badge_id");
            let tier_id: String = r.get("tier_id");
            let completed_at: DateTime<Utc> = r.get("completed_at");
            map.entry(badge_id).or_default().push((tier_id, completed_at));
        }
        Ok(map)
    }

    pub async fn user_badges(
        &self,
        address: &str,
        include_not_achieved: bool,
    ) -> Result<(Vec<BadgeData>, Vec<BadgeData>), ApiError> {
        let defs = self.load_definitions().await?;
        let tiers = self.load_all_tiers().await?;
        let progress = self.load_progress(address).await?;
        let achieved_tiers = self.load_achieved_tiers(address).await?;

        let mut achieved = Vec::new();
        let mut not_achieved = Vec::new();

        for def in &defs {
            let prog = progress.get(&def.id);
            let badge_tiers = tiers.get(&def.id);
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
                        completed_at: Some(epoch_ms(*at)),
                    })
                    .collect()
            })
            .unwrap_or_default();

        let last = user_tiers.and_then(|ts| ts.last());
        let last_tier_def = last.and_then(|(tier_id, _)| {
            badge_tiers.and_then(|defs| defs.iter().find(|t| &t.tier_id == tier_id))
        });
        let last_completed_tier_at = last.map(|(_, at)| epoch_ms(*at));
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

    async fn badge_is_tier(&self, badge_id: &str) -> Result<Option<bool>, ApiError> {
        let row = sqlx::query("SELECT is_tier FROM badge_definitions WHERE id = $1")
            .bind(badge_id)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.map(|r| r.get::<bool, _>("is_tier")))
    }

    /// Resolve the tier to grant: supplied `tier_id` (must exist) or the
    /// highest-ordinal tier. Returns `(tier_id, criteria_steps)`.
    async fn resolve_tier(
        &self,
        badge_id: &str,
        tier_id: Option<&str>,
    ) -> Result<(String, i32), ApiError> {
        match tier_id {
            Some(tid) => {
                let row = sqlx::query(
                    "SELECT tier_id, criteria_steps FROM badge_tiers \
                     WHERE badge_id = $1 AND tier_id = $2",
                )
                .bind(badge_id)
                .bind(tid)
                .fetch_optional(&self.pool)
                .await?;
                let row = row.ok_or_else(|| {
                    ApiError::not_found(format!("no tier '{tid}' on badge '{badge_id}'"))
                })?;
                Ok((row.get("tier_id"), row.get("criteria_steps")))
            }
            None => {
                let row = sqlx::query(
                    "SELECT tier_id, criteria_steps FROM badge_tiers \
                     WHERE badge_id = $1 ORDER BY ordinal DESC LIMIT 1",
                )
                .bind(badge_id)
                .fetch_optional(&self.pool)
                .await?;
                let row = row.ok_or_else(|| {
                    ApiError::bad_request(format!(
                        "tiered badge '{badge_id}' has no tiers; specify tierId"
                    ))
                })?;
                Ok((row.get("tier_id"), row.get("criteria_steps")))
            }
        }
    }

    /// Grant a badge. Non-tier badges are marked complete; tier badges record an
    /// achieved tier. Idempotent. Returns `false` if the badge id is unknown.
    pub async fn grant_badge(
        &self,
        address: &str,
        badge_id: &str,
        tier_id: Option<&str>,
        granted_by: &str,
    ) -> Result<bool, ApiError> {
        let is_tier = match self.badge_is_tier(badge_id).await? {
            Some(v) => v,
            None => return Ok(false),
        };

        let mut tx = self.pool.begin().await?;

        if is_tier {
            let (resolved_tier, steps) = self.resolve_tier(badge_id, tier_id).await?;
            sqlx::query(
                "INSERT INTO user_achieved_tiers \
                   (address, badge_id, tier_id, completed_at, granted_by, granted_at) \
                 VALUES ($1, $2, $3, now(), $4, now()) \
                 ON CONFLICT (address, badge_id, tier_id) DO UPDATE \
                   SET granted_by = EXCLUDED.granted_by, granted_at = now()",
            )
            .bind(address)
            .bind(badge_id)
            .bind(&resolved_tier)
            .bind(granted_by)
            .execute(&mut *tx)
            .await?;

            sqlx::query(
                "INSERT INTO user_badge_progress \
                   (address, badge_id, steps_done, completed_at, last_completed_tier_id, \
                    updated_at, granted_by) \
                 VALUES ($1, $2, $3, now(), $4, now(), $5) \
                 ON CONFLICT (address, badge_id) DO UPDATE SET \
                   steps_done = GREATEST(user_badge_progress.steps_done, EXCLUDED.steps_done), \
                   completed_at = COALESCE(user_badge_progress.completed_at, EXCLUDED.completed_at), \
                   last_completed_tier_id = EXCLUDED.last_completed_tier_id, \
                   updated_at = now(), \
                   granted_by = EXCLUDED.granted_by",
            )
            .bind(address)
            .bind(badge_id)
            .bind(steps)
            .bind(&resolved_tier)
            .bind(granted_by)
            .execute(&mut *tx)
            .await?;
        } else {
            sqlx::query(
                "INSERT INTO user_badge_progress \
                   (address, badge_id, steps_done, completed_at, updated_at, granted_by) \
                 VALUES ($1, $2, 1, now(), now(), $3) \
                 ON CONFLICT (address, badge_id) DO UPDATE SET \
                   steps_done = GREATEST(user_badge_progress.steps_done, 1), \
                   completed_at = COALESCE(user_badge_progress.completed_at, now()), \
                   updated_at = now(), \
                   granted_by = EXCLUDED.granted_by",
            )
            .bind(address)
            .bind(badge_id)
            .bind(granted_by)
            .execute(&mut *tx)
            .await?;
        }

        sqlx::query(
            "INSERT INTO badge_admin_audit (action, address, badge_id, tier_id, actor) \
             VALUES ('grant', $1, $2, $3, $4)",
        )
        .bind(address)
        .bind(badge_id)
        .bind(tier_id)
        .bind(granted_by)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(true)
    }

    /// Revoke a badge: delete the user's progress + achieved-tier rows. The
    /// deleted rows can't carry provenance, so the append-only `badge_admin_audit`
    /// log is the durable record. Idempotent. Returns `false` if the id is unknown.
    pub async fn revoke_badge(
        &self,
        address: &str,
        badge_id: &str,
        revoked_by: &str,
    ) -> Result<bool, ApiError> {
        if self.badge_is_tier(badge_id).await?.is_none() {
            return Ok(false);
        }
        let mut tx = self.pool.begin().await?;
        sqlx::query("DELETE FROM user_achieved_tiers WHERE address = $1 AND badge_id = $2")
            .bind(address)
            .bind(badge_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM user_badge_progress WHERE address = $1 AND badge_id = $2")
            .bind(address)
            .bind(badge_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query(
            "INSERT INTO badge_admin_audit (action, address, badge_id, actor) \
             VALUES ('revoke', $1, $2, $3)",
        )
        .bind(address)
        .bind(badge_id)
        .bind(revoked_by)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(true)
    }

    pub async fn latest_achieved(
        &self,
        address: &str,
        limit: i64,
    ) -> Result<Vec<LatestAchievedBadge>, ApiError> {
        let defs = self.load_definitions().await?;
        let def_by_id: HashMap<&str, &DefRow> =
            defs.iter().map(|d| (d.id.as_str(), d)).collect();
        let tiers = self.load_all_tiers().await?;
        let progress = self.load_progress(address).await?;
        let achieved_tiers = self.load_achieved_tiers(address).await?;

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

        rows.sort_by(|a, b| b.0.cmp(&a.0));
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
