use std::collections::HashMap;

pub const MAX_PAGE_SIZE: u32 = 1000;

pub const MAX_QUERY_PARAMS: usize = 2000;

pub type QueryParams = HashMap<String, Vec<String>>;

pub fn parse_query_string(raw: &str) -> QueryParams {
    let mut map: QueryParams = HashMap::new();
    if raw.is_empty() {
        return map;
    }
    for pair in raw.split('&').take(MAX_QUERY_PARAMS) {
        let mut parts = pair.splitn(2, '=');
        let key = parts.next().unwrap_or("");
        let value = parts.next().unwrap_or("");

        let decoded_key = urlencoding_decode(key);
        let decoded_value = urlencoding_decode(value);

        let normalized_key = strip_array_index(&decoded_key);

        map.entry(normalized_key).or_default().push(decoded_value);
    }
    map
}

pub fn qs_get_array(params: &QueryParams, key: &str) -> Vec<String> {
    params.get(key).cloned().unwrap_or_default()
}

pub fn qs_get_string(params: &QueryParams, key: &str) -> Option<String> {
    params.get(key).and_then(|v| v.first()).cloned()
}

pub fn qs_get_number(params: &QueryParams, key: &str) -> Option<i64> {
    qs_get_string(params, key).and_then(|s| s.parse::<i64>().ok())
}

pub fn qs_get_bool(params: &QueryParams, key: &str) -> Option<bool> {
    qs_get_string(params, key).map(|s| s == "true")
}

pub fn parse_entity_type(raw: &str) -> Option<&'static str> {
    let mut t = raw.to_string();
    if t.ends_with('s') {
        t.pop();
    }
    match t.to_uppercase().trim() {
        "SCENE" => Some("scene"),
        "PROFILE" => Some("profile"),
        "WEARABLE" => Some("wearable"),
        "STORE" => Some("store"),
        "EMOTE" => Some("emote"),
        "OUTFITS" => Some("outfits"),
        _ => None,
    }
}

pub fn to_query_string(filters: &HashMap<String, Vec<String>>) -> String {
    let mut pairs: Vec<String> = Vec::new();
    let mut keys: Vec<&String> = filters.keys().collect();
    keys.sort();
    for key in keys {
        if let Some(values) = filters.get(key) {
            for val in values {
                if !val.is_empty() {
                    pairs.push(format!(
                        "{}={}",
                        urlencoding_encode(key),
                        urlencoding_encode(val)
                    ));
                }
            }
        }
    }
    pairs.join("&")
}

pub fn camel_to_snake(s: &str) -> String {
    let mut result = String::with_capacity(s.len() + 4);
    for (i, ch) in s.chars().enumerate() {
        if ch.is_uppercase() {
            if i > 0 {
                result.push('_');
            }
            result.extend(ch.to_lowercase());
        } else {
            result.push(ch);
        }
    }
    result
}

#[derive(Debug, Clone)]
pub struct Pagination {
    pub page_size: i64,
    pub page_num: i64,
    pub offset: i64,
    pub limit: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OversizePolicy {
    Reject,
    Clamp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NonPositivePolicy {
    Reject,
    ClampToOne,
    PassThrough,
}

pub fn parse_pagination_with(
    params: &QueryParams,
    max_page_size: i64,
    oversize: OversizePolicy,
    non_positive: NonPositivePolicy,
) -> Result<Pagination, String> {
    let mut page_size = qs_get_number(params, "pageSize").unwrap_or(100);
    let mut page_num = qs_get_number(params, "pageNum").unwrap_or(1);

    match non_positive {
        NonPositivePolicy::Reject => {
            if page_size < 1 {
                return Err("pageSize must be a positive integer".to_string());
            }
            if page_num < 1 {
                return Err("pageNum must be a positive integer".to_string());
            }
        }
        NonPositivePolicy::ClampToOne => {
            if page_size < 1 {
                page_size = 1;
            }
            if page_num < 1 {
                page_num = 1;
            }
        }
        NonPositivePolicy::PassThrough => {}
    }

    if page_size > max_page_size {
        match oversize {
            OversizePolicy::Reject => {
                return Err(format!("max allowed pageSize is {}", max_page_size))
            }
            OversizePolicy::Clamp => page_size = max_page_size,
        }
    }

    let offset = page_num.saturating_sub(1).saturating_mul(page_size);
    let limit = page_size;
    Ok(Pagination {
        page_size,
        page_num,
        offset,
        limit,
    })
}

pub fn parse_pagination(params: &QueryParams, max_page_size: i64) -> Result<Pagination, String> {
    parse_pagination_with(
        params,
        max_page_size,
        OversizePolicy::Reject,
        NonPositivePolicy::Reject,
    )
}

pub fn is_valid_eth_address(addr: &str) -> bool {
    addr.len() == 42 && addr.starts_with("0x") && addr[2..].chars().all(|c| c.is_ascii_hexdigit())
}

fn urlencoding_decode(s: &str) -> String {
    let mut bytes: Vec<u8> = Vec::with_capacity(s.len());
    let mut buf = [0u8; 4];
    let mut chars = s.chars();
    while let Some(ch) = chars.next() {
        if ch == '%' {
            let hi = chars.next().unwrap_or('0');
            let lo = chars.next().unwrap_or('0');
            let hex = format!("{}{}", hi, lo);
            if let Ok(byte) = u8::from_str_radix(&hex, 16) {
                bytes.push(byte);
            } else {
                bytes.push(b'%');
                bytes.extend_from_slice(hi.encode_utf8(&mut buf).as_bytes());
                bytes.extend_from_slice(lo.encode_utf8(&mut buf).as_bytes());
            }
        } else if ch == '+' {
            bytes.push(b' ');
        } else {
            bytes.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
        }
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

fn urlencoding_encode(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    for byte in s.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                result.push(byte as char);
            }
            _ => {
                result.push_str(&format!("%{:02X}", byte));
            }
        }
    }
    result
}

fn strip_array_index(key: &str) -> String {
    if let Some(bracket_pos) = key.find('[') {
        if key.ends_with(']') {
            let inside = &key[bracket_pos + 1..key.len() - 1];
            if inside.chars().all(|c| c.is_ascii_digit()) {
                return key[..bracket_pos].to_string();
            }
        }
    }
    key.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_repeated_keys() {
        let params = parse_query_string("cid=abc&cid=def&cid=ghi");
        assert_eq!(qs_get_array(&params, "cid"), vec!["abc", "def", "ghi"]);
    }

    #[test]
    fn parse_indexed_array() {
        let params = parse_query_string("entityType[0]=scene&entityType[1]=profile");
        assert_eq!(
            qs_get_array(&params, "entityType"),
            vec!["scene", "profile"]
        );
    }

    #[test]
    fn parse_just_over_endpoint_cap_is_visible_to_handlers() {
        let query: String = (0..1001)
            .map(|i| format!("cid={}", i))
            .collect::<Vec<_>>()
            .join("&");
        let params = parse_query_string(&query);
        assert_eq!(qs_get_array(&params, "cid").len(), 1001);
    }

    #[test]
    fn parse_bounds_pathological_flood_to_param_limit() {
        let query: String = (0..3000)
            .map(|i| format!("cid={}", i))
            .collect::<Vec<_>>()
            .join("&");
        let params = parse_query_string(&query);
        assert!(qs_get_array(&params, "cid").len() <= MAX_QUERY_PARAMS);
    }

    #[test]
    fn parse_pagination_defaults() {
        let params = parse_query_string("");
        let p = parse_pagination(&params, 1000).unwrap();
        assert_eq!(p.page_size, 100);
        assert_eq!(p.page_num, 1);
        assert_eq!(p.offset, 0);
        assert_eq!(p.limit, 100);
    }

    #[test]
    fn camel_to_snake_works() {
        assert_eq!(camel_to_snake("localTimestamp"), "local_timestamp");
        assert_eq!(camel_to_snake("entityTimestamp"), "entity_timestamp");
    }

    #[test]
    fn pagination_oversize_clamp() {
        let params = parse_query_string("pageSize=99999");
        let p = parse_pagination_with(
            &params,
            1000,
            OversizePolicy::Clamp,
            NonPositivePolicy::ClampToOne,
        )
        .unwrap();
        assert_eq!(p.page_size, 1000);
        assert_eq!(p.limit, 1000);
    }

    #[test]
    fn pagination_oversize_reject() {
        let params = parse_query_string("pageSize=99999");
        let err = parse_pagination_with(
            &params,
            1000,
            OversizePolicy::Reject,
            NonPositivePolicy::PassThrough,
        )
        .unwrap_err();
        assert_eq!(err, "max allowed pageSize is 1000");
    }

    #[test]
    fn pagination_non_positive_clamp_to_one() {
        let params = parse_query_string("pageSize=0&pageNum=-3");
        let p = parse_pagination_with(
            &params,
            1000,
            OversizePolicy::Clamp,
            NonPositivePolicy::ClampToOne,
        )
        .unwrap();
        assert_eq!(p.page_size, 1);
        assert_eq!(p.page_num, 1);
        assert_eq!(p.offset, 0);
    }

    #[test]
    fn pagination_strict_rejects_non_positive() {
        let zero = parse_query_string("pageSize=0");
        assert_eq!(
            parse_pagination(&zero, 1000).unwrap_err(),
            "pageSize must be a positive integer"
        );
        let neg = parse_query_string("pageNum=-1");
        assert_eq!(
            parse_pagination(&neg, 1000).unwrap_err(),
            "pageNum must be a positive integer"
        );
    }

    #[test]
    fn pagination_passthrough_keeps_values() {
        let params = parse_query_string("pageSize=100&pageNum=0");
        let p = parse_pagination_with(
            &params,
            i64::MAX,
            OversizePolicy::Reject,
            NonPositivePolicy::PassThrough,
        )
        .unwrap();
        assert_eq!(p.page_num, 0);
        assert_eq!(p.offset, -100);
    }

    #[test]
    fn pagination_huge_pagenum_saturates() {
        let params = parse_query_string(&format!("pageNum={}&pageSize=100", i64::MAX));
        let p = parse_pagination_with(
            &params,
            1000,
            OversizePolicy::Reject,
            NonPositivePolicy::PassThrough,
        )
        .unwrap();
        assert_eq!(p.offset, i64::MAX);
    }

    #[test]
    fn decode_reassembles_multibyte_utf8() {
        let params = parse_query_string("textSearch=caf%C3%A9&name=%E4%B8%AD%E6%96%87");
        assert_eq!(
            qs_get_string(&params, "textSearch").as_deref(),
            Some("café")
        );
        assert_eq!(qs_get_string(&params, "name").as_deref(), Some("中文"));
    }

    #[test]
    fn decode_plus_is_space_and_malformed_percent_preserved() {
        let params = parse_query_string("q=a+b&bad=%ZZ");
        assert_eq!(qs_get_string(&params, "q").as_deref(), Some("a b"));
        assert_eq!(qs_get_string(&params, "bad").as_deref(), Some("%ZZ"));
    }

    #[test]
    fn eth_address_validation() {
        assert!(is_valid_eth_address(
            "0x1234567890abcdefABCDEF1234567890abcdef12"
        ));
        assert!(!is_valid_eth_address("0x1234"));
        assert!(!is_valid_eth_address(
            "1234567890abcdefabcdef1234567890abcdefab"
        ));
        assert!(!is_valid_eth_address(
            "0x1234567890abcdefABCDEF1234567890abcdefZZ"
        ));
    }
}
