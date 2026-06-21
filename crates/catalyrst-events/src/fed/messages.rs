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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileSettingsUpdate {
    pub target: String,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub email_verified: Option<bool>,
    #[serde(default)]
    pub use_local_time: Option<bool>,
    #[serde(default)]
    pub notify_by_email: Option<bool>,
    #[serde(default)]
    pub notify_by_browser: Option<bool>,
    #[serde(default)]
    pub permissions: Option<Vec<String>>,
    pub signed_at: i64,
}

impl TypedMessage for ProfileSettingsUpdate {
    const PRIMARY_TYPE: &'static str = "ProfileSettingsUpdate";
    fn encode_struct(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.target.len() + 64);
        out.extend_from_slice(self.target.as_bytes());
        if let Some(e) = &self.email {
            out.extend_from_slice(e.as_bytes());
        }
        out.push(self.email_verified.unwrap_or(false) as u8);
        out.push(self.use_local_time.unwrap_or(false) as u8);
        out.push(self.notify_by_email.unwrap_or(false) as u8);
        out.push(self.notify_by_browser.unwrap_or(false) as u8);
        for p in self.permissions.iter().flatten() {
            out.extend_from_slice(p.as_bytes());
            out.push(0);
        }
        out.extend_from_slice(&self.signed_at.to_be_bytes());
        out
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduleUpsert {
    #[serde(default)]
    pub schedule_id: Option<String>,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub image: Option<String>,
    #[serde(default)]
    pub theme: Option<String>,
    #[serde(default)]
    pub background: Vec<String>,
    pub active_since: i64,
    pub active_until: i64,
    #[serde(default = "default_true")]
    pub active: bool,
    pub signed_at: i64,
}

fn default_true() -> bool {
    true
}

impl TypedMessage for ScheduleUpsert {
    const PRIMARY_TYPE: &'static str = "ScheduleUpsert";
    fn encode_struct(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.name.len() + 64);
        if let Some(id) = &self.schedule_id {
            out.extend_from_slice(id.as_bytes());
        }
        out.extend_from_slice(self.name.as_bytes());
        if let Some(d) = &self.description {
            out.extend_from_slice(d.as_bytes());
        }
        if let Some(i) = &self.image {
            out.extend_from_slice(i.as_bytes());
        }
        if let Some(t) = &self.theme {
            out.extend_from_slice(t.as_bytes());
        }
        for c in &self.background {
            out.extend_from_slice(c.as_bytes());
            out.push(0);
        }
        out.extend_from_slice(&self.active_since.to_be_bytes());
        out.extend_from_slice(&self.active_until.to_be_bytes());
        out.push(self.active as u8);
        out.extend_from_slice(&self.signed_at.to_be_bytes());
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primary_types_are_stable() {
        assert_eq!(ProfileSettingsUpdate::PRIMARY_TYPE, "ProfileSettingsUpdate");
        assert_eq!(ScheduleUpsert::PRIMARY_TYPE, "ScheduleUpsert");
    }

    #[test]
    fn profile_settings_serde_round_trip() {
        let m = ProfileSettingsUpdate {
            target: "0xABC".into(),
            email: Some("a@b.c".into()),
            email_verified: Some(true),
            use_local_time: None,
            notify_by_email: Some(false),
            notify_by_browser: None,
            permissions: Some(vec!["edit_any_schedule".into()]),
            signed_at: 42,
        };
        let json = serde_json::to_value(&m).unwrap();
        let back: ProfileSettingsUpdate = serde_json::from_value(json).unwrap();
        assert_eq!(back.target, m.target);
        assert_eq!(back.permissions, m.permissions);

        assert_eq!(m.encode_struct(), back.encode_struct());
    }

    #[test]
    fn profile_settings_minimal_body_defaults() {
        let v = serde_json::json!({ "target": "0x1", "signed_at": 1 });
        let m: ProfileSettingsUpdate = serde_json::from_value(v).unwrap();
        assert!(m.email.is_none());
        assert!(m.permissions.is_none());
    }

    #[test]
    fn schedule_create_vs_update_encode_differs() {
        let create = ScheduleUpsert {
            schedule_id: None,
            name: "MVMF".into(),
            description: None,
            image: None,
            theme: None,
            background: vec![],
            active_since: 10,
            active_until: 20,
            active: true,
            signed_at: 5,
        };
        let mut update = create.clone();
        update.schedule_id = Some("sched-1".into());
        assert_ne!(create.encode_struct(), update.encode_struct());
    }

    #[test]
    fn schedule_active_defaults_true() {
        let v = serde_json::json!({
            "name": "x", "active_since": 1, "active_until": 2, "signed_at": 3
        });
        let m: ScheduleUpsert = serde_json::from_value(v).unwrap();
        assert!(m.active);
        assert!(m.schedule_id.is_none());
    }
}
