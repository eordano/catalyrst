pub use catalyrst_types::is_eth_address as is_address;

pub struct Params<'a> {
    pairs: &'a [(String, String)],
}

impl<'a> Params<'a> {
    pub fn new(pairs: &'a [(String, String)]) -> Self {
        Self { pairs }
    }

    pub fn get_string(&self, key: &str, default: Option<&str>) -> Option<String> {
        self.get_first(key)
            .map(|s| s.to_string())
            .or_else(|| default.map(|s| s.to_string()))
    }

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

    pub fn get_number(&self, key: &str, default: Option<f64>) -> Option<f64> {
        match self.get_first(key) {
            Some(s) => s.parse::<f64>().ok().or(default),
            None => default,
        }
    }

    pub fn get_boolean(&self, key: &str) -> bool {
        self.pairs.iter().any(|(k, _)| k == key)
    }

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

    pub fn get_address(&self, key: &str, lowercase: bool, default: Option<&str>) -> Option<String> {
        match self.get_first(key) {
            Some(s) if is_address(s) => Some(if lowercase {
                s.to_lowercase()
            } else {
                s.to_string()
            }),
            _ => default.map(|s| s.to_string()),
        }
    }

    pub fn get_address_list(&self, key: &str, lowercase: bool) -> Vec<String> {
        self.pairs
            .iter()
            .filter(|(k, _)| k == key)
            .filter_map(|(_, v)| {
                if is_address(v) {
                    Some(if lowercase {
                        v.to_lowercase()
                    } else {
                        v.clone()
                    })
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
