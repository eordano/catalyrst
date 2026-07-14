use std::sync::LazyLock;

use crate::sanitize::MAX_SANITIZE_PASSES;

use super::content_quality::build_content_quality_condition;
use super::rows::{PlaceListFilters, PlaceOrderBy};

// Strips markup for as many passes as src/sanitize.rs, then drops every angle
// bracket that survived them: the sanitizer's fail-closed end state, applied
// unconditionally because a text match has no use for a benign bracket either.
// A word nested deeper than the pass cap survives here as bare text exactly as
// it survives into a served description, so match and render agree.
// Mirrored by both description_plain columns in
// migrations/0004_place_plain_text.sql and pinned by the tests below; change
// the two together. Inline rather than a reference to that column because the
// archive's writers live outside this crate, so a query assuming 0004 had
// been applied would fail on a stack still on the earlier migrations.
pub(super) fn description_plain_sql() -> &'static str {
    static SQL: LazyLock<String> = LazyLock::new(|| {
        let mut stripped = "coalesce(description, '')".to_string();
        for _ in 0..MAX_SANITIZE_PASSES {
            stripped = format!("regexp_replace({stripped}, '</?[a-zA-Z][^>]*>', '', 'g')");
        }
        format!(
            "CASE WHEN strpos(coalesce(description, ''), '<') = 0 \
             AND strpos(coalesce(description, ''), '>') = 0 \
             THEN coalesce(description, '') \
             ELSE regexp_replace({stripped}, '[<>]', '', 'g') END"
        )
    });
    &SQL
}

pub(super) const SHOW_IN_PLACES_CLAUSE: &str =
    "(world IS FALSE OR COALESCE((raw->>'show_in_places')::bool, true) IS TRUE)";

#[derive(Debug)]
pub(super) enum Bind {
    Text(String),
    TextArray(Vec<String>),
    Int(i32),
}

pub(super) fn build_where(f: &PlaceListFilters, road_positions: bool) -> (String, Vec<Bind>) {
    let mut clauses: Vec<String> = vec!["disabled IS FALSE".to_string()];
    let mut binds: Vec<Bind> = Vec::new();
    let mut idx = 1;

    if !f.ids.is_empty() {
        clauses.push(format!("id = ANY(${})", idx));
        binds.push(Bind::TextArray(f.ids.clone()));
        idx += 1;
    }
    if f.only_worlds {
        clauses.push("world IS TRUE".to_string());
    } else if f.only_places {
        clauses.push("world IS FALSE".to_string());
    }
    // A world whose owner opted out of the places directory is not listed:
    // every upstream world query starts from `show_in_places IS true`.
    clauses.push(SHOW_IN_PLACES_CLAUSE.to_string());
    if f.only_highlighted {
        clauses.push("highlighted = TRUE".to_string());
    }
    let mut positions: Vec<String> = f.positions.clone();
    positions.extend(f.operated_positions.iter().cloned());
    let mut positions_clause = None;
    if !positions.is_empty() {
        positions_clause = Some(format!("raw->'positions' ?| ${}::text[]", idx));
        binds.push(Bind::TextArray(positions));
        idx += 1;
    } else if f.owner_filtered {
        clauses.push("FALSE".to_string());
    }
    let mut names_clause = None;
    if !f.names.is_empty() {
        names_clause = Some(format!("lower(world_name) = ANY(${})", idx));
        binds.push(Bind::TextArray(
            f.names.iter().map(|n| n.to_lowercase()).collect(),
        ));
        idx += 1;
    }
    // Parcels select places and names select worlds; asked for together they
    // are two selectors over one feed, so each row needs only its own.
    match (positions_clause, names_clause) {
        (Some(p), Some(n)) => clauses.push(format!("({p} OR {n})")),
        (Some(p), None) => clauses.push(p),
        (None, Some(n)) => clauses.push(n),
        (None, None) => {}
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
    // A world is identified by its name, not its scene title: searching
    // "flagtag" must reach flagtag.dcl.eth even though the scene is titled
    // "Flag Tag". Title/description matching alone silently misses every world.
    // Descriptions match with their markup stripped: the client renders none of
    // it, so a word that only ever appears inside a tag must not be findable.
    if let Some(s) = &f.search {
        clauses.push(format!(
            "(to_tsvector('english', coalesce(title,'') || ' ' || ({plain})) @@ plainto_tsquery('english', ${0}) \
             OR title ILIKE ${1} OR ({plain}) ILIKE ${1} OR world_name ILIKE ${1})",
            idx,
            idx + 1,
            plain = description_plain_sql(),
        ));
        binds.push(Bind::Text(s.clone()));
        binds.push(Bind::Text(format!("%{}%", s)));
        idx += 2;
    }
    if let Some((clause, gate_binds)) = build_content_quality_condition(f, road_positions, idx) {
        clauses.push(clause);
        binds.extend(gate_binds);
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
        format!("COALESCE(CASE lower(world_name){whens} ELSE 0 END, 0)")
    };

    let expr = format!("(CASE WHEN world THEN {worlds_case} ELSE {places_case} END)::int DESC, ");
    (expr, binds)
}

pub(super) fn destinations_highlighted_prefix(f: &PlaceListFilters) -> &'static str {
    if f.destinations_mode {
        "highlighted DESC, "
    } else {
        ""
    }
}

pub(super) fn destinations_ranking_prefix(f: &PlaceListFilters) -> &'static str {
    if f.destinations_mode {
        "NULLIF(raw->>'ranking','')::float8 DESC NULLS LAST, "
    } else {
        ""
    }
}

// Live users sort between the curation flag and the curated ranking: presence
// reshuffles the featured shelf but never lifts an uncurated scene over it.
pub(super) fn build_order_by(
    highlighted_prefix: &str,
    live_prefix: &str,
    ranking_prefix: &str,
    rank_prefix: &str,
    order_column: &str,
    dir: &str,
) -> String {
    format!(
        "{highlighted_prefix}{live_prefix}{ranking_prefix}{rank_prefix}{order_column} {dir} NULLS LAST, deployed_at DESC"
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

#[cfg(test)]
mod tests {
    use super::*;

    fn squeezed(sql: &str) -> String {
        sql.split_whitespace().collect::<Vec<_>>().join(" ")
    }

    #[test]
    fn description_plain_sql_strips_once_per_sanitizer_pass_then_fails_closed() {
        let sql = description_plain_sql();
        assert_eq!(
            sql.matches("'</?[a-zA-Z][^>]*>'").count(),
            MAX_SANITIZE_PASSES,
            "one strip pass per sanitizer pass: {sql}"
        );
        assert!(
            sql.ends_with("'[<>]', '', 'g') END"),
            "the passes must be followed by a fail-closed bracket strip: {sql}"
        );
    }

    #[test]
    fn description_plain_sql_is_mirrored_by_both_generated_columns() {
        let migration = squeezed(include_str!(
            "../../../migrations/0004_place_plain_text.sql"
        ));
        let expr = squeezed(description_plain_sql());
        assert_eq!(
            migration.matches(expr.as_str()).count(),
            2,
            "0004 must generate place and place_world_local with {expr}"
        );
    }

    #[test]
    fn content_gate_binds_follow_the_search_binds() {
        let f = PlaceListFilters {
            search: Some("garden".to_string()),
            destinations_mode: true,
            require_content_places: true,
            require_content_worlds: true,
            ..Default::default()
        };
        let (clause, binds) = build_where(&f, true);
        assert_eq!(
            binds.len(),
            7,
            "search takes $1/$2, the gate $3..$7: {clause}"
        );
        assert!(clause.contains("LIKE ANY ($3::text[])"), "{clause}");
        assert!(clause.contains("title !~ $4"), "{clause}");
        assert!(clause.contains("<> ALL ($6::text[])"), "{clause}");
        assert!(clause.contains("<> $7)"), "{clause}");
        assert!(clause.contains("FROM road_positions rp"), "{clause}");
    }

    #[test]
    fn content_gate_is_absent_from_lookup_and_places_queries() {
        let f = PlaceListFilters {
            destinations_mode: true,
            ..Default::default()
        };
        let (clause, binds) = build_where(&f, true);
        assert!(!clause.contains("highlighted IS TRUE"), "{clause}");
        assert!(binds.is_empty());
        let (clause, _) = build_where(&PlaceListFilters::default(), true);
        assert!(!clause.contains("highlighted IS TRUE"), "{clause}");
    }

    #[test]
    fn every_list_hides_worlds_that_opted_out_of_the_directory() {
        for f in [
            PlaceListFilters::default(),
            PlaceListFilters {
                only_worlds: true,
                ..Default::default()
            },
            PlaceListFilters {
                ids: vec!["world-row".to_string()],
                destinations_mode: true,
                ..Default::default()
            },
        ] {
            let (clause, _) = build_where(&f, true);
            assert!(clause.contains(SHOW_IN_PLACES_CLAUSE), "{clause}");
        }
        assert!(
            SHOW_IN_PLACES_CLAUSE.starts_with("(world IS FALSE OR"),
            "a place row must never be gated by a world-only flag"
        );
    }

    #[test]
    fn both_search_legs_read_the_stripped_text() {
        let (clause, _) = build_where(
            &PlaceListFilters {
                search: Some("garden".to_string()),
                ..Default::default()
            },
            false,
        );
        assert_eq!(
            clause.matches(description_plain_sql()).count(),
            2,
            "both the tsvector and the ILIKE leg must read the stripped text: {clause}"
        );
        assert!(
            !clause.contains("|| coalesce(description,'')"),
            "no search leg may read the raw description: {clause}"
        );
    }
}
