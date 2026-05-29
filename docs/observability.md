# Prometheus, blackbox, alerts

All exporters and Prometheus itself bind loopback only. To explore:
`ssh -L 9090:127.0.0.1:9090 <host>` then open `http://localhost:9090/`.

## Scrape targets

- `node` (`:9100`) — node_exporter with `systemd` and `textfile`
  collectors. The textfile dir (`/var/lib/node-exporter-textfile`) is how
  the LiveKit-rotation and CF-IPs-refresh jobs publish their metrics
  without running their own exporter.
- `catalyrst` (`:5141/metrics`) — the application.
- `archipelago` (`:5000`, `:5001`, `:5002`) — core, ws-connector, stats.
- `pulse` (`:5005/metrics`) — `Metrics__Type = "Prometheus"`.
- `blackbox_about` — black-box probe of
  `https://example.com/content/about` via blackbox_exporter on
  `:9115`, module `about_comms_healthy` (expects 200 + body matching
  `"comms":\{"healthy":true`).

## Alert rules

| Alert                  | Expression                                                                                 | For    | Severity  |
|------------------------|--------------------------------------------------------------------------------------------|--------|-----------|
| AboutDownOrCommsUnhealthy | `probe_success{job="blackbox_about"} == 0`                                              | 3m     | critical  |
| CertExpiringSoon       | `probe_ssl_earliest_cert_expiry - time() < 1209600` (14d)                                  | 1h     | warning   |
| ServiceDown            | `up{job=~"catalyrst|archipelago|pulse|node"} == 0`                                         | 3m     | critical  |
| LiveKitKeyStale        | `time() - livekit_rotation_timestamp_seconds > 100*86400`                                  | 1h     | warning   |
| CloudflareIpsStale     | `time() - cloudflare_ips_refresh_timestamp_seconds > 7*86400`                              | 1h     | warning   |
| DiskAlmostFull         | rootfs avail/size < 10% (excludes tmpfs/overlay/squashfs/ramfs)                            | 15m    | warning   |
| DiskCritical           | rootfs avail/size < 5%                                                                     | 5m     | critical  |

## Gaps

- **No Alertmanager delivery wired up.** Alerts fire in Prometheus but
  there's nowhere for them to go yet. Add an Alertmanager target +
  receiver before treating these as oncall pages.
