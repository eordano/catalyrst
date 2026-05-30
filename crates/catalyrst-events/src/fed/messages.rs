use catalyrst_fed::sig::TypedMessage;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventLocation {
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventCreate {
    pub title: String,
    pub description: Option<String>,
    pub start_at: i64,
    pub end_at: i64,
    pub location: EventLocation,
    pub signed_at: i64,
}

impl TypedMessage for EventCreate {
    const PRIMARY_TYPE: &'static str = "EventCreate";
    fn encode_struct(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.title.len() + 64);
        out.extend_from_slice(self.title.as_bytes());
        if let Some(d) = &self.description {
            out.extend_from_slice(d.as_bytes());
        }
        out.extend_from_slice(&self.start_at.to_be_bytes());
        out.extend_from_slice(&self.end_at.to_be_bytes());
        out.extend_from_slice(&self.location.x.to_be_bytes());
        out.extend_from_slice(&self.location.y.to_be_bytes());
        out.extend_from_slice(&self.signed_at.to_be_bytes());
        out
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AttendAction {
    Attend,
    Cancel,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventAttend {
    pub event_id: String,
    pub action: AttendAction,
    pub signed_at: i64,
}

impl TypedMessage for EventAttend {
    const PRIMARY_TYPE: &'static str = "EventAttend";
    fn encode_struct(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.event_id.len() + 16);
        out.extend_from_slice(self.event_id.as_bytes());
        out.push(match self.action {
            AttendAction::Attend => 1,
            AttendAction::Cancel => 0,
        });
        out.extend_from_slice(&self.signed_at.to_be_bytes());
        out
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModerateAction {
    Ban,
    Hide,
    Feature,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventModerate {
    pub event_id: String,
    pub action: ModerateAction,
    pub signed_at: i64,
}

impl TypedMessage for EventModerate {
    const PRIMARY_TYPE: &'static str = "EventModerate";
    fn encode_struct(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.event_id.len() + 16);
        out.extend_from_slice(self.event_id.as_bytes());
        out.push(match self.action {
            ModerateAction::Ban => 0,
            ModerateAction::Hide => 1,
            ModerateAction::Feature => 2,
        });
        out.extend_from_slice(&self.signed_at.to_be_bytes());
        out
    }
}
