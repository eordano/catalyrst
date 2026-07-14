use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "map/"))]
pub struct NftAttribute {
    pub trait_type: String,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub value: i64,
    pub display_type: String,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "map/"))]
pub struct NftMetadata {
    pub id: String,
    pub name: String,
    pub description: String,
    pub image: String,
    pub external_url: Option<String>,
    pub background_color: String,
    pub attributes: Vec<NftAttribute>,
}
