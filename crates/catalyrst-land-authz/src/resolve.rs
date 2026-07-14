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

type ParcelSubjectRow = (
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    bool,
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

    pub async fn operators(&self, x: i32, y: i32) -> Result<Option<LandOperators>, sqlx::Error> {
        let Some(subject) = self.parcel_subject(x, y).await? else {
            return Ok(None);
        };
        let update_managers = self
            .account_grants(&subject.registry, &subject.owner, KIND_UPDATE_MANAGER)
            .await?;
        let approved_for_all = self
            .account_grants(&subject.registry, &subject.owner, KIND_APPROVED_FOR_ALL)
            .await?;
        Ok(Some(LandOperators {
            operator: subject.operator,
            update_operator: subject.update_operator,
            update_managers,
            approved_for_all,
        }))
    }

    pub async fn parcel_owner(&self, x: i32, y: i32) -> Result<Option<String>, sqlx::Error> {
        Ok(self.parcel_subject(x, y).await?.map(|s| s.owner))
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
}
