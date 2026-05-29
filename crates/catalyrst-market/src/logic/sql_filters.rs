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
