use serde_json::Value;
use std::collections::HashSet;
use std::fmt;

use crate::volatility::Volatility;

#[derive(Debug, Clone)]
pub struct Difference {
    pub path: String,
    pub baseline_value: String,
    pub candidate_value: String,
}

impl fmt::Display for Difference {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}: baseline={} candidate={}",
            self.path, self.baseline_value, self.candidate_value
        )
    }
}

const IGNORED_FIELDS: &[&str] = &[
    "currentTime",
    "commitHash",
    "version",
    "realmName",
    "publicUrl",
    "url",
    "challengeText",
    "lastSyncWithDAO",
    "synchronizationTime",
    "generationTimestamp",
    "id",
    "scope",
    "next",
    "self",
    "previous",
];

const TIMESTAMP_TOLERANCE_MS: f64 = 1000.0;

pub fn compare_json(
    section: &str,
    name: &str,
    baseline_val: &Value,
    candidate_val: &Value,
    volatility: &Volatility,
) -> Vec<Difference> {
    let mut diffs = Vec::new();
    compare_recursive(
        section,
        name,
        baseline_val,
        candidate_val,
        &mut diffs,
        volatility,
    );
    diffs
}

fn is_globally_ignored(path: &str) -> bool {
    let leaf = path.rsplit('.').next().unwrap_or(path);
    let leaf_clean = leaf.split('[').next().unwrap_or(leaf);
    IGNORED_FIELDS.contains(&leaf_clean)
}

fn is_ignored_for_section(section: &str, path: &str, volatility: &Volatility) -> bool {
    if is_globally_ignored(path) {
        return true;
    }
    volatility.is_ignored(section, path)
}

fn looks_like_timestamp(n: f64) -> bool {
    n > 1_000_000_000_000.0 && n < 3_000_000_000_000.0
}

fn compare_recursive(
    section: &str,
    path: &str,
    baseline_val: &Value,
    candidate_val: &Value,
    diffs: &mut Vec<Difference>,
    volatility: &Volatility,
) {
    if is_ignored_for_section(section, path, volatility) {
        return;
    }

    match (baseline_val, candidate_val) {
        (Value::Object(b_map), Value::Object(c_map)) => {
            let all_keys: HashSet<&String> = b_map.keys().chain(c_map.keys()).collect();
            let mut keys: Vec<&&String> = all_keys.iter().collect();
            keys.sort();
            for key in keys {
                let child_path = if path.is_empty() {
                    key.to_string()
                } else {
                    format!("{}.{}", path, key)
                };
                match (b_map.get(*key), c_map.get(*key)) {
                    (Some(bv), Some(cv)) => {
                        compare_recursive(section, &child_path, bv, cv, diffs, volatility);
                    }
                    (Some(bv), None) => {
                        if !is_ignored_for_section(section, &child_path, volatility) {
                            diffs.push(Difference {
                                path: child_path,
                                baseline_value: truncate_value(bv),
                                candidate_value: "missing".to_string(),
                            });
                        }
                    }
                    (None, Some(cv)) => {
                        if !is_ignored_for_section(section, &child_path, volatility) {
                            diffs.push(Difference {
                                path: child_path,
                                baseline_value: "missing".to_string(),
                                candidate_value: truncate_value(cv),
                            });
                        }
                    }
                    (None, None) => unreachable!(),
                }
            }
        }
        (Value::Array(b_arr), Value::Array(c_arr)) => {
            if b_arr.len() != c_arr.len() {
                diffs.push(Difference {
                    path: format!("{}.length", path),
                    baseline_value: b_arr.len().to_string(),
                    candidate_value: c_arr.len().to_string(),
                });
            }
            let len = b_arr.len().min(c_arr.len());
            for i in 0..len {
                let child_path = format!("{}[{}]", path, i);
                compare_recursive(
                    section,
                    &child_path,
                    &b_arr[i],
                    &c_arr[i],
                    diffs,
                    volatility,
                );
            }
        }
        (Value::Number(b_n), Value::Number(c_n)) => {
            let b_f = b_n.as_f64().unwrap_or(0.0);
            let c_f = c_n.as_f64().unwrap_or(0.0);
            if looks_like_timestamp(b_f)
                && looks_like_timestamp(c_f)
                && (b_f - c_f).abs() <= TIMESTAMP_TOLERANCE_MS
            {
                return;
            }
            if b_f != c_f {
                diffs.push(Difference {
                    path: path.to_string(),
                    baseline_value: baseline_val.to_string(),
                    candidate_value: candidate_val.to_string(),
                });
            }
        }
        _ => {
            if baseline_val != candidate_val {
                diffs.push(Difference {
                    path: path.to_string(),
                    baseline_value: truncate_value(baseline_val),
                    candidate_value: truncate_value(candidate_val),
                });
            }
        }
    }
}

fn truncate_value(val: &Value) -> String {
    let s = val.to_string();
    if s.len() > 120 {
        let mut cut = 117;
        while !s.is_char_boundary(cut) {
            cut -= 1;
        }
        format!("{}...", &s[..cut])
    } else {
        s
    }
}
