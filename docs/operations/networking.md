# Networking, firewall, sandboxing

> Status: distilled 2026-07-04 from the reference deployment
> (`nixos/configuration.nix`); re-verified 2026-07-03 (docs-stale-audit).

## Ingress model

Everything that *can* ride the TLS reverse proxy *does*. Only UDP media/game
traffic bypasses it (proxies don't forward arbitrary UDP):

| Port | Proto | Service | Notes |
|---|---|---|---|
| 22 | TCP | sshd | key-only + brute-force protection |
| 80/443 | TCP | reverse proxy | accept only from your CDN's published v4/v6 ranges |
| 7881 | TCP | LiveKit RTC fallback | TCP fallback when UDP fails |
| 7777 | UDP | Pulse (ENet) | authoritative game server |
| 7882 | UDP | LiveKit media | SFU media |

Restrict 80/443 to the CDN ranges in the firewall itself, and feed the same
range list to the proxy's `real_ip` config (refreshed on a schedule, alert on
staleness — see below).

## The "peers in roster but no remote avatars" failure

If archipelago shows peers on the same island but remote avatars never render,
suspect **inbound UDP being dropped to the SFU** — DTLS handshakes time out
while the HTTPS signaling all works, so everything *looks* healthy except
media. Fixes seen in practice: open/forward the SFU UDP port range, or set the
LiveKit `node_ip` to an address peers can actually reach (e.g. an overlay
network address instead of a NATed one), then restart the SFU. Note that naked
STUN to a host that drops it fails silently.

## Cloudflare/CDN IP refresh — two decoupled sources of truth

- **Proxy `real_ip` include** — a dynamic file refreshed daily from
  `https://www.cloudflare.com/ips-v4` / `ips-v6`. **Fail-soft:** any HTTP error
  or sanity-check failure exits 0 leaving the previous snapshot intact — an
  empty include can never be produced. Sanity: v4 lines `^[0-9].*/[0-9]+$`, v6
  `^[0-9a-fA-F:].*/[0-9]+$`. Atomic `mktemp`+`mv`, proxy reload, then write a
  refresh-timestamp metric.
- **Firewall input rules** — hardcoded in the declarative host config (must
  build reproducibly), updated by hand. A staleness alert
  (`cloudflare_ips_refresh_timestamp_seconds` > 7 days) catches drift between
  the two.

A one-shot seed (identical to the firewall list) populates the include before
the first refresh so the proxy can start on a fresh host.

## systemd sandbox carve-outs — why some knobs are deliberately loose

`nixos/configuration.nix` derives every service from one of four hardening
profiles (`baseSandbox ⊂ commsHardening ⊂ noPgSandbox ⊂ noJitHardening`).
Deliberate omissions — do not "tighten" these without reading why:

- **`PrivateUsers` omitted from `baseSandbox`** — a child user namespace hides
  the real UID from Postgres's `SO_PEERCRED` peer auth, breaking every service
  that peer-auths to the local cluster. Re-added (`noPgSandbox`) only for
  services that don't touch postgres.
- **`~@resources` not filtered** — historical carve-out from the .NET Pulse era
  (`mbind`/`set_mempolicy`/`sched_setattr`); the Rust `catalyrst-pulse` no
  longer needs it, but the filter has not been re-tightened since the port.
  Candidate for cleanup.
- **`RestrictFileSystems` disabled** — needs the BPF LSM hook; the deployed
  kernel doesn't enable it and services exit 244 if set. Revisit when the
  kernel ships bpf-lsm.
- **MDWE excludes archipelago workers** — V8 JITs need W+X pages; with
  `MemoryDenyWriteExecute` they SIGTRAP on first JIT. (Pulse's matching
  exclusion is stale since the Rust port — it could move to the stricter
  profile; not yet re-evaluated.)
- **No IP allowlist on sync, LiveKit, Pulse egress/ingress** — the sync pool
  rotates and isn't CDN-fronted; ICE/STUN candidates are arbitrary client IPs;
  Pulse is a public UDP game server. The archipelago services *do* get an IP
  allowlist (loopback + CDN ranges) because their only external dependency is
  one CDN-fronted gatekeeper host.
- **No egress pinning on squid RPC providers** — operators switch RPC
  providers; pinning IPs is brittle across provider changes and CDN-fronted
  endpoints.
