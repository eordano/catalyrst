# Networking, firewall, and ingress

The deployment fronts public HTTPS through a CDN/reverse proxy and exposes only
a few ports directly. Everything that *can* go through the proxy *does*; UDP
media/game traffic bypasses it (the proxy doesn't forward arbitrary UDP).

## Firewall

A host firewall (e.g. `nftables`) is the backend. Open ports:

| Port  | Proto | Service             | Notes                            |
|-------|-------|---------------------|----------------------------------|
| 22    | TCP   | sshd                | key-only, with brute-force protection on top |
| 80/443| TCP   | reverse proxy       | accepted only from your CDN's published v4/v6 ranges |
| 7881  | TCP   | LiveKit RTC fallback| TCP fallback when UDP fails      |
| 7777  | UDP   | Pulse (ENet)        | authoritative game server        |
| 7882  | UDP   | LiveKit media       | SFU media                        |

Restrict 80/443 to your CDN/proxy's published IP ranges so the firewall has a
self-contained source of truth. If your reverse proxy needs the real client IP,
configure its `real_ip` source from the same CDN range list and refresh it
periodically, alerting on staleness.

## SSH

Disable per-source connection penalties if aggressive rate limiting risks
locking out trusted automation. Apply such options through the structured
settings of your SSH config so they aren't override-ordered out.

## Brute-force protection

A tool such as `fail2ban` works well; pick a retry threshold, a ban duration,
and a ban action that matches your firewall backend (e.g. an `nftables`-based
action when using nftables).
