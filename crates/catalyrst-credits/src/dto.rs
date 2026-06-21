use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
pub struct SeasonsData {
    #[serde(rename = "lastSeason")]
    pub last_season: SeasonData,
    #[serde(rename = "currentSeason")]
    pub current_season: CurrentSeasonInfo,
    #[serde(rename = "nextSeason")]
    pub next_season: SeasonData,
}

#[derive(Debug, Serialize)]
pub struct CurrentSeasonInfo {
    pub season: SeasonData,
    pub week: Week,
}

#[derive(Debug, Default, Serialize)]
pub struct SeasonData {
    pub id: i32,
    pub name: String,
    #[serde(rename = "startDate")]
    pub start_date: String,
    #[serde(rename = "endDate")]
    pub end_date: String,
    #[serde(rename = "maxMana")]
    pub max_mana: String,
    #[serde(rename = "timeLeft")]
    pub time_left: i64,
    #[serde(rename = "amountOfWeeks")]
    pub amount_of_weeks: i32,
    pub state: String,
}

#[derive(Debug, Default, Serialize)]
pub struct Week {
    #[serde(rename = "weekNumber")]
    pub week_number: i32,
    #[serde(rename = "timeLeft")]
    pub time_left: u64,
    #[serde(rename = "startDate")]
    pub start_date: String,
    #[serde(rename = "endDate")]
    pub end_date: String,
    #[serde(rename = "secondsRemaining")]
    pub seconds_remaining: u64,
}

#[derive(Debug, Serialize)]
pub struct CreditsProgramProgressResponse {
    pub user: UserData,
    pub credits: CreditsData,
    pub goals: Vec<GoalData>,
}

#[derive(Debug, Serialize)]
pub struct UserData {
    #[serde(rename = "hasStartedProgram")]
    pub has_started_program: bool,
}

#[derive(Debug, Serialize)]
pub struct CreditsData {
    pub available: f64,
    #[serde(rename = "expiresIn")]
    pub expires_in: u64,
    #[serde(rename = "isBlockedForClaiming")]
    pub is_blocked_for_claiming: bool,
}

#[derive(Debug, Serialize)]
pub struct GoalData {
    pub title: String,
    pub description: String,
    pub thumbnail: String,
    pub progress: GoalProgressData,
    pub reward: f64,
    #[serde(rename = "isClaimed")]
    pub is_claimed: bool,
}

#[derive(Debug, Serialize)]
pub struct GoalProgressData {
    #[serde(rename = "totalSteps")]
    pub total_steps: u64,
    #[serde(rename = "completedSteps")]
    pub completed_steps: u64,
}

#[derive(Debug, Deserialize)]
pub struct ClaimCreditsBody {
    pub x: f64,
}

// Wire shape is dictated by Unity's `ClaimCreditsResponse` struct
// (MarketplaceCreditsAPIService/ClaimCreditsResponse.cs), deserialized by
// JsonUtility, which is case-sensitive. Fields must match its mixed casing
// exactly: `ok`, `credits_granted` (snake_case), `isBlockedForClaiming`.
// A blanket `rename_all = camelCase` would mis-emit `creditsGranted`, leaving
// the client's `credits_granted` unbound (always 0).
#[derive(Debug, Serialize)]
pub struct ClaimCreditsResponse {
    pub ok: bool,
    pub credits_granted: f64,
    #[serde(rename = "isBlockedForClaiming")]
    pub is_blocked_for_claiming: bool,
}
