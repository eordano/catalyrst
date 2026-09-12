//! The peer admission gate: `federation-peers.toml`, read at boot, adjudicated, and
//! fail-closed. [`catalyrst_fed::FederationRegistry::parse_file`] rejects only *empty*
//! required fields; [`AdmittedPeer::admit`] is the actual gate.
//!
//! # The pin is the admission decision
//!
//! A peer's identity is established by TLS against **its own pinned root**
//! (`mtls_root_pem`) and by nothing else -- an `https` scheme proves nothing about who
//! answers, since any WebPKI-valid host that wins a DNS race is then the peer. So an
//! admitted peer carries a [`reqwest::Client`] trusting that root and no other, and
//! holding an [`AdmittedPeer`] is the evidence such a client was built.
//!
//! ## reqwest 0.13 hazard -- read before touching the client builder
//!
//! `ClientBuilder::tls_built_in_root_certs` does not exist in reqwest 0.13, and
//! `add_root_certificate` alone routes through
//! `rustls_platform_verifier::Verifier::new_with_extra_roots`, adding the pinned root
//! *alongside* the ambient system trust store -- precisely the defeat this module
//! exists to prevent. `ClientBuilder::tls_certs_only` is the method that actually
//! pins. **Do not replace it with `add_root_certificate` or `tls_certs_merge`**: that
//! compiles, and silently converts the pin back into ordinary WebPKI.
//!
//! The only test that catches that swap is
//! `pinned_client_rejects_a_webpki_valid_host` in
//! `tests/federation_peer_admission.rs`. Its sibling
//! `pinned_client_trusts_only_its_own_root` does not -- both roots there are private,
//! so a merged client rejects the wrong server for the same reason a pinned one does.
//! Measured against a regressed build, not assumed.
//!
//! Likewise `Certificate::from_pem` is deliberately not used: under `__rustls` it
//! parses nothing and returns `Ok` for any bytes, so a typo'd root yields an empty
//! trust store and a peer admitted at boot and unreachable forever.
//!
//! Nothing here reads, stores or forwards an ownership or permission claim. An
//! [`AdmittedPeer`] exposes one outbound capability -- fetch a listing from a URL this
//! file builds out of registry fields only -- so no peer *response* value ever
//! constructs a URL: no SSRF surface, no follow-up fetch.

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{anyhow, Result};
use catalyrst_fed::{canonical_peer_id, PeerCert};
use url::Url;

use crate::fed::config::WorldsFedConfig;
use crate::fed::names::PeerId;

/// Hostname suffixes reserved by RFC 2606 / RFC 6761. A peer id ending in one of these
/// is a copy-paste from an example file, never a peer.
const RESERVED_TEST_SUFFIXES: &[&str] = &[".invalid", ".example", ".test", ".localhost", ".local"];

/// Unsubstituted markers from the template `dao_proposal` line.
const DAO_PROPOSAL_TEMPLATE_MARKERS: &[&str] = &["<space>", "<id>"];

/// The Unix epoch date, which is what an unfilled `added_at` looks like.
const PLACEHOLDER_ADDED_AT: &str = "1970-01-01";

/// Why one entry in the peer file was refused. Every variant is **fatal**: it aborts
/// process startup, because a bad entry in a hand-curated DAO-gated allowlist is a
/// deploy-time operator error, and booting with four peers when the operator wrote
/// five makes "we federate with X" and "we tried to" the same observable state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PeerNotAdmitted {
    PlaceholderDaoProposal {
        peer_id: String,
        value: String,
    },
    PlaceholderAddedAt {
        peer_id: String,
        value: String,
    },
    ZeroGossipPubkey {
        peer_id: String,
    },
    ReservedTestHost {
        peer_id: String,
        suffix: &'static str,
    },
    NoPinnedRoot {
        peer_id: String,
    },
    /// A pinned root alongside a cleartext `http://` loopback URL: meaningless, since
    /// there is no handshake for the root to authenticate. Refused rather than warned
    /// about, or the boot log and `/federation/worlds/peers` would report the peer as
    /// pinned while the pin does nothing.
    PinnedRootOnCleartextUrl {
        peer_id: String,
        url: String,
    },
    UnusablePinnedRoot {
        peer_id: String,
        source: String,
    },
    WorldsUrlUnparseable {
        peer_id: String,
        url: String,
        source: String,
    },
    WorldsUrlNotHttps {
        peer_id: String,
        url: String,
        scheme: String,
    },
    WorldsUrlHasNoHost {
        peer_id: String,
        url: String,
    },
    ClientBuildFailed {
        peer_id: String,
        source: String,
    },
}

impl PeerNotAdmitted {
    pub fn peer_id(&self) -> &str {
        match self {
            Self::PlaceholderDaoProposal { peer_id, .. }
            | Self::PlaceholderAddedAt { peer_id, .. }
            | Self::ZeroGossipPubkey { peer_id }
            | Self::ReservedTestHost { peer_id, .. }
            | Self::NoPinnedRoot { peer_id }
            | Self::PinnedRootOnCleartextUrl { peer_id, .. }
            | Self::UnusablePinnedRoot { peer_id, .. }
            | Self::WorldsUrlUnparseable { peer_id, .. }
            | Self::WorldsUrlNotHttps { peer_id, .. }
            | Self::WorldsUrlHasNoHost { peer_id, .. }
            | Self::ClientBuildFailed { peer_id, .. } => peer_id,
        }
    }
}

impl std::fmt::Display for PeerNotAdmitted {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PlaceholderDaoProposal { peer_id, value } => write!(
                f,
                "peer {peer_id}: dao_proposal is still the placeholder ({value:?}); replace it \
                 with a real snapshot.dcl.eth proposal URL before any peer is read from"
            ),
            Self::PlaceholderAddedAt { peer_id, value } => write!(
                f,
                "peer {peer_id}: added_at is still the placeholder ({value:?}); record the date \
                 the DAO admitted this peer"
            ),
            Self::ZeroGossipPubkey { peer_id } => write!(
                f,
                "peer {peer_id}: gossip_pubkey is 32 zero bytes, which is a placeholder and not \
                 a key. This slice reads the key for nothing else \u{2014} there is no signed channel \
                 to worlds peers, and inventing a use for it would be a second, weaker \
                 verification"
            ),
            Self::ReservedTestHost { peer_id, suffix } => write!(
                f,
                "peer {peer_id}: peer_id ends in {suffix}, a reserved name (RFC 2606 / RFC 6761) \
                 that can never resolve to a real peer; this entry was copied from an example"
            ),
            Self::NoPinnedRoot { peer_id } => write!(
                f,
                "peer {peer_id}: mtls_root_pem is empty, so there is no way to establish that \
                 the host answering is this peer rather than any WebPKI-valid host. Supply the \
                 peer's root certificate, or (loopback only, dev only) set \
                 WORLDS_FED_ALLOW_INSECURE_LOOPBACK_PEERS=1"
            ),
            Self::PinnedRootOnCleartextUrl { peer_id, url } => write!(
                f,
                "peer {peer_id}: mtls_root_pem is set, but worlds_url is cleartext ({url}), so \
                 the pinned root authenticates nothing \u{2014} there is no TLS handshake for it to \
                 apply to. This entry claims to be pinned and is not. Use an https worlds_url \
                 to make the pin real, or clear mtls_root_pem to say plainly that this loopback \
                 peer is unauthenticated"
            ),
            Self::UnusablePinnedRoot { peer_id, source } => write!(
                f,
                "peer {peer_id}: mtls_root_pem is not a usable PEM certificate: {source}"
            ),
            Self::WorldsUrlUnparseable {
                peer_id,
                url,
                source,
            } => write!(
                f,
                "peer {peer_id}: worlds_url {url:?} does not parse as a URL: {source}"
            ),
            Self::WorldsUrlNotHttps {
                peer_id,
                url,
                scheme,
            } => write!(
                f,
                "peer {peer_id}: worlds_url {url:?} has scheme {scheme:?}; worlds federation \
                 speaks https only (plain http is permitted for a literal loopback host, and \
                 only with WORLDS_FED_ALLOW_INSECURE_LOOPBACK_PEERS=1)"
            ),
            Self::WorldsUrlHasNoHost { peer_id, url } => write!(
                f,
                "peer {peer_id}: worlds_url {url:?} has no host to pin a certificate against"
            ),
            Self::ClientBuildFailed { peer_id, source } => write!(
                f,
                "peer {peer_id}: could not build a TLS client pinned to this peer's root: \
                 {source}"
            ),
        }
    }
}

/// A peer that is in the file, is valid, and is not a *worlds* peer. Recorded rather
/// than dropped so `GET /federation/worlds/peers` can distinguish "absent because it
/// runs no worlds server" from "absent because we forgot to look".
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "reason", rename_all = "camelCase")]
pub enum PeerOmitted {
    #[serde(rename_all = "camelCase")]
    NoWorldsUrl { peer_id: String },
}

impl std::fmt::Display for PeerOmitted {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoWorldsUrl { peer_id } => write!(
                f,
                "peer {peer_id}: no worlds_url, so this peer federates other scopes and runs no \
                 worlds server; omitted from worlds federation, not rejected"
            ),
        }
    }
}

/// A peer that cleared every gate in [`PeerNotAdmitted`] and for which a root-pinned
/// TLS client was built.
///
/// A witness type in the house style of `FederatedCommunityWriteAuthority`
/// (`catalyrst-social-service/src/rest/fed/authority.rs`): private fields, no public
/// constructor, obtainable only from [`AdmittedPeer::admit`], so a function taking
/// `&AdmittedPeer` cannot be handed a peer that was merely present in the file.
#[derive(Clone)]
pub struct AdmittedPeer {
    peer_id: PeerId,
    /// Parsed, normalised: no trailing slash, no query, no fragment, no userinfo.
    worlds_url: Url,
    dao_proposal: String,
    added_at: String,
    /// Admitted through the loopback dev escape hatch, so the `http` client is *not*
    /// pinned. Surfaced on `/federation/worlds/peers` so an operator can see that a
    /// peer is unauthenticated.
    insecure_loopback: bool,
    /// Trusts this peer's pinned root and nothing else -- unless
    /// [`Self::insecure_loopback`] is set, in which case it speaks plain http to a
    /// literal loopback address.
    http: reqwest::Client,
}

impl std::fmt::Debug for AdmittedPeer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AdmittedPeer")
            .field("peer_id", &self.peer_id)
            .field("worlds_url", &self.worlds_url.as_str())
            .field("insecure_loopback", &self.insecure_loopback)
            .finish_non_exhaustive()
    }
}

impl AdmittedPeer {
    pub fn peer_id(&self) -> &PeerId {
        &self.peer_id
    }

    pub fn worlds_url(&self) -> &Url {
        &self.worlds_url
    }

    pub fn dao_proposal(&self) -> &str {
        &self.dao_proposal
    }

    pub fn added_at(&self) -> &str {
        &self.added_at
    }

    pub fn is_insecure_loopback(&self) -> bool {
        self.insecure_loopback
    }

    /// Usable only against this peer: it trusts only this peer's root.
    pub fn http(&self) -> &reqwest::Client {
        &self.http
    }

    /// The **one** place a peer URL is constructed. The path is fixed here and only
    /// integer `limit`/`offset` are appended, so no value from a peer response can
    /// reach it: no SSRF surface, no follow-up fetch.
    pub fn worlds_listing_url(&self, limit: i64, offset: i64) -> Url {
        let mut u = self.worlds_url.clone();
        {
            let mut segments = u
                .path_segments_mut()
                .expect("an admitted worlds_url is always a base URL");
            segments.pop_if_empty().push("worlds");
        }
        u.query_pairs_mut()
            .clear()
            .append_pair("limit", &limit.to_string())
            .append_pair("offset", &offset.to_string());
        u
    }

    /// Adjudicate one peer-file entry. Evaluation order is fixed and tested --
    /// `dao_proposal`, `added_at`, `gossip_pubkey`, `peer_id` suffix, then the URL and
    /// the pinned root -- so the first reason reported for an entry is reproducible.
    ///
    /// Deliberate consequence: the placeholder and pinned-root checks run **before**
    /// the "no worlds_url => omit" branch, so an entry with a `TODO:` proposal and no
    /// worlds URL is fatal rather than omitted. An entry naming no proposal, carrying
    /// no key and pinning no root proves nothing about anybody, whatever scope it was
    /// meant for.
    pub fn admit(
        cert: &PeerCert,
        cfg: &WorldsFedConfig,
    ) -> Result<AdmissionOutcome, PeerNotAdmitted> {
        let canonical_id = canonical_peer_id(&cert.peer_id);

        let dao = cert.dao_proposal.trim();
        let dao_upper = dao.to_ascii_uppercase();
        let templated = DAO_PROPOSAL_TEMPLATE_MARKERS
            .iter()
            .any(|marker| dao.contains(marker));
        if dao_upper.starts_with("TODO") || templated {
            return Err(PeerNotAdmitted::PlaceholderDaoProposal {
                peer_id: canonical_id,
                value: cert.dao_proposal.clone(),
            });
        }

        let added = cert.added_at.trim();
        if added == PLACEHOLDER_ADDED_AT || added.starts_with(&format!("{PLACEHOLDER_ADDED_AT}T")) {
            return Err(PeerNotAdmitted::PlaceholderAddedAt {
                peer_id: canonical_id,
                value: cert.added_at.clone(),
            });
        }

        if cert.gossip_pubkey == [0u8; 32] {
            return Err(PeerNotAdmitted::ZeroGossipPubkey {
                peer_id: canonical_id,
            });
        }

        if let Some(suffix) = RESERVED_TEST_SUFFIXES
            .iter()
            .find(|s| canonical_id.ends_with(**s))
        {
            return Err(PeerNotAdmitted::ReservedTestHost {
                peer_id: canonical_id,
                suffix,
            });
        }

        let pem = cert.mtls_root_pem.trim();
        let raw_worlds_url = cert.worlds_url.trim();

        if raw_worlds_url.is_empty() {
            if pem.is_empty() {
                return Err(PeerNotAdmitted::NoPinnedRoot {
                    peer_id: canonical_id,
                });
            }
            return Ok(AdmissionOutcome::Omitted(PeerOmitted::NoWorldsUrl {
                peer_id: canonical_id,
            }));
        }

        let mut url =
            Url::parse(raw_worlds_url).map_err(|e| PeerNotAdmitted::WorldsUrlUnparseable {
                peer_id: canonical_id.clone(),
                url: raw_worlds_url.to_string(),
                source: e.to_string(),
            })?;

        if url.cannot_be_a_base() || url.host_str().map(str::is_empty).unwrap_or(true) {
            return Err(PeerNotAdmitted::WorldsUrlHasNoHost {
                peer_id: canonical_id,
                url: raw_worlds_url.to_string(),
            });
        }

        let loopback = url.host_str().map(is_loopback_host).unwrap_or(false);
        let scheme = url.scheme().to_ascii_lowercase();
        let loopback_opt_out = cfg.allow_insecure_loopback_peers && loopback && scheme == "http";

        if scheme != "https" && !loopback_opt_out {
            return Err(PeerNotAdmitted::WorldsUrlNotHttps {
                peer_id: canonical_id,
                url: raw_worlds_url.to_string(),
                scheme,
            });
        }

        if !url.username().is_empty() || url.password().is_some() {
            tracing::warn!(
                peer_id = %canonical_id,
                "worlds_url carries userinfo; dropping it \u{2014} worlds federation sends no \
                 credentials to a peer"
            );
            let _ = url.set_username("");
            let _ = url.set_password(None);
        }
        if url.query().is_some() || url.fragment().is_some() {
            tracing::warn!(
                peer_id = %canonical_id,
                "worlds_url carries a query or fragment; dropping it \u{2014} the request path is \
                 constructed by worlds_listing_url, not by the registry"
            );
            url.set_query(None);
            url.set_fragment(None);
        }
        if url.path().ends_with('/') && url.path() != "/" {
            let trimmed = url.path().trim_end_matches('/').to_string();
            url.set_path(&trimmed);
        }

        if loopback_opt_out && !pem.is_empty() {
            return Err(PeerNotAdmitted::PinnedRootOnCleartextUrl {
                peer_id: canonical_id,
                url: url.to_string(),
            });
        }

        let http = if pem.is_empty() {
            if !loopback_opt_out {
                return Err(PeerNotAdmitted::NoPinnedRoot {
                    peer_id: canonical_id,
                });
            }
            tracing::warn!(
                peer_id = %canonical_id,
                worlds_url = %url,
                "admitting an UNAUTHENTICATED loopback peer because \
                 WORLDS_FED_ALLOW_INSECURE_LOOPBACK_PEERS=1; nothing establishes that the \
                 process answering on this port is the peer. Never set this outside a \
                 local test."
            );
            base_client_builder()
                .build()
                .map_err(|e| PeerNotAdmitted::ClientBuildFailed {
                    peer_id: canonical_id.clone(),
                    source: e.to_string(),
                })?
        } else {
            let roots = reqwest::Certificate::from_pem_bundle(pem.as_bytes()).map_err(|e| {
                PeerNotAdmitted::UnusablePinnedRoot {
                    peer_id: canonical_id.clone(),
                    source: e.to_string(),
                }
            })?;
            if roots.is_empty() {
                return Err(PeerNotAdmitted::UnusablePinnedRoot {
                    peer_id: canonical_id,
                    source: "contains no -----BEGIN CERTIFICATE----- block".to_string(),
                });
            }
            base_client_builder()
                .tls_certs_only(roots)
                .build()
                .map_err(|e| PeerNotAdmitted::ClientBuildFailed {
                    peer_id: canonical_id.clone(),
                    source: e.to_string(),
                })?
        };

        Ok(AdmissionOutcome::Admitted(AdmittedPeer {
            peer_id: PeerId::from_admitted(&canonical_id),
            worlds_url: url,
            dao_proposal: dao.to_string(),
            added_at: added.to_string(),
            insecure_loopback: loopback_opt_out,
            http,
        }))
    }
}

fn base_client_builder() -> reqwest::ClientBuilder {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(30))
        .user_agent(concat!("catalyrst-worlds/", env!("CARGO_PKG_VERSION")))
}

/// `127.0.0.0/8`, `::1`, or the literal name `localhost`. A *literal* check, never a
/// resolution: a hostname that happens to resolve to 127.0.0.1 today is not loopback
/// here, because what it resolves to is not under our control.
fn is_loopback_host(host: &str) -> bool {
    let bare = host.trim_start_matches('[').trim_end_matches(']');
    if bare.eq_ignore_ascii_case("localhost") {
        return true;
    }
    match bare.parse::<std::net::IpAddr>() {
        Ok(ip) => ip.is_loopback(),
        Err(_) => false,
    }
}

#[derive(Debug)]
pub enum AdmissionOutcome {
    Admitted(AdmittedPeer),
    Omitted(PeerOmitted),
}

/// The worlds-federation peer set for the lifetime of this process.
///
/// Two states, not `Option<Vec<_>>`: "federation was never requested" and "requested
/// and yielded nothing" must not collapse at a call site into "no allowlist, allow
/// everyone". [`Self::NotConfigured`] makes the federation routes answer **503**
/// naming the variable, and never produces an appendable empty list.
#[derive(Debug)]
pub enum WorldsFederationPeers {
    /// `WORLDS_FED_PEERS_FILE` was never set. Not an error; federation is off.
    NotConfigured,
    Admitted {
        path: PathBuf,
        peers: Vec<AdmittedPeer>,
        /// Surfaced on `GET /federation/worlds/peers` so an omission is never silent.
        omitted: Vec<PeerOmitted>,
    },
}

impl WorldsFederationPeers {
    /// Call this from `build_state` **before** the `Arc<AppStateInner>` is built, so a
    /// refusal aborts startup rather than degrading a process that is already serving.
    pub fn load_from_env() -> Result<Self> {
        Self::load(&WorldsFedConfig::from_env()?)
    }

    /// The env-free core, so tests do not have to mutate process environment.
    pub fn load(cfg: &WorldsFedConfig) -> Result<Self> {
        let Some(path) = cfg.peers_file.as_deref() else {
            tracing::info!(
                "WORLDS_FED_PEERS_FILE unset; worlds federation is off. This is a normal \
                 configuration, not a degraded one: /federation/worlds/* will answer 503 \
                 naming the variable rather than returning an empty peer list."
            );
            return Ok(Self::NotConfigured);
        };
        Self::load_file(path, cfg)
    }

    /// A missing, unreadable or malformed file is a **boot failure**, following
    /// `catalyrst-fed/src/gossip.rs`: an operator who named a peer file and got a
    /// running server with no federation has been told nothing.
    pub fn load_file(path: &Path, cfg: &WorldsFedConfig) -> Result<Self> {
        let registry = catalyrst_fed::FederationRegistry::from_file(path).map_err(|e| {
            anyhow!(
                "WORLDS_FED_PEERS_FILE={} could not be loaded: {e}. Fix the file, or unset \
                 WORLDS_FED_PEERS_FILE to run without federation.",
                path.display()
            )
        })?;

        let mut certs = registry.all();
        certs.sort_by(|a, b| a.peer_id.cmp(&b.peer_id));
        debug_assert!(
            certs.windows(2).all(|w| w[0].peer_id != w[1].peer_id),
            "FederationRegistry handed back two entries with the same id; parse_file is \
             supposed to have refused that file"
        );
        let total = certs.len();

        let mut peers = Vec::new();
        let mut omitted = Vec::new();
        let mut rejected: Vec<PeerNotAdmitted> = Vec::new();

        for cert in &certs {
            match AdmittedPeer::admit(cert, cfg) {
                Ok(AdmissionOutcome::Admitted(p)) => {
                    tracing::info!(
                        peer_id = %p.peer_id(),
                        worlds_url = %p.worlds_url(),
                        dao_proposal = %p.dao_proposal(),
                        added_at = %p.added_at(),
                        pinned = !p.is_insecure_loopback(),
                        "federation peer admitted"
                    );
                    peers.push(p);
                }
                Ok(AdmissionOutcome::Omitted(o)) => {
                    tracing::info!("federation peer omitted: {o}");
                    omitted.push(o);
                }
                Err(e) => {
                    tracing::error!("federation peer rejected: {e}");
                    rejected.push(e);
                }
            }
        }

        if let Some(first) = rejected.first() {
            return Err(anyhow!(
                "{} of {} entries in {} were not admitted; refusing to start. First: {}. \
                 Unset WORLDS_FED_PEERS_FILE to run without federation.",
                rejected.len(),
                total,
                path.display(),
                first
            ));
        }

        tracing::info!(
            path = %path.display(),
            admitted = peers.len(),
            omitted = omitted.len(),
            "worlds federation peer registry loaded"
        );

        Ok(Self::Admitted {
            path: path.to_path_buf(),
            peers,
            omitted,
        })
    }

    /// An empty `peers` list is still configured -- the distinction the enum exists
    /// to preserve.
    pub fn is_configured(&self) -> bool {
        matches!(self, Self::Admitted { .. })
    }

    /// `true` when the peer file has at least one entry, admitted or omitted.
    ///
    /// Load-bearingly distinct from `!peers().is_empty()`: a file whose peers are all
    /// `Omitted` still had entries written into it, whereas a file naming nobody at
    /// all cannot be told apart from a truncated write or `[[peers]]` for `[[peer]]`
    /// -- the only state
    /// [`RemoteWorldsComponent::revoke_peers_no_longer_admitted`] refuses to sweep on.
    pub fn names_any_peer(&self) -> bool {
        !self.peers().is_empty() || !self.omitted().is_empty()
    }

    /// An empty slice when unconfigured. Callers that must distinguish "no peers" from
    /// "no federation" match on the enum instead.
    pub fn peers(&self) -> &[AdmittedPeer] {
        match self {
            Self::NotConfigured => &[],
            Self::Admitted { peers, .. } => peers,
        }
    }

    pub fn omitted(&self) -> &[PeerOmitted] {
        match self {
            Self::NotConfigured => &[],
            Self::Admitted { omitted, .. } => omitted,
        }
    }

    pub fn path(&self) -> Option<&Path> {
        match self {
            Self::NotConfigured => None,
            Self::Admitted { path, .. } => Some(path),
        }
    }

    /// `None` for an id that is in the file but was omitted -- an omitted peer is not
    /// a worlds peer and must not be addressable as one.
    pub fn get(&self, peer_id: &str) -> Option<&AdmittedPeer> {
        let needle = canonical_peer_id(peer_id);
        self.peers().iter().find(|p| p.peer_id().as_str() == needle)
    }

    /// The message the federation routes answer 503 with when unconfigured.
    pub const NOT_CONFIGURED_DETAIL: &'static str =
        "worlds federation is not configured: WORLDS_FED_PEERS_FILE is unset";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loopback_host_check_is_literal_not_resolved() {
        assert!(is_loopback_host("127.0.0.1"));
        assert!(is_loopback_host("127.9.9.9"));
        assert!(is_loopback_host("localhost"));
        assert!(is_loopback_host("LOCALHOST"));
        assert!(is_loopback_host("[::1]"));
        assert!(is_loopback_host("::1"));

        assert!(!is_loopback_host("example.org"));
        assert!(!is_loopback_host("127.0.0.1.evil.example"));
        assert!(!is_loopback_host("localhost.evil.example"));
        assert!(!is_loopback_host("10.0.0.1"));
        assert!(!is_loopback_host("0.0.0.0"));
    }

    #[test]
    fn not_configured_never_yields_a_list_that_could_be_appended_to() {
        let peers = WorldsFederationPeers::NotConfigured;
        assert!(!peers.is_configured());
        assert!(peers.peers().is_empty());
        assert!(peers.omitted().is_empty());
        assert!(peers.path().is_none());
        assert!(peers.get("anything").is_none());
    }
}
