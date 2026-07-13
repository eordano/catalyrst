use serde_json::Value;

#[derive(Debug, Clone, PartialEq)]
pub struct Peer {
    pub address: String,
    pub parcel_x: Option<i32>,
    pub parcel_y: Option<i32>,
    pub position_x: Option<f64>,
    pub position_y: Option<f64>,
    pub position_z: Option<f64>,
    pub last_ping: Option<i64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Island {
    pub island_id: String,
    pub peer_count: i32,
    pub max_peers: Option<i32>,
    pub center_x: Option<f64>,
    pub center_y: Option<f64>,
    pub center_z: Option<f64>,
    pub radius: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HotScene {
    pub scene_id: String,
    pub name: Option<String>,
    pub base_x: Option<i32>,
    pub base_y: Option<i32>,
    pub users_count: Option<i32>,
    pub parcel_count: i32,
    pub creator: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ActiveWorld {
    pub world_name: String,
    pub users: i32,
}

fn unwrap_array<'a>(v: &'a Value, keys: &[&str]) -> Vec<&'a Value> {
    let arr = match v {
        Value::Array(a) => Some(a),
        Value::Object(_) => {
            let mut found = None;
            for k in keys {
                if let Some(Value::Array(a)) = v.get(*k) {
                    found = Some(a);
                    break;
                }
            }
            found
        }
        _ => None,
    };
    arr.map(|a| a.iter().collect()).unwrap_or_default()
}

fn as_i32(v: Option<&Value>) -> Option<i32> {
    v.and_then(|x| x.as_i64()).map(|x| x as i32)
}

fn as_i64(v: Option<&Value>) -> Option<i64> {
    v.and_then(|x| x.as_i64())
}

fn as_f64(v: Option<&Value>) -> Option<f64> {
    v.and_then(|x| x.as_f64())
}

fn as_string(v: Option<&Value>) -> Option<String> {
    v.and_then(|x| x.as_str()).map(|s| s.to_string())
}

fn coord_i32(arr: Option<&Value>, idx: usize) -> Option<i32> {
    arr.and_then(|a| a.as_array())
        .and_then(|a| a.get(idx))
        .and_then(|x| x.as_i64())
        .map(|x| x as i32)
}

fn coord_f64(arr: Option<&Value>, idx: usize) -> Option<f64> {
    arr.and_then(|a| a.as_array())
        .and_then(|a| a.get(idx))
        .and_then(|x| x.as_f64())
}

pub fn parse_peers(v: &Value) -> Vec<Peer> {
    unwrap_array(v, &["peers", "data"])
        .into_iter()
        .filter_map(|p| {
            let address = as_string(p.get("address"))
                .or_else(|| as_string(p.get("id")))
                .filter(|s| !s.is_empty())?;
            let parcel = p.get("parcel");
            let position = p.get("position");
            Some(Peer {
                address,
                parcel_x: coord_i32(parcel, 0),
                parcel_y: coord_i32(parcel, 1),
                position_x: coord_f64(position, 0),
                position_y: coord_f64(position, 1),
                position_z: coord_f64(position, 2),
                last_ping: as_i64(p.get("lastPing")),
            })
        })
        .collect()
}

pub fn parse_islands(v: &Value) -> Vec<Island> {
    unwrap_array(v, &["islands", "data"])
        .into_iter()
        .filter_map(|i| {
            let island_id = as_string(i.get("id")).filter(|s| !s.is_empty())?;
            let peer_count = i
                .get("peers")
                .and_then(|p| p.as_array())
                .map(|a| a.len() as i32)
                .unwrap_or(0);
            let center = i.get("center");
            Some(Island {
                island_id,
                peer_count,
                max_peers: as_i32(i.get("maxPeers")),
                center_x: coord_f64(center, 0),
                center_y: coord_f64(center, 1),
                center_z: coord_f64(center, 2),
                radius: as_f64(i.get("radius")),
            })
        })
        .collect()
}

pub fn parse_hot_scenes(v: &Value) -> Vec<HotScene> {
    unwrap_array(v, &["hotScenes", "data"])
        .into_iter()
        .filter_map(|s| {
            let scene_id = as_string(s.get("id")).filter(|s| !s.is_empty())?;
            let base = s.get("baseCoords");
            let parcel_count = s
                .get("parcels")
                .and_then(|p| p.as_array())
                .map(|a| a.len() as i32)
                .unwrap_or(0);
            Some(HotScene {
                scene_id,
                name: as_string(s.get("name")),
                base_x: coord_i32(base, 0),
                base_y: coord_i32(base, 1),
                users_count: as_i32(s.get("usersTotalCount")),
                parcel_count,
                creator: as_string(s.get("creator")),
                description: as_string(s.get("description")),
            })
        })
        .collect()
}

pub fn hot_scene_pointer(s: &HotScene) -> Option<String> {
    match (s.base_x, s.base_y) {
        (Some(x), Some(y)) => Some(format!("{},{}", x, y)),
        _ => None,
    }
}

pub fn parse_participants(v: &Value) -> Vec<String> {
    let inner = v.get("data").unwrap_or(v);
    let raw: Vec<&Value> = match inner {
        Value::Array(a) => a.iter().collect(),
        Value::Object(_) => inner
            .get("addresses")
            .and_then(|a| a.as_array())
            .map(|a| a.iter().collect())
            .unwrap_or_default(),
        _ => Vec::new(),
    };
    dedup_lower(raw.into_iter().filter_map(|a| a.as_str()))
}

pub fn parse_active_worlds(v: &Value) -> (Vec<ActiveWorld>, Option<i32>) {
    let data = v.get("data").unwrap_or(v);
    let total = as_i32(data.get("totalUsers"));
    let worlds = data
        .get("perWorld")
        .and_then(|p| p.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|w| {
                    let name = as_string(w.get("worldName")).filter(|s| !s.is_empty())?;
                    let users = as_i32(w.get("users")).unwrap_or(0);
                    if users > 0 {
                        Some(ActiveWorld {
                            world_name: name,
                            users,
                        })
                    } else {
                        None
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    (worlds, total)
}

fn dedup_lower<'a>(it: impl Iterator<Item = &'a str>) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for a in it {
        if a.is_empty() {
            continue;
        }
        let lo = a.to_lowercase();
        if seen.insert(lo.clone()) {
            out.push(lo);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn peers_bare_array_and_wrapped() {
        let bare = json!([
            {"address": "0xAbC", "parcel": [10, -20], "position": [1.5, 0.0, 2.5], "lastPing": 99},
            {"id": "0xDef"}
        ]);
        let peers = parse_peers(&bare);
        assert_eq!(peers.len(), 2);
        assert_eq!(peers[0].address, "0xAbC");
        assert_eq!(peers[0].parcel_x, Some(10));
        assert_eq!(peers[0].parcel_y, Some(-20));
        assert_eq!(peers[0].position_z, Some(2.5));
        assert_eq!(peers[0].last_ping, Some(99));

        assert_eq!(peers[1].address, "0xDef");
        assert_eq!(peers[1].parcel_x, None);

        let wrapped = json!({"peers": [{"address": "0x1"}]});
        assert_eq!(parse_peers(&wrapped).len(), 1);

        let data_wrapped = json!({"data": [{"address": "0x2"}]});
        assert_eq!(parse_peers(&data_wrapped).len(), 1);
    }

    #[test]
    fn peers_skip_missing_address() {
        let v = json!([{"parcel": [1, 2]}, {"address": ""}]);
        assert!(parse_peers(&v).is_empty());
    }

    #[test]
    fn islands_counts_peers() {
        let v = json!({"islands": [
            {"id": "i-1", "peers": ["a", "b", "c"], "maxPeers": 100, "center": [1.0, 2.0, 3.0], "radius": 50.0}
        ]});
        let islands = parse_islands(&v);
        assert_eq!(islands.len(), 1);
        assert_eq!(islands[0].island_id, "i-1");
        assert_eq!(islands[0].peer_count, 3);
        assert_eq!(islands[0].max_peers, Some(100));
        assert_eq!(islands[0].center_y, Some(2.0));
        assert_eq!(islands[0].radius, Some(50.0));
    }

    #[test]
    fn hot_scenes_and_pointer() {
        let v = json!([
            {"id": "s1", "name": "Plaza", "baseCoords": [96, -132],
             "usersTotalCount": 7, "parcels": [[96,-132],[97,-132]],
             "creator": "0xc", "description": "d"},
            {"id": "s2"}
        ]);
        let scenes = parse_hot_scenes(&v);
        assert_eq!(scenes.len(), 2);
        assert_eq!(scenes[0].name.as_deref(), Some("Plaza"));
        assert_eq!(scenes[0].base_x, Some(96));
        assert_eq!(scenes[0].base_y, Some(-132));
        assert_eq!(scenes[0].users_count, Some(7));
        assert_eq!(scenes[0].parcel_count, 2);
        assert_eq!(hot_scene_pointer(&scenes[0]).as_deref(), Some("96,-132"));

        assert_eq!(hot_scene_pointer(&scenes[1]), None);
        assert_eq!(scenes[1].parcel_count, 0);
    }

    #[test]
    fn participants_shapes() {
        let ok = json!({"ok": true, "data": {"addresses": ["0xAA", "0xbb", "0xAA"]}});
        assert_eq!(parse_participants(&ok), vec!["0xaa", "0xbb"]);

        let flat = json!({"addresses": ["0xCc"]});
        assert_eq!(parse_participants(&flat), vec!["0xcc"]);

        let data_list = json!({"data": ["0xDD"]});
        assert_eq!(parse_participants(&data_list), vec!["0xdd"]);

        let bare = json!(["0xEE", "", "0xee"]);
        assert_eq!(parse_participants(&bare), vec!["0xee"]);

        let empty = json!({"ok": true, "data": {}});
        assert!(parse_participants(&empty).is_empty());
    }

    #[test]
    fn active_worlds_filters_zero_users() {
        let v = json!({"data": {
            "totalUsers": 12,
            "perWorld": [
                {"worldName": "alice.eth", "users": 5},
                {"worldName": "empty.eth", "users": 0},
                {"worldName": "bob.dcl.eth", "users": 7},
                {"users": 3}
            ]
        }});
        let (worlds, total) = parse_active_worlds(&v);
        assert_eq!(total, Some(12));
        assert_eq!(worlds.len(), 2);
        assert_eq!(worlds[0].world_name, "alice.eth");
        assert_eq!(worlds[0].users, 5);
        assert_eq!(worlds[1].world_name, "bob.dcl.eth");
    }

    #[test]
    fn active_worlds_missing_data() {
        let v = json!({"data": {}});
        let (worlds, total) = parse_active_worlds(&v);
        assert!(worlds.is_empty());
        assert_eq!(total, None);
    }
}
