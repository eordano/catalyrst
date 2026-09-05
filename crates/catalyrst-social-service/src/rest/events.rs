use serde::Serialize;

pub const EVENT_TYPE_COMMUNITY: &str = "community";
pub const SUBTYPE_MEMBER_LEFT: &str = "community-member-left";

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct CommunityMemberLeftMetadata {
    pub id: String,
    #[serde(rename = "memberAddress")]
    pub member_address: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct CommunityMemberLeftEvent {
    #[serde(rename = "type")]
    pub event_type: String,
    #[serde(rename = "subType")]
    pub sub_type: String,
    pub key: String,
    pub timestamp: i64,
    pub metadata: CommunityMemberLeftMetadata,
}

pub fn community_member_left_event(
    community_id: &str,
    member_address: &str,
    timestamp_ms: i64,
) -> CommunityMemberLeftEvent {
    CommunityMemberLeftEvent {
        event_type: EVENT_TYPE_COMMUNITY.to_string(),
        sub_type: SUBTYPE_MEMBER_LEFT.to_string(),
        key: format!("{}-{}-{}", community_id, member_address, timestamp_ms),
        timestamp: timestamp_ms,
        metadata: CommunityMemberLeftMetadata {
            id: community_id.to_string(),
            member_address: member_address.to_string(),
        },
    }
}

pub fn note_member_left(community_id: &str, member_address: &str) -> CommunityMemberLeftEvent {
    let ts = chrono::Utc::now().timestamp_millis();
    let ev = community_member_left_event(community_id, member_address, ts);
    tracing::info!(
        community_id = %community_id,
        member_address = %member_address,
        key = %ev.key,
        "COMMUNITY/MEMBER_LEFT event constructed but NOT broadcast (out-of-model: \
         catalyrst-communities has no event bus / SNS; no downstream consumer seam)"
    );
    ev
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn member_left_event_matches_upstream_shape() {
        let ev = community_member_left_event(
            "00000000-0000-0000-0000-000000000001",
            "0xabc",
            1_700_000_000_000,
        );
        assert_eq!(ev.event_type, "community");
        assert_eq!(ev.sub_type, "community-member-left");
        assert_eq!(ev.timestamp, 1_700_000_000_000);
        assert_eq!(
            ev.key,
            "00000000-0000-0000-0000-000000000001-0xabc-1700000000000"
        );
        assert_eq!(ev.metadata.id, "00000000-0000-0000-0000-000000000001");
        assert_eq!(ev.metadata.member_address, "0xabc");
    }

    #[test]
    fn member_left_event_serializes_with_schema_wire_keys() {
        let ev = community_member_left_event("c1", "0xdead", 42);
        let v = serde_json::to_value(&ev).unwrap();
        let obj = v.as_object().unwrap();
        for key in ["type", "subType", "key", "timestamp", "metadata"] {
            assert!(obj.contains_key(key), "event missing `{key}`: {v}");
        }
        assert!(
            !obj.contains_key("event_type"),
            "must use `type` not `event_type`"
        );
        assert_eq!(v["type"], "community");
        assert_eq!(v["subType"], "community-member-left");
        assert_eq!(v["timestamp"], 42);
        assert_eq!(v["key"], "c1-0xdead-42");
        let md = v["metadata"].as_object().unwrap();
        assert_eq!(md.len(), 2, "metadata is exactly {{ id, memberAddress }}");
        assert_eq!(md["id"], "c1");
        assert_eq!(md["memberAddress"], "0xdead");
    }

    #[test]
    fn note_member_left_returns_the_same_event() {
        let ev = note_member_left("c9", "0xfeed");
        assert_eq!(ev.event_type, "community");
        assert_eq!(ev.sub_type, "community-member-left");
        assert_eq!(ev.metadata.id, "c9");
        assert_eq!(ev.metadata.member_address, "0xfeed");
        assert_eq!(ev.key, format!("c9-0xfeed-{}", ev.timestamp));
    }
}
