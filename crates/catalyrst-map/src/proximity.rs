use std::collections::HashMap;
use std::sync::OnceLock;

use serde::Deserialize;
use serde_json::{json, Value};

const PROXIMITY_JSON: &str = include_str!("../data/proximity.json");

#[derive(Debug, Clone, Deserialize)]
struct CoordProximity {
    district: Option<i64>,
    plaza: Option<i64>,
    road: Option<i64>,
}

fn table() -> &'static HashMap<String, CoordProximity> {
    static T: OnceLock<HashMap<String, CoordProximity>> = OnceLock::new();
    T.get_or_init(|| serde_json::from_str(PROXIMITY_JSON).expect("proximity.json must parse"))
}

fn min_proximity(coords: &[(i32, i32)]) -> Option<(Option<i64>, Option<i64>, Option<i64>)> {
    let t = table();
    let mut district: Option<i64> = None;
    let mut plaza: Option<i64> = None;
    let mut road: Option<i64> = None;
    let mut any = false;
    for (x, y) in coords {
        let id = format!("{},{}", x, y);
        if let Some(cp) = t.get(&id) {
            any = true;
            if let Some(d) = cp.district {
                if district.map(|cur| d < cur).unwrap_or(true) {
                    district = Some(d);
                }
            }
            if let Some(p) = cp.plaza {
                if plaza.map(|cur| p < cur).unwrap_or(true) {
                    plaza = Some(p);
                }
            }
            if let Some(r) = cp.road {
                if road.map(|cur| r < cur).unwrap_or(true) {
                    road = Some(r);
                }
            }
        }
    }
    if any {
        Some((district, plaza, road))
    } else {
        None
    }
}

pub fn append_attributes(attributes: &mut Vec<Value>, coords: &[(i32, i32)]) {
    let Some((district, plaza, road)) = min_proximity(coords) else {
        return;
    };
    if let Some(d) = district {
        attributes.push(distance_attr("District", d));
    }
    if let Some(p) = plaza {
        attributes.push(distance_attr("Plaza", p));
    }
    if let Some(r) = road {
        attributes.push(distance_attr("Road", r));
    }
}

fn distance_attr(name: &str, value: i64) -> Value {
    json!({
        "trait_type": format!("Distance to {name}"),
        "value": value,
        "display_type": "number",
    })
}
