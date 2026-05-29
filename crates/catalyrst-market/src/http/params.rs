//! Direct port of `marketplace-server/src/logic/http/params.ts`.
//!
//! Wraps `Vec<(String, String)>` (the same shape `axum::extract::Query`
//! gives you) and offers the same typed-getter API the upstream `Params`
//! class does.

pub struct Params<'a> {
    pairs: &'a [(String, String)],
}

impl<'a> Params<'a> {
    pub fn new(pairs: &'a [(String, String)]) -> Self {
        Self { pairs }
    }

    /// `params.getString(key, defaultValue?)`
    pub fn get_string(&self, key: &str, default: Option<&str>) -> Option<String> {
        self.get_first(key)
            .map(|s| s.to_string())
            .or_else(|| default.map(|s| s.to_string()))
    }

    /// `params.getList(key, values?)` — collects both `?key=a&key=b` and
    /// `?key[]=a&key[]=b`. If `valid_values` is non-empty, filters to that set.
    pub fn get_list(&self, key: &str, valid_values: &[&str]) -> Vec<String> {
        let bracket_key = format!("{}[]", key);
        let mut out = Vec::new();
        for (k, v) in self.pairs {
            if k == key || k == &bracket_key {
                if valid_values.is_empty() || valid_values.iter().any(|valid| valid == v) {
                    out.push(v.clone());
                }
            }
        }
        out
    }

    /// `params.getNumber(key, defaultValue?)`. Returns `None` if absent and no
    /// default; returns the default on parse failure (mirroring `parseFloat` + `isNaN` check).
    pub fn get_number(&self, key: &str, default: Option<f64>) -> Option<f64> {
        match self.get_first(key) {
            Some(s) => s.parse::<f64>().ok().or(default),
            None => default,
        }
    }

    /// `params.getBoolean(key)` — true iff the key is present (regardless of value).
    pub fn get_boolean(&self, key: &str) -> bool {
        self.pairs.iter().any(|(k, _)| k == key)
    }

    /// `params.getValue(key, values?, defaultValue?)` — like `get_string` but
    /// rejects values not in the allow-list (returns the default in that case).
    pub fn get_value(
        &self,
        key: &str,
        valid_values: &[&str],
        default: Option<&str>,
    ) -> Option<String> {
        let raw = self.get_first(key).map(|s| s.to_string());
        if let Some(ref v) = raw {
            if valid_values.is_empty() || valid_values.iter().any(|valid| valid == v) {
                return Some(v.clone());
            }
        }
        default.map(|s| s.to_string())
    }

    /// `params.getAddress(key, lowercase, defaultValue?)`. Returns `None` /
    /// default if not a syntactically-valid address.
    pub fn get_address(
        &self,
        key: &str,
        lowercase: bool,
        default: Option<&str>,
    ) -> Option<String> {
        match self.get_first(key) {
            Some(s) if is_address(s) => Some(if lowercase { s.to_lowercase() } else { s.to_string() }),
            _ => default.map(|s| s.to_string()),
        }
    }

    /// `params.getAddressList(key, lowercase)`.
    pub fn get_address_list(&self, key: &str, lowercase: bool) -> Vec<String> {
        self.pairs
            .iter()
            .filter(|(k, _)| k == key)
            .filter_map(|(_, v)| {
                if is_address(v) {
                    Some(if lowercase { v.to_lowercase() } else { v.clone() })
                } else {
                    None
                }
            })
            .collect()
    }

    fn get_first(&self, key: &str) -> Option<&str> {
        self.pairs
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }
}

/// Mirrors `marketplace-server/src/logic/address.ts:isAddress` — `0x` prefix +
/// 40 hex chars.
pub fn is_address(value: &str) -> bool {
    value.len() == 42
        && value.starts_with("0x")
        && value[2..].bytes().all(|b| b.is_ascii_hexdigit())
}
