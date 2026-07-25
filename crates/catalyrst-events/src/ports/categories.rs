use chrono::{DateTime, TimeZone, Utc};
use serde_json::{Map, Value};
use sqlx::PgPool;

use crate::http::response::ApiError;
use crate::schemas::EventCategoryRecord;

pub struct CategoriesComponent {
    _pool: PgPool,
}

const CATEGORY_I18N: &[(&str, &str)] = &[
    ("art", "Art & Culture"),
    ("causes", "Causes"),
    ("competition", "Competition"),
    ("education", "Education"),
    ("gambling", "Gambling"),
    ("gaming", "Gaming"),
    ("giveaway", "Giveaway"),
    ("health", "Health & Wellbeing"),
    ("hobbies", "Hobbies & Passions"),
    ("identity", "Identity & Language"),
    ("live", "Live Performances"),
    ("music", "Music"),
    ("networking", "Networking"),
    ("nft", "NFT"),
    ("other", "Other"),
    ("party", "Party"),
    ("play", "Play to Earn"),
    ("poap", "POAP"),
    ("religion", "Religion & Spirituality"),
    ("shopping", "Shopping"),
    ("social", "Social Activities"),
    ("sports", "Sports"),
    ("talks", "Talks & Presentations"),
    ("town", "Town Hall"),
    ("tv", "TV & Movies"),
];

impl CategoriesComponent {
    pub fn new(pool: PgPool) -> Self {
        Self { _pool: pool }
    }

    pub async fn list(&self) -> Result<Vec<EventCategoryRecord>, ApiError> {
        let epoch: DateTime<Utc> = Utc.timestamp_opt(0, 0).single().unwrap_or_else(Utc::now);
        Ok(CATEGORY_I18N
            .iter()
            .map(|(name, en)| {
                let mut i18n = Map::new();
                i18n.insert("en".to_string(), Value::String((*en).to_string()));
                EventCategoryRecord {
                    name: (*name).to_string(),
                    active: true,
                    created_at: epoch,
                    updated_at: epoch,
                    i18n,
                }
            })
            .collect())
    }
}
