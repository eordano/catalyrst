//! Newtypes that carry provenance: a value that came from a peer and a value that came
//! from a local request path must not be the same Rust type, because a `bool` on a
//! shared type means every reader has to remember to check it.

/// An admitted peer's id, in the canonical form defined by
/// [`catalyrst_fed::canonical_peer_id`].
///
/// Minted only by [`crate::fed::peers::AdmittedPeer::admit`], so holding one is
/// evidence that the entry cleared every gate in
/// [`crate::fed::peers::PeerNotAdmitted`], and evidence of *which* entry: both sides
/// now fold through the same function and `FederationRegistry::parse_file` refuses a
/// file naming two case-variant entries, so one `PeerId` is exactly one line of the
/// peer file. (Previously `catalyrst-fed` keyed on the raw id while this crate
/// lowercased it, and whichever of the two polled second erased the other's mirror.)
///
/// Distinct from [`catalyrst_fed::PeerId`], a bare `String` alias for any id in a peer
/// file, admitted or not -- deliberately not re-exported here.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize)]
pub struct PeerId(String);

impl PeerId {
    /// The only constructor, `pub(crate)` so no caller outside this crate can mint one.
    /// Named for its provenance so a reviewer sees at the call site that the value came
    /// out of admission.
    pub(crate) fn from_admitted(canonical: &str) -> Self {
        debug_assert_eq!(
            canonical,
            catalyrst_fed::canonical_peer_id(canonical),
            "PeerId::from_admitted must be handed an id already folded by \
             catalyrst_fed::canonical_peer_id; the remote_worlds CHECK constraints and \
             the registry's own keying both assume that exact form"
        );
        Self(canonical.to_string())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for PeerId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// A world name that arrived on a local request path: the only type
/// [`crate::handlers::permissions::resolve_world_owner`] accepts. No
/// `From`/`TryFrom<RemoteWorldName>` and no constructor taking one, so a peer-reported
/// name cannot reach the owner resolver without somebody writing a line that names it.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LocalWorldName(String);

impl LocalWorldName {
    /// The only constructor. Named for its provenance, in the style of
    /// `CommunityBanAuthority::resolve_from_gossip_envelope_relayed_by_a_peer_catalyst_server`
    /// (catalyrst-social-service/src/rest/fed/consumer.rs), so a reviewer sees where the
    /// value came from at the call site.
    ///
    /// Lowercasing preserves existing behaviour -- `resolve_world_owner` already
    /// lowercased internally before this type existed.
    pub fn from_request_path(raw: &str) -> Self {
        Self(raw.to_ascii_lowercase())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for LocalWorldName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// A world name as a *peer reported it*. Not verified, resolved, or agreed to serve.
///
/// Deliberately absent: `Deref<Target = str>`, `AsRef<str>`, `Display`,
/// `Into<String>`, `Into<LocalWorldName>`. The single escape hatch is
/// `as_peer_reported_str`, greppable by name and gated by
/// [`super::wire::provenance_gate`] to `fed/{names,wire,store,handlers}.rs`.
///
/// Needed here but not in social federation because `community_id_hex(creator, name,
/// nonce)` puts the creator inside the identifier, whereas nothing inside `foo.dcl.eth`
/// distinguishes our record from a peer's.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RemoteWorldName(String);

/// Longer than any real `.dcl.eth` name; the cap stops a peer spending our row width on
/// a megabyte of its own choosing.
const MAX_WORLD_NAME_LEN: usize = 255;

impl RemoteWorldName {
    /// Fails closed on anything that is not a plausible world name: `/`, `%`,
    /// whitespace, control bytes and non-ASCII are rejected, so `../`, `%00` and an
    /// embedded URL cannot survive.
    pub(crate) fn from_peer_listing(raw: &str) -> Option<Self> {
        Self::shaped(raw)
    }

    /// An operator naming a row *already in the mirror*, on the admin veto route. Same
    /// shape rules, different provenance: it asserts nothing about the peer and reaches
    /// nothing but a primary-key lookup in `remote_worlds`.
    ///
    /// `pub` where [`Self::from_peer_listing`] is `pub(crate)`, because this one cannot
    /// introduce a peer's claim (only an authenticated admin route and tests reach it)
    /// while `from_peer_listing` is the trust boundary and stays sealed inside `fed`.
    /// Neither yields a [`LocalWorldName`], so neither reaches `resolve_world_owner`.
    pub fn from_operator_veto_path(raw: &str) -> Option<Self> {
        Self::shaped(raw)
    }

    fn shaped(raw: &str) -> Option<Self> {
        let n = raw.trim().to_ascii_lowercase();
        let shaped = !n.is_empty()
            && n.len() <= MAX_WORLD_NAME_LEN
            && n.bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_'));
        shaped.then_some(Self(n))
    }

    /// The one way to get the bytes out. Greppable on purpose.
    pub fn as_peer_reported_str(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_world_name_rejects_shapes() {
        assert_eq!(
            RemoteWorldName::from_peer_listing("FOO.DCL.ETH")
                .expect("a plain name is accepted")
                .as_peer_reported_str(),
            "foo.dcl.eth"
        );
        assert_eq!(
            RemoteWorldName::from_peer_listing("  spaced.dcl.eth  ")
                .expect("surrounding whitespace is trimmed, not rejected")
                .as_peer_reported_str(),
            "spaced.dcl.eth"
        );

        for hostile in [
            "",
            "   ",
            "../../etc/passwd",
            "a/b",
            "name%00.dcl.eth",
            "name with spaces.dcl.eth",
            "n\u{E4}me.dcl.eth",
            "name\n.dcl.eth",
            "http://evil.example/x",
            "name:8080",
            "name?query=1",
        ] {
            assert!(
                RemoteWorldName::from_peer_listing(hostile).is_none(),
                "peer-reported name {hostile:?} must be refused, not stored"
            );
        }

        let too_long = "a".repeat(MAX_WORLD_NAME_LEN + 1);
        assert!(RemoteWorldName::from_peer_listing(&too_long).is_none());
        let at_limit = "a".repeat(MAX_WORLD_NAME_LEN);
        assert!(RemoteWorldName::from_peer_listing(&at_limit).is_some());
    }

    #[test]
    fn the_operator_veto_constructor_applies_the_same_shape_rules() {
        assert!(RemoteWorldName::from_operator_veto_path("../x").is_none());
        assert_eq!(
            RemoteWorldName::from_operator_veto_path("A.dcl.eth")
                .unwrap()
                .as_peer_reported_str(),
            "a.dcl.eth"
        );
    }

    #[test]
    fn a_local_name_and_a_remote_name_are_not_the_same_type() {
        let local = LocalWorldName::from_request_path("Foo.dcl.eth");
        let remote = RemoteWorldName::from_peer_listing("Foo.dcl.eth").unwrap();
        assert_eq!(local.as_str(), remote.as_peer_reported_str());
        assert_ne!(
            std::any::TypeId::of::<LocalWorldName>(),
            std::any::TypeId::of::<RemoteWorldName>(),
            "equal bytes, different types \u{2014} that is the whole point"
        );
    }
}
