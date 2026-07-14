// The unified destination surface keys places and worlds through one
// polymorphic id: a place id is a UUID, anything else names a world.

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, utoipa::ToSchema)]
#[serde(rename_all = "lowercase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "places/"))]
pub enum EntityType {
    Place,
    World,
}

impl EntityType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Place => "place",
            Self::World => "world",
        }
    }
}

pub fn is_place_id(entity_id: &str) -> bool {
    let bytes = entity_id.as_bytes();
    bytes.len() == 36
        && bytes.iter().enumerate().all(|(i, c)| match i {
            8 | 13 | 18 | 23 => *c == b'-',
            _ => c.is_ascii_hexdigit(),
        })
}

pub fn resolve_entity_type(entity_id: &str) -> EntityType {
    if is_place_id(entity_id) {
        EntityType::Place
    } else {
        EntityType::World
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const UUID: &str = "123e4567-e89b-12d3-a456-426614174000";

    #[test]
    fn a_uuid_is_a_place_id() {
        assert!(is_place_id(UUID));
        assert_eq!(resolve_entity_type(UUID), EntityType::Place);
    }

    #[test]
    fn uuid_matching_is_case_insensitive() {
        assert!(is_place_id(&UUID.to_ascii_uppercase()));
        assert_eq!(
            resolve_entity_type(&UUID.to_ascii_uppercase()),
            EntityType::Place
        );
    }

    #[test]
    fn a_world_name_is_not_a_place_id() {
        assert!(!is_place_id("my-world.dcl.eth"));
        assert_eq!(resolve_entity_type("my-world.dcl.eth"), EntityType::World);
    }

    #[test]
    fn dashes_and_hex_are_required_in_the_uuid_positions() {
        assert!(!is_place_id("123e4567e89b12d3a456426614174000"));
        assert!(!is_place_id("123e4567-e89b-12d3-a456-42661417400g"));
        assert!(!is_place_id("123e4567-e89b-12d3-a456-4266141740000"));
        assert!(!is_place_id("-------------------------------------"));
        assert!(!is_place_id(""));
    }

    #[test]
    fn entity_type_names_match_the_wire_kind() {
        assert_eq!(EntityType::Place.as_str(), "place");
        assert_eq!(EntityType::World.as_str(), "world");
        assert_eq!(
            serde_json::to_value(EntityType::Place).unwrap(),
            serde_json::json!("place")
        );
        assert_eq!(
            serde_json::to_value(EntityType::World).unwrap(),
            serde_json::json!("world")
        );
    }
}
