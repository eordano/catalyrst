# Crate boundaries

The workspace is small library crates, each owning one domain and exposing
`build_state()` and `api_router()`. Standalone bins and thin bundle bins
compose these libraries; no domain's source lives inside another's crate.
the dev deployment runs one standalone systemd unit per domain; nixos runs four thin
bundle bins — catalyrst-create, catalyrst-data, catalyrst-explore,
catalyrst-social — on ports 5143-5146, composing the same libraries. Bundle
membership is a Cargo.toml dependency line, not a source-tree fusion: a
crate joins or leaves a bundle in one line.

Each database backs exactly one migration history — one `_sqlx_migrations`
table, one numbering sequence, owned by one crate. Two crates on the same
physical database each running an independently numbered `sqlx::migrate!`
collide on that table once both run against a shared dataset; the fix is
one crate owning the domain, or at least its migrations, not an
environment-variable workaround.

Crate names map one-to-one to the upstream reference service they port:
catalyrst-social-service ports `decentraland/social-service-ea`;
catalyrst-places and catalyrst-events port separate upstream services. A
merge that erases this mapping — folding crates that port different
upstreams into one name — is out of bounds regardless of any code-sharing
argument for it.

## Do not merge

Every boundary below does quiet work: build parallelism, dependency
quarantine, feature isolation, log-filter naming, nix packaging granularity,
upstream-repo mapping. A merge is warranted only when a boundary is
provably re-glued elsewhere, one crate owns half an artifact whose other
half lives elsewhere, or a crate has no independent existence — no bin, no
database, no unit, no port.

- **places and events stay separate from the social service.** They serve
  the discovery plane over `places_events` via catalyrst-fed, a clean
  library over external NATS; social-service's rpc half does not federate.
  Folding them in would build the workspace's largest crate, bleed
  dcl-rpc/prost/openmls into the lean explore packages, and add a third
  migration stream on a third database.
- **communities, social-rpc, notifications, badges, and media stay separate
  crates**, beyond the REST/rpc fold inside catalyrst-social-service.
  Production runs each as a standalone systemd unit; fusing all six into
  one produces a ~21.6k-line crate to delete five ~40-line Cargo.tomls, and
  fusing social with badges, media, and notifications alone drags openmls
  into three cheap nix builds and silences `catalyrst_badges=` in RUST_LOG.
- **comms, archipelago, and presence stay separate crates.** presence has
  three upstreams, one external; comms proxies to LiveKit regardless of
  process boundary, so fusing removes no HTTP hop, and an in-process
  archipelago read would split-brain the live Cluster state
  catalyrst-explore owns.
- **market, economy, credits, price, and signatures stay separate crates.**
  Every call between them is a reqwest client against a config URL, so
  fusion removes zero hops; four live units would collapse onto one
  ~44k-line crate with four migration streams on the real-money surface,
  where small review units matter most; market, economy, and credits
  already compose in-process via catalyrst-data.
- **create, builder, and camera-reel stay separate crates.** builder is not
  create's sole dependent — fuzz targets depend on it too — and the live
  stack runs create (`:5144`) and camera-reel (`:5163`) as standalone bins
  with no bundle unit, so fusing deletes essentially nothing.
- **scene-state and world-storage stay separate crates.** They sit on
  opposite sides of a trust boundary (ephemeral keys vs. a trusted-signer
  set plus an encryption key) and a build boundary (world-storage's V8
  dependency stays out of the dev hot-rebuild loop); shared constants are
  mirrored and pinned by a cross-crate test, not by proximity.
- **types, envcfg, hashing, and storage stay four separate leaf crates.**
  Eleven crates take envcfg alone, so "always pulled together" does not
  hold; the export tooling's version-source line depends on the exact
  crate path; folding storage in forces sqlx onto the sqlx-free dcl-one-sdk
  export.
- **types, crypto, envcfg, and hashing stay separate leaf crates**, plus:
  crypto already holds the auth_chain landing pad (see Open work), and
  gating envcfg's thirteen dependents on the ethers-core stack has no
  supporting co-change history.
- **catalyrst-data does not absorb price or rpc.** Folding price in pulls
  the alloy-heavy data-plane closure into an otherwise light nix package;
  the standalone rpc deploy that staying separate preserves is real and in
  use.
- **catalyrst-deployer does not merge into catalyrst-fuzz.** fuzz is the QA
  home for six production crates; deployer's mock-only stress bins are not
  worth entrenching, so deployer's dead code is deleted outright, not
  relocated.
- **the four bundle bins stay separate crates, not fused into one
  catalyrst-bundles.** The nixos deploy kit is bundle-first, and a fused
  dependency closure is the union of all eighteen member crates — roughly
  a twentyfold inflation for the create bundle alone — where a small
  shared helper achieves the same deduplication.
- **bench, fuzz, oracle-tests, and conformance stay four separate crates,
  not one catalyrst-devtools.** They share no code; conformance, a
  zero-workspace-dependency black-box HTTP client, would inherit the
  heaviest dependency frontier in the group if fused with the others.
- **catalyrst-db, catalyrst-validator, and catalyrst-bench stay out of
  catalyrst-server**, beyond the sync fold already inside it. validator is
  a coherent domain port with standalone consumers (oracle-tests, the
  entity_parser fuzz target) and quarantines the ethers-core 2.0.14
  outlier from the alloy-based majority; bench is a dev-dependency
  quarantine (criterion, ethers-signers) that would bleed into the hot
  content-serving crate.

## Open work

- **Hoist auth_chain.rs into catalyrst-crypto.** Fourteen crates vendor a
  signed-fetch auth-chain wrapper, eleven distinct hashes among them, TTLs
  drifting 5-30 minutes apart. All fourteen already import
  `catalyrst_crypto::verify::verify_auth_chain`, so consolidating the
  wrapper there and parameterizing the TTL removes roughly 1,500 duplicated
  lines.
- **The communities database data migration is pending.** pg_dump the six
  social tables — friendships, friendship_actions, blocks, social_settings,
  user_mutes, private_voice_chats, explicitly not `_sqlx_migrations` — out
  of the live `social_rpc` database and into `communities`; grant access to
  the cmm_ role; flip social-rpc's `DATABASE_URL` to point at `communities`;
  freeze the old `social_rpc` database for rollback. It is complete when
  the community voice role check on :5149 succeeds against the unified
  database.
- **Delete `mutes_pool` and `MUTES_PG_CONNECTION_STRING`** once the
  migration above lands — communities' own connection pool covers the same
  tables after the flip.
- **Rename the `RUST_LOG` target in live env files to
  `catalyrst_social_service`**, matching the crate that serves both REST
  and rpc traffic, so the filter selects the right log lines.
