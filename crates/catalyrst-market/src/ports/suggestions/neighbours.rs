use std::collections::{HashMap, HashSet};

use sqlx::{Executor, PgConnection, Row};

use super::co_ownership::{
    build_co_ownership_neighbors, AcquisitionMatrix, CoOwnershipOptions, NeighborRow,
};
use super::constants::{
    ALGORITHM_VERSION, MAX_WALLET_ITEMS, MIN_WALLET_ITEMS, NEIGHBORS_INSERT_BATCH_SIZE,
    NEIGHBORS_ITEM_INDEX, NEIGHBORS_META_TABLE, NEIGHBORS_PER_ITEM, NEIGHBORS_TABLE,
    NEIGHBORS_TABLE_NAME, NEIGHBOR_SOURCE_CF, NEIGHBOR_SOURCE_CONTENT, RARITY_TIERS,
};
use super::content::{
    assign_price_bands, build_tag_vectors, content_neighbors_by_anchor, ContentItem, ContentOptions,
};
use crate::{BUILDER_SERVER_TABLE_SCHEMA, MARKETPLACE_SQUID_SCHEMA};

/// Any positive constant works; it only has to be the same in every instance of this service.
const REBUILD_ADVISORY_LOCK_KEY: i64 = 8_421_311;
/// Distinct from the transaction-scoped key the swap uses: this one guards the whole run.
pub const REBUILD_SESSION_LOCK_KEY: i64 = 8_421_312;

pub fn staging_table() -> String {
    format!("{BUILDER_SERVER_TABLE_SCHEMA}.{NEIGHBORS_TABLE_NAME}_staging")
}

fn staging_index() -> String {
    format!("{NEIGHBORS_ITEM_INDEX}_staging")
}

fn staging_primary_key() -> String {
    format!("{NEIGHBORS_TABLE_NAME}_staging_pkey")
}

fn neighbors_primary_key() -> String {
    format!("{NEIGHBORS_TABLE_NAME}_pkey")
}

/// Every (wallet, item) PURCHASE, one row each.
///
/// Dated events, never `nft.owner_address`: the current owner tells you nothing about who
/// acquired what, and a resold item would be credited to the wrong wallet.
///
/// Only PAID acquisitions. Unpaid ones -- airdrops, free claims, gifts -- drag the hybrid below a
/// plain popularity ranking (see `FREE_ACQUISITION_WEIGHT`): read as one person, a wallet has five
/// to fifteen of them per purchase, and they pull the co-ownership vectors towards whatever was
/// mass-distributed. Dropping them here rather than weighting them at zero also takes most of the
/// mint table out of the scan.
///
/// `mint.beneficiary` is NOT a bare address: it is `<address>-POLYGON` or `<address>-ETHEREUM`,
/// while `sale.buyer` is the address alone. Lowercasing the two and calling it a day splits every
/// person who both minted and bought into two separate owners, which is exactly the co-occurrence
/// this table is built to find -- the suffix has to come off.
pub fn select_acquisitions() -> String {
    format!(
        "WITH acquisitions AS (\n\
           SELECT split_part(beneficiary, '-', 1) AS wallet, item_id\n\
             FROM {MARKETPLACE_SQUID_SCHEMA}.mint\n\
            WHERE beneficiary IS NOT NULL\n\
              AND item_id IS NOT NULL\n\
              AND COALESCE(search_primary_sale_price, 0) > 0\n\
           UNION ALL\n\
           SELECT buyer AS wallet,\n\
                  COALESCE(item_id, search_contract_address || '-' || search_item_id::text) AS item_id\n\
             FROM {MARKETPLACE_SQUID_SCHEMA}.sale\n\
            WHERE buyer IS NOT NULL AND (item_id IS NOT NULL OR search_item_id IS NOT NULL)\n\
         ), pairs AS (\n\
           SELECT wallet, item_id FROM acquisitions GROUP BY wallet, item_id\n\
         ), band AS (\n\
           SELECT wallet\n\
             FROM pairs\n\
            GROUP BY wallet\n\
           HAVING count(*) BETWEEN {MIN_WALLET_ITEMS} AND {MAX_WALLET_ITEMS}\n\
         )\n\
         SELECT p.wallet, p.item_id\n\
           FROM pairs p\n\
           JOIN band b ON b.wallet = p.wallet\n\
          ORDER BY p.wallet"
    )
}

/// The item attributes the content pass needs, plus the candidacy flag.
///
/// `is_candidate` is deliberately BROADER than "sellable in the shop right now": it is every
/// approved, non-social item. The request-time query joins against the item-unified core, which is
/// the one definition of sellable, so narrowing here would only mean an item listed between two
/// job runs has no neighbour rows and stays invisible until the next rebuild. Listing state
/// changes constantly; approval does not.
pub fn select_items() -> String {
    format!(
        "SELECT\n\
           id::text AS item_id,\n\
           COALESCE(creator, '') AS creator,\n\
           COALESCE(collection_id, '') AS collection_id,\n\
           CASE WHEN item_type LIKE 'emote%' THEN 'emote' ELSE 'wearable' END\n\
             || ':' || COALESCE(search_wearable_category, search_emote_category, '') AS sub_category,\n\
           COALESCE(rarity, '') AS rarity,\n\
           (COALESCE(price, 0) / 1e18)::float8 AS price,\n\
           (search_is_collection_approved = true AND search_emote_outcome_type IS NULL) AS is_candidate\n\
         FROM {MARKETPLACE_SQUID_SCHEMA}.item"
    )
}

/// The builder-server item tags. That relation is an upstream mirror this tree does not carry, so
/// a missing one costs the content pass its `tags` weight and nothing else -- see `load_tags`.
pub fn select_tags() -> String {
    format!(
        "SELECT item_id, lower(tag) AS tag\n\
           FROM {BUILDER_SERVER_TABLE_SCHEMA}.mv_builder_server_items\n\
          WHERE tag IS NOT NULL AND tag <> ''"
    )
}

#[derive(Debug, Clone, PartialEq)]
pub struct NeighborInsertRow {
    pub item_id: String,
    pub source: &'static str,
    pub neighbor_id: String,
    pub sim: f32,
    pub support: i32,
    pub rank: i16,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct NeighborsMeta {
    pub cf_rows: i64,
    pub content_rows: i64,
    pub items_covered: i64,
    pub duration_ms: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RebuildOutcome {
    Rebuilt,
    Skipped,
}

#[derive(Debug, Clone)]
pub struct ItemRecord {
    pub index: usize,
    pub id: String,
    pub creator: String,
    pub collection: String,
    pub sub_category: String,
    pub rarity_tier: i32,
    pub price: f64,
    pub is_candidate: bool,
}

#[derive(Debug, Default)]
pub struct LoadedCatalogue {
    pub items: Vec<ItemRecord>,
    pub index_by_id: HashMap<String, usize>,
}

fn rarity_tier(rarity: &str) -> i32 {
    RARITY_TIERS
        .iter()
        .position(|tier| *tier == rarity.to_ascii_lowercase())
        .map(|tier| tier as i32)
        .unwrap_or(-1)
}

pub async fn load_catalogue(conn: &mut PgConnection) -> Result<LoadedCatalogue, sqlx::Error> {
    let rows = sqlx::query(sqlx::AssertSqlSafe(select_items()))
        .fetch_all(&mut *conn)
        .await?;
    let mut items: Vec<ItemRecord> = Vec::with_capacity(rows.len());
    let mut index_by_id: HashMap<String, usize> = HashMap::with_capacity(rows.len());
    for row in rows {
        let id: String = row.try_get("item_id").unwrap_or_default();
        if id.is_empty() {
            continue;
        }
        let index = items.len();
        index_by_id.insert(id.clone(), index);
        items.push(ItemRecord {
            index,
            id,
            creator: row.try_get("creator").unwrap_or_default(),
            collection: row.try_get("collection_id").unwrap_or_default(),
            sub_category: row.try_get("sub_category").unwrap_or_default(),
            rarity_tier: rarity_tier(&row.try_get::<String, _>("rarity").unwrap_or_default()),
            price: row.try_get::<f64, _>("price").unwrap_or(0.0),
            is_candidate: row.try_get("is_candidate").unwrap_or(false),
        });
    }
    Ok(LoadedCatalogue { items, index_by_id })
}

/// The hoarder damping, `|items(u)|^(-1/4)`, which makes the dot product over a wallet equal
/// `sum(1 / sqrt(|items(u)|))`. Every row is already a purchase (unpaid ones are filtered in SQL),
/// so this is the only per-row weight left.
pub fn damp(wallet_size: usize) -> f32 {
    if wallet_size == 0 {
        return 0.0;
    }
    (wallet_size as f64).powf(-0.25) as f32
}

/// Acquisitions as a compressed row matrix, STREAMED.
///
/// The query returns rows ordered by wallet and they are folded into flat vectors as they arrive,
/// so the (wallet, item) pairs never exist as row objects at the same time. They are retained
/// rather than consumed wallet-by-wallet because the neighbour accumulator is blocked over columns
/// and therefore makes several passes.
pub async fn load_acquisitions(
    conn: &mut PgConnection,
    catalogue: &LoadedCatalogue,
    deadline: Option<std::time::Duration>,
) -> Result<(AcquisitionMatrix, usize), sqlx::Error> {
    use futures::TryStreamExt;

    let started = std::time::Instant::now();
    let mut offsets: Vec<i32> = vec![0];
    let mut items: Vec<i32> = Vec::new();
    let mut current_wallet: Option<String> = None;
    let mut rows_read = 0usize;

    let mut stream = sqlx::query(sqlx::AssertSqlSafe(select_acquisitions())).fetch(&mut *conn);
    while let Some(row) = stream.try_next().await? {
        rows_read += 1;
        let item_id: String = row.try_get("item_id").unwrap_or_default();
        let Some(index) = catalogue.index_by_id.get(&item_id) else {
            continue;
        };
        let wallet: String = row.try_get("wallet").unwrap_or_default();
        if current_wallet.as_deref() != Some(wallet.as_str()) {
            if current_wallet.is_some() {
                offsets.push(items.len() as i32);
            }
            current_wallet = Some(wallet);
        }
        items.push(*index as i32);

        if let Some(deadline) = deadline {
            if started.elapsed() > deadline {
                return Err(sqlx::Error::Protocol(format!(
                    "acquisition scan exceeded {} ms",
                    deadline.as_millis()
                )));
            }
        }
    }
    if current_wallet.is_some() {
        offsets.push(items.len() as i32);
    }

    let wallet_count = offsets.len() - 1;
    let mut weights = vec![0.0f32; items.len()];
    for wallet in 0..wallet_count {
        let from = offsets[wallet] as usize;
        let to = offsets[wallet + 1] as usize;
        let damping = damp(to - from);
        for weight in weights[from..to].iter_mut() {
            *weight = damping;
        }
    }

    Ok((
        AcquisitionMatrix {
            offsets,
            items,
            weights,
            wallet_count,
            item_count: catalogue.items.len(),
        },
        rows_read,
    ))
}

/// The builder-server tag mirror is optional here: without it the content pass keeps its creator,
/// collection, sub-category, rarity and price-band weights and loses only the tag one.
async fn load_tags(conn: &mut PgConnection) -> Vec<(String, String)> {
    match sqlx::query(sqlx::AssertSqlSafe(select_tags()))
        .fetch_all(&mut *conn)
        .await
    {
        Ok(rows) => rows
            .into_iter()
            .filter_map(|row| {
                Some((
                    row.try_get::<String, _>("item_id").ok()?,
                    row.try_get::<String, _>("tag").ok()?,
                ))
            })
            .collect(),
        Err(e) => {
            tracing::info!(
                error = %e,
                "no builder-server tag mirror: content neighbours run without the tag weight"
            );
            Vec::new()
        }
    }
}

pub async fn load_content_items(
    conn: &mut PgConnection,
    catalogue: &LoadedCatalogue,
) -> Vec<ContentItem> {
    let rows = load_tags(conn).await;

    let mut tag_ids: HashMap<String, u32> = HashMap::new();
    let mut document_frequency: Vec<usize> = Vec::new();
    let mut tags_by_item: HashMap<usize, Vec<u32>> = HashMap::new();

    for (item_id, tag) in rows {
        let Some(index) = catalogue.index_by_id.get(&item_id) else {
            continue;
        };
        let next = tag_ids.len() as u32;
        let tag_id = *tag_ids.entry(tag).or_insert_with(|| {
            document_frequency.push(0);
            next
        });
        let list = tags_by_item.entry(*index).or_default();
        if !list.contains(&tag_id) {
            list.push(tag_id);
            document_frequency[tag_id as usize] += 1;
        }
    }

    let vectors = build_tag_vectors(&tags_by_item, &document_frequency, catalogue.items.len());
    let bands = assign_price_bands(
        &catalogue
            .items
            .iter()
            .map(|item| (item.index, item.sub_category.clone(), item.price))
            .collect::<Vec<_>>(),
    );

    catalogue
        .items
        .iter()
        .map(|item| {
            let vector = vectors.get(&item.index);
            ContentItem {
                index: item.index,
                creator: item.creator.clone(),
                collection: item.collection.clone(),
                sub_category: item.sub_category.clone(),
                rarity_tier: item.rarity_tier,
                price_band: bands.get(&item.index).copied().unwrap_or(-1),
                is_candidate: item.is_candidate,
                tags: vector.map(|v| v.tags.clone()).unwrap_or_default(),
                tag_weights: vector.map(|v| v.weights.clone()).unwrap_or_default(),
            }
        })
        .collect()
}

/// Neighbour rows -> insertable rows, numbering each anchor's list so the endpoint can cut by rank.
pub fn to_insert_rows(
    rows: &[NeighborRow],
    source: &'static str,
    items: &[ItemRecord],
) -> Vec<NeighborInsertRow> {
    let mut out: Vec<NeighborInsertRow> = Vec::new();
    let mut current_item: Option<usize> = None;
    let mut rank = 0i16;
    for row in rows {
        if current_item != Some(row.item) {
            current_item = Some(row.item);
            rank = 0;
        }
        if rank as usize >= NEIGHBORS_PER_ITEM {
            continue;
        }
        out.push(NeighborInsertRow {
            item_id: items[row.item].id.clone(),
            source,
            neighbor_id: items[row.neighbor].id.clone(),
            sim: row.sim as f32,
            support: row.support as i32,
            rank,
        });
        rank += 1;
    }
    out
}

/// Content rows held before a write. Each anchor contributes at most `NEIGHBORS_PER_ITEM`, so this
/// is a few hundred anchors' worth: enough to keep the inserts batched, small enough that the whole
/// content set never exists at once.
const CONTENT_FLUSH_SIZE: usize = 20_000;

#[derive(Default)]
pub struct BuildOptions {
    pub block_width: Option<usize>,
    pub acquisition_deadline: Option<std::time::Duration>,
}

/// Computes both neighbour sets and hands each to `insert` as soon as it exists.
///
/// The two sets are produced and released one at a time on purpose: holding the co-ownership rows,
/// the content rows and the concatenation of both is the largest single item in this job's memory
/// profile.
pub async fn produce_neighbor_rows(
    read: &mut PgConnection,
    write: &mut PgConnection,
    options: BuildOptions,
) -> Result<NeighborsMeta, sqlx::Error> {
    let started = std::time::Instant::now();
    let catalogue = load_catalogue(read).await?;
    let (matrix, _rows_read) =
        load_acquisitions(read, &catalogue, options.acquisition_deadline).await?;

    let is_candidate: Vec<bool> = catalogue
        .items
        .iter()
        .map(|item| item.is_candidate)
        .collect();
    let mut covered: HashSet<String> = HashSet::new();

    let co_options = CoOwnershipOptions {
        block_width: options
            .block_width
            .unwrap_or(super::co_ownership::DEFAULT_BLOCK_WIDTH),
        ..Default::default()
    };
    let cf_rows = to_insert_rows(
        &build_co_ownership_neighbors(&matrix, &is_candidate, co_options),
        NEIGHBOR_SOURCE_CF,
        &catalogue.items,
    );
    let cf_count = cf_rows.len() as i64;
    for row in &cf_rows {
        covered.insert(row.item_id.clone());
    }
    insert_in_batches(write, &cf_rows).await?;
    drop(cf_rows);
    drop(matrix);

    let content_items = load_content_items(read, &catalogue).await;
    let mut produced: Vec<Vec<NeighborRow>> = Vec::new();
    content_neighbors_by_anchor(&content_items, ContentOptions::default(), |rows| {
        produced.push(rows)
    });
    drop(content_items);

    let mut content_count = 0i64;
    let mut buffer: Vec<NeighborInsertRow> = Vec::new();
    for anchor_rows in produced {
        let converted = to_insert_rows(&anchor_rows, NEIGHBOR_SOURCE_CONTENT, &catalogue.items);
        content_count += converted.len() as i64;
        for row in &converted {
            covered.insert(row.item_id.clone());
        }
        buffer.extend(converted);
        if buffer.len() >= CONTENT_FLUSH_SIZE {
            let batch = std::mem::take(&mut buffer);
            insert_in_batches(write, &batch).await?;
        }
    }
    if !buffer.is_empty() {
        insert_in_batches(write, &buffer).await?;
    }

    Ok(NeighborsMeta {
        cf_rows: cf_count,
        content_rows: content_count,
        items_covered: covered.len() as i64,
        duration_ms: started.elapsed().as_millis() as i64,
    })
}

pub async fn insert_in_batches(
    conn: &mut PgConnection,
    rows: &[NeighborInsertRow],
) -> Result<(), sqlx::Error> {
    let staging = staging_table();
    for batch in rows.chunks(NEIGHBORS_INSERT_BATCH_SIZE) {
        let placeholders: Vec<String> = (0..batch.len())
            .map(|i| {
                let base = i * 6;
                format!(
                    "(${}, ${}, ${}, ${}, ${}, ${})",
                    base + 1,
                    base + 2,
                    base + 3,
                    base + 4,
                    base + 5,
                    base + 6
                )
            })
            .collect();
        let sql = format!(
            "INSERT INTO {staging} (item_id, source, neighbor_id, sim, support, rank) VALUES {}",
            placeholders.join(",")
        );
        let mut q = sqlx::query(sqlx::AssertSqlSafe(sql));
        for row in batch {
            q = q
                .bind(row.item_id.clone())
                .bind(row.source)
                .bind(row.neighbor_id.clone())
                .bind(row.sim)
                .bind(row.support)
                .bind(row.rank);
        }
        q.execute(&mut *conn).await?;
    }
    Ok(())
}

/// The statements the swap runs, in order, once the producer has filled the staging table.
///
/// `LIKE` copies columns and defaults but NOT the primary key -- that needs INCLUDING INDEXES,
/// which would also copy the secondary index under a generated name this code could not rename
/// afterwards. So the key is added explicitly AFTER the rows are in: building it once over a full
/// table is cheaper than maintaining it across every insert, and a duplicate row fails the whole
/// swap rather than being silently dropped, which is the right outcome for a generator bug.
pub fn swap_statements() -> Vec<String> {
    let staging = staging_table();
    let staging_index = staging_index();
    let staging_key = staging_primary_key();
    let key = neighbors_primary_key();
    vec![
        format!(
            "ALTER TABLE {staging} ADD CONSTRAINT {staging_key} PRIMARY KEY (item_id, source, neighbor_id)"
        ),
        format!("CREATE INDEX {staging_index} ON {staging} (item_id)"),
        format!("ANALYZE {staging}"),
        format!("DROP TABLE IF EXISTS {NEIGHBORS_TABLE}"),
        format!("ALTER TABLE {staging} RENAME TO {NEIGHBORS_TABLE_NAME}"),
        format!(
            "ALTER INDEX {BUILDER_SERVER_TABLE_SCHEMA}.{staging_index} RENAME TO {NEIGHBORS_ITEM_INDEX}"
        ),
        format!("ALTER TABLE {NEIGHBORS_TABLE} RENAME CONSTRAINT {staging_key} TO {key}"),
    ]
}

pub fn meta_upsert_sql() -> String {
    format!(
        "INSERT INTO {NEIGHBORS_META_TABLE} (id, built_at, duration_ms, cf_rows, content_rows, items_covered, algorithm)\n\
         VALUES (true, now(), $1, $2, $3, $4, $5)\n\
         ON CONFLICT (id) DO UPDATE SET\n\
           built_at = EXCLUDED.built_at,\n\
           duration_ms = EXCLUDED.duration_ms,\n\
           cf_rows = EXCLUDED.cf_rows,\n\
           content_rows = EXCLUDED.content_rows,\n\
           items_covered = EXCLUDED.items_covered,\n\
           algorithm = EXCLUDED.algorithm"
    )
}

/// Opens the swap.
///
/// One transaction spans the whole rebuild: the live table is untouched until the drop-and-rename
/// at the very end, so a failure -- a statement timeout included -- rolls back and leaves the
/// previous neighbours serving. Readers block only for the rename.
///
/// `None` means another replica holds the lock and this run must not proceed.
pub async fn begin_swap(
    conn: &mut PgConnection,
) -> Result<Option<sqlx::Transaction<'_, sqlx::Postgres>>, sqlx::Error> {
    let mut tx = conn.begin().await?;

    let acquired: bool = sqlx::query_scalar("SELECT pg_try_advisory_xact_lock($1)")
        .bind(REBUILD_ADVISORY_LOCK_KEY)
        .fetch_one(&mut *tx)
        .await?;
    if !acquired {
        tx.rollback().await?;
        return Ok(None);
    }

    let staging = staging_table();
    tx.execute(sqlx::AssertSqlSafe(format!(
        "DROP TABLE IF EXISTS {staging}"
    )))
    .await?;
    tx.execute(sqlx::AssertSqlSafe(format!(
        "CREATE TABLE {staging} (LIKE {NEIGHBORS_TABLE} INCLUDING DEFAULTS)"
    )))
    .await?;

    Ok(Some(tx))
}

/// Indexes the filled staging table, swaps it in and records the run. Committing is the only
/// point at which anything a reader can see changes.
pub async fn finish_swap(
    mut tx: sqlx::Transaction<'_, sqlx::Postgres>,
    meta: &NeighborsMeta,
) -> Result<RebuildOutcome, sqlx::Error> {
    for statement in swap_statements() {
        tx.execute(sqlx::AssertSqlSafe(statement)).await?;
    }

    sqlx::query(sqlx::AssertSqlSafe(meta_upsert_sql()))
        .bind(meta.duration_ms as i32)
        .bind(meta.cf_rows as i32)
        .bind(meta.content_rows as i32)
        .bind(meta.items_covered as i32)
        .bind(ALGORITHM_VERSION)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;
    Ok(RebuildOutcome::Rebuilt)
}

use sqlx::Connection;

#[cfg(test)]
mod tests {
    use super::*;

    fn record(index: usize, id: &str) -> ItemRecord {
        ItemRecord {
            index,
            id: id.to_string(),
            creator: String::new(),
            collection: String::new(),
            sub_category: String::new(),
            rarity_tier: -1,
            price: 0.0,
            is_candidate: true,
        }
    }

    /// Unpaid acquisitions are dropped in SQL rather than carried at zero weight: same result,
    /// far less to read.
    #[test]
    fn the_acquisition_scan_reads_paid_events_only_and_strips_the_mint_suffix() {
        let sql = select_acquisitions();
        assert!(
            sql.contains("COALESCE(search_primary_sale_price, 0) > 0"),
            "{sql}"
        );
        assert!(
            sql.contains("split_part(beneficiary, '-', 1) AS wallet"),
            "a mint beneficiary carries a -POLYGON/-ETHEREUM suffix: {sql}"
        );
        assert!(
            !sql.contains("owner_address"),
            "dated events, not holdings: {sql}"
        );
        assert!(
            sql.contains(&format!(
                "BETWEEN {MIN_WALLET_ITEMS} AND {MAX_WALLET_ITEMS}"
            )),
            "{sql}"
        );
        assert!(
            sql.contains("ORDER BY p.wallet"),
            "the fold needs wallet order: {sql}"
        );
    }

    /// Broader than "sellable right now" on purpose: listing state changes between two rebuilds,
    /// approval does not.
    #[test]
    fn candidacy_is_approval_and_not_listing_state() {
        let sql = select_items();
        assert!(
            sql.contains("search_is_collection_approved = true"),
            "{sql}"
        );
        assert!(sql.contains("search_emote_outcome_type IS NULL"), "{sql}");
        assert!(!sql.contains("mv_trades"), "{sql}");
    }

    #[test]
    fn the_hoarder_damping_makes_a_wallets_own_dot_product_one_over_sqrt_of_its_size() {
        for size in [1usize, 4, 16, 400] {
            let weight = damp(size) as f64;
            assert!(
                (weight * weight - 1.0 / (size as f64).sqrt()).abs() < 1e-6,
                "size {size}"
            );
        }
        assert_eq!(damp(0), 0.0);
    }

    #[test]
    fn insert_rows_are_ranked_per_anchor_and_cut_at_the_cap() {
        let items: Vec<ItemRecord> = (0..NEIGHBORS_PER_ITEM + 5)
            .map(|i| record(i, &format!("0xa-{i}")))
            .collect();
        let rows: Vec<NeighborRow> = (1..NEIGHBORS_PER_ITEM + 5)
            .map(|neighbor| NeighborRow {
                item: 0,
                neighbor,
                sim: 1.0 / neighbor as f64,
                support: 3,
            })
            .collect();

        let inserts = to_insert_rows(&rows, NEIGHBOR_SOURCE_CF, &items);
        assert_eq!(inserts.len(), NEIGHBORS_PER_ITEM);
        assert_eq!(inserts[0].rank, 0);
        assert_eq!(
            inserts[NEIGHBORS_PER_ITEM - 1].rank,
            (NEIGHBORS_PER_ITEM - 1) as i16
        );
        assert!(inserts.iter().all(|r| r.source == NEIGHBOR_SOURCE_CF));
    }

    #[test]
    fn a_new_anchor_restarts_the_rank() {
        let items: Vec<ItemRecord> = (0..4).map(|i| record(i, &format!("0xa-{i}"))).collect();
        let rows = vec![
            NeighborRow {
                item: 0,
                neighbor: 1,
                sim: 0.9,
                support: 3,
            },
            NeighborRow {
                item: 0,
                neighbor: 2,
                sim: 0.5,
                support: 3,
            },
            NeighborRow {
                item: 3,
                neighbor: 1,
                sim: 0.4,
                support: 3,
            },
        ];
        let ranks: Vec<i16> = to_insert_rows(&rows, NEIGHBOR_SOURCE_CONTENT, &items)
            .iter()
            .map(|r| r.rank)
            .collect();
        assert_eq!(ranks, vec![0, 1, 0]);
    }

    /// The live table must be untouched until the very end, and the staging key added only once
    /// the rows are in.
    #[test]
    fn the_swap_builds_the_key_before_it_touches_the_live_table() {
        let statements = swap_statements();
        let drop_at = statements
            .iter()
            .position(|s| s.starts_with("DROP TABLE"))
            .expect("a drop");
        let key_at = statements
            .iter()
            .position(|s| s.contains("ADD CONSTRAINT"))
            .expect("a key");
        let rename_at = statements
            .iter()
            .position(|s| s.contains("RENAME TO item_neighbors"))
            .expect("a rename");

        assert!(key_at < drop_at, "{statements:?}");
        assert!(drop_at < rename_at, "{statements:?}");
        assert!(
            statements.iter().any(|s| s.starts_with("ANALYZE")),
            "{statements:?}"
        );
        assert!(
            statements.iter().any(|s| s.contains("RENAME CONSTRAINT")),
            "the key name has to survive the swap: {statements:?}"
        );
    }

    #[test]
    fn the_meta_row_is_a_singleton_upsert() {
        let sql = meta_upsert_sql();
        assert!(sql.contains("ON CONFLICT (id) DO UPDATE"), "{sql}");
        assert!(sql.contains("built_at = EXCLUDED.built_at"), "{sql}");
    }
}
