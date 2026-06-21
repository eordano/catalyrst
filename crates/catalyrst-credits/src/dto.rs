use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "credits/"))]
pub struct SeasonsData {
    #[serde(rename = "lastSeason")]
    pub last_season: SeasonData,
    #[serde(rename = "currentSeason")]
    pub current_season: CurrentSeasonInfo,
    #[serde(rename = "nextSeason")]
    pub next_season: SeasonData,
}

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "credits/"))]
pub struct CurrentSeasonInfo {
    pub season: SeasonData,
    pub week: Week,
}

#[derive(Debug, Default, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "credits/"))]
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
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub time_left: i64,
    #[serde(rename = "amountOfWeeks")]
    pub amount_of_weeks: i32,
    pub state: String,
}

#[derive(Debug, Default, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "credits/"))]
pub struct Week {
    #[serde(rename = "weekNumber")]
    pub week_number: i32,
    #[serde(rename = "timeLeft")]
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub time_left: u64,
    #[serde(rename = "startDate")]
    pub start_date: String,
    #[serde(rename = "endDate")]
    pub end_date: String,
    #[serde(rename = "secondsRemaining")]
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub seconds_remaining: u64,
}

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "credits/"))]
pub struct CreditsProgramProgressResponse {
    pub user: UserData,
    pub credits: CreditsData,
    pub goals: Vec<GoalData>,
}

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "credits/"))]
pub struct UserData {
    #[serde(rename = "hasStartedProgram")]
    pub has_started_program: bool,
}

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "credits/"))]
pub struct CreditsData {
    pub available: f64,
    pub earned: f64,
    pub paid: f64,
    #[serde(rename = "expiresIn")]
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub expires_in: u64,
    #[serde(rename = "isBlockedForClaiming")]
    pub is_blocked_for_claiming: bool,
}

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "credits/"))]
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
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "credits/"))]
pub struct GoalProgressData {
    #[serde(rename = "totalSteps")]
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub total_steps: u64,
    #[serde(rename = "completedSteps")]
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub completed_steps: u64,
}

#[derive(Debug, Deserialize)]
pub struct ClaimCreditsBody {
    pub x: f64,

    #[serde(default)]
    pub token: Option<String>,
}

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "credits/"))]
pub struct ClaimCreditsResponse {
    pub ok: bool,
    pub credits_granted: f64,
    #[serde(rename = "isBlockedForClaiming")]
    pub is_blocked_for_claiming: bool,
}
