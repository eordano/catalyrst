use sqlx::types::Json as SqlxJson;

use super::types::{
    GroupedEmote, GroupedWearable, IndividualData, ProfileEmote, ProfileName, ProfileWearable,
    UserAssetsFilters,
};

#[derive(Debug, sqlx::FromRow)]
pub(super) struct ProfileRow {
    id: String,
    contract_address: Option<String>,
    token_id: String,
    #[allow(dead_code)]
    network: Option<String>,
    #[allow(dead_code)]
    created_at: Option<i64>,
    #[allow(dead_code)]
    updated_at: Option<i64>,
    urn: Option<String>,
    #[allow(dead_code)]
    owner: Option<String>,
    #[allow(dead_code)]
    image: Option<String>,
    #[allow(dead_code)]
    item_id: Option<String>,
    category: Option<String>,
    rarity: Option<String>,
    name: Option<String>,
    #[allow(dead_code)]
    item_type: Option<String>,
    #[allow(dead_code)]
    description: Option<String>,
    transferred_at: Option<i64>,
    price: Option<String>,
    #[sqlx(default)]
    is_leased: bool,
}

#[derive(Debug, sqlx::FromRow)]
pub(super) struct GroupedWearableRow {
    urn: String,
    category: Option<String>,
    rarity: Option<String>,
    name: Option<String>,
    item_type: Option<String>,
    amount: i64,
    min_transferred_at: Option<i64>,
    max_transferred_at: Option<i64>,
    #[allow(dead_code)]
    min_created_at: Option<i64>,
    individual_data: SqlxJson<Vec<IndividualData>>,
    #[allow(dead_code)]
    rarity_order: i32,
    is_leased: bool,
}

#[derive(Debug, sqlx::FromRow)]
pub(super) struct GroupedEmoteRow {
    urn: String,
    category: Option<String>,
    rarity: Option<String>,
    name: Option<String>,
    amount: i64,
    min_transferred_at: Option<i64>,
    max_transferred_at: Option<i64>,
    #[allow(dead_code)]
    min_created_at: Option<i64>,
    individual_data: SqlxJson<Vec<IndividualData>>,
    #[allow(dead_code)]
    rarity_order: i32,
    is_leased: bool,
}

pub fn fix_urn(urn: &str) -> String {
    urn.replace("mainnet", "ethereum")
}

fn fix_individual_data(individual_data: Vec<IndividualData>) -> Vec<IndividualData> {
    individual_data
        .into_iter()
        .map(|mut row| {
            if row.id.starts_with("urn:decentraland:") {
                row.id = fix_urn(&row.id);
            }
            row
        })
        .collect()
}

pub(super) fn from_db_row_to_wearable(row: ProfileRow) -> (ProfileWearable, bool) {
    let is_leased = row.is_leased;
    let transferred_at = Some(match row.transferred_at {
        Some(t) => t.to_string(),
        None => "null".to_string(),
    });
    (
        ProfileWearable {
            urn: fix_urn(&row.urn.clone().unwrap_or_default()),
            id: row.id,
            token_id: row.token_id,
            category: row.category.unwrap_or_else(|| "eyewear".to_string()),
            transferred_at,
            name: row.name.unwrap_or_default(),
            rarity: row.rarity.unwrap_or_else(|| "common".to_string()),
            price: row.price,
            status: None,
            unlock_at: None,
        },
        is_leased,
    )
}

pub(super) fn from_db_row_to_emote(row: ProfileRow) -> (ProfileEmote, bool) {
    let is_leased = row.is_leased;
    let transferred_at = Some(match row.transferred_at {
        Some(t) => t.to_string(),
        None => "null".to_string(),
    });
    (
        ProfileEmote {
            urn: fix_urn(&row.urn.clone().unwrap_or_default()),
            id: row.id,
            token_id: row.token_id,
            category: row.category.unwrap_or_else(|| "dance".to_string()),
            transferred_at,
            name: row.name.unwrap_or_default(),
            rarity: row.rarity.unwrap_or_else(|| "common".to_string()),
            price: row.price,
            status: None,
            unlock_at: None,
        },
        is_leased,
    )
}

pub(super) fn from_db_row_to_name(row: ProfileRow) -> ProfileName {
    ProfileName {
        name: row.name.unwrap_or_default(),
        contract_address: row.contract_address.unwrap_or_default(),
        token_id: row.token_id,
        price: row.price,
    }
}

pub(super) fn from_grouped_row_to_wearable(row: GroupedWearableRow) -> (GroupedWearable, bool) {
    let is_leased = row.is_leased;
    (
        GroupedWearable {
            urn: fix_urn(&row.urn),
            amount: row.amount.to_string(),
            individual_data: fix_individual_data(row.individual_data.0),
            name: row.name.unwrap_or_default(),
            rarity: row.rarity.unwrap_or_else(|| "common".to_string()),
            min_transferred_at: row.min_transferred_at.unwrap_or(0).to_string(),
            max_transferred_at: row.max_transferred_at.unwrap_or(0).to_string(),
            category: row.category.unwrap_or_else(|| "eyewear".to_string()),
            item_type: row.item_type.unwrap_or_default(),
            status: None,
            unlock_at: None,
        },
        is_leased,
    )
}

pub(super) fn from_grouped_row_to_emote(row: GroupedEmoteRow) -> (GroupedEmote, bool) {
    let is_leased = row.is_leased;
    (
        GroupedEmote {
            urn: fix_urn(&row.urn),
            amount: row.amount.to_string(),
            individual_data: fix_individual_data(row.individual_data.0),
            name: row.name.unwrap_or_default(),
            rarity: row.rarity.unwrap_or_else(|| "common".to_string()),
            min_transferred_at: row.min_transferred_at.unwrap_or(0).to_string(),
            max_transferred_at: row.max_transferred_at.unwrap_or(0).to_string(),
            category: row.category.unwrap_or_else(|| "dance".to_string()),
            status: None,
            unlock_at: None,
        },
        is_leased,
    )
}

pub(super) fn build_order_by_clause(filters: &UserAssetsFilters) -> (&'static str, ()) {
    let sort_field = filters
        .order_by
        .as_deref()
        .unwrap_or("rarity")
        .to_lowercase();
    let default_dir = if sort_field == "name" { "ASC" } else { "DESC" };
    let dir = filters
        .direction
        .as_deref()
        .unwrap_or(default_dir)
        .to_uppercase();

    let clause = match sort_field.as_str() {
        "rarity" if dir == "ASC" => " ORDER BY rarity_order ASC, urn ASC",
        "name" => {
            if dir == "ASC" {
                " ORDER BY name ASC, urn ASC"
            } else {
                " ORDER BY name DESC, urn ASC"
            }
        }
        "date" => {
            if dir == "ASC" {
                " ORDER BY min_transferred_at ASC, urn DESC"
            } else {
                " ORDER BY max_transferred_at DESC, urn ASC"
            }
        }
        _ => " ORDER BY rarity_order DESC, urn ASC",
    };
    (clause, ())
}
