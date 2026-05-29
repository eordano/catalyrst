//! Direct port of `marketplace-server/src/ports/accounts/{component,queries,types}.ts`
//! plus `adapters/accounts/index.ts:fromDBAccountToAccount`.

use serde::Serialize;
use sqlx::PgPool;

use crate::dcl_schemas::{get_db_networks, Network};
use crate::http::response::ApiError;
use crate::MARKETPLACE_SQUID_SCHEMA;

/// `types.ts:AccountSortBy`.
#[derive(Debug, Clone, Copy)]
pub enum AccountSortBy {
    MostSales,
    MostPurchases,
    MostRoyalties,
    MostCollections,
    MostEarned,
    MostSpent,
}

/// `types.ts:AccountFilters`.
#[derive(Debug, Clone, Default)]
pub struct AccountFilters {
    pub first: Option<i64>,
    pub skip: Option<i64>,
    pub sort_by: Option<AccountSortBy>,
    pub id: Option<String>,
    pub address: Vec<String>,
    pub network: Option<Network>,
}

/// `types.ts:Account` — the JSON shape returned by the handler.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Account {
    pub id: String,
    pub address: String,
    pub sales: i32,
    pub purchases: i32,
    pub spent: String,
    pub earned: String,
    pub royalties: String,
    pub collections: i32,
}

pub struct AccountsComponent {
    pool: PgPool,
}

impl AccountsComponent {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// `component.ts:getAccounts`.
    pub async fn get_accounts(
        &self,
        filters: &AccountFilters,
    ) -> Result<(Vec<Account>, i64), ApiError> {
        const MAX_LIMIT: i64 = 1000;
        let limit = filters.first.map(|f| f.min(MAX_LIMIT)).unwrap_or(MAX_LIMIT);
        let offset = filters.skip.unwrap_or(0);

        // Build WHERE clause with optional filters. Bind indices start at 1
        // and walk forward as we attach each filter. We always end with two
        // additional binds (limit + offset) so we track the current bind
        // index in a counter.
        let mut where_clauses: Vec<String> = Vec::new();
        let mut bind_idx: usize = 0;

        let id_values: Option<Vec<String>> = filters.id.as_ref().map(|id| {
            vec![
                id.clone(),
                format!("{}-ETHEREUM", id),
                format!("{}-POLYGON", id),
            ]
        });
        if id_values.is_some() {
            bind_idx += 1;
            where_clauses.push(format!("id = ANY(${}::text[])", bind_idx));
        }

        let addresses: Option<Vec<String>> = if !filters.address.is_empty() {
            Some(filters.address.iter().map(|a| a.to_lowercase()).collect())
        } else {
            None
        };
        if addresses.is_some() {
            bind_idx += 1;
            where_clauses.push(format!("address = ANY(${}::text[])", bind_idx));
        }

        let networks: Option<Vec<String>> = filters
            .network
            .map(|n| get_db_networks(n).into_iter().map(String::from).collect());
        if networks.is_some() {
            bind_idx += 1;
            where_clauses.push(format!("network = ANY(${}::text[])", bind_idx));
        }

        let where_sql = if where_clauses.is_empty() {
            String::new()
        } else {
            format!(" WHERE {}", where_clauses.join(" AND "))
        };

        let sort_clause = match filters.sort_by {
            Some(AccountSortBy::MostSales) => " ORDER BY sales DESC ",
            Some(AccountSortBy::MostPurchases) => " ORDER BY purchases DESC ",
            Some(AccountSortBy::MostRoyalties) => " ORDER BY royalties DESC ",
            Some(AccountSortBy::MostCollections) => " ORDER BY collections DESC ",
            Some(AccountSortBy::MostEarned) => " ORDER BY earned DESC ",
            Some(AccountSortBy::MostSpent) => " ORDER BY spent DESC ",
            None => " ORDER BY earned DESC ",
        };

        let limit_idx = bind_idx + 1;
        let offset_idx = bind_idx + 2;

        let select_sql = format!(
            "SELECT id, address, sales, purchases, spent::text AS spent, \
                    earned::text AS earned, royalties::text AS royalties, collections \
             FROM {schema}.account {where_} {sort_} LIMIT ${limit_idx} OFFSET ${offset_idx}",
            schema = MARKETPLACE_SQUID_SCHEMA,
            where_ = where_sql,
            sort_ = sort_clause,
            limit_idx = limit_idx,
            offset_idx = offset_idx,
        );

        let mut q = sqlx::query_as::<_, DbAccount>(&select_sql);
        if let Some(ids) = &id_values {
            q = q.bind(ids);
        }
        if let Some(addrs) = &addresses {
            q = q.bind(addrs);
        }
        if let Some(nets) = &networks {
            q = q.bind(nets);
        }
        q = q.bind(limit).bind(offset);
        let rows = q.fetch_all(&self.pool).await?;

        // count query — same WHERE clause, but bind indices are now 1..bind_idx.
        let count_sql = format!(
            "SELECT COUNT(*) FROM {schema}.account {where_}",
            schema = MARKETPLACE_SQUID_SCHEMA,
            where_ = where_sql,
        );
        let mut cq = sqlx::query_scalar::<_, i64>(&count_sql);
        if let Some(ids) = &id_values {
            cq = cq.bind(ids);
        }
        if let Some(addrs) = &addresses {
            cq = cq.bind(addrs);
        }
        if let Some(nets) = &networks {
            cq = cq.bind(nets);
        }
        let total = cq.fetch_one(&self.pool).await.unwrap_or(0);

        let data = rows.into_iter().map(from_db_account_to_account).collect();
        Ok((data, total))
    }
}

/// `types.ts:DBAccount`.
#[derive(Debug, sqlx::FromRow)]
struct DbAccount {
    id: String,
    address: String,
    sales: i32,
    purchases: i32,
    spent: String,
    earned: String,
    royalties: String,
    collections: i32,
}

/// `adapters/accounts/index.ts:fromDBAccountToAccount` — note the deliberate
/// `id: dbAccount.address` swap; the public id is the lowercase address, not
/// the `address-NETWORK` row primary key.
fn from_db_account_to_account(db: DbAccount) -> Account {
    Account {
        id: db.address.clone(),
        address: db.address,
        sales: db.sales,
        purchases: db.purchases,
        spent: db.spent,
        earned: db.earned,
        royalties: db.royalties,
        collections: db.collections,
    }
}
