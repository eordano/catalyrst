//! Worlds federation: the peer registry, and the types that keep a peer's word
//! separable from ours.
//!
//! A peer is a **source of content claims and nothing else**: which world names it
//! holds and what public metadata it prints for them. It may never tell us who owns a
//! world, who may deploy to one, or what a local ACL says. Every ownership and
//! permission question resolves through the local path --
//! [`crate::handlers::permissions::resolve_world_owner`] against
//! `squid_marketplace.ens` and the local `world_permissions` table -- which nothing in
//! this module is reachable from.
//!
//! [`peers`] and [`config`] are the admission gate: `federation-peers.toml` is read at
//! boot, every entry adjudicated, and a bad entry aborts startup. [`names`] holds the
//! newtypes that make provenance a compile-time property.
//!
//! Admission is also **revocation**. A peer removed from the file stops being published
//! at the next boot, by two independent mechanisms -- one on the write path
//! ([`store::RemoteWorldsComponent::revoke_peers_no_longer_admitted`], before the router
//! exists) and one on the read path
//! ([`store::RemoteWorldsComponent::list_mirror`] filtering against the admitted set).
//!
//! [`wire`], [`store`], [`poll`] and [`handlers`] are the read mirror: peer world names
//! and public metadata in their own tables keyed by `(peer_id, world_name)`, served only
//! on peer-qualified routes. No blobs, no `/about`, no comms, and -- load-bearing --
//! zero writes to `worlds` or `world_scenes`, because `resolve_world_owner` returns
//! `stored_owner` **first** and consults squid ENS only when it is `NULL`, so a written
//! `worlds.owner` would become the permanent authority over the chain. The rule is
//! "never touch that table", not "don't copy the owner field".

pub mod config;
pub mod handlers;
pub mod names;
pub mod peers;
pub mod poll;
pub mod store;
pub mod wire;
