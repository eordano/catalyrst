# Networking, firewall, and ingress

The deployment fronts public HTTPS through Cloudflare and exposes only a few
ports directly. Everything that *can* go through CF *does*; UDP media/game
traffic bypasses it (CF doesn't proxy arbitrary UDP).

## Firewall

`networking.nftables` is the backend. Open ports:

| Port  | Proto | Service             | Notes                            |
|-------|-------|---------------------|----------------------------------|
| 22    | TCP   | sshd                | key-only, fail2ban on top        |
| 80/443| TCP   | nginx               | accepted only from CF v4/v6 ranges (see `extraInputRules`) |
| 7881  | TCP   | LiveKit RTC fallback| TCP fallback when UDP fails      |
| 7777  | UDP   | Pulse (ENet)        | authoritative game server        |
| 7882  | UDP   | LiveKit media       | SFU media                        |

The CF ranges in `extraInputRules` are hardcoded so the firewall has a
self-contained source of truth. The nginx `real_ip` include
(`/var/lib/cloudflare/nginx-real-ip.conf`) is refreshed daily by
`cloudflare-ips-refresh.service` — see `docs/cloudflare-ips.md` for the
refresh logic and the `CloudflareIpsStale` alert that catches drift.

## SSH

`PerSourcePenalties = "no"` — leaving it on locked the automation IP out
during a previous incident. Set via `settings = { ... }` (not
`extraConfig`) so it isn't override-ordered out.

## fail2ban

`maxretry = 8`, `bantime = "1h"`, `banaction = "nftables-multiport"` to
match the firewall backend.
