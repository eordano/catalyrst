use serde::Serialize;

pub type Assets = serde_json::Value;

#[derive(Debug, Clone, Serialize)]
pub struct LatestAchievedBadge {
    pub id: String,
    pub name: String,
    #[serde(rename = "tierName")]
    pub tier_name: Option<String>,
    pub image: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AchievedTier {
    #[serde(rename = "tierId")]
    pub tier_id: String,
    #[serde(rename = "completedAt")]
    pub completed_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BadgeProgress {
    #[serde(rename = "stepsDone")]
    pub steps_done: i32,
    #[serde(rename = "nextStepsTarget")]
    pub next_steps_target: Option<i32>,
    #[serde(rename = "totalStepsTarget")]
    pub total_steps_target: i32,
    #[serde(rename = "lastCompletedTierAt")]
    pub last_completed_tier_at: Option<String>,
    #[serde(rename = "lastCompletedTierName")]
    pub last_completed_tier_name: Option<String>,
    #[serde(rename = "lastCompletedTierImage")]
    pub last_completed_tier_image: Option<String>,
    #[serde(rename = "achievedTiers")]
    pub achieved_tiers: Vec<AchievedTier>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BadgeData {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub category: Option<String>,
    #[serde(rename = "isTier")]
    pub is_tier: bool,
    #[serde(rename = "completedAt")]
    pub completed_at: Option<String>,
    pub assets: Assets,
    pub progress: BadgeProgress,
}

#[derive(Debug, Clone, Serialize)]
pub struct TierData {
    #[serde(rename = "tierId")]
    pub tier_id: String,
    #[serde(rename = "tierName")]
    pub tier_name: String,
    pub description: Option<String>,
    pub assets: Assets,
    pub criteria: TierCriteria,
}

#[derive(Debug, Clone, Serialize)]
pub struct TierCriteria {
    pub steps: i32,
}
