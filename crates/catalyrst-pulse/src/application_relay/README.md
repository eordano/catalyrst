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
Transport permits remain held through ENet acknowledgement or QUIC stream write.
A reliable payload that finds its recipient's queue full retires only that
recipient's scope in that room, and a reliable send over the sender limits retires
only the sender's scope; an over-limit unreliable send is dropped. The retired peer
gets its self-left notification and uses its fallback while typed Pulse movement
continues. Membership notifications may draw on a separate 8 KiB reserve per
connection, at most 2 MiB per transport host, when the payload queue is full. Any
other reliable enqueue failure, a notification that fits neither budget, or a
payload outside the advertised limits retires the connection rather than leaving
it ready.

Each room carries a roster version: 1 at its first join, plus one per later join
and per leave. The join result and both membership notices carry it, and a client
stamps every send with the last version it applied; zero keeps the earlier
behaviour and is never answered. A stamped send skips members that joined after
it, which the sender still reaches through LiveKit. A stamped reliable send that
predates a leave (log of 64 leaves, 10 seconds) is answered with one send-failed
notice naming those identities and echoing the payload, split only when it would
pass the 4096-byte frame; a targeted send names only its recipient. Only a member
that had joined by the stamped version is named: one that came and went after it
was never in the sender's Pulse roster and already got the LiveKit copy. A queue-full
retirement answers the blocked and the purged payloads the same way when their
sender has stamped a send. The notice is payload-class: one that cannot be queued
retires the sender's scope and is not answered. Unreliable sends are never
answered. A connection holding a scope gets a flat 5 second ENet timeout, back to
the default when its last scope leaves. A WebTransport connection that holds a
scope and sends input is watched instead: after 5 seconds without an inbound
message its scopes are retired on the next tick, and its session is kept so a
stalled client can join again. Listeners send no input and are not watched.
Reliable payloads queued to a member inside that window are lost.

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
