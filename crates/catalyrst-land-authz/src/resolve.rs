use async_trait::async_trait;
use sqlx::PgPool;

use catalyrst_validator::squid_checker::{LandOperatorResolver, LandOperators};

use crate::events::{
    ESTATE_REGISTRY_MAINNET, KIND_APPROVED_FOR_ALL, KIND_UPDATE_MANAGER, LAND_REGISTRY_MAINNET,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParcelSubject {
    pub owner: String,
    pub registry: String,
    pub operator: Option<String>,
    pub update_operator: Option<String>,
    pub belongs_to_estate: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdatableParcel {
    pub token_id: String,
    pub x: i32,
    pub y: i32,
    pub owner: String,
    pub via_estate: bool,
}

/// What one parcel resolves to: its subject and the four operator legs.
#[derive(Debug, Clone)]
pub struct ParcelRights {
    pub subject: ParcelSubject,
    pub operators: LandOperators,
}

type ParcelSubjectRow = (
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    bool,
);

type ParcelRightsRow = (
    i64,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    bool,
    Vec<String>,
    Vec<String>,
);

#[derive(Clone)]
pub struct LandAuthzStore {
    pool: PgPool,
    land_registry: String,
    estate_registry: String,
}

impl LandAuthzStore {
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            land_registry: LAND_REGISTRY_MAINNET.to_string(),
            estate_registry: ESTATE_REGISTRY_MAINNET.to_string(),
        }
    }

    pub fn with_registries(mut self, land: impl Into<String>, estate: impl Into<String>) -> Self {
        self.land_registry = land.into().to_lowercase();
        self.estate_registry = estate.into().to_lowercase();
        self
    }

    pub fn land_registry(&self) -> &str {
        &self.land_registry
    }

    /// Mirrors the land-manager subgraph's own reading: an estate parcel takes
    /// the estate's owner and operator, but a per-parcel update operator still
    /// wins over the estate's. `None` means no such parcel is indexed.
    pub async fn parcel_subject(
        &self,
        x: i32,
        y: i32,
    ) -> Result<Option<ParcelSubject>, sqlx::Error> {
        let row: Option<ParcelSubjectRow> = sqlx::query_as(
            "SELECT split_part(p.owner_id, '-', 1)  AS parcel_owner,
                    split_part(e.owner_id, '-', 1)  AS estate_owner,
                    CASE WHEN p.estate_id IS NULL THEN pt.operator ELSE et.operator END AS operator,
                    COALESCE(pt.update_operator, CASE WHEN p.estate_id IS NULL THEN NULL ELSE et.update_operator END)
                        AS update_operator,
                    (p.estate_id IS NOT NULL) AS belongs_to_estate
             FROM squid_marketplace.parcel p
             LEFT JOIN squid_marketplace.estate e ON e.id = p.estate_id
             LEFT JOIN land_authz.token_right pt
                    ON pt.token_address = $3 AND pt.token_id = p.token_id
             LEFT JOIN land_authz.token_right et
                    ON et.token_address = $4 AND et.token_id = e.token_id
             WHERE p.x = $1 AND p.y = $2",
        )
        .bind(x)
        .bind(y)
        .bind(&self.land_registry)
        .bind(&self.estate_registry)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(
            |(parcel_owner, estate_owner, operator, update_operator, belongs_to_estate)| {
                let owner = if belongs_to_estate {
                    estate_owner.or(parcel_owner)
                } else {
                    parcel_owner
                };
                ParcelSubject {
                    owner: owner.unwrap_or_default().to_lowercase(),
                    registry: if belongs_to_estate {
                        self.estate_registry.clone()
                    } else {
                        self.land_registry.clone()
                    },
                    operator: operator.map(|o| o.to_lowercase()),
                    update_operator: update_operator.map(|o| o.to_lowercase()),
                    belongs_to_estate,
                }
            },
        ))
    }

    pub async fn account_grants(
        &self,
        registry: &str,
        owner: &str,
        kind: &str,
    ) -> Result<Vec<String>, sqlx::Error> {
        sqlx::query_scalar(
            "SELECT operator FROM land_authz.account_right
             WHERE token_address = $1 AND account = lower($2) AND kind = $3 AND is_approved
             ORDER BY operator",
        )
        .bind(registry)
        .bind(owner)
        .bind(kind)
        .fetch_all(&self.pool)
        .await
    }

    /// Subject plus both grant legs per parcel in one round trip, positional; `None` = not indexed.
    pub async fn rights_batch(
        &self,
        parcels: &[(i32, i32)],
    ) -> Result<Vec<Option<ParcelRights>>, sqlx::Error> {
        if parcels.is_empty() {
            return Ok(Vec::new());
        }
        let xs: Vec<i32> = parcels.iter().map(|p| p.0).collect();
        let ys: Vec<i32> = parcels.iter().map(|p| p.1).collect();
        let rows: Vec<ParcelRightsRow> = sqlx::query_as(
            "SELECT t.i,
                    split_part(p.owner_id, '-', 1)  AS parcel_owner,
                    split_part(e.owner_id, '-', 1)  AS estate_owner,
                    CASE WHEN p.estate_id IS NULL THEN pt.operator ELSE et.operator END AS operator,
                    COALESCE(pt.update_operator, CASE WHEN p.estate_id IS NULL THEN NULL ELSE et.update_operator END)
                        AS update_operator,
                    (p.estate_id IS NOT NULL) AS belongs_to_estate,
                    um.operators AS update_managers,
                    af.operators AS approved_for_all
             FROM unnest($1::int[], $2::int[]) WITH ORDINALITY AS t(x, y, i)
             JOIN squid_marketplace.parcel p ON p.x = t.x AND p.y = t.y
             LEFT JOIN squid_marketplace.estate e ON e.id = p.estate_id
             LEFT JOIN land_authz.token_right pt
                    ON pt.token_address = $3 AND pt.token_id = p.token_id
             LEFT JOIN land_authz.token_right et
                    ON et.token_address = $4 AND et.token_id = e.token_id
             CROSS JOIN LATERAL (
                 SELECT CASE WHEN p.estate_id IS NULL THEN $3::text ELSE $4::text END AS registry,
                        lower(COALESCE(
                            CASE WHEN p.estate_id IS NOT NULL THEN split_part(e.owner_id, '-', 1) END,
                            split_part(p.owner_id, '-', 1), '')) AS account
             ) s
             LEFT JOIN LATERAL (
                 SELECT COALESCE(array_agg(ar.operator ORDER BY ar.operator), '{}'::text[]) AS operators
                 FROM land_authz.account_right ar
                 WHERE ar.token_address = s.registry AND ar.account = s.account
                   AND ar.kind = $5 AND ar.is_approved
             ) um ON true
             LEFT JOIN LATERAL (
                 SELECT COALESCE(array_agg(ar.operator ORDER BY ar.operator), '{}'::text[]) AS operators
                 FROM land_authz.account_right ar
                 WHERE ar.token_address = s.registry AND ar.account = s.account
                   AND ar.kind = $6 AND ar.is_approved
             ) af ON true",
        )
        .bind(&xs)
        .bind(&ys)
        .bind(&self.land_registry)
        .bind(&self.estate_registry)
        .bind(KIND_UPDATE_MANAGER)
        .bind(KIND_APPROVED_FOR_ALL)
        .fetch_all(&self.pool)
        .await?;

        let mut out: Vec<Option<ParcelRights>> = vec![None; parcels.len()];
        for (
            i,
            parcel_owner,
            estate_owner,
            operator,
            update_operator,
            belongs_to_estate,
            update_managers,
            approved_for_all,
        ) in rows
        {
            let Some(slot) = usize::try_from(i - 1).ok().and_then(|i| out.get_mut(i)) else {
                continue;
            };
            let owner = if belongs_to_estate {
                estate_owner.or(parcel_owner)
            } else {
                parcel_owner
            };
            let subject = ParcelSubject {
                owner: owner.unwrap_or_default().to_lowercase(),
                registry: if belongs_to_estate {
                    self.estate_registry.clone()
                } else {
                    self.land_registry.clone()
                },
                operator: operator.map(|o| o.to_lowercase()),
                update_operator: update_operator.map(|o| o.to_lowercase()),
                belongs_to_estate,
            };
            let operators = LandOperators {
                operator: subject.operator.clone(),
                update_operator: subject.update_operator.clone(),
                update_managers,
                approved_for_all,
            };
            *slot = Some(ParcelRights { subject, operators });
        }
        Ok(out)
    }

    pub async fn rights(&self, x: i32, y: i32) -> Result<Option<ParcelRights>, sqlx::Error> {
        Ok(self.rights_batch(&[(x, y)]).await?.pop().flatten())
    }

    pub async fn operators(&self, x: i32, y: i32) -> Result<Option<LandOperators>, sqlx::Error> {
        Ok(self.rights(x, y).await?.map(|r| r.operators))
    }

    pub async fn parcel_owner(&self, x: i32, y: i32) -> Result<Option<String>, sqlx::Error> {
        Ok(self.parcel_subject(x, y).await?.map(|s| s.owner))
    }

    /// One page of `parcels_with_update_operator` with its total (`count(*) OVER ()`, COUNT fallback).
    pub async fn parcels_with_update_operator_page(
        &self,
        address: &str,
        limit: i64,
        offset: i64,
    ) -> Result<(Vec<UpdatableParcel>, i64), sqlx::Error> {
        let rows: Vec<(String, i32, i32, Option<String>, i64)> = sqlx::query_as(
            "SELECT tr.token_id::text, tr.x, tr.y, split_part(p.owner_id, '-', 1), count(*) OVER ()
             FROM land_authz.token_right tr
             JOIN squid_marketplace.parcel p ON p.token_id = tr.token_id
             WHERE tr.token_address = $1 AND tr.update_operator = lower($2)
             ORDER BY tr.token_id
             LIMIT $3 OFFSET $4",
        )
        .bind(&self.land_registry)
        .bind(address)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;
        let total = match rows.first() {
            Some(row) => row.4,
            None => {
                sqlx::query_scalar(
                    "SELECT count(*) FROM land_authz.token_right tr
                     JOIN squid_marketplace.parcel p ON p.token_id = tr.token_id
                     WHERE tr.token_address = $1 AND tr.update_operator = lower($2)",
                )
                .bind(&self.land_registry)
                .bind(address)
                .fetch_one(&self.pool)
                .await?
            }
        };
        Ok((
            rows.into_iter()
                .map(|(token_id, x, y, owner, _)| UpdatableParcel {
                    token_id,
                    x,
                    y,
                    owner: owner.unwrap_or_default().to_lowercase(),
                    via_estate: false,
                })
                .collect(),
            total,
        ))
    }

    /// The direct reverse lookup the lands-permissions route answers: parcels
    /// whose own update operator is this address. Served by the partial index
    /// on `token_right.update_operator`, not by a scan.
    pub async fn parcels_with_update_operator(
        &self,
        address: &str,
    ) -> Result<Vec<UpdatableParcel>, sqlx::Error> {
        let rows: Vec<(String, i32, i32, Option<String>)> = sqlx::query_as(
            "SELECT tr.token_id::text, tr.x, tr.y, split_part(p.owner_id, '-', 1)
             FROM land_authz.token_right tr
             JOIN squid_marketplace.parcel p ON p.token_id = tr.token_id
             WHERE tr.token_address = $1 AND tr.update_operator = lower($2)
             ORDER BY tr.token_id",
        )
        .bind(&self.land_registry)
        .bind(address)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|(token_id, x, y, owner)| UpdatableParcel {
                token_id,
                x,
                y,
                owner: owner.unwrap_or_default().to_lowercase(),
                via_estate: false,
            })
            .collect())
    }

    /// Every parcel this address may update, including the ones it reaches
    /// only through an estate-level grant. Strictly a superset of the direct
    /// lookup, kept separate so the route's subgraph-parity answer stays
    /// exactly that.
    pub async fn parcels_updatable_by(
        &self,
        address: &str,
    ) -> Result<Vec<UpdatableParcel>, sqlx::Error> {
        let rows: Vec<(String, i32, i32, Option<String>, bool)> = sqlx::query_as(
            "SELECT tr.token_id::text, tr.x, tr.y, split_part(p.owner_id, '-', 1), false
             FROM land_authz.token_right tr
             JOIN squid_marketplace.parcel p ON p.token_id = tr.token_id
             WHERE tr.token_address = $1 AND tr.update_operator = lower($2)
             UNION
             SELECT p.token_id::text, p.x::int, p.y::int, split_part(e.owner_id, '-', 1), true
             FROM land_authz.token_right et
             JOIN squid_marketplace.estate e ON e.token_id = et.token_id
             JOIN squid_marketplace.parcel p ON p.estate_id = e.id
             LEFT JOIN land_authz.token_right pt
                    ON pt.token_address = $1 AND pt.token_id = p.token_id
             WHERE et.token_address = $3 AND et.update_operator = lower($2)
               AND pt.update_operator IS NULL
             ORDER BY 2, 3",
        )
        .bind(&self.land_registry)
        .bind(address)
        .bind(&self.estate_registry)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|(token_id, x, y, owner, via_estate)| UpdatableParcel {
                token_id,
                x,
                y,
                owner: owner.unwrap_or_default().to_lowercase(),
                via_estate,
            })
            .collect())
    }
}

#[async_trait]
impl LandOperatorResolver for LandAuthzStore {
    async fn operators(&self, x: i32, y: i32) -> Result<Option<LandOperators>, String> {
        LandAuthzStore::operators(self, x, y)
            .await
            .map_err(|e| e.to_string())
    }

    async fn operators_batch(
        &self,
        parcels: &[(i32, i32)],
    ) -> Vec<Result<Option<LandOperators>, String>> {
        match self.rights_batch(parcels).await {
            Ok(rights) => rights
                .into_iter()
                .map(|r| Ok(r.map(|r| r.operators)))
                .collect(),
            Err(e) => {
                let e = e.to_string();
                parcels.iter().map(|_| Err(e.clone())).collect()
            }
        }
    }
}
