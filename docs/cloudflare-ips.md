# Cloudflare IP range refresh

nginx needs `set_real_ip_from` to span the current Cloudflare edge ranges
so `$remote_addr` resolves to the client, not the CF POP. The list at
`/var/lib/cloudflare/nginx-real-ip.conf` is refreshed daily.

## Sources of truth

- **nginx `real_ip` include** (`/var/lib/cloudflare/nginx-real-ip.conf`):
  refreshed daily. Editable at runtime without a `nixos-rebuild`.
- **Firewall `extraInputRules`** (hardcoded in `nixos/configuration.nix`):
  static, refreshed only when someone edits the file. The
  `CloudflareIpsStale` alert fires if the dynamic list and the firewall
  list drift apart for too long.

The two are deliberately decoupled because the firewall is part of the
NixOS module set and must produce the same config every build, while the
nginx include is dynamic state.

## Seed (`cloudflare-ips-seed.service`)

`before = [ "nginx.service" ]`. On first boot (or after the file is
deleted) copies the hardcoded seed (`/etc/cf-nginx-real-ip-seed.conf`,
identical to the firewall list) to `/var/lib/cloudflare/nginx-real-ip.conf`
so nginx has something to include before the first refresh runs.

## Refresh (`cloudflare-ips-refresh.service` + `.timer`)

Daily. Fetches `https://www.cloudflare.com/ips-v4` and `…/ips-v6`. **Fail-soft:**
on any HTTP error or sanity-check failure, exit 0 — the previous snapshot stays
intact, nginx keeps working, no empty include can ever be produced.

Sanity check: v4 lines must match `^[0-9].*/[0-9]+$`; v6 lines
`^[0-9a-fA-F:].*/[0-9]+$`. On success: build the include atomically via
`mktemp` + `mv`, reload nginx, write the
`cloudflare_ips_refresh_timestamp_seconds` Prometheus textfile metric.

## Alert

`CloudflareIpsStale`: `time() - cloudflare_ips_refresh_timestamp_seconds > 7*86400`
for 1h, severity warning.
