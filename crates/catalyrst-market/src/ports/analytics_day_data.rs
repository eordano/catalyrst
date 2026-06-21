use chrono::{Duration, Utc};
use serde::Serialize;
use sqlx::PgPool;

use crate::http::response::ApiError;
use crate::MARKETPLACE_SQUID_SCHEMA;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnalyticsTimeframe {
    Day,
    Week,
    Month,
    All,
}

impl AnalyticsTimeframe {
    pub fn parse_str(s: &str) -> Self {
        match s {
            "day" => AnalyticsTimeframe::Day,
            "week" => AnalyticsTimeframe::Week,
            "month" => AnalyticsTimeframe::Month,
            "all" => AnalyticsTimeframe::All,
            _ => AnalyticsTimeframe::All,
        }
    }
}

pub fn get_timestamp_from_timeframe(tf: AnalyticsTimeframe) -> i64 {
    let days = match tf {
        AnalyticsTimeframe::Day => 1,
        AnalyticsTimeframe::Week => 7,
        AnalyticsTimeframe::Month => 30,
        AnalyticsTimeframe::All => return 0,
    };
    let d = (Utc::now() - Duration::days(days))
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .unwrap()
        .and_utc();
    d.timestamp_millis()
}

#[derive(Debug, Clone, Serialize)]
pub struct AnalyticsDayData {
    pub id: String,
    pub date: i64,
    pub sales: i64,
    pub volume: String,
    #[serde(rename = "creatorsEarnings")]
    pub creators_earnings: String,
    #[serde(rename = "daoEarnings")]
    pub dao_earnings: String,
}

pub struct AnalyticsDayDataComponent {
    pool: PgPool,
}

impl AnalyticsDayDataComponent {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn fetch(&self, from_ms: i64) -> Result<Vec<AnalyticsDayData>, ApiError> {
        let rows: Vec<AnalyticsDayData> = if from_ms == 0 {
            let sql = format!(
                r#"
SELECT
  id,
  0::int4                            AS date,
  COALESCE(SUM(sales), 0)::int8      AS sales,
  COALESCE(SUM(volume), 0)::text     AS volume,
  COALESCE(SUM(creators_earnings), 0)::text AS creators_earnings,
  COALESCE(SUM(dao_earnings), 0)::text      AS dao_earnings
FROM {schema}.analytics_day_data
GROUP BY id
"#,
                schema = MARKETPLACE_SQUID_SCHEMA,
            );
            sqlx::query_as::<_, (String, i32, i64, String, String, String)>(sqlx::AssertSqlSafe(
                sql,
            ))
            .fetch_all(&self.pool)
            .await?
            .into_iter()
            .map(|(id, date, sales, volume, ce, de)| AnalyticsDayData {
                id,
                date: date as i64,
                sales,
                volume,
                creators_earnings: ce,
                dao_earnings: de,
            })
            .collect()
        } else {
            let from_s = from_ms / 1000;
            let sql = format!(
                r#"
SELECT
  id,
  date::int4                        AS date,
  sales::int8                       AS sales,
  volume::text                      AS volume,
  creators_earnings::text           AS creators_earnings,
  dao_earnings::text                AS dao_earnings
FROM {schema}.analytics_day_data
WHERE date > $1
"#,
                schema = MARKETPLACE_SQUID_SCHEMA,
            );
            sqlx::query_as::<_, (String, i32, i64, String, String, String)>(sqlx::AssertSqlSafe(
                sql,
            ))
            .bind(from_s)
            .fetch_all(&self.pool)
            .await?
            .into_iter()
            .map(|(id, date, sales, volume, ce, de)| AnalyticsDayData {
                id,
                date: date as i64,
                sales,
                volume,
                creators_earnings: ce,
                dao_earnings: de,
            })
            .collect()
        };
        Ok(rows)
    }
}
