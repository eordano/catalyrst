use crate::verified_wallet_address::VerifiedWalletAddress;

/// Wallet addresses read from **operator configuration**, never from a request.
///
/// # What a value of this type proves
///
/// That an operator wrote these addresses into a named environment variable. Membership in
/// the list is a fact about the deployment's configuration.
///
/// # What it does NOT prove
///
/// - **Not that any entry is correct.** A well-formed address that is simply the wrong
///   address parses fine and this type cannot tell.
/// - **Not what the list authorizes.** The same shape backs at least five unrelated
///   powers across the fleet -- `ADMIN_ADDRESSES` in `catalyrst-server`, `admin_addresses`
///   in `catalyrst-builder` and `catalyrst-places`, `moderator_addresses` in
///   `catalyrst-comms` -- and they are not interchangeable. Each consumer wraps its own
///   allowlist in its own crate-local authority type with its own name; this type is the
///   storage and the comparison, not the authority.
///
/// # How a value is obtained
///
/// [`Self::parse_comma_separated`], or [`Default`] for an empty list naming no variable.
///
/// # Why this replaces `Vec<String>` / `HashSet<String>`
///
/// Two reasons, both defects that exist today.
///
/// First, the comparison. A bare `Vec<String>` compares against anything stringy, including
/// a request body field. [`Self::contains`] takes
/// [`VerifiedWalletAddress`] and nothing else.
///
/// Second, the parse. `catalyrst_types::parse_eth_address` and
/// `catalyrst_types::is_eth_address` exist and are used by **none** of the four allowlist
/// parsers in the workspace, so a typo in an operator's environment variable yields an
/// entry that matches nothing, silently, forever. Rejected entries are kept here so that
/// startup can log them: see [`Self::entries_rejected_as_not_address_shaped`].
#[derive(Debug, Clone, Default)]
pub struct ConfiguredWalletAllowlist {
    environment_variable_name: &'static str,
    lowercased_addresses: Vec<String>,
    entries_rejected_as_not_address_shaped: Vec<String>,
}

impl ConfiguredWalletAllowlist {
    /// Parse a comma-separated operator-configured list.
    ///
    /// `is_address_shaped` is injected rather than hard-coded so that this crate does not
    /// silently become the workspace's authority on what an address looks like. The
    /// intended argument is `catalyrst_types::is_eth_address`, which is what every one of
    /// these lists *should* have been using:
    ///
    /// ```
    /// use catalyrst_authenticated_principal::ConfiguredWalletAllowlist;
    ///
    /// let allowlist = ConfiguredWalletAllowlist::parse_comma_separated(
    ///     "ADMIN_ADDRESSES",
    ///     Some("0x00000000000000000000000000000000000000AA, 0xtypo"),
    ///     catalyrst_types::is_eth_address,
    /// );
    /// assert_eq!(allowlist.len(), 1);
    /// assert_eq!(allowlist.entries_rejected_as_not_address_shaped(), ["0xtypo"]);
    /// ```
    ///
    /// Entries are trimmed; entries that are empty after trimming are dropped without being
    /// reported, so a trailing comma is not an error. Accepted entries are lowercased.
    /// Order is preserved and duplicates are **not** removed -- an operator's list is
    /// reported back as written, minus whitespace.
    pub fn parse_comma_separated(
        environment_variable_name: &'static str,
        raw: Option<&str>,
        is_address_shaped: impl Fn(&str) -> bool,
    ) -> Self {
        let mut lowercased_addresses = Vec::new();
        let mut entries_rejected_as_not_address_shaped = Vec::new();

        for entry in raw.unwrap_or_default().split(',') {
            let trimmed = entry.trim();
            if trimmed.is_empty() {
                continue;
            }
            if is_address_shaped(trimmed) {
                lowercased_addresses.push(trimmed.to_ascii_lowercase());
            } else {
                entries_rejected_as_not_address_shaped.push(trimmed.to_string());
            }
        }

        Self {
            environment_variable_name,
            lowercased_addresses,
            entries_rejected_as_not_address_shaped,
        }
    }

    /// The only comparison this type offers.
    ///
    /// It takes [`VerifiedWalletAddress`], so a claimed address, a request
    /// body field, a query parameter or a header value cannot be handed to it -- those are
    /// `&str` and this is not.
    pub fn contains(&self, wallet: &VerifiedWalletAddress) -> bool {
        self.lowercased_addresses
            .iter()
            .any(|configured| configured == wallet.as_lowercased_hex_text())
    }

    /// Entries the operator wrote that are not address-shaped, in the order written and in
    /// their original case.
    ///
    /// A non-empty result is a misconfiguration that today fails silently. Log it at
    /// startup, naming [`Self::environment_variable_name`].
    pub fn entries_rejected_as_not_address_shaped(&self) -> &[String] {
        &self.entries_rejected_as_not_address_shaped
    }

    /// The environment variable this list was read from, for the startup log and for the
    /// refusal message. Empty for a [`Default`] list.
    pub fn environment_variable_name(&self) -> &'static str {
        self.environment_variable_name
    }

    /// How many accepted, address-shaped entries the list holds.
    pub fn len(&self) -> usize {
        self.lowercased_addresses.len()
    }

    /// Whether the list authorizes nobody.
    ///
    /// An empty allowlist is a legitimate configuration -- it means "no wallet holds this
    /// power" -- and is **not** the same as an absent one. A caller that wants to fail
    /// closed on an unconfigured list should check this explicitly rather than letting
    /// [`Self::contains`] answer `false` for both cases.
    pub fn is_empty(&self) -> bool {
        self.lowercased_addresses.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ADDRESS_A: &str = "0x00000000000000000000000000000000000000aa";
    const ADDRESS_B: &str = "0x00000000000000000000000000000000000000bb";

    fn wallet(address: &str) -> VerifiedWalletAddress {
        VerifiedWalletAddress::unchecked_for_unit_tests_within_this_crate_only(address)
    }

    fn allowlist(raw: Option<&str>) -> ConfiguredWalletAllowlist {
        ConfiguredWalletAllowlist::parse_comma_separated(
            "ADMIN_ADDRESSES",
            raw,
            catalyrst_types::is_eth_address,
        )
    }

    #[test]
    fn membership_is_case_insensitive_in_both_directions() {
        let list = allowlist(Some(&ADDRESS_A.to_ascii_uppercase().replace("0X", "0x")));
        assert!(list.contains(&wallet(ADDRESS_A)));
        assert!(list.contains(&wallet(&ADDRESS_A.to_ascii_uppercase())));
    }

    #[test]
    fn a_wallet_that_is_not_listed_is_not_a_member() {
        let list = allowlist(Some(ADDRESS_A));
        assert!(!list.contains(&wallet(ADDRESS_B)));
    }

    /// P17: a typo is visible instead of silently matching nothing.
    #[test]
    fn entries_that_are_not_address_shaped_are_reported_not_swallowed() {
        let list = allowlist(Some("0xnotanaddress"));
        assert!(list.is_empty(), "a typo must not become a member");
        assert_eq!(
            list.entries_rejected_as_not_address_shaped(),
            ["0xnotanaddress"]
        );
        assert_eq!(list.environment_variable_name(), "ADMIN_ADDRESSES");
    }

    #[test]
    fn a_good_entry_beside_a_typo_still_works_and_the_typo_is_still_reported() {
        let list = allowlist(Some(&format!("{ADDRESS_A}, 0xtypo, {ADDRESS_B}")));
        assert!(list.contains(&wallet(ADDRESS_A)));
        assert!(list.contains(&wallet(ADDRESS_B)));
        assert_eq!(list.len(), 2);
        assert_eq!(list.entries_rejected_as_not_address_shaped(), ["0xtypo"]);
    }

    #[test]
    fn whitespace_and_trailing_commas_are_tolerated_and_not_reported_as_rejections() {
        let list = allowlist(Some(&format!("  {ADDRESS_A} , , {ADDRESS_B} ,")));
        assert_eq!(list.len(), 2);
        assert!(list.entries_rejected_as_not_address_shaped().is_empty());
    }

    #[test]
    fn an_unset_variable_and_an_empty_variable_both_yield_an_empty_list() {
        for raw in [None, Some(""), Some("   "), Some(",,,")] {
            let list = allowlist(raw);
            assert!(list.is_empty(), "{raw:?} should authorize nobody");
            assert!(list.entries_rejected_as_not_address_shaped().is_empty());
            assert!(!list.contains(&wallet(ADDRESS_A)));
        }
    }

    #[test]
    fn the_default_list_authorizes_nobody_and_names_no_variable() {
        let list = ConfiguredWalletAllowlist::default();
        assert!(list.is_empty());
        assert!(!list.contains(&wallet(ADDRESS_A)));
        assert_eq!(list.environment_variable_name(), "");
    }

    #[test]
    fn duplicates_are_preserved_rather_than_silently_deduplicated() {
        let list = allowlist(Some(&format!("{ADDRESS_A},{ADDRESS_A}")));
        assert_eq!(list.len(), 2);
        assert!(list.contains(&wallet(ADDRESS_A)));
    }

    /// The injected shape predicate is the only thing that decides acceptance, so a caller
    /// migrating a list that historically accepted non-address entries can keep its old
    /// behaviour explicitly rather than by accident.
    #[test]
    fn the_injected_predicate_decides_and_nothing_else_does() {
        let accepts_everything = ConfiguredWalletAllowlist::parse_comma_separated(
            "LEGACY_LIST",
            Some("not-an-address"),
            |_| true,
        );
        assert_eq!(accepts_everything.len(), 1);
        assert!(accepts_everything
            .entries_rejected_as_not_address_shaped()
            .is_empty());
        assert!(accepts_everything.contains(&wallet("not-an-address")));
    }

    // Deliberately impossible, and a compile error today:
    //
    //   list.contains("0xdeadbeef");                                     // E0308
    //   list.contains(&claimed_wallet_address_from_the_request_body);    // E0308
}
