use axum::http::HeaderMap;

pub const IMMUTABLE_CACHE_CONTROL: &str = "public,max-age=31536000,s-maxage=31536000,immutable";

pub fn to_etag(hash: &str) -> String {
    format!("\"{}\"", hash)
}

pub fn check_not_modified(headers: &HeaderMap, hash: &str) -> Option<Vec<(&'static str, String)>> {
    let etag = to_etag(hash);
    let if_none_match = headers.get("if-none-match")?.to_str().ok()?;

    let not_modified_headers = vec![
        ("ETag", etag.clone()),
        ("Cache-Control", IMMUTABLE_CACHE_CONTROL.to_string()),
        ("Access-Control-Expose-Headers", "ETag".to_string()),
    ];

    if if_none_match == "*" {
        return Some(not_modified_headers);
    }

    let tags: Vec<String> = if_none_match
        .split(',')
        .map(|t| t.trim().trim_start_matches("W/").to_string())
        .collect();

    if tags.contains(&etag) {
        return Some(not_modified_headers);
    }

    None
}

#[derive(Debug, Clone)]
pub enum ParsedRange {
    Range { start: u64, end: u64 },
    Unsatisfiable,
}

pub fn parse_range_header(
    range_header: Option<&str>,
    total_size: Option<u64>,
) -> Option<ParsedRange> {
    let header = range_header?;
    let total = total_size?;

    if let Some(rest) = header.strip_prefix("bytes=") {
        if let Some(suffix) = rest.strip_prefix('-') {
            let suffix_len: u64 = suffix.parse().ok()?;
            if suffix_len == 0 || total == 0 {
                return Some(ParsedRange::Unsatisfiable);
            }
            let start = total.saturating_sub(suffix_len);
            return Some(ParsedRange::Range {
                start,
                end: total - 1,
            });
        }

        let parts: Vec<&str> = rest.splitn(2, '-').collect();
        if parts.len() == 2 {
            let start: u64 = parts[0].parse().ok()?;
            let end: u64 = if parts[1].is_empty() {
                total - 1
            } else {
                parts[1].parse().ok()?
            };

            if start > end || start >= total {
                return Some(ParsedRange::Unsatisfiable);
            }

            return Some(ParsedRange::Range {
                start,
                end: end.min(total - 1),
            });
        }
    }

    None
}

pub fn content_file_headers(
    hash: &str,
    size: Option<u64>,
    encoding: Option<&str>,
) -> Vec<(&'static str, String)> {
    let mut headers = vec![
        ("Content-Type", "application/octet-stream".to_string()),
        ("ETag", to_etag(hash)),
        (
            "Access-Control-Expose-Headers",
            "ETag, Content-Range, Accept-Ranges, Content-Length".to_string(),
        ),
        ("Accept-Ranges", "bytes".to_string()),
        ("Cache-Control", IMMUTABLE_CACHE_CONTROL.to_string()),
    ];

    if let Some(enc) = encoding {
        headers.push(("Content-Encoding", enc.to_string()));
    }
    if let Some(s) = size {
        headers.push(("Content-Length", s.to_string()));
    }

    headers
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntityField {
    Content,
    Pointers,
    Metadata,
}

impl EntityField {
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_uppercase().as_str() {
            "CONTENT" => Some(EntityField::Content),
            "POINTERS" => Some(EntityField::Pointers),
            "METADATA" => Some(EntityField::Metadata),
            _ => None,
        }
    }
}

pub fn mask_entity(
    entity: &serde_json::Value,
    fields: Option<&[EntityField]>,
) -> serde_json::Value {
    let Some(obj) = entity.as_object() else {
        return entity.clone();
    };

    let mut result = serde_json::Map::new();

    for key in &["version", "id", "type", "timestamp"] {
        if let Some(v) = obj.get(*key) {
            result.insert(key.to_string(), v.clone());
        }
    }

    let include_all = fields.is_none();
    let fields = fields.unwrap_or(&[]);

    if include_all || fields.contains(&EntityField::Pointers) {
        result.insert(
            "pointers".to_string(),
            obj.get("pointers")
                .cloned()
                .unwrap_or(serde_json::Value::Array(vec![])),
        );
    } else {
        result.insert("pointers".to_string(), serde_json::Value::Array(vec![]));
    }

    if include_all || fields.contains(&EntityField::Content) {
        result.insert(
            "content".to_string(),
            obj.get("content")
                .cloned()
                .unwrap_or(serde_json::Value::Array(vec![])),
        );
    } else {
        result.insert("content".to_string(), serde_json::Value::Array(vec![]));
    }

    if include_all || fields.contains(&EntityField::Metadata) {
        if let Some(m) = obj.get("metadata") {
            result.insert("metadata".to_string(), m.clone());
        }
    }

    serde_json::Value::Object(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn etag_format() {
        assert_eq!(to_etag("abc123"), "\"abc123\"");
    }

    #[test]
    fn parse_range_normal() {
        let r = parse_range_header(Some("bytes=0-499"), Some(1000));
        match r {
            Some(ParsedRange::Range { start, end }) => {
                assert_eq!(start, 0);
                assert_eq!(end, 499);
            }
            _ => panic!("Expected Range"),
        }
    }

    #[test]
    fn parse_range_suffix() {
        let r = parse_range_header(Some("bytes=-200"), Some(1000));
        match r {
            Some(ParsedRange::Range { start, end }) => {
                assert_eq!(start, 800);
                assert_eq!(end, 999);
            }
            _ => panic!("Expected Range"),
        }
    }

    #[test]
    fn parse_range_unsatisfiable() {
        let r = parse_range_header(Some("bytes=1000-2000"), Some(500));
        assert!(matches!(r, Some(ParsedRange::Unsatisfiable)));
    }
}
