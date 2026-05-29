//! Port of `marketplace-server/src/ports/utils.ts:getWhereStatementFromFilters`.
//!
//! The TS side uses `sql-template-strings` to build parameterised SQL with `$1`,
//! `$2`, etc. placeholders. The Rust side leaves placeholders as caller-managed
//! and only stitches the WHERE clause text together. Bind parameter ordering is
//! the responsibility of the query builder that calls into us.
//!
//! Helper kept tiny on purpose — every individual `FILTER_BY_*` constant the TS
//! source emits is materialised by the per-port builders (`ports::nfts`,
//! `ports::items`) directly into a `Vec<String>` of fragments.

/// Join a sequence of pre-formatted SQL fragments with ` AND `, prefixed with
/// ` WHERE `. Returns an empty string when no fragments are present so the
/// builder can blindly concatenate it.
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
