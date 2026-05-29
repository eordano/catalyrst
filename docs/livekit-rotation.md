# LiveKit API key rotation

Quarterly rotation of the LiveKit API key + secret with atomic rollback.
Timer: `*-01,04,07,10-01 03:00:00` with `RandomizedDelaySec = 1h`,
`Persistent = true` so a missed window catches up on next boot.

## Rotation procedure (`livekit-rotate.service`)

1. Snapshot `livekit.yaml` and `livekit-api.env` to `.prev`.
2. Generate `KEY=API<12-hex>` and `SECRET=base64(36 bytes)` via `openssl`.
3. Write a new `livekit.yaml` (single-key map) and `livekit-api.env`
   atomically via `mktemp` + `mv`. Both files are mode 0600, owned by root.
4. `systemctl restart livekit.service`; sleep 5.
5. **Rollback path:** if `livekit.service` isn't active, move `.prev`
   files back, restart `livekit.service` *and* `archipelago-core.service`,
   exit 1. (archipelago-core mints tokens against the LiveKit key, so it
   must restart against whichever key won.)
6. On success: restart `archipelago-core.service` so it picks up the new key.
7. Write a Prometheus metric to
   `/var/lib/node-exporter-textfile/livekit_rotation.prom`:
   `livekit_rotation_timestamp_seconds <unix>`.

## Alert

`LiveKitKeyStale`: `time() - livekit_rotation_timestamp_seconds > 100*86400`
for 1h, severity warning. Catches a stuck timer that would let the key age
indefinitely.
