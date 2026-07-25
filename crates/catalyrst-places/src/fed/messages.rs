use catalyrst_fed::TypedMessage;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaceFavorite {
    pub place_id: String,
    pub action: PlaceFavoriteAction,
    pub signed_at: i64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PlaceFavoriteAction {
    Add,
    Remove,
}

impl TypedMessage for PlaceFavorite {
    const PRIMARY_TYPE: &'static str = "PlaceFavorite";
    fn encode_struct(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.place_id.len() + 1 + 8);
        out.extend_from_slice(self.place_id.as_bytes());
        out.push(match self.action {
            PlaceFavoriteAction::Add => 1,
            PlaceFavoriteAction::Remove => 0,
        });
        out.extend_from_slice(&self.signed_at.to_be_bytes());
        out
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaceVote {
    pub place_id: String,
    pub score: i8,
    pub signed_at: i64,
}

impl TypedMessage for PlaceVote {
    const PRIMARY_TYPE: &'static str = "PlaceVote";
    fn encode_struct(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.place_id.len() + 1 + 8);
        out.extend_from_slice(self.place_id.as_bytes());
        out.push(self.score as u8);
        out.extend_from_slice(&self.signed_at.to_be_bytes());
        out
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaceReport {
    pub place_id: String,
    pub reason: PlaceReportReason,
    pub signed_at: i64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PlaceReportReason {
    Spam,
    Harassment,
    Nsfw,
    Other,
}

impl TypedMessage for PlaceReport {
    const PRIMARY_TYPE: &'static str = "PlaceReport";
    fn encode_struct(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.place_id.len() + 1 + 8);
        out.extend_from_slice(self.place_id.as_bytes());
        out.push(match self.reason {
            PlaceReportReason::Spam => 1,
            PlaceReportReason::Harassment => 2,
            PlaceReportReason::Nsfw => 3,
            PlaceReportReason::Other => 4,
        });
        out.extend_from_slice(&self.signed_at.to_be_bytes());
        out
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaceMetadataUpdate {
    pub place_id: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub categories: Vec<String>,
    pub signed_at: i64,
}

impl TypedMessage for PlaceMetadataUpdate {
    const PRIMARY_TYPE: &'static str = "PlaceMetadataUpdate";
    fn encode_struct(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.place_id.len() + 32 + 8);
        out.extend_from_slice(self.place_id.as_bytes());
        if let Some(t) = &self.title {
            out.extend_from_slice(t.as_bytes());
        }
        if let Some(d) = &self.description {
            out.extend_from_slice(d.as_bytes());
        }
        for c in &self.categories {
            out.extend_from_slice(c.as_bytes());
        }
        out.extend_from_slice(&self.signed_at.to_be_bytes());
        out
    }
}
