//! Search predicates shared by every feed (marketplace-server 8a46f36,
//! dfc17f9, d0daa22).
//!
//! Collectibles match WORD BY WORD: a term is trigram-similar (`%`, pg_trgm's
//! similarity_threshold) to some word of the item's name or of its
//! collection's name. A substring over the whole name required the words in
//! the typed order ("hat pirate" found nothing named "pirate hat") and never
//! saw the collection, which is where brand and collaboration names live;
//! whole-string similarity over `search_text` scored a short term against the
//! description too and made items with rich descriptions unsearchable by their
//! own name. Multi-word input compares each word against the WHOLE phrase, the
//! rule /v2/catalog has always applied, so every feed agrees on what a query
//! matches. Upstream serves the words from a pre-split table; the words here
//! are split per row instead, which keeps the semantics and needs no rebuild
//! pipeline. Tags are not matched: no tag source is wired into this schema.
//!
//! LAND matches its `search_text` (coordinates, name, description) on the
//! whole string OR on its best run of words (`<%`): the whole-string measure is
//! the forgiving one for multi-word input, the word measure is what catches a
//! term buried in a long description, and OR-ing them loses nothing either
//! side found.

use crate::MARKETPLACE_SQUID_SCHEMA;

pub fn escape_like(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for c in value.chars() {
        if matches!(c, '\\' | '%' | '_') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

fn words_match(source_expr: &str, term_param: &str) -> String {
    format!(
        "EXISTS (SELECT 1 FROM unnest(string_to_array({source_expr}, ' ')) AS search_word(text) \
         WHERE search_word.text <> '' AND lower(search_word.text) % lower({term_param}))"
    )
}

fn collection_words_match(collection_id_expr: &str, term_param: &str) -> String {
    format!(
        "EXISTS (SELECT 1 FROM {schema}.collection AS search_collection \
         CROSS JOIN LATERAL unnest(string_to_array(search_collection.name, ' ')) AS search_word(text) \
         WHERE search_collection.id = {collection_id_expr} \
         AND search_word.text <> '' AND lower(search_word.text) % lower({term_param}))",
        schema = MARKETPLACE_SQUID_SCHEMA,
    )
}

/// Does the item behind a row match the term? `name_expr` names the item's
/// name and `collection_id_expr` its collection, both in the caller's own
/// aliases; `term_param` is the placeholder bound to the raw term.
pub fn item_search_where(name_expr: &str, collection_id_expr: &str, term_param: &str) -> String {
    format!(
        "({} OR {})",
        words_match(name_expr, term_param),
        collection_words_match(collection_id_expr, term_param)
    )
}

/// `item_search_where` for feeds that also carry rows that are not
/// collection items (LAND, estates, names). The word arms are keyed on the
/// row having an item, as upstream keys its word-table lookups on the item
/// id: a row with none has nothing to look up, so it matches only through the
/// substring on `non_item_name_expr` via `like_param`, a placeholder bound to
/// `%escape_like(term)%`.
pub fn shop_search_where(
    item_id_expr: &str,
    name_expr: &str,
    collection_id_expr: &str,
    non_item_name_expr: &str,
    term_param: &str,
    like_param: &str,
) -> String {
    format!(
        "(({item_id_expr} IS NOT NULL AND ({} OR {})) \
         OR ({item_id_expr} IS NULL AND {non_item_name_expr} ILIKE {like_param}))",
        words_match(name_expr, term_param),
        collection_words_match(collection_id_expr, term_param),
    )
}

pub fn land_search_where(column: &str, term_param: &str) -> String {
    format!("({column} % {term_param} OR {term_param} <% {column})")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escape_like_neutralizes_metacharacters() {
        assert_eq!(escape_like("50%_off\\"), "50\\%\\_off\\\\");
        assert_eq!(escape_like("plain"), "plain");
    }

    #[test]
    fn item_search_matches_name_words_or_collection_words() {
        let sql = item_search_where("COALESCE(w.name, e.name)", "item.collection_id", "$2");
        assert!(sql.starts_with(
            "(EXISTS (SELECT 1 FROM unnest(string_to_array(COALESCE(w.name, e.name), ' '))"
        ));
        assert!(sql.contains("lower(search_word.text) % lower($2)"));
        assert!(sql.contains(&format!(
            "FROM {MARKETPLACE_SQUID_SCHEMA}.collection AS search_collection"
        )));
        assert!(sql.contains("WHERE search_collection.id = item.collection_id"));
        assert_eq!(sql.matches("% lower($2)").count(), 2);
        assert!(!sql.contains("ILIKE"));
        assert!(!sql.contains("search_text"));
    }

    #[test]
    fn shop_search_keeps_a_substring_fallback_for_rows_without_an_item() {
        let sql = shop_search_where(
            "COALESCE(item_p.id, item_s.id)",
            "COALESCE(nft.name, w_p.name, e_p.name)",
            "COALESCE(item_p.collection_id, item_s.collection_id)",
            "nft.name",
            "$4",
            "$5",
        );
        assert!(sql.starts_with(
            "((COALESCE(item_p.id, item_s.id) IS NOT NULL AND (EXISTS (SELECT 1 FROM unnest("
        ));
        assert!(sql.contains("lower(search_word.text) % lower($4)"));
        assert!(sql.ends_with("OR (COALESCE(item_p.id, item_s.id) IS NULL AND nft.name ILIKE $5))"));
        assert_eq!(
            sql.matches("COALESCE(item_p.id, item_s.id) IS").count(),
            2,
            "one arm for rows with an item, one for rows without: {sql}"
        );
    }

    #[test]
    fn land_search_ors_whole_string_and_word_similarity() {
        assert_eq!(
            land_search_where("nft.search_text", "$3"),
            "(nft.search_text % $3 OR $3 <% nft.search_text)"
        );
    }
}
