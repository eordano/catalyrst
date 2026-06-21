pub const MAX_PAGE_LIMIT: i64 = 1000;

pub fn clamp_first(first: Option<i64>, default: i64) -> i64 {
    first.unwrap_or(default).clamp(0, MAX_PAGE_LIMIT)
}

pub fn clamp_skip(skip: Option<i64>) -> i64 {
    skip.unwrap_or(0).max(0)
}

pub fn where_from(filters: &[String]) -> String {
    let non_empty: Vec<&str> = filters
        .iter()
        .filter_map(|f| {
            let t = f.trim();
            if t.is_empty() {
                None
            } else {
                Some(t)
            }
        })
        .collect();
    if non_empty.is_empty() {
        String::new()
    } else {
        format!(" WHERE {} ", non_empty.join(" AND "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamp_first_caps_and_floors() {
        assert_eq!(clamp_first(None, 100), 100);
        assert_eq!(clamp_first(Some(2), 100), 2);
        assert_eq!(clamp_first(Some(0), 100), 0);
        assert_eq!(clamp_first(Some(-1), 100), 0);
        assert_eq!(clamp_first(Some(10_000_000), 100), MAX_PAGE_LIMIT);
        assert_eq!(clamp_first(Some(MAX_PAGE_LIMIT + 1), 100), MAX_PAGE_LIMIT);
    }

    #[test]
    fn clamp_skip_floors_negative() {
        assert_eq!(clamp_skip(None), 0);
        assert_eq!(clamp_skip(Some(5)), 5);
        assert_eq!(clamp_skip(Some(-1)), 0);
    }
}
