use chrono::{DateTime, Utc};
use sqlx::postgres::PgPool;
use sqlx::Row;
use std::time::Duration;

use super::credits::CreditsComponent;
use crate::http::ApiError;

#[derive(Debug, Clone)]
pub enum GoalEvent {
    Login,
    SceneVisit { scene: String },
    Purchase { checkout_id: i64 },
}

impl GoalEvent {
    pub fn kind(&self) -> &'static str {
        match self {
            GoalEvent::Login => "login",
            GoalEvent::SceneVisit { .. } => "scene_visit",
            GoalEvent::Purchase { .. } => "purchase",
        }
    }

    pub fn dedup_key(&self, now: DateTime<Utc>) -> String {
        match self {
            GoalEvent::Login => now.format("%Y-%m-%d").to_string(),
            GoalEvent::SceneVisit { scene } => scene.clone(),
            GoalEvent::Purchase { checkout_id } => checkout_id.to_string(),
        }
    }
}

impl CreditsComponent {
    pub async fn record_event(
        &self,
        address: &str,
        event: &GoalEvent,
        now: DateTime<Utc>,
    ) -> Result<u32, ApiError> {
        let kind = event.kind();
        let key = event.dedup_key(now);
        let mut tx = self.pool.begin().await?;

        let advanced = sqlx::query(
            "WITH current_goals AS ( \
                 SELECT g.id FROM credits_goals g \
                 JOIN credits_weeks w ON w.id = g.week_id \
                 WHERE g.kind = $2 AND w.start_date <= $4 AND w.end_date >= $4 \
             ) \
             INSERT INTO user_goal_events (address, goal_id, dedup_key) \
             SELECT $1, id, $3 FROM current_goals \
             ON CONFLICT DO NOTHING \
             RETURNING goal_id",
        )
        .bind(address)
        .bind(kind)
        .bind(&key)
        .bind(now)
        .fetch_all(&mut *tx)
        .await?;

        if advanced.is_empty() {
            tx.commit().await?;
            return Ok(0);
        }
        let goal_ids: Vec<i32> = advanced.iter().map(|r| r.get("goal_id")).collect();

        sqlx::query(
            "INSERT INTO user_goal_progress (address, goal_id, completed_steps) \
             SELECT $1, g.id, LEAST(g.total_steps, ( \
                        SELECT count(*) FROM user_goal_events e \
                        WHERE e.address = $1 AND e.goal_id = g.id))::int \
             FROM credits_goals g WHERE g.id = ANY($2) \
             ON CONFLICT (address, goal_id) DO UPDATE \
             SET completed_steps = GREATEST(user_goal_progress.completed_steps, \
                                            EXCLUDED.completed_steps)",
        )
        .bind(address)
        .bind(&goal_ids)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(goal_ids.len() as u32)
    }

    pub async fn scan_presence(
        &self,
        presence: &PgPool,
        now: DateTime<Utc>,
    ) -> Result<u32, ApiError> {
        let rows = sqlx::query(
            "SELECT s.taken_at, p.address, p.parcel_x, p.parcel_y \
             FROM peer_snapshots p \
             JOIN snapshots s ON s.id = p.snapshot_id \
             WHERE s.taken_at > $1 - interval '2 hours'",
        )
        .bind(now)
        .fetch_all(presence)
        .await?;

        let mut advanced = 0u32;
        for r in rows {
            let taken_at: DateTime<Utc> = r.get("taken_at");
            let address: String = r.get::<String, _>("address").to_lowercase();
            let (x, y): (i32, i32) = (r.get("parcel_x"), r.get("parcel_y"));
            advanced += self
                .record_event(&address, &GoalEvent::Login, taken_at)
                .await?;
            advanced += self
                .record_event(
                    &address,
                    &GoalEvent::SceneVisit {
                        scene: format!("{},{}", x, y),
                    },
                    taken_at,
                )
                .await?;
        }
        Ok(advanced)
    }

    pub async fn sweep_expired_earned(&self) -> Result<u32, ApiError> {
        let due: Vec<String> = sqlx::query(
            "SELECT address FROM user_credits \
             WHERE earned_available > 0 AND earned_expires_at IS NOT NULL \
               AND earned_expires_at < now() LIMIT 200",
        )
        .fetch_all(&self.pool)
        .await?
        .into_iter()
        .map(|r| r.get("address"))
        .collect();

        let mut swept = 0u32;
        for address in due {
            let mut tx = self.pool.begin().await?;
            sqlx::query("SELECT 1 FROM user_credits WHERE address = $1 FOR UPDATE")
                .bind(&address)
                .fetch_optional(&mut *tx)
                .await?;
            let expired = self.expire_earned_in_tx(&mut tx, &address).await?;
            tx.commit().await?;
            if expired.parse::<f64>().unwrap_or(0.0) > 0.0 {
                swept += 1;
            }
        }
        Ok(swept)
    }

    pub async fn scan_fulfilled_checkouts(&self, now: DateTime<Utc>) -> Result<u32, ApiError> {
        let rows = sqlx::query(
            "SELECT id, address FROM checkouts \
             WHERE status = 'fulfilled' AND updated_at >= $1 - interval '9 days'",
        )
        .bind(now)
        .fetch_all(&self.pool)
        .await?;

        let mut advanced = 0u32;
        for r in rows {
            let checkout_id: i64 = r.get("id");
            let address: String = r.get("address");
            advanced += self
                .record_event(&address, &GoalEvent::Purchase { checkout_id }, now)
                .await?;
        }
        Ok(advanced)
    }
}

pub fn spawn_progress_worker(
    credits: CreditsComponent,
    presence: Option<PgPool>,
    interval_secs: u64,
) {
    if presence.is_none() {
        tracing::warn!(
            "PROGRESS_PRESENCE_PG_CONNECTION_STRING unset: explorer login/scene-visit \
             goal tracking is OFF (purchase tracking still on)"
        );
    }
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(Duration::from_secs(interval_secs.max(1)));
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            ticker.tick().await;
            let now = Utc::now();
            match credits.sweep_expired_earned().await {
                Ok(0) => {}
                Ok(n) => tracing::info!(wallets = n, "expired end-of-season earned credits"),
                Err(e) => tracing::warn!(error = %e, "earned-credit expiry sweep failed"),
            }
            match credits.scan_fulfilled_checkouts(now).await {
                Ok(0) => {}
                Ok(n) => {
                    tracing::info!(advanced = n, "goal progress: purchase scan advanced goals")
                }
                Err(e) => tracing::warn!(error = %e, "goal progress: purchase scan failed"),
            }
            if let Some(presence) = presence.as_ref() {
                match credits.scan_presence(presence, now).await {
                    Ok(0) => {}
                    Ok(n) => {
                        tracing::info!(advanced = n, "goal progress: presence scan advanced goals")
                    }
                    Err(e) => tracing::warn!(error = %e, "goal progress: presence scan failed"),
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dedup_keys_are_stable_per_semantics() {
        let now = DateTime::parse_from_rfc3339("2026-07-02T15:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        assert_eq!(GoalEvent::Login.dedup_key(now), "2026-07-02");
        assert_eq!(
            GoalEvent::SceneVisit {
                scene: "bafkreiabc".into()
            }
            .dedup_key(now),
            "bafkreiabc"
        );
        assert_eq!(GoalEvent::Purchase { checkout_id: 41 }.dedup_key(now), "41");
    }

    #[test]
    fn kinds_match_migration_check() {
        assert_eq!(GoalEvent::Login.kind(), "login");
        assert_eq!(
            GoalEvent::SceneVisit {
                scene: String::new()
            }
            .kind(),
            "scene_visit"
        );
        assert_eq!(GoalEvent::Purchase { checkout_id: 0 }.kind(), "purchase");
    }
}
