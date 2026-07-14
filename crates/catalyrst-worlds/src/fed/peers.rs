//! The peer admission gate: `federation-peers.toml`, read at boot, adjudicated, and
//! fail-closed.
//!
//! # Why this file exists
//!
//! [`catalyrst_fed::FederationRegistry::from_file`] is well built and, before this
//! module, was never called anywhere in the workspace. `PeerCert::mtls_root_pem` was
//! declared and never read. `PeerCert::gossip_pubkey` was declared and never read.
//! The shipped `deploy/config/federation-peers.toml` has been sitting on disk
//! containing a `TODO:` placeholder that nothing would have rejected, because nothing
//! looked. An allowlist nobody consults is not an allowlist; it is a comment that
//! costs a file.
//!
//! `parse_file` rejects only *empty* required fields. It is a well-made gate with no
//! door attached. [`AdmittedPeer::admit`] is the door.
//!
//! # The pin is the admission decision
//!
//! A peer's identity is established by TLS against **its own pinned root**, taken
//! from `mtls_root_pem` in the registry, and by nothing else. Checking that a URL's
//! scheme is `https` proves nothing about *who answers*: any WebPKI-valid host that
//! wins a DNS race is then the peer. So an admitted peer carries a
//! [`reqwest::Client`] whose trust store contains that root **and no other**, and
//! holding an [`AdmittedPeer`] is the evidence that such a client was successfully
//! built.
//!
//! ## reqwest 0.13 hazard -- read before touching the client builder
//!
//! `ClientBuilder::tls_built_in_root_certs` **does not exist in reqwest 0.13** (it was
//! removed; verified by compile against the workspace's exact feature set, which
//! errors `E0599: no method named tls_built_in_root_certs`). In 0.13,
//! `add_root_certificate` alone routes through
//! `rustls_platform_verifier::Verifier::new_with_extra_roots` -- that is, the pinned
//! root is added *alongside* the ambient system trust store, which is precisely the
//! defeat this module exists to prevent.
//!
//! [`ClientBuilder::tls_certs_only`] is the method that actually pins: its
//! documentation is "This option disables any native or built-in roots, and **only**
//! uses the roots provided to this method", and it is the branch that reaches
//! `config_builder.with_root_certificates(..)`. **Do not replace it with
//! `add_root_certificate` or `tls_certs_merge`.** Doing so compiles, and silently
//! converts the pin back into ordinary WebPKI.
//!
//! The test that catches that swap is `pinned_client_rejects_a_webpki_valid_host` in
//! `tests/federation_peer_admission.rs` -- and *only* that one. Its sibling
//! `pinned_client_trusts_only_its_own_root` does **not** catch it: both roots in that
//! test are private, so a merged client rejects the wrong server for the same reason
//! a pinned one does. This was measured, not assumed -- the regressed build was built
//! and run. Deleting the WebPKI test because "the other one covers TLS" removes the
//! only thing standing between this file and an ordinary HTTPS client.
//!
//! Likewise `Certificate::from_pem` is deliberately **not** used: under `__rustls` it
//! parses nothing and returns `Ok` for any bytes, so a typo'd root yields an empty
//! trust store and a peer that is admitted at boot and unreachable forever. See the
//! comment at the call site.
//!
//! # Scope of what a peer may say
//!
//! Nothing in this module reads, stores, or forwards an ownership or permission claim.
//! An [`AdmittedPeer`] exposes exactly one outbound capability -- fetch a listing from
//! a URL this file constructs -- and that URL is built from registry fields only. No
//! value from any peer *response* ever constructs a URL, so there is no SSRF surface
//! and no follow-up fetch.

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{anyhow, Result};
use catalyrst_fed::{canonical_peer_id, PeerCert};
use url::Url;

use crate::fed::config::WorldsFedConfig;
use crate::fed::names::PeerId;

/// Hostname suffixes reserved by RFC 2606 / RFC 6761. A peer id ending in one of
/// these is a copy-paste from an example file, never a peer.
const RESERVED_TEST_SUFFIXES: &[&str] = &[".invalid", ".example", ".test", ".localhost", ".local"];

/// Unsubstituted markers from the template `dao_proposal` line.
const DAO_PROPOSAL_TEMPLATE_MARKERS: &[&str] = &["<space>", "<id>"];

/// The Unix epoch date, which is what an unfilled `added_at` looks like.
const PLACEHOLDER_ADDED_AT: &str = "1970-01-01";

/// Why one entry in the peer file was refused. Every variant is **fatal**: it aborts
/// process startup.
///
/// The peer file is a small, hand-curated, DAO-gated allowlist. A bad entry is a
/// deploy-time operator error, not a runtime condition to route around. Booting with
/// four peers when the operator wrote five makes "we federate with X" and "we *tried*
/// to federate with X" the same observable state, and there is no later moment at
/// which anyone finds out which one happened.
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
    /// A pinned root supplied alongside a cleartext `http://` loopback URL.
    ///
    /// The two are individually valid and together meaningless: a root certificate
    /// authenticates a TLS handshake, and there is no handshake on cleartext. Refused
    /// rather than warned about, because every reader of that entry -- the operator,
    /// the boot log, `/federation/worlds/peers` -- would otherwise be told the peer is
    /// pinned, and the pin would be doing nothing.
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

/// A peer that is in the file, is valid, and is not a *worlds* peer.
///
/// Recorded rather than dropped so `GET /federation/worlds/peers` can show why a peer
/// present in the file is absent from the peer list. "Absent because it runs no
/// worlds server" and "absent because we forgot to look" must never be the same
/// observable state -- that confusion is the whole failure mode this module was
/// written to end.
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

/// A peer that cleared every gate in [`PeerNotAdmitted`] and for which a
/// root-pinned TLS client was successfully built.
///
/// A witness type in the house style of `FederatedCommunityWriteAuthority`
/// (`catalyrst-social-service/src/rest/fed/authority.rs`): private fields, no public
/// constructor, obtainable only from [`AdmittedPeer::admit`]. A function that takes
/// an `&AdmittedPeer` cannot be handed a peer that was merely *present in the file*.
#[derive(Clone)]
pub struct AdmittedPeer {
    peer_id: PeerId,
    /// Parsed, normalised: no trailing slash, no query, no fragment, no userinfo.
    worlds_url: Url,
    dao_proposal: String,
    added_at: String,
    /// True when this peer was admitted through the loopback dev escape hatch, so
    /// its `http` client is *not* pinned to anything. Surfaced so an operator reading
    /// `/federation/worlds/peers` can see that a peer is unauthenticated.
    insecure_loopback: bool,
    /// Trusts this peer's pinned root and nothing else -- unless
    /// [`Self::insecure_loopback`] is set, in which case it speaks plain http to a
    /// literal loopback address and trusts nothing because there is nothing to trust.
    http: reqwest::Client,
}

impl std::fmt::Debug for AdmittedPeer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Hand-written so a stray `{:?}` in a log line can never print the client's
        // trust material or a future credential field.
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

    /// Whether this peer's channel is unauthenticated (loopback dev opt-out).
    pub fn is_insecure_loopback(&self) -> bool {
        self.insecure_loopback
    }

    /// The pinned client. Only usable against this peer, because it trusts only this
    /// peer's root.
    pub fn http(&self) -> &reqwest::Client {
        &self.http
    }

    /// The **one** place a peer URL is constructed.
    ///
    /// The path is fixed here; only integer `limit`/`offset` are appended. No value
    /// from any peer *response* reaches this function -- there is no parameter that
    /// could carry one -- so there is no SSRF surface and no follow-up fetch.
    pub fn worlds_listing_url(&self, limit: i64, offset: i64) -> Url {
        let mut u = self.worlds_url.clone();
        {
            // `expect` is sound: admission rejects any URL that `cannot_be_a_base`,
            // and http/https never can.
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

    /// Adjudicate one peer-file entry.
    ///
    /// Evaluation order is fixed and tested: `dao_proposal`, `added_at`,
    /// `gossip_pubkey`, `peer_id` suffix, then the URL and the pinned root. It is
    /// fixed so that the *first* reason reported for a given entry is stable across
    /// runs, which is what makes an operator-facing error message reproducible.
    ///
    /// Note the ordering consequence, which is deliberate: the placeholder and
    /// pinned-root checks run **before** the "no worlds_url => omit" branch. An entry
    /// with a `TODO:` proposal and no worlds URL is *fatal*, not omitted. A peer file
    /// entry that names no proposal, carries no key and pins no root proves nothing
    /// about anybody, whatever scope it was meant for, and refusing it is the
    /// fail-closed direction.
    pub fn admit(
        cert: &PeerCert,
        cfg: &WorldsFedConfig,
    ) -> Result<AdmissionOutcome, PeerNotAdmitted> {
        // `catalyrst_fed::canonical_peer_id`, not a local `to_ascii_lowercase`. A
        // private fold here is exactly what made two file entries mint one `PeerId`:
        // the registry keyed on the raw string and this line quietly disagreed with
        // it. There is now one definition and both sides call it.
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

        // A placeholder check and nothing more. The key is not otherwise read: this
        // slice has no signed channel to a worlds peer, and inventing a use for the
        // key here would be exactly the second, weaker verification that must not
        // exist alongside `consumer.rs::preverify`.
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
                // The loopback opt-out cannot apply: with no URL there is no host to
                // check against loopback, so there is nothing that could make an
                // absent root safe.
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

        // Normalise: credentials would be sent on every poll, a query would be
        // clobbered by `worlds_listing_url`, and a fragment is never transmitted.
        // Dropping them is the conservative reading, and it is loud.
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
        // Trailing slash off, so `worlds_listing_url` appends exactly one segment.
        if url.path().ends_with('/') && url.path() != "/" {
            let trimmed = url.path().trim_end_matches('/').to_string();
            url.set_path(&trimmed);
        }

        // A root pinned over cleartext is an orphaned config field: read, stored,
        // and inert. Refused here so no reader is told the peer is pinned when the
        // transport cannot carry a pin. `loopback_opt_out` is the only way to reach
        // this function with a non-https scheme, so this is exactly the http case.
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
            // `from_pem_bundle`, NOT `from_pem`. Under the `__rustls` feature --
            // which is what this workspace builds -- `Certificate::from_pem` parses
            // *nothing*: it stores the bytes verbatim (`Cert::Pem(buf)`) and returns
            // `Ok` for any input whatsoever, deferring the parse to `build()`. And
            // the deferred parse does not fail either, because
            // `read_pem_certs` on a string containing no PEM block yields an empty
            // vector, not an error. The result would be an *empty* root store, a
            // peer admitted at boot, and a poller that reports "unreachable" for the
            // rest of the process's life over what is really a typo in a config file.
            //
            // That is fail-closed at connect time and fail-OPEN at review time, and
            // it is exactly the orphaned-config defect this module exists to end: a
            // field that is technically read but whose garbage value produces no
            // error anybody sees.
            //
            // `from_pem_bundle` runs `read_pem_certs` eagerly, so malformed base64
            // is an error here; the explicit emptiness check below catches the "no
            // CERTIFICATE block at all" case that `read_pem_certs` reports as
            // success. A block that is well-formed base64 but is not a certificate
            // survives both and is caught by `build()` below as `ClientBuildFailed`,
            // when `RootCertStore::add` rejects the DER. Between the three, every
            // unusable value is refused at boot.
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
                // `tls_certs_only`, NOT `add_root_certificate`. See the module docs:
                // in reqwest 0.13 the latter *adds* to the platform trust store, so
                // any WebPKI-valid host would still answer as this peer. This is the
                // single line that makes the peer file mean anything.
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
            // `loopback_opt_out` alone, not `&& pem.is_empty()`. The conjunction was
            // the bug: a cleartext peer that also carried a pem reported itself as
            // pinned and secure, on a channel with no TLS at all. The contradiction is
            // now refused above, so the two spellings agree -- but this stays the
            // single-fact version, because what this field answers is "is the channel
            // authenticated", and cleartext is the whole answer.
            insecure_loopback: loopback_opt_out,
            http,
        }))
    }
}

fn base_client_builder() -> reqwest::ClientBuilder {
    reqwest::Client::builder()
        // A redirect off the pinned host silently defeats the pin: the pin is
        // checked per-connection, and the second connection would be to whatever
        // host the peer named.
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(30))
        .user_agent(concat!("catalyrst-worlds/", env!("CARGO_PKG_VERSION")))
}

/// `127.0.0.0/8`, `::1`, or the literal name `localhost`.
///
/// Deliberately strict: it is a *literal* check, not a resolution. A hostname that
/// happens to resolve to 127.0.0.1 today is not loopback for this purpose, because
/// what it resolves to is not under our control.
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

/// One entry's verdict.
#[derive(Debug)]
pub enum AdmissionOutcome {
    Admitted(AdmittedPeer),
    Omitted(PeerOmitted),
}

/// The worlds-federation peer set for the lifetime of this process.
///
/// Two states, not `Option<Vec<_>>`. "Federation was never requested" and "federation
/// was requested and yielded nothing" must not be the same value at a call site: that
/// collapse is precisely how the shipped config came to be orphaned, and it is how a
/// caller ends up treating an empty allowlist as "no allowlist, allow everyone".
///
/// [`Self::NotConfigured`] makes the federation routes answer **503** naming the
/// variable. It never produces an empty list that a later code path could append to.
#[derive(Debug)]
pub enum WorldsFederationPeers {
    /// `WORLDS_FED_PEERS_FILE` was never set. Not an error; federation is off.
    NotConfigured,
    /// The file loaded and every entry was adjudicated.
    Admitted {
        path: PathBuf,
        peers: Vec<AdmittedPeer>,
        /// Entries deliberately not contacted, each with a legible reason. Surfaced
        /// on `GET /federation/worlds/peers` so an omission is never silent.
        omitted: Vec<PeerOmitted>,
    },
}

impl WorldsFederationPeers {
    /// Read `WORLDS_FED_PEERS_FILE` and adjudicate it.
    ///
    /// Call this from `build_state` **before** the `Arc<AppStateInner>` is
    /// constructed, so any refusal aborts process startup rather than degrading a
    /// process that is already serving.
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

    /// Adjudicate a specific file.
    ///
    /// A missing, unreadable, or malformed file is a **boot failure**, following
    /// `catalyrst-fed/src/gossip.rs`, which refuses to start rather than hand out a
    /// publisher that silently reaches nobody. An operator who named a peer file and
    /// got a running server with no federation has been told nothing.
    pub fn load_file(path: &Path, cfg: &WorldsFedConfig) -> Result<Self> {
        let registry = catalyrst_fed::FederationRegistry::from_file(path).map_err(|e| {
            anyhow!(
                "WORLDS_FED_PEERS_FILE={} could not be loaded: {e}. Fix the file, or unset \
                 WORLDS_FED_PEERS_FILE to run without federation.",
                path.display()
            )
        })?;

        // `FederationRegistry` stores a HashMap, so `all()` order is arbitrary. Sort
        // by peer_id: the "first rejection" carried in the boot error must be the
        // same one on every run, or the operator-facing message is a coin flip.
        //
        // This sort no longer decides anything but that message. It used to order the
        // *raw* ids, and because two entries could then mint one `PeerId`, the last
        // one to poll took the other's mirror namespace -- so an ASCII comparison
        // nobody chose was picking which of two DAO-admitted hosts we published. That
        // cannot happen now: `parse_file` refuses a file whose ids collide under
        // `canonical_peer_id`, so these ids are canonical, distinct, and unique. The
        // comparison is therefore a total order with no ties, and every peer here has
        // its own namespace whatever order it is visited in.
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
                    // Every rejection is logged, not just the first, so one boot
                    // attempt tells the operator about all five problems in the file
                    // rather than making them fix one per restart.
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

    /// `true` only in the [`Self::Admitted`] state. An empty `peers` list is still
    /// configured -- that is the distinction the enum exists to preserve.
    pub fn is_configured(&self) -> bool {
        matches!(self, Self::Admitted { .. })
    }

    /// `true` when the peer file has at least one entry in it, admitted or omitted.
    ///
    /// Distinct from `!peers().is_empty()` on purpose, and the distinction is load
    /// bearing: a file listing peers that are all `Omitted` for running no worlds
    /// server admits nobody, but it is still a file somebody wrote entries into. A
    /// file that names nobody at all is the one that cannot be told apart from a
    /// truncated write or `[[peers]]` for `[[peer]]`, and it is the only state
    /// [`RemoteWorldsComponent::revoke_peers_no_longer_admitted`] refuses to sweep on.
    pub fn names_any_peer(&self) -> bool {
        !self.peers().is_empty() || !self.omitted().is_empty()
    }

    /// The admitted peers, or an empty slice when unconfigured.
    ///
    /// Callers that must distinguish "no peers" from "no federation" match on the
    /// enum instead; this accessor exists for iteration by the poller, which has
    /// nothing to do in either case.
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

    /// Look up one admitted peer by id. Returns `None` for an id that is in the file
    /// but was omitted -- an omitted peer is not a worlds peer, and must not be
    /// addressable as one.
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
