use super::rows::{PlaceListFilters, PlaceOrderBy};

#[derive(Debug)]
pub(super) enum Bind {
    Text(String),
    TextArray(Vec<String>),
    Int(i32),
}

pub(super) fn build_where(f: &PlaceListFilters) -> (String, Vec<Bind>) {
    let mut clauses: Vec<String> = vec!["disabled IS FALSE".to_string()];
    let mut binds: Vec<Bind> = Vec::new();
    let mut idx = 1;

    if !f.ids.is_empty() {
        clauses.push(format!("id = ANY(${})", idx));
        binds.push(Bind::TextArray(f.ids.clone()));
        idx += 1;
    } else if f.only_worlds {
        clauses.push("COALESCE((raw->>'world')::bool, false) IS TRUE".to_string());
    } else if f.only_places {
        clauses.push("COALESCE((raw->>'world')::bool, false) IS FALSE".to_string());
    }
    if f.only_highlighted {
        clauses.push("highlighted = TRUE".to_string());
    }
    let mut positions: Vec<String> = f.positions.clone();
    positions.extend(f.operated_positions.iter().cloned());
    if !positions.is_empty() {
        clauses.push(format!("raw->'positions' ?| ${}::text[]", idx));
        binds.push(Bind::TextArray(positions));
        idx += 1;
    } else if f.owner_filtered {
        clauses.push("FALSE".to_string());
    }
    if !f.names.is_empty() {
        clauses.push(format!("lower(raw->>'world_name') = ANY(${})", idx));
        binds.push(Bind::TextArray(
            f.names.iter().map(|n| n.to_lowercase()).collect(),
        ));
        idx += 1;
    }
    if !f.categories.is_empty() {
        clauses.push(format!("categories && ${}", idx));
        binds.push(Bind::TextArray(f.categories.clone()));
        idx += 1;
    }
    if let Some(addr) = &f.creator_address {
        clauses.push(format!("LOWER(creator_address) = ${}", idx));
        binds.push(Bind::Text(addr.to_lowercase()));
        idx += 1;
    }
    if let Some(sdk) = &f.sdk {
        let null_clause = if sdk == "6" {
            " OR raw->>'sdk' IS NULL"
        } else {
            ""
        };
        clauses.push(format!(
            "(raw->>'sdk' = ${0} OR raw->>'sdk' LIKE ${1}{2})",
            idx,
            idx + 1,
            null_clause
        ));
        binds.push(Bind::Text(sdk.clone()));
        binds.push(Bind::Text(format!("{}.%", sdk)));
        idx += 2;
    }
    if let Some(s) = &f.search {
        clauses.push(format!(
            "(to_tsvector('english', coalesce(title,'') || ' ' || coalesce(description,'')) @@ plainto_tsquery('english', ${0}) \
             OR title ILIKE ${1} OR description ILIKE ${1})",
            idx,
            idx + 1,
        ));
        binds.push(Bind::Text(s.clone()));
        binds.push(Bind::Text(format!("%{}%", s)));
    }
    (clauses.join(" AND "), binds)
}

pub(super) fn build_live_user_count_order(
    f: &PlaceListFilters,
    start_idx: usize,
) -> (String, Vec<Bind>) {
    if !matches!(f.order_by, PlaceOrderBy::MostActive) {
        return (String::new(), Vec::new());
    }
    if f.place_user_counts.is_empty() && f.world_user_counts.is_empty() {
        return (String::new(), Vec::new());
    }
    let mut binds: Vec<Bind> = Vec::new();
    let mut idx = start_idx;

    let places_case = if f.place_user_counts.is_empty() {
        "0".to_string()
    } else {
        let mut whens = String::new();
        for (pos, count) in &f.place_user_counts {
            whens.push_str(&format!(" WHEN ${} THEN ${}", idx, idx + 1));
            binds.push(Bind::Text(pos.clone()));
            binds.push(Bind::Int(*count));
            idx += 2;
        }
        format!("COALESCE(CASE base_position{whens} ELSE 0 END, 0)")
    };

    let worlds_case = if f.world_user_counts.is_empty() {
        "0".to_string()
    } else {
        let mut whens = String::new();
        for (name, count) in &f.world_user_counts {
            whens.push_str(&format!(" WHEN ${} THEN ${}", idx, idx + 1));
            binds.push(Bind::Text(name.to_lowercase()));
            binds.push(Bind::Int(*count));
            idx += 2;
        }
        format!("COALESCE(CASE lower(raw->>'world_name'){whens} ELSE 0 END, 0)")
    };

    let expr = format!(
        "(CASE WHEN COALESCE((raw->>'world')::bool, false) THEN {worlds_case} ELSE {places_case} END)::int DESC, "
    );
    (expr, binds)
}

pub(super) fn destinations_order_prefix(f: &PlaceListFilters) -> &'static str {
    if f.destinations_mode {
        "highlighted DESC, NULLIF(raw->>'ranking','')::float8 DESC NULLS LAST, "
    } else {
        ""
    }
}

pub(super) fn build_order_by(
    dest_prefix: &str,
    live_prefix: &str,
    rank_prefix: &str,
    order_column: &str,
    dir: &str,
) -> String {
    format!(
        "{dest_prefix}{live_prefix}{rank_prefix}{order_column} {dir} NULLS LAST, deployed_at DESC"
    )
}

pub(super) fn bind_param<'a>(
    q: sqlx::query::Query<'a, sqlx::Postgres, sqlx::postgres::PgArguments>,
    b: &'a Bind,
) -> sqlx::query::Query<'a, sqlx::Postgres, sqlx::postgres::PgArguments> {
    match b {
        Bind::Text(s) => q.bind(s),
        Bind::TextArray(v) => q.bind(v),
        Bind::Int(n) => q.bind(*n),
    }
}
