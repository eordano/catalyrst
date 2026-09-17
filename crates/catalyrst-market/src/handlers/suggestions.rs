use axum::extract::{Query, State};
use axum::http::header::CACHE_CONTROL;
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Response};
use axum::Json;

use crate::auth_chain;
use crate::http::params::Params;
use crate::http::response::ApiError;
use crate::ports::suggestions::{
    normalize_body_shape, normalize_category, SuggestionRequest, MAX_EQUIPPED, MAX_EXCLUDE,
    MAX_SEEDS,
};
use crate::AppState;

/// Request-size guard, an order of magnitude above the source's own budget.
///
/// The budget itself is applied AFTER validation, by `to_item_ids`: an entry that does not parse
/// -- a base avatar, a name, a foreign-chain URN -- must not spend a slot a usable id could have
/// had, or a caller whose leading entries happen to be unparseable gets a thinner profile than
/// the same caller who sent them in a different order. This ceiling only stops one request
/// carrying a list long enough to be an attack on its own.
const REQUEST_LIST_CEILING: usize = 10;

/// Comma-separated OR repeated, either spelling.
fn id_list(p: &Params, key: &str, cap: usize) -> Vec<String> {
    p.get_list(key, &[])
        .into_iter()
        .flat_map(|value| {
            value
                .split(',')
                .map(|part| part.trim().to_string())
                .filter(|part| !part.is_empty())
                .collect::<Vec<_>>()
        })
        .take(cap.saturating_mul(REQUEST_LIST_CEILING))
        .collect()
}

/// `/v3/catalog/suggested`.
///
/// `?address=` WINS over the verified signer for the profile: a caller who signed and then asked
/// about somebody else means the somebody else, and gets the half of that profile that is public
/// on-chain anyway (what it bought, what it is wearing). Favourites are the half that is not, so
/// they are read only when the proven identity IS the wallet the rail is being built for --
/// mixing one person's holdings with another's saved items produces a rail belonging to neither.
/// A caller who signed and named nobody means themselves.
///
/// `Cache-Control: private, no-store` is not optional here. The response is built from one
/// wallet's purchases, favourites and worn items; a shared cache holding it would serve one
/// person's taste profile to the next caller through the same proxy.
pub async fn get_suggested_catalog(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(pairs): Query<Vec<(String, String)>>,
) -> Result<Response, ApiError> {
    let signer = auth_chain::optional_signer(&headers, "get", "/v3/catalog/suggested")
        .await?
        .map(|s| s.to_lowercase());
    let p = Params::new(&pairs);

    let request = suggestion_request(&p, signer.as_deref());
    let favorites_wallet = favorites_for(&p, signer.as_deref());

    let body = state
        .suggestions
        .get_suggestions(
            &request,
            favorites_wallet.as_deref(),
            state.mana_usd_rate.get_rate(),
        )
        .await?;

    Ok(([(CACHE_CONTROL, "private, no-store")], Json(body)).into_response())
}

/// An address that is not an address is IGNORED rather than carried through: a junk value would
/// otherwise become a distinct profile lookup that can only ever come back empty.
fn suggestion_request(p: &Params, signer: Option<&str>) -> SuggestionRequest {
    SuggestionRequest {
        wallet: p
            .get_address("address", true, None)
            .or_else(|| signer.map(str::to_string)),
        first: p.get_number("first", None).map(|n| n as i64),
        seeds: id_list(p, "seeds", MAX_SEEDS),
        equipped: id_list(p, "equipped", MAX_EQUIPPED),
        exclude: id_list(p, "exclude", MAX_EXCLUDE),
        category: normalize_category(p.get_string("category", None).as_deref()),
        body_shape: normalize_body_shape(p.get_string("bodyShape", None).as_deref()),
    }
}

fn favorites_for(p: &Params, signer: Option<&str>) -> Option<String> {
    let address = p.get_address("address", true, None);
    signer
        .filter(|s| address.as_deref().is_none_or(|a| a == *s))
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONTRACT: &str = "0x1234567890abcdef1234567890abcdef12345678";
    const SIGNER: &str = "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const OTHER: &str = "0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    fn pairs(values: &[(&str, &str)]) -> Vec<(String, String)> {
        values
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect()
    }

    /// The shop sends `seeds`, comma-joined. Reading `seed` instead discards every one of them,
    /// which empties the profile of exactly the signed-out visitor the seeds exist for.
    #[test]
    fn the_seed_list_is_read_from_the_plural_key_the_shop_sends() {
        let pairs = pairs(&[("seeds", &format!("{CONTRACT}-1,{CONTRACT}-2"))]);
        let request = suggestion_request(&Params::new(&pairs), None);
        assert_eq!(
            request.seeds,
            vec![format!("{CONTRACT}-1"), format!("{CONTRACT}-2")]
        );

        let singular = pairs_singular();
        let request = suggestion_request(&Params::new(&singular), None);
        assert!(request.seeds.is_empty(), "{:?}", request.seeds);
    }

    fn pairs_singular() -> Vec<(String, String)> {
        pairs(&[("seed", &format!("{CONTRACT}-1"))])
    }

    /// The bracket spelling and repeated keys are the same list.
    #[test]
    fn the_seed_list_accepts_repeated_and_bracketed_keys() {
        let pairs = pairs(&[
            ("seeds[]", &format!("{CONTRACT}-1")),
            ("seeds[]", &format!("{CONTRACT}-2")),
        ]);
        let request = suggestion_request(&Params::new(&pairs), None);
        assert_eq!(request.seeds.len(), 2);
    }

    /// A signed caller who names nobody means themselves, and reads their own favourites.
    #[test]
    fn a_signed_caller_naming_nobody_gets_their_own_profile_and_favourites() {
        let pairs = pairs(&[]);
        let p = Params::new(&pairs);
        assert_eq!(
            suggestion_request(&p, Some(SIGNER)).wallet,
            Some(SIGNER.to_string())
        );
        assert_eq!(favorites_for(&p, Some(SIGNER)), Some(SIGNER.to_string()));
    }

    /// Naming their own address changes nothing.
    #[test]
    fn a_signed_caller_naming_themselves_is_the_same_request() {
        let pairs = pairs(&[("address", SIGNER)]);
        let p = Params::new(&pairs);
        assert_eq!(
            suggestion_request(&p, Some(SIGNER)).wallet,
            Some(SIGNER.to_string())
        );
        assert_eq!(favorites_for(&p, Some(SIGNER)), Some(SIGNER.to_string()));
    }

    /// Asking about SOMEONE ELSE gets that someone else's public half and no favourites: the
    /// query parameter wins the profile, and the signature unlocks nothing it did not prove.
    #[test]
    fn a_signed_caller_naming_someone_else_gets_that_profile_without_favourites() {
        let pairs = pairs(&[("address", OTHER)]);
        let p = Params::new(&pairs);
        assert_eq!(
            suggestion_request(&p, Some(SIGNER)).wallet,
            Some(OTHER.to_string())
        );
        assert_eq!(favorites_for(&p, Some(SIGNER)), None);
    }

    /// A value that is not an address is not a distinct profile, it is no profile.
    #[test]
    fn a_junk_address_is_ignored_rather_than_looked_up() {
        let pairs = pairs(&[("address", "not-an-address")]);
        let p = Params::new(&pairs);
        assert_eq!(suggestion_request(&p, None).wallet, None);
        assert_eq!(favorites_for(&p, Some(SIGNER)), Some(SIGNER.to_string()));
    }

    /// Unparseable leading entries must not spend the budget: the cap belongs after validation.
    #[test]
    fn unusable_entries_do_not_spend_the_equipped_budget() {
        let mut values: Vec<String> = (0..5)
            .map(|i| format!("urn:decentraland:off-chain:base-avatars:eyebrows_0{i}"))
            .collect();
        values.extend((0..MAX_EQUIPPED).map(|i| format!("{CONTRACT}-{i}")));
        let joined = values.join(",");
        let pairs = pairs(&[("equipped", joined.as_str())]);
        let request = suggestion_request(&Params::new(&pairs), None);
        assert_eq!(
            crate::ports::suggestions::to_item_ids(&request.equipped, MAX_EQUIPPED).len(),
            MAX_EQUIPPED
        );
    }
}
