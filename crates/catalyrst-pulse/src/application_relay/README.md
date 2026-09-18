# Scoped application relay

This optional v4 capability transports opaque application payloads inside the exact
LiveKit room authorized by a signed admission grant. Media remains in LiveKit.
Legacy clients continue using their existing routes. The server selects
`application_relay` only when a live authority adapter is configured; clients must
wait for the reliable room result before routing application data through Pulse.
The listener role remains read-only for typed avatar state. A listener with its
own valid `canPublishData` room grant may publish application messages in that
scope, including the separately authorized scene-server identity.

V4 reuses the successful authenticated challenge's 32-byte random value. The
result's optional nonce field stays empty, adding no repeated nonce bytes. A room
join contains only the original JWT header.payload and an HMAC possession proof,
not its bearer signature. The proof binds the challenge, authenticated wallet and
signing session, and the hash of the exact signing input. Neither proof nor claims
are logged. Identity and guest status come from signed claims, never the payload.

JWT possession alone is insufficient. The separate comms authority checks current
room purpose, bans, world access and protected island ownership/fencing. Its
internal protobuf endpoint uses a distinct HMAC service key and HTTPS. Initial
JWT expiry is checked at admission; continuous live leases may outlast it.
Renewal never turns into a fresh join. Permission lasts at most two seconds from
request start, and a delayed reply cannot revive an expired lease. This bounds
revocation delay; it does not provide an atomic guarantee with a LiveKit kick.
Authority denial or expiry removes the exact scope and sends a reliable self-left
notification so the client uses its fallback while typed Pulse movement continues.

The server bounds rooms (1024), participants per room (64), scopes per connection
(16), concurrent authority work (128), join attempts (16 per second), and sender
traffic (120 messages / 128 KiB per second). Reliable payloads are at most 3072
bytes, unreliable payloads 1024 bytes, and unsigned credentials 3072 bytes. These
fit the existing 4096-byte reliable WebTransport frame. Membership rosters are
reliable and independent of movement interest. Fanout visits only the source room.
Transport permits remain held through ENet acknowledgement or QUIC stream write;
reliable enqueue failure retires the affected peer rather than leaving it ready.

Enable with `PULSE_APPLICATION_RELAY_ENABLED=true` and `PULSE_V4_ENABLED=true`,
`PULSE_APPLICATION_RELAY_LIVEKIT_API_KEY`, `PULSE_APPLICATION_RELAY_LIVEKIT_SECRET`,
`PULSE_ROOM_AUTHORITY_URL` (exact `/internal/pulse/room-authority/v1` endpoint), and
`PULSE_ROOM_AUTHORITY_KEY` (32 distinct random bytes, base64url without padding).
`PULSE_ROOM_AUTHORITY_ALLOW_LOOPBACK_HTTP=true` permits numeric loopback fixtures
only. The feature defaults off. Production enablement requires the composed comms
policy/PG and client relay checks; the strict test authority is not a substitute.

The existing connection-model theorems do not establish this new relay's
cryptographic, HTTP, policy, or whole-program correctness. Focused Rust and real
transport integration tests supply implementation evidence for these boundaries.
