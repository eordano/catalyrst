//! Worlds federation: the peer registry, and the types that keep a peer's word
//! separable from ours.
//!
//! # What a peer is allowed to be
//!
//! A peer is a **source of content claims and nothing else**. It may tell us which
//! world names it holds and what public metadata it prints for them. It may never
//! tell us who owns a world, who may deploy to one, or what a local ACL says. Every
//! ownership and permission question in this crate resolves through the existing
//! local path -- [`crate::handlers::permissions::resolve_world_owner`] against
//! `squid_marketplace.ens` and the local `world_permissions` table -- and nothing in
//! this module is reachable from it.
//!
//! # What this module currently contains
//!
//! [`peers`] and [`config`]: the admission gate. `federation-peers.toml` is read at
//! boot, every entry is adjudicated, and a bad entry aborts startup. Before this
//! module existed, `FederationRegistry` was constructible and never constructed, and
//! `PeerCert::mtls_root_pem` was declared and never read -- a peer allowlist that
//! admitted nobody because nobody asked it.
//!
//! [`names`] holds the newtypes that make provenance a compile-time property rather
//! than a comment.
//!
//! Admission is also **revocation**. A peer removed from the file stops being published
//! at the next boot, because [`store::RemoteWorldsComponent::revoke_peers_no_longer_admitted`]
//! runs before the router exists and deletes its rows, and because
//! [`store::RemoteWorldsComponent::list_mirror`] filters every row against the admitted
//! set on the way out. Two mechanisms, one on the write path and one on the read path:
//! the mirror is not allowed to depend on either alone.
//!
//! [`wire`], [`store`], [`poll`] and [`handlers`] are the read mirror: a peer's world
//! **names and public metadata** land in their own tables, keyed by `(peer_id,
//! world_name)`, and are served only on peer-qualified routes. No blobs, no `/about`,
//! no comms, no serving of peer content under our origin, and -- the load-bearing part
//! -- zero writes to `worlds` or `world_scenes`. Since
//! [`crate::handlers::permissions::resolve_world_owner`] returns `stored_owner`
//! **first** and only consults squid ENS when it is `NULL`, any write that populated
//! `worlds.owner` would become the permanent authority over the chain. So the rule is
//! not "don't copy the owner field"; it is "never touch that table".

pub mod config;
pub mod handlers;
pub mod names;
pub mod peers;
pub mod poll;
pub mod store;
pub mod wire;
