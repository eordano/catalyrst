# LiveKit - keys, rotation, SFU gotchas

## Key facts that bite

- `devkey`/`devsecret`: `catalyrst-comms` and `catalyrst-worlds` now FAIL FAST at boot when `LIVEKIT_API_KEY`/`LIVEKIT_API_SECRET` are unset, unless `LIVEKIT_ALLOW_DEV_CREDS=1` explicitly opts into the dev defaults. `catalyrst-archipelago` still boots on the dev defaults with only a warning - its minted JWTs parse fine locally but a real SFU rejects them, and `livekit_configured=false` is surfaced only in `/status`. Set the real key/secret/host consistently across every service that mints or validates tokens for the same SFU (comms, worlds, archipelago - in bundle terms: the social and explore env files).
- `/rtc` 502 / "peers in roster, no remote avatars": HTTPS signaling can be healthy while media is dead - if inbound UDP to the SFU is dropped, DTLS times out and remote avatars never render even though archipelago rosters show the peers. Check the SFU UDP port range in the firewall and make sure the LiveKit `node_ip` is an address peers can actually reach (overlay/tailnet address rather than a NATed one). See [networking.md](./networking.md).
- The Twirp admin API listens on the same port as `/rtc`; the edge config deliberately 404s `/` on the SFU vhost so it never reaches the internet.

## Quarterly rotation (`livekit-rotate.service`)

Timer `*-01,04,07,10-01 03:00:00`, `RandomizedDelaySec=1h`, `Persistent=true` (missed windows catch up
on boot). Procedure, with atomic rollback:

1. Snapshot `livekit.yaml` + `livekit-api.env` to `.prev`.
2. Generate `KEY=API<12-hex>`, `SECRET=base64(36 bytes)`.
3. Write both files atomically (`mktemp` + `mv`, mode 0600, root).
4. Restart `livekit.service`; sleep 5.
5. Rollback: if the SFU isn't active, restore `.prev`, restart the SFU AND `archipelago-core` (it mints tokens against whichever key won), exit 1.
6. On success, restart `archipelago-core` to pick up the new key.
7. Publish `livekit_rotation_timestamp_seconds` via the node-exporter textfile dir - the `LiveKitKeyStale` alert (>100 days) catches a stuck timer.

If you rotate by hand, replicate step 5's pairing: the SFU and every token-minting service must agree
on the key or comms dies quietly.
