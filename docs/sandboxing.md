# systemd sandbox carve-outs

`nixos/configuration.nix` derives every long-running service from one of four
hardening profiles (`baseSandbox`, `commsHardening`, `noPgSandbox`,
`noJitHardening`). The composition is `baseSandbox <= commsHardening <=
noPgSandbox <= noJitHardening` with each step adding restrictions back in.
Some standard settings are deliberately omitted; this file explains why so a
future maintainer doesn't "tighten" them and break a service.

## PrivateUsers OMITTED from `baseSandbox`

`PrivateUsers=true` puts the service in a child user namespace. Postgres's
`SO_PEERCRED` peer-auth check then cannot see the real UID of the client, so
`catalyrst-sync` and the three `squid-*` services would fail authentication
against the local cluster. `PrivateUsers` is added back as `noPgSandbox` for
services that don't connect to postgres.

## `~@resources` dropped from `SystemCallFilter`

Pulse (.NET 10) needs `mbind`, `set_mempolicy`, and `sched_setattr` for its
allocator/scheduler. Filtering `@resources` away (the default tightening) makes
it crash. The current filter is `@system-service ~@privileged`.

## `RestrictFileSystems` disabled

`RestrictFileSystems=` requires the BPF LSM hook. NixOS 25.11's kernel
doesn't enable it; services exit with code 244 if it's set. Revisit when
nixpkgs ships a kernel with bpf-lsm enabled.

## `MemoryDenyWriteExecute` (MDWE) excludes Pulse and archipelago-*

Pulse (.NET RyuJIT) and the archipelago Node workers (V8) both JIT and need
W+X pages. With MDWE on they SIGTRAP on first JIT. Both run on
`noPgSandbox`, not `noJitHardening`.

## No IPAddress filter on `catalyrst-sync`, LiveKit, Pulse

- **catalyrst-sync:** `SYNC_SOURCE` includes non-Cloudflare peers
  (some peers in the `SYNC_SOURCE` pool may not be behind Cloudflare) and
  operators may rotate the pool.
  An IP allowlist would silently break sync after the next pool change.
- **LiveKit:** ICE/STUN candidates are arbitrary client IPs; an allowlist
  would break media.
- **Pulse:** Public ENet/UDP game server; clients connect from anywhere.

The archipelago services *do* have an IP allowlist (loopback + the two
Cloudflare /13s) because their only external dependency is
`comms-gatekeeper.decentraland.org`, which is CF-fronted.

## No IP-level egress allowlist on squid-eth / squid-polygon

Operators may switch RPC providers. Pinning the upstream IPs is brittle
across provider changes, key rotations, and CF-fronted RPC endpoints.
