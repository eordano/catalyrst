# Cloudflare IP range refresh

When running behind Cloudflare, your reverse proxy needs its trusted-proxy
list (e.g. nginx `set_real_ip_from`) to span the current Cloudflare edge
ranges so `$remote_addr` resolves to the real client, not the CF POP. Keep a
dynamic include file (e.g. `<DATA_DIR>/cloudflare/nginx-real-ip.conf`) and
refresh it on a schedule.

## Sources of truth

- **Proxy `real_ip` include** (`<DATA_DIR>/cloudflare/nginx-real-ip.conf`):
  refreshed on a schedule (e.g. daily). Editable at runtime without
  rebuilding the proxy config.
- **Firewall input rules** (hardcoded in your host/firewall config):
  static, refreshed only when someone edits the config. A staleness alert
  can fire if the dynamic list and the firewall list drift apart for too
  long.

The two are deliberately decoupled: the firewall is part of the declarative
config and must produce the same output every build, while the proxy include
is dynamic runtime state.

## Seed

Run a one-shot seed step before the proxy starts. On first boot (or after the
include is deleted) it copies a hardcoded seed (identical to the firewall
list) into the include path so the proxy has something to include before the
first refresh runs.

## Refresh (scheduled job)

Run on a schedule (e.g. daily). Fetch `https://www.cloudflare.com/ips-v4`
and `https://www.cloudflare.com/ips-v6`. **Fail-soft:** on any HTTP error or
sanity-check failure, exit 0 — the previous snapshot stays intact, the proxy
keeps working, and no empty include can ever be produced.

Sanity check: v4 lines must match `^[0-9].*/[0-9]+$`; v6 lines
`^[0-9a-fA-F:].*/[0-9]+$`. On success: build the include atomically via
`mktemp` + `mv`, reload the proxy, and write a refresh-timestamp metric for
monitoring.

## Alert

Alert when the include goes stale, e.g.
`time() - cloudflare_ips_refresh_timestamp_seconds > 7*86400` for 1h,
severity warning.
