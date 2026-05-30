# nginx (edge / TLS terminator)

nginx terminates TLS for `example.com` and `livekit.example.com`,
then reverse-proxies to loopback ports.

## Real-client IP

If you front the edge with a CDN/proxy, restore the real client IP by
including your proxy's trusted-IP list (e.g. `include <DATA_DIR>/nginx-real-ip.conf;`)
plus `real_ip_header <YOUR-CONNECTING-IP-HEADER>` and `real_ip_recursive on`.
See `docs/cloudflare-ips.md` for the refresh logic.

## Rate limits

| Zone        | Rate      | Applied to                       |
|-------------|-----------|----------------------------------|
| `catread`   | 30 r/s    | every location under `/` (burst 60, nodelay) |
| `catdeploy` | 2 r/s     | `POST /content/entities` only (burst 4, nodelay) |
| `catws`     | conn-based| `/ws` (max 8 concurrent per IP)  |

429 status for both `limit_req_status` and `limit_conn_status`.

## Security headers (apex vhost)

`Strict-Transport-Security` (2-year, preload), `X-Frame-Options SAMEORIGIN`,
`X-Content-Type-Options nosniff`, `Referrer-Policy strict-origin-when-cross-origin`,
`Permissions-Policy "interest-cohort=()"`, restrictive CSP,
`Cross-Origin-Opener-Policy same-origin`, `Cross-Origin-Resource-Policy same-origin`.

`client_max_body_size 1m` apex-wide; the deploy location overrides to 200m.

## Endpoint allowlist (defense in depth)

The catch-all `location /` proxies to catalyrst, so we explicitly 404 a few
internal-only paths to prevent leakage if the upstream ever exposes them:

```
locations."= /metrics" → return 404;
locations."= /admin"   → return 404;
locations."= /debug"   → return 404;
```

## LiveKit vhost (`livekit.<domain>`)

- `onlySSL = true` (no HTTP redirect on the SFU vhost).
- `locations."/rtc"` proxies WebSockets to the SFU on `127.0.0.1:<SFU_PORT>`,
  `proxy_read_timeout 3600s`.
- `locations."/"` → 404. The Twirp admin API also lives on `<SFU_PORT>` (same
  process); this keeps it off the internet even if someone later adds a
  generic `/` location.

## Deploy endpoint

`POST /content/entities` proxies to catalyrst on `:<CONTENT_PORT>`:

- `proxy_buffering off` (streaming deploy body).
- `client_max_body_size 200m`, `client_body_timeout 300s`,
  `proxy_read_timeout 600s`.
- Goes through the `catdeploy` rate-limit zone.
