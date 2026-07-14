//! Newtypes that carry provenance.
//!
//! The rule this file exists to enforce: a value that came from a peer and a value
//! that came from a local request path must not be the same Rust type. A `bool` on a
//! shared type means every reader has to remember to check it, forever, including the
//! reader who is added next quarter. A distinct type means the compiler remembers.

/// An admitted peer's id, in the canonical form defined by
/// [`catalyrst_fed::canonical_peer_id`].
///
/// Minted only by [`crate::fed::peers::AdmittedPeer::admit`], so holding one is
/// evidence that the entry cleared every gate in
/// [`crate::fed::peers::PeerNotAdmitted`] -- not merely that a string was parsed out
/// of a TOML file.
///
/// And, since the collision refusal landed, evidence of *which* entry. That word was
/// previously untrue: `catalyrst-fed` keyed its registry on the raw id while this
/// crate lowercased it, so two entries differing only in case -- two DAO proposals,
/// two pinned roots, two hosts -- minted one `PeerId`, and whichever polled second
/// erased the other's mirror. `FederationRegistry::parse_file` now refuses such a
/// file naming both entries, and both sides fold through the same function, so one
/// `PeerId` corresponds to exactly one line of the peer file.
///
/// Distinct from [`catalyrst_fed::PeerId`], which is a bare `String` alias for any
/// id appearing in a peer file, admitted or not. That alias is deliberately not
/// re-exported here: the whole point is that the two are not interchangeable.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize)]
pub struct PeerId(String);

impl PeerId {
    /// The only constructor, and it is `pub(crate)` so no caller outside this crate
    /// can mint one. Named for its provenance so a reviewer can see at the call site
    /// that the value came out of admission.
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

/// A world name that arrived on a local request path.
///
/// The only type [`crate::handlers::permissions::resolve_world_owner`] accepts. There
/// is no `From<RemoteWorldName>`, no `TryFrom<RemoteWorldName>`, and no constructor
/// taking one, so a peer-reported name cannot reach the owner resolver without
/// somebody writing a line that names the lie.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LocalWorldName(String);

impl LocalWorldName {
    /// The only constructor. Named for its provenance, in the style of
    /// `CommunityBanAuthority::resolve_from_gossip_envelope_relayed_by_a_peer_catalyst_server`
    /// (catalyrst-social-service/src/rest/fed/consumer.rs): a reviewer must be able to
    /// see where the value came from at the call site, not by chasing types.
    ///
    /// Lowercasing here preserves the existing behaviour exactly --
    /// `resolve_world_owner` already lowercased internally before this type existed.
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

/// A world name as a *peer reported it*. Not a name we have verified, resolved, or
/// agreed to serve.
///
/// Deliberately absent: `Deref<Target = str>`, `AsRef<str>`, `Display`,
/// `Into<String>`, `Into<LocalWorldName>`. The single escape hatch is
/// `as_peer_reported_str`, whose name exists to be greppable, and whose use is gated
/// by [`super::wire::provenance_gate`] to `fed/{names,wire,store,handlers}.rs`.
///
/// Why this is needed here and not in social federation: `community_id_hex(creator,
/// name, nonce)` puts the creator *inside* the identifier, so two peers physically
/// cannot disagree about who owns it. An ENS name carries no creator. Nothing inside
/// `foo.dcl.eth` distinguishes our record from a peer's, so the type must do the work
/// the identifier cannot.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RemoteWorldName(String);

/// Longer than any real `.dcl.eth` name; the cap exists so a peer cannot spend our
/// row width on a megabyte of its own choosing.
const MAX_WORLD_NAME_LEN: usize = 255;

impl RemoteWorldName {
    /// Constructible only from a peer response. Fails closed on anything that is not a
    /// plausible world name rather than storing a peer-chosen string of arbitrary
    /// shape: `/`, `%`, whitespace, control bytes and non-ASCII are all rejected, so
    /// `../`, `%00` and a name carrying an embedded URL cannot survive.
    pub(crate) fn from_peer_listing(raw: &str) -> Option<Self> {
        Self::shaped(raw)
    }

    /// An operator naming a row that is *already in the mirror*, on the admin veto
    /// route. Same shape rules, different provenance, and deliberately a different
    /// name: this one asserts nothing about the peer and reaches nothing but a
    /// primary-key lookup in `remote_worlds`.
    ///
    /// `pub` where [`Self::from_peer_listing`] is `pub(crate)`, and the asymmetry is
    /// the point. This constructor cannot introduce a *peer's* claim into the process
    /// -- it is reached only from an already-authenticated admin route and from tests
    /// -- whereas `from_peer_listing` is the trust boundary itself and stays sealed
    /// inside `fed`. Neither yields a [`LocalWorldName`], so neither can reach
    /// `resolve_world_owner`.
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
        // This test is documentation for the compile-time property; the property
        // itself is that the two lines below cannot be written:
        //
        //     let _: LocalWorldName = RemoteWorldName::from_peer_listing("x").unwrap();
        //     resolve_world_owner(&state, &remote_name, None)
        //
        // Both are rejected by the compiler because there is no conversion in either
        // direction and `resolve_world_owner` names `&LocalWorldName` in its signature.
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
