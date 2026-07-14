use serde::Serialize;
use sqlx::PgPool;
use sqlx::Row;

use crate::http::response::ApiError;
use crate::ports::catalog::PickStats;
use crate::MARKETPLACE_SQUID_SCHEMA;

#[derive(sqlx::FromRow)]
struct PickStatsRow {
    item_id: String,
    count: i64,
    picked_by_user: Option<bool>,
}

pub const DEFAULT_LIST_NAME: &str = "Favorites";

/// Upstream's globally shared default list (marketplace-server migration
/// `1678303321034_default-list`, renamed to "Wishlist" by `1687172729802`).
/// Seeded by our migration `0010_favorites_shared_default_list.sql`. The shop
/// frontend hardcodes this id for its signed-in favorites (shop c0cc5df).
pub const DEFAULT_LIST_ID: &str = "70ab6873-4a03-4eb2-b331-4b8be0e0b8af";

/// Upstream's `DEFAULT_LIST_USER_ADDRESS`: the zero address owning the shared
/// default Wishlist. `get_lists` surfaces that list for every caller
/// (upstream `WHERE l.user_address = $user OR l.user_address = $default`).
pub const DEFAULT_LIST_USER_ADDRESS: &str = "0x0000000000000000000000000000000000000000";

/// Upstream's `GRANTED_TO_ALL` ACL grantee wildcard.
pub const GRANTED_TO_ALL: &str = "*";

/// One favorited item inside a list, as returned by `GET /v1/lists/{id}/picks`.
#[derive(Debug)]
pub struct ListPick {
    pub item_id: String,
    /// Epoch millis, matching upstream's `Number(pick.created_at)`.
    pub created_at: i64,
}

#[derive(Debug, Serialize)]
pub struct FavoriteList {
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(rename = "userAddress")]
    pub user_address: String,

    #[serde(rename = "createdAt")]
    pub created_at: i64,
    #[serde(rename = "updatedAt", skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<i64>,
    #[serde(rename = "isPrivate")]
    pub is_private: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permission: Option<String>,
    /// True for the globally shared default Wishlist (owned by the zero
    /// address). Upstream projects this for its `ORDER BY is_default_list
    /// DESC` contract; we also surface it on the wire.
    #[serde(rename = "isDefaultList")]
    pub is_default_list: bool,
    #[serde(rename = "itemsCount")]
    pub items_count: i64,
    #[serde(rename = "previewOfItemIds")]
    pub preview_of_item_ids: Vec<String>,
    #[serde(rename = "isItemInList", skip_serializing_if = "Option::is_none")]
    pub is_item_in_list: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListSortBy {
    CreatedAt,
    Name,
    UpdatedAt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListSortDirection {
    Asc,
    Desc,
}

pub struct GetListsOptions<'a> {
    pub limit: i64,
    pub offset: i64,
    pub sort_by: ListSortBy,
    pub sort_direction: ListSortDirection,
    pub item_id: Option<&'a str>,
    pub q: Option<&'a str>,
}

pub struct ListsComponent {
    pool: PgPool,
    write: Option<PgPool>,
}

fn is_missing_favorites(e: &sqlx::Error) -> bool {
    if let sqlx::Error::Database(db) = e {
        matches!(
            db.code().as_deref(),
            Some("42P01") | Some("42501") | Some("3F000")
        )
    } else {
        false
    }
}

/// `GET /v1/lists/{id}/picks` page query. Mirrors upstream's
/// `SELECT DISTINCT(p.item_id), p.*, COUNT(*) OVER() as picks_count`: because
/// upstream selects `p.*`, its DISTINCT is row-level, so the same item picked
/// by two users in a shared list yields TWO rows. Selecting the full picks
/// identity (item_id, user_address, list_id -- the primary key) reproduces
/// that; deduping on item_id + ms-truncated created_at alone would collapse
/// same-millisecond picks by different users while the COUNT window still
/// counted both. The DISTINCT collapses only the ACL join fan-out, and the
/// COUNT window carries the (pre-DISTINCT, as upstream) total.
pub(crate) const PICKS_BY_LIST_SQL: &str = "SELECT DISTINCT p.item_id, p.user_address, p.list_id, \
       (EXTRACT(EPOCH FROM p.created_at) * 1000)::int8 AS created_at, \
       COUNT(*) OVER() AS picks_count \
     FROM favorites.picks p \
     LEFT JOIN favorites.acl ON p.list_id = favorites.acl.list_id \
     WHERE p.list_id = $1::uuid \
       AND (p.user_address = $2 OR favorites.acl.grantee = $2 OR favorites.acl.grantee = $3) \
     ORDER BY created_at DESC \
     LIMIT $4 OFFSET $5";

pub fn is_uuid(s: &str) -> bool {
    let b = s.as_bytes();
    if b.len() != 36 {
        return false;
    }
    for (i, c) in b.iter().enumerate() {
        match i {
            8 | 13 | 18 | 23 => {
                if *c != b'-' {
                    return false;
                }
            }
            _ => {
                if !c.is_ascii_hexdigit() {
                    return false;
                }
            }
        }
    }
    true
}

/// Builds the `GET /v1/lists` page + count SQL. Ported invariants (upstream
/// `getLists`, marketplace-server src/ports/favorites/lists/component.ts:110-149):
///
/// * Every picks read is scoped to the CALLER -- upstream joins
///   `LEFT JOIN favorites.picks p ON l.id = p.list_id AND p.user_address =
///   ${userAddress}`, so `items_count`, the preview and `is_item_in_list`
///   only ever reflect the caller's own picks. Any other user may legitimately
///   hold picks in a visible list (the shared default Wishlist, or any
///   ACL-less list -- `check_non_editable_lists` lets those through), and
///   those picks must never leak into another caller's counts or thumbnails.
/// * The globally shared default Wishlist (owned by the zero address) is
///   always visible: `WHERE l.user_address = $1 OR l.user_address =
///   $default`, projected as `is_default_list` and sorted first
///   (`ORDER BY is_default_list DESC` ahead of the requested sort).
/// * The preview is the caller's 4 OLDEST picks, ascending -- upstream's
///   `(ARRAY_REMOVE(ARRAY_AGG(p.item_id ORDER BY p.created_at), NULL))[:4]`
///   aggregates every pick oldest-first and slices the head. Ours reproduces
///   that with the same slice over a caller-scoped aggregate (no
///   `ARRAY_REMOVE` needed: the subquery reads `favorites.picks` directly, so
///   no LEFT-JOIN NULL rows exist to strip).
/// * `is_private` is DERIVED from the ACL, not read from the stored
///   `l.is_private` column -- upstream's `(SELECT COUNT(1) FROM favorites.acl
///   WHERE list_id = l.id AND (grantee = $user OR grantee = '*')) = 0`
///   (component.ts:114): a list is private TO THE CALLER unless an ACL row
///   grants them (or everyone) access. The stored column is write-path
///   metadata only. Consequence: the seeded shared Wishlist (stored
///   `is_private = false`, zero ACL rows) reads `isPrivate: true`, exactly as
///   upstream serves it.
///
/// Accepted divergence: upstream appends its `q` filter without parentheses
/// (`a OR b AND ilike` -- the name filter binds only to the default-list arm,
/// an operator-precedence artifact); we parenthesize the ownership arms so
/// `q` filters both, per the reviewed fix spec. Beyond precedence, upstream's
/// `q` filter is entirely INERT: sql-template-strings interpolates the
/// placeholder INSIDE the string literal (`l.name ILIKE '%$7%'` -- upstream's
/// own test at test/unit/lists-component.spec.ts:475 asserts that literal),
/// so the bind never happens and upstream's `q` matches nothing -- do not
/// "restore parity" onto that broken behavior.
///
/// Binds: `$1` = caller's lowercased address, then optionally q, then
/// optionally itemId, then limit, offset (count SQL: `$1` + optional q only).
pub(crate) fn build_get_lists_sql(opts: &GetListsOptions<'_>) -> (String, String) {
    let mut next_param = 2;
    let mut take = || {
        let i = next_param;
        next_param += 1;
        i
    };
    let q_idx = opts.q.map(|_| take());
    let item_idx = opts.item_id.map(|_| take());
    let limit_idx = take();
    let offset_idx = take();

    let q_clause = q_idx
        .map(|i| format!("AND l.name ILIKE '%' || ${i} || '%'"))
        .unwrap_or_default();
    let item_select = item_idx
        .map(|i| {
            format!(
                ",\n  EXISTS(SELECT 1 FROM favorites.picks ip \
                 WHERE ip.list_id = l.id AND ip.item_id = ${i} \
                 AND ip.user_address = $1) AS is_item_in_list"
            )
        })
        .unwrap_or_default();

    let dir = match opts.sort_direction {
        ListSortDirection::Asc => "ASC",
        ListSortDirection::Desc => "DESC",
    };
    let order = match opts.sort_by {
        ListSortBy::CreatedAt => format!("l.created_at {dir}"),
        ListSortBy::UpdatedAt => format!("l.updated_at {dir} NULLS LAST"),
        ListSortBy::Name => format!("l.name {dir}"),
    };

    let sql = format!(
        r#"
SELECT
  l.id::text                                   AS id,
  l.name                                       AS name,
  l.description                                AS description,
  l.user_address                               AS user_address,
  (EXTRACT(EPOCH FROM l.created_at) * 1000)::int8 AS created_at,
  (EXTRACT(EPOCH FROM l.updated_at) * 1000)::int8 AS updated_at,
  (SELECT COUNT(1) FROM favorites.acl
     WHERE favorites.acl.list_id = l.id
       AND (favorites.acl.grantee = $1 OR favorites.acl.grantee = '{granted_to_all}')) = 0
                                               AS is_private,
  l.permission                                 AS permission,
  (l.user_address = '{default_addr}')          AS is_default_list,
  COALESCE(pc.cnt, 0)::int8                    AS items_count,
  COALESCE(pp.preview, ARRAY[]::text[])        AS preview{item_select}
FROM favorites.lists l
LEFT JOIN (
  SELECT list_id, COUNT(*) AS cnt FROM favorites.picks
  WHERE user_address = $1 GROUP BY list_id
) pc ON pc.list_id = l.id
LEFT JOIN (
  SELECT list_id, (ARRAY_AGG(item_id ORDER BY created_at ASC))[:4] AS preview
  FROM favorites.picks
  WHERE user_address = $1
  GROUP BY list_id
) pp ON pp.list_id = l.id
WHERE (l.user_address = $1 OR l.user_address = '{default_addr}') {q_clause}
ORDER BY is_default_list DESC, {order}, l.id ASC
LIMIT ${limit_idx} OFFSET ${offset_idx}
"#,
        default_addr = DEFAULT_LIST_USER_ADDRESS,
        granted_to_all = GRANTED_TO_ALL,
    );

    let count_sql = format!(
        "SELECT COUNT(*)::int8 AS total FROM favorites.lists l \
         WHERE (l.user_address = $1 OR l.user_address = '{default_addr}') {q_clause}",
        default_addr = DEFAULT_LIST_USER_ADDRESS,
    );

    (sql, count_sql)
}

impl ListsComponent {
    pub fn new(pool: PgPool) -> Self {
        Self { pool, write: None }
    }

    pub fn with_write(mut self, pool: PgPool) -> Self {
        self.write = Some(pool);
        self
    }

    fn write_pool(&self) -> &PgPool {
        self.write.as_ref().unwrap_or(&self.pool)
    }

    async fn notify_dirty(&self) {
        match sqlx::query("SELECT pg_notify('catalyrst_market_dirty', 'favorites')")
            .execute(self.write_pool())
            .await
        {
            Ok(_) => tracing::debug!("favorites dirty notify sent"),
            Err(err) => tracing::warn!(
                %err,
                "favorites dirty notify failed (stale reads bounded by cache TTL)"
            ),
        }
    }

    pub async fn item_exists(&self, item_id: &str) -> Result<bool, ApiError> {
        let sql = format!(
            "SELECT EXISTS(SELECT 1 FROM {schema}.item WHERE id = $1 OR (collection_id || '-' || blockchain_id::text) = $1) AS found",
            schema = MARKETPLACE_SQUID_SCHEMA,
        );
        let row = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(item_id)
            .fetch_one(&self.pool)
            .await?;
        Ok(row.try_get::<bool, _>("found").unwrap_or(false))
    }

    /// Upstream `checkNonEditableLists`: a list is editable when the caller
    /// owns it, or when its ACL grants `edit` to the caller or to everyone.
    /// Faithfully ported LEFT JOIN semantics included: a list the caller does
    /// NOT own but that has no ACL rows at all yields NULL for the ACL arm and
    /// is therefore NOT flagged -- this is exactly what lets every user pick
    /// into the globally shared default Wishlist (owned by 0x0), which the
    /// shop's server-side favorites depend on. Nonexistent ids are not flagged
    /// either (the later INSERT ... SELECT simply skips them), matching
    /// upstream's silent no-op.
    pub async fn check_non_editable_lists(
        &self,
        list_ids: &[String],
        user_address: &str,
    ) -> Result<Vec<String>, ApiError> {
        if list_ids.is_empty() {
            return Ok(Vec::new());
        }
        let rows = sqlx::query(sqlx::AssertSqlSafe(
            "SELECT favorites.lists.id::text AS id FROM favorites.lists \
             LEFT JOIN favorites.acl ON favorites.lists.id = favorites.acl.list_id \
             WHERE favorites.lists.id = ANY($1::uuid[]) AND favorites.lists.user_address != $2 \
             AND (favorites.acl.permission != 'edit' OR favorites.acl.grantee NOT IN ($2, $3))"
                .to_string(),
        ))
        .bind(list_ids)
        .bind(user_address.to_lowercase())
        .bind(GRANTED_TO_ALL)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .iter()
            .filter_map(|r| r.try_get::<String, _>("id").ok())
            .collect())
    }

    /// Upstream `getPicksByListId`: the item ids favorited within one list,
    /// newest first, plus the pre-pagination total. Visibility per row: the
    /// caller's own picks, or any picks when the list's ACL grants the caller
    /// (or everyone) access. Anonymous callers only see ACL-public lists.
    pub async fn get_picks_by_list_id(
        &self,
        list_id: &str,
        user_address: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<(Vec<ListPick>, i64), ApiError> {
        let user = user_address.map(|u| u.to_lowercase());
        let rows = match sqlx::query(sqlx::AssertSqlSafe(PICKS_BY_LIST_SQL.to_string()))
            .bind(list_id)
            .bind(user)
            .bind(GRANTED_TO_ALL)
            .bind(limit)
            .bind(offset)
            .fetch_all(&self.pool)
            .await
        {
            Ok(rows) => rows,
            Err(e) if is_missing_favorites(&e) => return Ok((Vec::new(), 0)),
            Err(e) => return Err(e.into()),
        };

        let total = rows
            .first()
            .and_then(|r| r.try_get::<i64, _>("picks_count").ok())
            .unwrap_or(0);
        let picks = rows
            .iter()
            .map(|r| ListPick {
                item_id: r.try_get("item_id").unwrap_or_default(),
                created_at: r.try_get::<i64, _>("created_at").unwrap_or(0),
            })
            .collect();
        Ok((picks, total))
    }

    pub async fn get_picks_stats(
        &self,
        item_ids: &[String],
        user_address: Option<&str>,
    ) -> Result<Vec<PickStats>, ApiError> {
        if item_ids.is_empty() {
            return Ok(Vec::new());
        }
        let user = user_address.map(|u| u.to_lowercase());
        let rows: Vec<PickStatsRow> = match sqlx::query_as(
            "SELECT items_to_find.item_id AS item_id, \
                    COUNT(DISTINCT p.user_address)::int8 AS count, \
                    CASE WHEN $2::text IS NULL THEN CAST(NULL AS boolean) \
                         ELSE COALESCE(BOOL_OR(p.user_address = $2), false) END AS picked_by_user \
             FROM (SELECT unnest($1::text[]) AS item_id) items_to_find \
             LEFT JOIN favorites.picks p ON p.item_id = items_to_find.item_id \
             GROUP BY items_to_find.item_id",
        )
        .bind(item_ids)
        .bind(user.as_deref())
        .fetch_all(&self.pool)
        .await
        {
            Ok(rows) => rows,
            Err(e) if is_missing_favorites(&e) => return Ok(Vec::new()),
            Err(e) => return Err(e.into()),
        };
        Ok(rows
            .into_iter()
            .map(|r| PickStats {
                count: r.count,
                item_id: r.item_id,
                picked_by_user: r.picked_by_user,
            })
            .collect())
    }

    pub async fn get_or_create_default_list(&self, user_address: &str) -> Result<String, ApiError> {
        let user = user_address.to_lowercase();
        let existing = sqlx::query(sqlx::AssertSqlSafe(
            "SELECT id::text AS id FROM favorites.lists \
             WHERE user_address = $1 AND name = $2 \
             ORDER BY created_at ASC LIMIT 1"
                .to_string(),
        ))
        .bind(&user)
        .bind(DEFAULT_LIST_NAME)
        .fetch_optional(&self.pool)
        .await?;
        if let Some(row) = existing {
            return Ok(row.try_get::<String, _>("id").unwrap_or_default());
        }
        let row = sqlx::query(sqlx::AssertSqlSafe(
            "INSERT INTO favorites.lists (name, user_address, is_private) \
             VALUES ($1, $2, true) RETURNING id::text AS id"
                .to_string(),
        ))
        .bind(DEFAULT_LIST_NAME)
        .bind(&user)
        .fetch_one(self.write_pool())
        .await?;
        Ok(row.try_get::<String, _>("id").unwrap_or_default())
    }

    pub async fn pick_in_lists(
        &self,
        item_id: &str,
        user_address: &str,
        list_ids: &[String],
    ) -> Result<(), ApiError> {
        if list_ids.is_empty() {
            return Ok(());
        }
        // Upstream inserts into ANY existing list in the (already
        // authorization-checked) set -- deliberately NOT owner-filtered, so a
        // user can pick into the shared default Wishlist they do not own.
        // Nonexistent ids silently drop out of the SELECT, as upstream.
        // The conflict target is the upstream picks identity
        // (item_id, user_address, list_id): two users favoriting the same
        // item in a shared list are two distinct picks.
        sqlx::query(sqlx::AssertSqlSafe(
            "INSERT INTO favorites.picks (item_id, user_address, list_id) \
             SELECT $1, $2, id FROM favorites.lists \
             WHERE id = ANY($3::uuid[]) \
             ON CONFLICT (item_id, user_address, list_id) DO NOTHING"
                .to_string(),
        ))
        .bind(item_id)
        .bind(user_address.to_lowercase())
        .bind(list_ids)
        .execute(self.write_pool())
        .await?;
        self.notify_dirty().await;
        Ok(())
    }

    pub async fn unpick_from_lists(
        &self,
        item_id: &str,
        user_address: &str,
        list_ids: &[String],
    ) -> Result<(), ApiError> {
        if list_ids.is_empty() {
            return Ok(());
        }
        sqlx::query(sqlx::AssertSqlSafe(
            "DELETE FROM favorites.picks \
             WHERE item_id = $1 AND user_address = $2 AND list_id = ANY($3::uuid[])"
                .to_string(),
        ))
        .bind(item_id)
        .bind(user_address.to_lowercase())
        .bind(list_ids)
        .execute(self.write_pool())
        .await?;
        self.notify_dirty().await;
        Ok(())
    }

    pub async fn unpick_everywhere(
        &self,
        item_id: &str,
        user_address: &str,
    ) -> Result<u64, ApiError> {
        let res = sqlx::query(sqlx::AssertSqlSafe(
            "DELETE FROM favorites.picks WHERE item_id = $1 AND user_address = $2".to_string(),
        ))
        .bind(item_id)
        .bind(user_address.to_lowercase())
        .execute(self.write_pool())
        .await?;
        self.notify_dirty().await;
        Ok(res.rows_affected())
    }

    pub async fn is_picked_by_user(
        &self,
        item_id: &str,
        user_address: &str,
    ) -> Result<bool, ApiError> {
        let row = sqlx::query(sqlx::AssertSqlSafe(
            "SELECT EXISTS(SELECT 1 FROM favorites.picks \
             WHERE item_id = $1 AND user_address = $2) AS found"
                .to_string(),
        ))
        .bind(item_id)
        .bind(user_address.to_lowercase())
        .fetch_one(&self.pool)
        .await?;
        Ok(row.try_get::<bool, _>("found").unwrap_or(false))
    }

    pub async fn get_lists(
        &self,
        user_address: &str,
        opts: &GetListsOptions<'_>,
    ) -> Result<(Vec<FavoriteList>, i64), ApiError> {
        let item_idx = opts.item_id.is_some();
        let (sql, count_sql) = build_get_lists_sql(opts);

        let mut q = sqlx::query(sqlx::AssertSqlSafe(sql));
        q = q.bind(user_address.to_lowercase());
        if let Some(needle) = opts.q {
            q = q.bind(needle.to_string());
        }
        if let Some(item) = opts.item_id {
            q = q.bind(item.to_string());
        }
        q = q.bind(opts.limit).bind(opts.offset);

        let rows = match q.fetch_all(&self.pool).await {
            Ok(rows) => rows,
            Err(e) if is_missing_favorites(&e) => return Ok((Vec::new(), 0)),
            Err(e) => return Err(e.into()),
        };

        let lists: Vec<FavoriteList> = rows
            .iter()
            .map(|r| FavoriteList {
                id: r.try_get("id").unwrap_or_default(),
                name: r.try_get("name").unwrap_or_default(),
                description: r
                    .try_get::<Option<String>, _>("description")
                    .unwrap_or(None),
                user_address: r.try_get("user_address").unwrap_or_default(),
                created_at: r.try_get::<i64, _>("created_at").unwrap_or(0),
                updated_at: r.try_get::<Option<i64>, _>("updated_at").unwrap_or(None),
                is_private: r.try_get::<bool, _>("is_private").unwrap_or(false),
                permission: r.try_get::<Option<String>, _>("permission").unwrap_or(None),
                is_default_list: r.try_get::<bool, _>("is_default_list").unwrap_or(false),
                items_count: r.try_get::<i64, _>("items_count").unwrap_or(0),
                preview_of_item_ids: r.try_get::<Vec<String>, _>("preview").unwrap_or_default(),
                is_item_in_list: if item_idx {
                    r.try_get::<bool, _>("is_item_in_list").ok()
                } else {
                    None
                },
            })
            .collect();

        let mut cq = sqlx::query(sqlx::AssertSqlSafe(count_sql));
        cq = cq.bind(user_address.to_lowercase());
        if let Some(needle) = opts.q {
            cq = cq.bind(needle.to_string());
        }
        let total = match cq.fetch_one(&self.pool).await {
            Ok(row) => row.try_get::<i64, _>("total").unwrap_or(0),
            Err(e) if is_missing_favorites(&e) => 0,
            Err(e) => return Err(e.into()),
        };

        Ok((lists, total))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notify_dirty_literal_matches_dirty_channel() {
        assert_eq!(
            crate::ports::catalog_cache::DIRTY_CHANNEL,
            "catalyrst_market_dirty"
        );
    }

    #[test]
    fn uuid_validator() {
        assert!(is_uuid("01337f44-b985-45be-a4f6-6a4efeb40412"));
        assert!(!is_uuid("01337f44b98545bea4f66a4efeb40412"));
        assert!(!is_uuid("zz337f44-b985-45be-a4f6-6a4efeb40412"));
    }

    fn opts<'a>(item_id: Option<&'a str>, q: Option<&'a str>) -> GetListsOptions<'a> {
        GetListsOptions {
            limit: 24,
            offset: 0,
            sort_by: ListSortBy::CreatedAt,
            sort_direction: ListSortDirection::Desc,
            item_id,
            q,
        }
    }

    /// The HIGH-severity ACL invariant: every picks read inside the lists
    /// query is scoped to the caller (`$1`), mirroring upstream's
    /// `LEFT JOIN favorites.picks p ON ... AND p.user_address = $user`.
    /// Without this, a foreign pick (inserted through the shared-Wishlist /
    /// ACL-less editability path) inflates another caller's itemsCount and
    /// leaks into their preview thumbnails.
    #[test]
    fn get_lists_sql_scopes_every_picks_read_to_the_caller() {
        let (sql, _) = build_get_lists_sql(&opts(Some("0xitem-1"), None));

        // (a) is_item_in_list EXISTS is caller-scoped.
        assert!(
            sql.contains("AND ip.user_address = $1) AS is_item_in_list"),
            "is_item_in_list EXISTS must filter on the caller:\n{sql}"
        );
        // (b) the items_count subquery only counts the caller's picks.
        assert!(
            sql.contains(
                "SELECT list_id, COUNT(*) AS cnt FROM favorites.picks\n  \
                 WHERE user_address = $1 GROUP BY list_id"
            ),
            "pc count subquery must filter on the caller:\n{sql}"
        );
        // (c) the preview subquery only aggregates the caller's picks.
        assert!(
            sql.contains(
                "FROM favorites.picks\n  \
                 WHERE user_address = $1\n  \
                 GROUP BY list_id\n) pp"
            ),
            "pp preview subquery must filter on the caller:\n{sql}"
        );
        // No unscoped read of favorites.picks remains.
        for (i, _) in sql.match_indices("favorites.picks") {
            let tail = &sql[i..];
            assert!(
                tail.contains("user_address = $1"),
                "unscoped favorites.picks read at byte {i}:\n{sql}"
            );
        }
    }

    /// The shared default Wishlist (owned by the zero address) is visible to
    /// every caller, projected as `is_default_list`, and sorted first --
    /// upstream's `WHERE l.user_address = $user OR l.user_address = $default`
    /// plus `ORDER BY is_default_list DESC`. The count query must agree with
    /// the page query about which rows exist.
    #[test]
    fn get_lists_sql_surfaces_the_shared_default_wishlist() {
        let (sql, count_sql) = build_get_lists_sql(&opts(None, None));

        let where_arm = format!(
            "WHERE (l.user_address = $1 OR l.user_address = '{DEFAULT_LIST_USER_ADDRESS}')"
        );
        assert!(
            sql.contains(&where_arm),
            "page WHERE missing default arm:\n{sql}"
        );
        assert!(
            count_sql.contains(&where_arm),
            "count WHERE missing default arm:\n{count_sql}"
        );
        assert!(
            sql.contains(&format!(
                "(l.user_address = '{DEFAULT_LIST_USER_ADDRESS}')          AS is_default_list"
            )),
            "is_default_list projection missing:\n{sql}"
        );
        assert!(
            sql.contains("ORDER BY is_default_list DESC, l.created_at DESC, l.id ASC"),
            "default list must sort ahead of the requested key:\n{sql}"
        );
    }

    /// Upstream previews a list with its 4 OLDEST picks, ascending --
    /// `(ARRAY_REMOVE(ARRAY_AGG(p.item_id ORDER BY p.created_at), NULL))[:4]`
    /// aggregates oldest-first and slices the head. The original port ranked
    /// newest-first with ROW_NUMBER; pin the flipped semantics.
    #[test]
    fn get_lists_sql_previews_the_four_oldest_picks_ascending() {
        let (sql, _) = build_get_lists_sql(&opts(None, None));
        assert!(
            sql.contains("(ARRAY_AGG(item_id ORDER BY created_at ASC))[:4] AS preview"),
            "preview must be the oldest-first head slice:\n{sql}"
        );
        assert!(
            !sql.contains("ROW_NUMBER"),
            "the newest-first ROW_NUMBER ranking must be gone:\n{sql}"
        );
    }

    /// `is_private` is derived from the ACL per caller (upstream
    /// component.ts:114), never read from the stored `l.is_private` column:
    /// a list with no grant to the caller (or to `'*'`) reads private --
    /// including the seeded shared Wishlist, whose stored column says false.
    #[test]
    fn get_lists_sql_derives_is_private_from_the_acl() {
        let (sql, _) = build_get_lists_sql(&opts(None, None));
        assert!(
            sql.contains("(SELECT COUNT(1) FROM favorites.acl"),
            "is_private must be an ACL subquery:\n{sql}"
        );
        assert!(
            sql.contains(&format!(
                "AND (favorites.acl.grantee = $1 OR favorites.acl.grantee = '{GRANTED_TO_ALL}')) = 0"
            )),
            "the ACL arm must be caller-scoped with the '*' wildcard:\n{sql}"
        );
        assert!(
            !sql.contains("l.is_private"),
            "the stored is_private column must not be projected:\n{sql}"
        );
    }

    /// Bind-order contract: $1 caller, then q, then itemId, then limit/offset.
    #[test]
    fn get_lists_sql_bind_indices() {
        let (sql, count_sql) = build_get_lists_sql(&opts(Some("item"), Some("needle")));
        assert!(sql.contains("l.name ILIKE '%' || $2 || '%'"));
        assert!(sql.contains("ip.item_id = $3"));
        assert!(sql.contains("LIMIT $4 OFFSET $5"));
        assert!(count_sql.contains("l.name ILIKE '%' || $2 || '%'"));

        let (sql, _) = build_get_lists_sql(&opts(None, None));
        assert!(sql.contains("LIMIT $2 OFFSET $3"));
    }

    /// Upstream picks dedup is row-level (`SELECT DISTINCT(p.item_id), p.*`):
    /// the DISTINCT list must carry the full picks identity so two users'
    /// picks of the same item in a shared list stay two rows even when their
    /// ms-truncated created_at collide.
    #[test]
    fn picks_by_list_sql_dedups_on_the_full_pick_identity() {
        assert!(
            PICKS_BY_LIST_SQL.contains("SELECT DISTINCT p.item_id, p.user_address, p.list_id,"),
            "DISTINCT key narrower than upstream:\n{PICKS_BY_LIST_SQL}"
        );
        assert!(PICKS_BY_LIST_SQL.contains("COUNT(*) OVER() AS picks_count"));
        assert!(PICKS_BY_LIST_SQL.contains("ORDER BY created_at DESC"));
    }
}
