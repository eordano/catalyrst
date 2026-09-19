use crate::{bad, discovery, now, ApiResult, Scene};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// One-time events use the Foundation event service's millisecond duration.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EventDraft {
    pub name: String,
    pub description: String,
    pub start_at: String,
    pub duration: i64,
    pub scene: Scene,
}
impl EventDraft {
    pub fn validate(&self) -> ApiResult<()> {
        if self.name.trim().is_empty()
            || self.name.chars().count() > 150
            || self.description.chars().count() > 5000
        {
            return Err(bad(
                "Use a name up to 150 characters and a description up to 5,000",
            ));
        }
        let start = chrono::DateTime::parse_from_rfc3339(&self.start_at)
            .map_err(|_| bad("Choose a valid start date"))?
            .timestamp_millis();
        if !(60_000..=86_400_000).contains(&self.duration)
            || start
                .checked_add(self.duration)
                .is_none_or(|end| end <= now())
        {
            return Err(bad(
                "Choose an upcoming event lasting between one minute and 24 hours",
            ));
        }
        if !(-150..=150).contains(&self.scene.x)
            || !(-150..=150).contains(&self.scene.y)
            || self
                .scene
                .world
                .as_ref()
                .is_some_and(|world| !discovery::valid_world(world))
        {
            return Err(bad(
                "Choose a valid World or coordinates between -150 and 150",
            ));
        }
        Ok(())
    }
    pub fn body(&self) -> Value {
        json!({
            "name": self.name.trim(), "description": self.description.trim(),
            "start_at": self.start_at, "duration": self.duration,
            "x": self.scene.x, "y": self.scene.y,
            "world": self.scene.world.is_some(), "server": self.scene.world,
            "recurrent": false, "all_day": false, "categories": [],
        })
    }
}
