use std::time::Duration;

use serde_json::{json, Value as Json};
use sqlx::postgres::PgPoolOptions;
use sqlx::{PgPool, Row};
use uuid::Uuid;

pub struct FirstWearPools {
    pub own: PgPool,
    pub content: PgPool,
    pub social: PgPool,
    pub squid: PgPool,
    pub telemetry: Option<PgPool>,
}

const POLL_SECS: u64 = 60;
const BATCH: i64 = 200;
pub const FIRST_WEAR_TYPE: &str = "friend_first_wear";

pub const EXPERIMENT_KEY: &str = "ffw_rules";
pub const RATE_WINDOW_MS: i64 = 3_600_000;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FfwRule {
    Off,
    Limit1h,
    OnlineBypass,
    Unlimited,
}

impl FfwRule {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Limit1h => "limit_1h",
            Self::OnlineBypass => "online_bypass",
            Self::Unlimited => "unlimited",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "off" => Self::Off,
            "limit_1h" => Self::Limit1h,
            "online_bypass" => Self::OnlineBypass,
            "unlimited" => Self::Unlimited,
            _ => return None,
        })
    }
}

pub const DEFAULT_ARMS: &[(FfwRule, u32)] = &[
    (FfwRule::Off, 10),
    (FfwRule::Limit1h, 20),
    (FfwRule::OnlineBypass, 60),
    (FfwRule::Unlimited, 10),
];

#[derive(Clone, Debug, Default)]
pub struct FfwExperiment {
    pub killed: bool,
    pub forced: Option<FfwRule>,
    pub weights: Vec<(FfwRule, u32)>,
}

impl FfwExperiment {
    pub fn arms(&self) -> &[(FfwRule, u32)] {
        if self.weights.is_empty() {
            DEFAULT_ARMS
        } else {
            &self.weights
        }
    }
    pub fn rule_for(&self, address: &str) -> FfwRule {
        if let Some(f) = self.forced {
            return f;
        }
        bucket_rule(address, self.arms())
    }
}

pub fn cyrb53(s: &str) -> u64 {
    let mut h1: u32 = 0xdead_beef;
    let mut h2: u32 = 0x41c6_ce57;
    for ch in s.encode_utf16() {
        h1 = (h1 ^ u32::from(ch)).wrapping_mul(2_654_435_761);
        h2 = (h2 ^ u32::from(ch)).wrapping_mul(1_597_334_677);
    }
    h1 = (h1 ^ (h1 >> 16)).wrapping_mul(2_246_822_507);
    h1 ^= (h2 ^ (h2 >> 13)).wrapping_mul(3_266_489_909);
    h2 = (h2 ^ (h2 >> 16)).wrapping_mul(2_246_822_507);
    h2 ^= (h1 ^ (h1 >> 13)).wrapping_mul(3_266_489_909);
    4_294_967_296u64 * u64::from(h2 & 2_097_151) + u64::from(h1)
}

fn bucket_rule(address: &str, arms: &[(FfwRule, u32)]) -> FfwRule {
    if arms.is_empty() {
        return FfwRule::OnlineBypass;
    }
    if arms.len() == 1 {
        return arms[0].0;
    }
    let total: u64 = arms.iter().map(|(_, w)| u64::from(*w)).sum();
    let total = if total == 0 { arms.len() as u64 } else { total };
    let unit = cyrb53(&format!("{}:{}", address, EXPERIMENT_KEY)) as f64 / 9_007_199_254_740_992f64;
    let target = unit * total as f64;
    let mut acc = 0f64;
    for (rule, w) in arms {
        let w = if *w > 0 { f64::from(*w) } else { 1.0 };
        acc += w;
        if target < acc {
            return *rule;
        }
    }
    arms[arms.len() - 1].0
}

pub fn rate_allows(
    rule: FfwRule,
    last_emit_ms: Option<i64>,
    last_fetch_ms: Option<i64>,
    now_ms: i64,
) -> bool {
    match rule {
        FfwRule::Off => false,
        FfwRule::Unlimited => true,
        FfwRule::Limit1h => last_emit_ms.is_none_or(|t| now_ms - t >= RATE_WINDOW_MS),
        FfwRule::OnlineBypass => last_emit_ms
            .is_none_or(|t| now_ms - t >= RATE_WINDOW_MS || last_fetch_ms.is_some_and(|f| f > t)),
    }
}

async fn load_experiment(telemetry: Option<&PgPool>) -> FfwExperiment {
    let Some(pool) = telemetry else {
        return FfwExperiment::default();
    };
    let row: Option<(bool, Option<String>, Json)> = sqlx::query_as(
        "SELECT killed, forced_variant, flags FROM experiment_overrides WHERE exp_key = $1",
    )
    .bind(EXPERIMENT_KEY)
    .fetch_optional(pool)
    .await
    .unwrap_or_default();
    let Some((killed, forced, flags)) = row else {
        return FfwExperiment::default();
    };
    let weights = flags["weights"]
        .as_object()
        .map(|m| {
            m.iter()
                .filter_map(|(k, v)| Some((FfwRule::parse(k)?, v.as_u64().unwrap_or(0) as u32)))
                .collect()
        })
        .unwrap_or_default();
    FfwExperiment {
        killed,
        forced: forced.as_deref().and_then(FfwRule::parse),
        weights,
    }
}

async fn funnel_event(telemetry: Option<&PgPool>, kind: &str, body: Json) {
    let Some(pool) = telemetry else { return };
    let _ = sqlx::query(
        "INSERT INTO telemetry_events (source, project, event_kind, body) VALUES ($1, $2, $3, $4)",
    )
    .bind("catalyrst-notifications")
    .bind("dcl-one")
    .bind(kind)
    .bind(body)
    .execute(pool)
    .await;
}

pub async fn connect_pool(url: &str) -> Result<PgPool, sqlx::Error> {
    PgPoolOptions::new()
        .max_connections(2)
        .acquire_timeout(Duration::from_secs(10))
        .connect(url)
        .await
}

pub fn spawn_first_wear(pools: FirstWearPools, shop_item_base: String) {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(Duration::from_secs(POLL_SECS));
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            ticker.tick().await;
            match run_once(&pools, &shop_item_base).await {
                Ok(0) => {}
                Ok(n) => tracing::info!(emitted = n, "friend_first_wear notifications emitted"),
                Err(e) => tracing::warn!(error = %e, "friend_first_wear pass failed"),
            }
        }
    });
}

fn normalize_urn(raw: &str) -> String {
    let parts: Vec<&str> = raw.split(':').collect();
    let take = parts.len().min(6);
    parts[..take].join(":").to_lowercase()
}

fn is_market_urn(urn: &str) -> bool {
    urn.contains(":collections-")
}

fn profile_wearables(metadata: &Json) -> Vec<String> {
    let avatar = &metadata["v"]["avatars"][0];
    let list = avatar["avatar"]["wearables"].as_array();
    let mut out: Vec<String> = list
        .map(|ws| {
            ws.iter()
                .filter_map(|w| w.as_str())
                .map(normalize_urn)
                .filter(|u| is_market_urn(u))
                .collect()
        })
        .unwrap_or_default();
    out.sort();
    out.dedup();
    out
}

fn profile_name(metadata: &Json) -> Option<String> {
    metadata["v"]["avatars"][0]["name"]
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
}

fn image_catalyst_base() -> String {
    std::env::var("FIRST_WEAR_IMAGE_BASE")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "https://peer.decentraland.org".to_string())
}

fn repoint_image(image: Option<String>) -> Option<String> {
    let img = image?;
    match img.find("/lambdas/") {
        Some(ix) => Some(format!("{}{}", image_catalyst_base(), &img[ix..])),
        None => Some(img),
    }
}

fn short_address(addr: &str) -> String {
    if addr.len() > 12 {
        format!("{}…{}", &addr[..6], &addr[addr.len() - 4..])
    } else {
        addr.to_string()
    }
}

struct ResolvedItem {
    item_id: String,
    name: String,
    creator: String,
    image: Option<String>,
}

pub async fn run_once(pools: &FirstWearPools, shop_item_base: &str) -> anyhow::Result<u64> {
    let experiment = load_experiment(pools.telemetry.as_ref()).await;
    if experiment.killed {
        tracing::debug!("ffw_rules experiment is killed; suppressing all emissions this pass");
    }
    let cursor: Option<chrono::NaiveDateTime> =
        sqlx::query_scalar("SELECT last_seen FROM first_wear_cursor WHERE id = 1")
            .fetch_optional(&pools.own)
            .await?;
    let Some(cursor) = cursor else {
        sqlx::query("INSERT INTO first_wear_cursor (id, last_seen) VALUES (1, now() AT TIME ZONE 'utc') ON CONFLICT (id) DO NOTHING")
            .execute(&pools.own)
            .await?;
        tracing::info!("friend_first_wear cursor initialized (no history replay)");
        return Ok(0);
    };

    let rows = sqlx::query(
        "SELECT entity_pointers[1] AS address, entity_timestamp, entity_metadata::text AS meta
         FROM deployments
         WHERE entity_type = 'profile' AND entity_timestamp > $1
         ORDER BY entity_timestamp ASC
         LIMIT $2",
    )
    .bind(cursor)
    .bind(BATCH)
    .fetch_all(&pools.content)
    .await?;
    if rows.is_empty() {
        return Ok(0);
    }

    let mut emitted = 0u64;
    let mut max_ts = cursor;
    for row in rows {
        let address: String = row
            .try_get::<Option<String>, _>("address")?
            .unwrap_or_default()
            .to_lowercase();
        let entity_ts: chrono::NaiveDateTime = row.try_get("entity_timestamp")?;
        if entity_ts > max_ts {
            max_ts = entity_ts;
        }
        if address.is_empty() {
            continue;
        }
        let meta: Json = match serde_json::from_str(&row.try_get::<String, _>("meta")?) {
            Ok(m) => m,
            Err(_) => continue,
        };
        let worn_now = profile_wearables(&meta);
        let entity_ms = entity_ts.and_utc().timestamp_millis();

        let has_baseline: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM first_wear_baseline WHERE address = $1)",
        )
        .bind(&address)
        .fetch_one(&pools.own)
        .await?;

        if !has_baseline {
            let history = full_history_wearables(&pools.content, &address).await?;
            let all: Vec<String> = history.into_iter().chain(worn_now).collect();
            seed_history(&pools.own, &address, &all, entity_ms).await?;
            sqlx::query(
                "INSERT INTO first_wear_baseline (address, seeded_at) VALUES ($1, $2)
                 ON CONFLICT (address) DO NOTHING",
            )
            .bind(&address)
            .bind(entity_ms)
            .execute(&pools.own)
            .await?;
            continue;
        }
        if worn_now.is_empty() {
            continue;
        }

        let baseline: Vec<String> =
            sqlx::query_scalar("SELECT urn FROM worn_history WHERE address = $1")
                .bind(&address)
                .fetch_all(&pools.own)
                .await?;

        for urn in &worn_now {
            if baseline.contains(urn) {
                continue;
            }
            let claimed = sqlx::query(
                "INSERT INTO worn_history (address, urn, first_seen) VALUES ($1, $2, $3)
                 ON CONFLICT (address, urn) DO NOTHING",
            )
            .bind(&address)
            .bind(urn)
            .bind(entity_ms)
            .execute(&pools.own)
            .await?;
            if claimed.rows_affected() == 0 {
                continue;
            }

            let Some(item) = resolve_buyable_item(&pools.squid, urn).await? else {
                tracing::debug!(
                    urn,
                    "first wear not notifiable (item unknown or not buyable)"
                );
                continue;
            };
            let friends = active_friends(&pools.social, &address).await?;
            if friends.is_empty() {
                continue;
            }

            let wearer = profile_name(&meta).unwrap_or_else(|| short_address(&address));
            let creator_name = creator_profile_name(&pools.content, &item.creator)
                .await?
                .unwrap_or_else(|| short_address(&item.creator));
            let image = repoint_image(item.image.clone());
            let now_ms = chrono::Utc::now().timestamp_millis();

            for friend in &friends {
                let rule = experiment.rule_for(friend);
                let allowed = if experiment.killed {
                    false
                } else {
                    let last_emit: Option<i64> = sqlx::query_scalar(
                        "SELECT max(timestamp) FROM notifications WHERE address = $1 AND type = $2",
                    )
                    .bind(friend)
                    .bind(FIRST_WEAR_TYPE)
                    .fetch_one(&pools.own)
                    .await?;
                    let last_fetch: Option<i64> = sqlx::query_scalar(
                        "SELECT last_fetch_at FROM notification_reader_seen WHERE address = $1",
                    )
                    .bind(friend)
                    .fetch_optional(&pools.own)
                    .await?;
                    rate_allows(rule, last_emit, last_fetch, now_ms)
                };

                if !allowed {
                    let reason = if experiment.killed {
                        "killed"
                    } else if rule == FfwRule::Off {
                        "arm_off"
                    } else {
                        "rate"
                    };
                    tracing::debug!(friend, rule = rule.as_str(), reason, "ffw suppressed");
                    funnel_event(
                        pools.telemetry.as_ref(),
                        "ffw_suppressed",
                        json!({
                            "arm": rule.as_str(), "reason": reason, "recipient": friend,
                            "itemId": item.item_id, "urn": urn, "wearer": address,
                        }),
                    )
                    .await;
                    continue;
                }

                let nid = Uuid::new_v4();
                let link = format!("{}{}?src=ffw&nid={}", shop_item_base, item.item_id, nid);
                let metadata = json!({
                    "title": format!("{} is wearing something new", wearer),
                    "description": format!(
                        "Your friend {} is now wearing {} by {} for the first time!",
                        wearer, item.name, creator_name
                    ),
                    "link": link,
                    "image": image,
                    "friendAddress": address,
                    "friendName": wearer,
                    "wearableUrn": urn,
                    "wearableName": item.name,
                    "creatorName": creator_name,
                    "itemId": item.item_id,
                    "arm": rule.as_str(),
                });
                sqlx::query(
                    "INSERT INTO notifications (id, address, type, metadata, timestamp)
                     VALUES ($1, $2, $3, $4, $5)",
                )
                .bind(nid)
                .bind(friend)
                .bind(FIRST_WEAR_TYPE)
                .bind(&metadata)
                .bind(now_ms)
                .execute(&pools.own)
                .await?;
                emitted += 1;
                funnel_event(
                    pools.telemetry.as_ref(),
                    "ffw_emitted",
                    json!({
                        "nid": nid, "arm": rule.as_str(), "recipient": friend,
                        "itemId": item.item_id, "urn": urn, "wearer": address,
                    }),
                )
                .await;
            }
        }
    }

    sqlx::query("UPDATE first_wear_cursor SET last_seen = $1 WHERE id = 1")
        .bind(max_ts)
        .execute(&pools.own)
        .await?;
    Ok(emitted)
}

async fn full_history_wearables(content: &PgPool, address: &str) -> anyhow::Result<Vec<String>> {
    let rows = sqlx::query_scalar::<_, String>(
        "SELECT entity_metadata::text FROM deployments
         WHERE entity_type = 'profile' AND entity_pointers @> ARRAY[$1]",
    )
    .bind(address)
    .fetch_all(content)
    .await?;
    let mut urns = Vec::new();
    for raw in rows {
        if let Ok(meta) = serde_json::from_str::<Json>(&raw) {
            urns.extend(profile_wearables(&meta));
        }
    }
    urns.sort();
    urns.dedup();
    Ok(urns)
}

async fn seed_history(
    own: &PgPool,
    address: &str,
    urns: &[String],
    ts_ms: i64,
) -> anyhow::Result<()> {
    if urns.is_empty() {
        return Ok(());
    }
    sqlx::query(
        "INSERT INTO worn_history (address, urn, first_seen)
         SELECT $1, u, $3 FROM unnest($2::text[]) AS t(u)
         ON CONFLICT (address, urn) DO NOTHING",
    )
    .bind(address)
    .bind(urns)
    .bind(ts_ms)
    .execute(own)
    .await?;
    Ok(())
}

async fn resolve_buyable_item(squid: &PgPool, urn: &str) -> anyhow::Result<Option<ResolvedItem>> {
    let row = sqlx::query(
        r#"SELECT i.collection_id || '-' || i.blockchain_id::text AS item_id,
                  COALESCE(w.name, e.name) AS name,
                  i.creator, i.image,
                  ((i.search_is_store_minter AND i.available > 0)
                   OR EXISTS (SELECT 1 FROM squid_marketplace."order" o
                              WHERE o.status = 'open' AND o.expires_at_normalized > now()
                                AND o.item_id = i.id)) AS buyable
           FROM squid_marketplace.item i
           LEFT JOIN squid_marketplace.metadata m ON m.id = i.metadata_id
           LEFT JOIN squid_marketplace.wearable w ON w.id = m.wearable_id
           LEFT JOIN squid_marketplace.emote e ON e.id = m.emote_id
           WHERE i.urn = $1"#,
    )
    .bind(urn)
    .fetch_optional(squid)
    .await?;
    let Some(row) = row else { return Ok(None) };
    if !row.try_get::<bool, _>("buyable")? {
        return Ok(None);
    }
    let name: Option<String> = row.try_get("name")?;
    let Some(name) = name else { return Ok(None) };
    Ok(Some(ResolvedItem {
        item_id: row.try_get("item_id")?,
        name,
        creator: row
            .try_get::<Option<String>, _>("creator")?
            .unwrap_or_default()
            .to_lowercase(),
        image: row.try_get("image")?,
    }))
}

async fn active_friends(social: &PgPool, address: &str) -> anyhow::Result<Vec<String>> {
    let rows = sqlx::query_scalar::<_, String>(
        "SELECT CASE WHEN lower(address_requester) = $1
                     THEN lower(address_requested)
                     ELSE lower(address_requester) END
         FROM friendships
         WHERE is_active
           AND (lower(address_requester) = $1 OR lower(address_requested) = $1)",
    )
    .bind(address)
    .fetch_all(social)
    .await?;
    Ok(rows.into_iter().filter(|f| f != address).collect())
}

async fn creator_profile_name(content: &PgPool, creator: &str) -> anyhow::Result<Option<String>> {
    if creator.is_empty() {
        return Ok(None);
    }
    let raw: Option<String> = sqlx::query_scalar(
        "SELECT entity_metadata::text FROM deployments
         WHERE entity_type = 'profile' AND deleter_deployment IS NULL
           AND entity_pointers @> ARRAY[$1]
         ORDER BY entity_timestamp DESC LIMIT 1",
    )
    .bind(creator)
    .fetch_optional(content)
    .await?;
    Ok(raw
        .and_then(|r| serde_json::from_str::<Json>(&r).ok())
        .and_then(|m| profile_name(&m)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cyrb53_matches_the_js_reference() {
        assert_eq!(cyrb53("a"), 7929297801672961);
        assert_eq!(cyrb53("hello"), 4625896200565286);
        assert_eq!(
            cyrb53("0x229d59c24f50698600117ed1ae40aa0ddf6e2c69:ffw_rules"),
            2734530341693953
        );
    }

    #[test]
    fn bucketing_is_deterministic_and_respects_forced() {
        let exp = FfwExperiment::default();
        let a = exp.rule_for("0x229d59c24f50698600117ed1ae40aa0ddf6e2c69");
        let b = exp.rule_for("0x229d59c24f50698600117ed1ae40aa0ddf6e2c69");
        assert_eq!(a, b);
        let forced = FfwExperiment {
            forced: Some(FfwRule::Off),
            ..Default::default()
        };
        assert_eq!(forced.rule_for("0xanything"), FfwRule::Off);
    }

    #[test]
    fn rate_decision_table() {
        let now = 10_000_000;
        let recent_ts = now - RATE_WINDOW_MS / 2;
        let recent = Some(recent_ts);
        let old = Some(now - RATE_WINDOW_MS - 1);
        assert!(!rate_allows(FfwRule::Off, None, None, now));
        assert!(rate_allows(FfwRule::Unlimited, recent, None, now));
        assert!(rate_allows(FfwRule::Limit1h, None, None, now));
        assert!(!rate_allows(FfwRule::Limit1h, recent, Some(now), now));
        assert!(rate_allows(FfwRule::Limit1h, old, None, now));
        assert!(rate_allows(FfwRule::OnlineBypass, None, None, now));
        assert!(!rate_allows(FfwRule::OnlineBypass, recent, None, now));
        assert!(!rate_allows(
            FfwRule::OnlineBypass,
            recent,
            Some(recent_ts - 1),
            now
        ));
        assert!(rate_allows(
            FfwRule::OnlineBypass,
            recent,
            Some(recent_ts + 1),
            now
        ));
        assert!(rate_allows(FfwRule::OnlineBypass, old, None, now));
    }

    #[test]
    fn urn_normalization_strips_token_and_lowercases() {
        assert_eq!(
            normalize_urn("urn:decentraland:matic:collections-v2:0xABC:1:4567"),
            "urn:decentraland:matic:collections-v2:0xabc:1"
        );
        assert_eq!(
            normalize_urn("urn:decentraland:off-chain:base-avatars:eyes_00"),
            "urn:decentraland:off-chain:base-avatars:eyes_00"
        );
    }

    #[test]
    fn image_repointed_to_our_catalyst() {
        assert_eq!(
            repoint_image(Some(
                "https://peer.decentraland.org/lambdas/collections/contents/urn:x/thumbnail".into()
            )),
            Some(
                "https://peer.decentraland.org/lambdas/collections/contents/urn:x/thumbnail".into()
            )
        );
        assert_eq!(
            repoint_image(Some("https://other/img.png".into())),
            Some("https://other/img.png".into())
        );
        assert_eq!(repoint_image(None), None);
    }

    #[test]
    fn market_urn_filter_excludes_base_avatars() {
        assert!(is_market_urn(
            "urn:decentraland:matic:collections-v2:0xabc:1"
        ));
        assert!(is_market_urn(
            "urn:decentraland:mainnet:collections-v1:halloween_2019:jester_feet"
        ));
        assert!(!is_market_urn(
            "urn:decentraland:off-chain:base-avatars:balbo_beard"
        ));
    }

    #[test]
    fn wearables_extraction_normalizes_and_dedupes() {
        let meta = serde_json::json!({"v": {"avatars": [{"name": "Wearer", "avatar": {"wearables": [
            "urn:decentraland:off-chain:base-avatars:eyes_00",
            "urn:decentraland:matic:collections-v2:0xABC:1:999",
            "urn:decentraland:matic:collections-v2:0xabc:1"
        ]}}]}});
        assert_eq!(
            profile_wearables(&meta),
            vec!["urn:decentraland:matic:collections-v2:0xabc:1".to_string()]
        );
        assert_eq!(profile_name(&meta).as_deref(), Some("Wearer"));
    }
}
