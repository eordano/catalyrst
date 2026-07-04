use sqlx::PgPool;

fn escape_like(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' | '%' | '_' => {
                out.push('\\');
                out.push(c);
            }
            _ => out.push(c),
        }
    }
    out
}

pub async fn get_item_entities_ids_matching_collection_urn_prefix(
    pool: &PgPool,
    collection_urn: &str,
) -> Result<Vec<String>, sqlx::Error> {
    let pattern = format!("{}%", escape_like(collection_urn));

    let rows: Vec<(String,)> = sqlx::query_as(
        "SELECT entity_id FROM active_pointers AS p WHERE p.pointer LIKE $1 ESCAPE '\\'",
    )
    .bind(&pattern)
    .fetch_all(pool)
    .await?;

    Ok(rows.into_iter().map(|r| r.0).collect())
}

pub async fn get_third_party_collection_items_entity_ids_matching_urn_prefix(
    pool: &PgPool,
    collection_urn: &str,
) -> Result<Vec<String>, sqlx::Error> {
    let pattern = format!("{}%", escape_like(collection_urn));

    let rows: Vec<(String,)> = sqlx::query_as(
        r#"
        SELECT entity_id
        FROM active_third_party_collection_items_deployments_with_content
        WHERE pointer LIKE $1 ESCAPE '\'
        "#,
    )
    .bind(&pattern)
    .fetch_all(pool)
    .await?;

    Ok(rows.into_iter().map(|r| r.0).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escape_like_passes_plain_text() {
        assert_eq!(
            escape_like("urn:decentraland:matic"),
            "urn:decentraland:matic"
        );
    }

    #[test]
    fn escape_like_escapes_percent_and_underscore() {
        assert_eq!(escape_like("a%b_c"), r"a\%b\_c");
    }

    #[test]
    fn escape_like_escapes_backslash() {
        assert_eq!(escape_like(r"a\b"), r"a\\b");
    }

    #[test]
    fn escape_like_combined() {
        assert_eq!(escape_like(r"x_%\y"), r"x\_\%\\y");
    }
}
