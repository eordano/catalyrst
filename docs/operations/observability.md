# Observability — Prometheus, blackbox, alerts

> Status: distilled 2026-07-04 from `nixos/configuration.nix`; re-verified
> 2026-07-03 (docs-stale-audit).

All exporters and Prometheus bind loopback only; tunnel to explore
(`ssh -L 9090:127.0.0.1:9090 <host>`).

## Scrape targets

- `node` (`:9100`) — node_exporter with `systemd` + `textfile` collectors. The
  textfile dir is how periodic jobs (LiveKit key rotation, CDN-IP refresh)
  publish metrics without their own exporter.
- `catalyrst` (`:5141/metrics`) — the content core.
- `archipelago` (`:5000/:5001/:5002`) — core, ws-connector, stats.
- `blackbox_about` — probe of `https://<host>/content/about` via
  blackbox_exporter `:9115`, module `about_comms_healthy` (200 + body matching
  `"comms":{"healthy":true`).

## Alert rules

| Alert | Expression | For | Severity |
|---|---|---|---|
| AboutDownOrCommsUnhealthy | `probe_success{job="blackbox_about"} == 0` | 3m | critical |
| CertExpiringSoon | `probe_ssl_earliest_cert_expiry - time() < 14d` | 1h | warning |
| ServiceDown | `up{job=~"catalyrst\|archipelago\|pulse\|node"} == 0` | 3m | critical |
| LiveKitKeyStale | rotation timestamp older than 100d | 1h | warning |
| CloudflareIpsStale | refresh timestamp older than 7d | 1h | warning |
| DiskAlmostFull / DiskCritical | rootfs avail < 10% / < 5% | 15m / 5m | warning / critical |
| SyncHeartbeatStale | `time() - catalyrst_sync_heartbeat_timestamp_seconds > 900` | 5m | critical |
| SyncIngestSilent | `increase(catalyrst_sync_deployments_total[2h]) == 0` | 30m | warning |

Sync-liveness metrics (added 2026-07-04, on `:5141/metrics`):
`catalyrst_sync_heartbeat_timestamp_seconds` (beats ≤ every 10 s per fetched
pointer-changes page — the liveness signal), `catalyrst_sync_frontier_timestamp_seconds`
(the **persisted** frontier — coarse, advances at phase ends, do not alert on
it), and the pre-existing `catalyrst_sync_deployments_total` counter (actual
ingest). The gauges only exist on sync-enabled nodes after the first beat, so
read-only nodes never page; `SyncIngestSilent` likewise can't fire until the
counter's first post-restart increment — `SyncHeartbeatStale` is the
loop-dead pager, `SyncIngestSilent` the loop-beats-but-nothing-lands check.
See [../sync.md](../sync.md) for the underlying keys.

## Known gaps (transparency)

- **No Alertmanager delivery is wired.** Alerts fire in Prometheus with
  nowhere to go — add a receiver before treating these as on-call pages.
- **The `pulse` scrape target scrapes nothing.** `catalyrst-pulse` (Rust,
  plain tokio/ENet UDP) exposes no HTTP endpoint; the `:5005/metrics` target
  and the `pulse` leg of `ServiceDown` are stale from the pre-Rust .NET era
  and will read as perpetually down until an exporter is added or the target
  removed.
- Most service crates beyond the content core expose `/health` (probed by the
  landing page / admin console via `CATALYRST_SERVICE_URLS`) but no
  per-service Prometheus metrics yet; the content core's sync-liveness gauges
  above are the exception.
