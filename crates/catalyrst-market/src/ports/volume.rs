use serde::Serialize;

use crate::http::response::ApiError;
use crate::logic::numeric::bn_add;
use crate::ports::analytics_day_data::{
    get_timestamp_from_timeframe, AnalyticsDayDataComponent, AnalyticsTimeframe,
};

#[derive(Debug, Serialize)]
pub struct VolumeData {
    pub sales: i64,
    pub volume: String,
    #[serde(rename = "creatorsEarnings")]
    pub creators_earnings: String,
    #[serde(rename = "daoEarnings")]
    pub dao_earnings: String,
}

pub struct VolumeComponent {
    analytics: AnalyticsDayDataComponent,
}

impl VolumeComponent {
    pub fn new(analytics: AnalyticsDayDataComponent) -> Self {
        Self { analytics }
    }

    pub async fn fetch(&self, timeframe: AnalyticsTimeframe) -> Result<VolumeData, ApiError> {
        let from = get_timestamp_from_timeframe(timeframe);
        let days = self.analytics.fetch(from).await?;
        Ok(accumulate(&days))
    }
}

fn accumulate(days: &[crate::ports::analytics_day_data::AnalyticsDayData]) -> VolumeData {
    let mut sales: i64 = 0;
    let mut volume = "0".to_string();
    let mut ce = "0".to_string();
    let mut de = "0".to_string();
    for d in days {
        sales += d.sales;
        volume = bn_add(&volume, &d.volume);
        ce = bn_add(&ce, &d.creators_earnings);
        de = bn_add(&de, &d.dao_earnings);
    }
    VolumeData {
        sales,
        volume,
        creators_earnings: ce,
        dao_earnings: de,
    }
}
