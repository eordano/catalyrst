# Pulse byte-exact implementation plan

Grounded from upstream decentraland/Pulse (C#) + protocol origin/quantization. Drives tasks #45 (ENet), #46 (bitwise codec + messages), #47 (handshake).

## ENet UDP Transport (Datagram Protocol + Channel Semantics)

### Wire spec
## ENet UDP Wire Format (lsalzman/enet + ENet-CSharp variant used by Decentraland/Pulse)

### Datagram Structure
Every UDP datagram = ProtocolHeader (2-4 bytes) + Commands (1..N per datagram)

### ProtocolHeader (ENet.cs:72-86)
**Current catalyrst impl (WRONG):** 4 bytes fixed (peer_id:u16 BE + sent_time:u16 BE)
**Upstream/real ENet:** 2-4 bytes variable, with session ID + flag bits packed into peer_id field

**Byte Layout (Big-Endian):**
```
Offset  Size  Field Name          Encoding
------  ----  ---------------     -------------------------------------------
0-1     2     peerID              u16 = (outgoing_peer_id & 0xFFF) 
                                      | ((session_id & 0xF) << 12)
                                  Bits [11:0] = actual peer index (0-4095)
                                  Bits [15:12] = session ID (0-15)
        
1 bit (implicit in peerID[15:12])
        1     SENT_TIME_FLAG      if set, sentTime field follows (4 bytes total)
                                  if clear, no sentTime; header is 2 bytes
        
        3 bits reserved/flags     (part of session nibble bits)

2-3 or 4-5  2 or 0  sentTime    u16 BE (milliseconds), ONLY present if SENT_TIME_FLAG=1
                                  header is 4 bytes when flag set; 2 bytes when clear
                                  when present: timing info from reliable channel ACK tracking
```

**File references:**
- `decentraland/Pulse/src/DCLPulse.Transport.Shared/Runtime/ENet.cs` lines 72-77 (ENetAddress struct)
- `catalyrst/crates/catalyrst-enet/src/protocol.rs` lines 64-86 (current simplified ProtocolHeader)

---

### Command Header (per-command frame header)
**Bytes [0:4]** of each command (immediately after ProtocolHeader, repeating)

```
Offset  Size  Field Name            Encoding
------  ----  -------------------   -----------------------------------------
0       1     commandByte           u8:
                                    Bits [3:0]  = Command type (enum below)
                                    Bit  [6]    = COMMAND_FLAG_UNSEQUENCED (0x40)
                                    Bit  [7]    = COMMAND_FLAG_ACKNOWLEDGE (0x80)
1       1     channelID             u8, channel index (0-255, typically 0-2)
2-3     2     reliableSeqNumber     u16 BE, sequence number for reliable-ordered commands
                                    unused (0) for unreliable/unsequenced
```

**File reference:** `catalyrst/crates/catalyrst-enet/src/protocol.rs` lines 88-121

---

### Command Types (lower 4 bits of commandByte)
```
Value   Enum Name                Size (excluding CommandHeader)
-----   --------------------     --------------------------------
0       None                     0 bytes
1       Acknowledge              4 bytes: receivedReliableSeq(u16) + receivedSentTime(u16)
2       Connect                  40 bytes + optional 4-byte data field (total 44 for CONNECT)
3       VerifyConnect            40 bytes (same ConnectBody, no data field)
4       Disconnect               4 bytes: data(u32 BE)
5       Ping                     0 bytes
6       SendReliable             2-byte length-prefix + payload
7       SendUnreliable           4 bytes header + 2-byte length-prefix + payload
8       SendFragment             variable (not implemented in catalyrst yet)
9       SendUnsequenced          variable (not implemented in catalyrst yet)
10      BandwidthLimit           8 bytes
11      ThrottleConfigure        16 bytes
12      SendUnreliableFragment   variable (not implemented in catalyrst yet)
```

---

### ConnectBody (CONNECT / VERIFY_CONNECT command payload)
**40 bytes fixed, encoding:**

```
Offset  Size  Field Name                      Encoding
------  ----  -----————————————————————————    -----------------------------------------
0-1     2     outgoingPeerID                  u16 BE, peer ID the sender assigned to us
2       1     incomingSessionID               u8, session ID being acknowledged
3       1     outgoingSessionID               u8, session ID for our peer's outgoing direction
4-7     4     mtu                             u32 BE, max transmission unit [576, 4096]
8-11    4     windowSize                      u32 BE, receive window size (4096 typical)
12-15   4     channelCount                    u32 BE, negotiated channel count (0-255, usually 2-3)
16-19   4     incomingBandwidth               u32 BE, bits/sec, 0=unlimited
20-23   4     outgoingBandwidth               u32 BE, bits/sec, 0=unlimited
24-27   4     packetThrottleInterval          u32 BE, milliseconds (5000 typical)
28-31   4     packetThrottleAcceleration      u32 BE, scaling factor (2 typical)
32-35   4     packetThrottleDeceleration      u32 BE, scaling factor (2 typical)
36-39   4     connectID                       u32 BE, CRITICAL: random token echoed in VERIFY_CONNECT
                                              (catalyrst hardcodes 0, causing handshake failure)
40-43   4     (CONNECT only) data             u32 BE, optional connection data, SKIPPED in VERIFY_CONNECT
```

**File references:**
- ENet.cs lines 123-186 (ConnectBody encoding/decoding)
- catalyrst/crates/catalyrst-enet/src/protocol.rs lines 125-186 (ConnectBody struct + encode/decode)

---

### SendReliable Command Body
```
Offset  Size  Field                   Encoding
------  ----  --------————————————    -----------------------------------------
0-1     2     payloadLength           u16 BE, byte count of following data
2..N    var   payload                 raw application bytes (0 to MTU-12)
```

**File reference:** catalyrst-enet/src/protocol.rs lines 261-277

---

### SendUnreliable Command Body
```
Offset  Size  Field                   Encoding
------  ----  -----------————————    -----------------------------------------
0-1     2     unreliableSeqNumber     u16 BE, per-channel sequence number
2-3     2     payloadLength           u16 BE, byte count of following data
4..N    var   payload                 raw application bytes
```

**File reference:** catalyrst-enet/src/protocol.rs lines 279-296

---

### AcknowledgeBody (ACKNOWLEDGE command)
```
Offset  Size  Field                       Encoding
------  ----  ---------—————————————————  -----------------------------------------
0-1     2     receivedReliableSeqNumber   u16 BE, sequence being acknowledged
2-3     2     receivedSentTime            u16 BE, echoed sentTime from received header
```

**File reference:** catalyrst-enet/src/protocol.rs lines 188-212

---

### Channel Conventions (enforced by application flags, not ENet itself)
**Per ENetHostedService.cs:170-176 and ENetChannel.cs:**

| Channel | ID | PacketFlags              | Semantics                          | Usage in Pulse                              |
|---------|----|--------------------------|------------------------------------|---------------------------------------------|
| 0       | 0  | Reliable                 | Reliable ordered                   | Handshake, RESYNC, STATE_FULL, events       |
| 1       | 1  | None (or Unthrottled)    | Unreliable sequenced               | STATE_DELTA, MovementInput (high-frequency) |
| 2       | 2  | Unsequenced              | Unreliable unordered               | (reserved, not used in current Pulse)       |

**File reference:** ENetChannel.cs lines 27-46

---

### Session ID & Flags Bits (key divergence from catalyrst)
**Real ENet peerID packing (NOT currently in catalyrst):**
- Bits [11:0]: actual peer index (12 bits = 4096 max peers)
- Bits [14:12]: session ID (3 bits = 0-7 sessions per peer, used to distinguish stale packets)
- Bit [15]: reserved or used by certain implementations for flags (e.g., SENT_TIME_FLAG in some stacks)

**sentTime flag logic (NOT in catalyrst):**
- If SENT_TIME_FLAG is set in the peerID high bits, the header is 4 bytes
- If SENT_TIME_FLAG is clear, the header is 2 bytes (just peerID)
- sentTime is always 2 bytes when present

This reduces header overhead for pure-control datagrams from 4→2 bytes.

---

### Optional CRC32 Trailer (NOT in catalyrst)
Some ENet variants append an optional 4-byte CRC32 at the end of the datagram if checksumming is enabled. Current catalyrst does not support; upstream Pulse does not require (may be off).

**File reference:** ENet.cs lines 713-718 (SetChecksumCallback in native library)

### Rust plan
## Rust Implementation Plan for catalyrst-enet + catalyrst-pulse (byte-exact parity)

### Phase 1: catalyrst-enet protocol.rs — peerID bit-packing & variable sentTime

1. **Modify ProtocolHeader struct:**
   - Change from simple `{ peer_id: u16, sent_time: u16 }` to a packed encoding
   - **Option A (recommended):** Keep the struct fields but encode/decode to/from the bitwise layout
     ```rust
     pub struct ProtocolHeader {
         pub peer_id: u16,           // actual 12-bit index
         pub session_id: u8,         // 3 bits (0-7)
         pub has_sent_time: bool,    // 1 bit flag
         pub sent_time: u16,         // only present if has_sent_time=true
     }
     ```
   - **Option B (direct packing):** Keep public wire-agnostic API but encode peer_id as `(idx | (session << 12))` internally
   
2. **Update ProtocolHeader::encode():**
   - Pack into 2 bytes (no sentTime) or 4 bytes (with sentTime)
   - Byte 0-1: `u16::to_be_bytes((peer_id & 0xFFF) | ((session_id as u16 & 0xF) << 12))`
   - If has_sent_time: Bytes 2-3: `sent_time.to_be_bytes()`
   - Return a Vec with correct length (2 or 4)
   
3. **Update ProtocolHeader::decode():**
   - Read 2 bytes for peerID+session
   - Extract session_id = (u16 >> 12) & 0xF
   - Extract peer_id = u16 & 0xFFF
   - Check if SENT_TIME_FLAG bit is set; if yes, read 2 more bytes
   - Return ProtocolHeader + bytes_consumed count for buffer advancement

4. **Update tests in protocol.rs:**
   - Test roundtrip with session_id set
   - Test 2-byte header (no sentTime) roundtrip
   - Test 4-byte header (with sentTime) roundtrip
   - Verify masking/extraction of session bits

**Files to modify:**
- `crates/catalyrst-enet/src/protocol.rs` lines 64-86 (ProtocolHeader struct + encode/decode)

---

### Phase 2: catalyrst-enet host.rs & peer.rs — session ID tracking

1. **Add session_id field to Peer struct** (`peer.rs`):
   ```rust
   pub struct Peer {
       pub id: PeerId,
       pub session_id: u8,      // NEW: track assigned session per peer
       pub outgoing_peer_id: u16,
       pub state: PeerState,
       // ... rest of fields
   }
   ```

2. **Update Peer::header()** to include session:
   ```rust
   pub fn header(&self, sent_time: Option<u16>) -> ProtocolHeader {
       ProtocolHeader {
           peer_id: self.id,
           session_id: self.session_id,
           has_sent_time: sent_time.is_some(),
           sent_time: sent_time.unwrap_or(0),
       }
   }
   ```

3. **Update host.rs handshake:**
   - On incoming CONNECT: extract `session_id` from ConnectBody.incomingSessionID
   - Generate `outgoingSessionID` (can be a counter or random in [0, 15])
   - Send VERIFY_CONNECT with matching session IDs

4. **Validate connectID echo (CRITICAL FOR HANDSHAKE):**
   - Store client's `connect_id` from incoming CONNECT
   - In VERIFY_CONNECT response, **echo the same connectID** (NOT hardcoded 0)
   - Client validates that incoming VERIFY_CONNECT has the same connectID it sent
   - This prevents replay and cross-connection confusion

**Files to modify:**
- `crates/catalyrst-enet/src/peer.rs` (add session_id field, update header())
- `crates/catalyrst-enet/src/host.rs` (extract session on CONNECT, echo connectID on VERIFY_CONNECT)

---

### Phase 3: catalyrst-enet protocol.rs — per-peer sentTime tracking (optional but recommended)

1. **Track per-peer sent timestamps:**
   - Each peer keeps `last_sent_time: u16`
   - On datagram send to peer, include the current timestamp if enough time has elapsed
   - This helps the receiving end with RTT measurement

2. **Implementation note:** ENet uses a global millisecond clock (`Library.Time`). The sentTime is a 16-bit value (wraps every 65 seconds), meant to be echoed back in ACKs for latency measurement.

**Files to modify:**
- `catalyrst-enet/src/peer.rs` (add last_sent_time tracking)
- `catalyrst-enet/src/host.rs` (stamp sentTime when sending)

---

### Phase 4: catalyrst-enet protocol.rs — MTU clamping [576, 4096]

1. **In ConnectBody::decode():**
   - After decoding mtu field, clamp to `[576, 4096]`
   - Reject or warn if received MTU is outside this range

2. **In host.rs VERIFY_CONNECT sending:**
   - Clamp the server's advertised MTU to [576, 4096] before encoding

**Files to modify:**
- `catalyrst-enet/src/protocol.rs` lines 145-166 (ConnectBody::decode)
- `catalyrst-enet/src/host.rs` (verify_connect sending)

---

### Phase 5: catalyrst-enet — SendFragment & SendUnsequenced stubs (low priority)

1. **Add Command::SendFragment and Command::SendUnsequenced parsing:**
   - In `parse_command()`, add match branches that return `DecodedCommand::Other(cmd)`
   - This allows the parser to skip over these commands instead of failing

2. **No full reassembly logic needed yet** — just skip bytes correctly so multi-command datagrams parse through

**Files to modify:**
- `catalyrst-enet/src/protocol.rs` lines 237-301 (parse_command match statement)

---

### Phase 6: catalyrst-enet tests — wire round-trip validation

1. **Add test asserting roundtrip with peerID masking:**
   ```rust
   #[test]
   fn protocol_header_session_roundtrip() {
       let h = ProtocolHeader {
           peer_id: 0x123,         // 12-bit index
           session_id: 0x7,        // 3-bit session
           has_sent_time: true,
           sent_time: 0xABCD,
       };
       let encoded = h.encode();
       assert_eq!(encoded.len(), 4);  // 2 + 2 for sentTime
       let decoded = ProtocolHeader::decode(&encoded).unwrap();
       assert_eq!(decoded, h);
   }
   ```

2. **Test 2-byte header (no sentTime):**
   ```rust
   #[test]
   fn protocol_header_no_sent_time() {
       let h = ProtocolHeader {
           peer_id: 0x100,
           session_id: 0x0,
           has_sent_time: false,
           sent_time: 0,
       };
       let encoded = h.encode();
       assert_eq!(encoded.len(), 2);
       let decoded = ProtocolHeader::decode(&encoded).unwrap();
       assert_eq!(decoded.peer_id, 0x100);
       assert_eq!(decoded.session_id, 0x0);
       assert!(!decoded.has_sent_time);
   }
   ```

3. **Test connectID echo in VERIFY_CONNECT:**
   - Send CONNECT with connect_id=0xDEADBEEF
   - Verify server's VERIFY_CONNECT echoes the same 0xDEADBEEF
   - (requires integration with host.rs test flow)

**Files to modify:**
- `catalyrst-enet/src/protocol.rs` lines 304-391 (add test cases)

---

### Phase 7: catalyrst-pulse — wire frame wrapping

**Current catalyrst-pulse state:** 248-LOC skeleton, route() no-op, homegrown 5-tag codec
**Goal:** Replace with real protobuf framing over ENet channels 0 & 1

**Note:** This is Phase 2 of the overall work (after enet protocol is byte-exact). File references:
- `crates/catalyrst-pulse/src/lib.rs` (current stub)

Will require:
1. Generate Rust prost types from `decentraland/protocol` pulse_server.proto / pulse_client.proto
2. Implement BitReader/BitWriter for quantization codec (or port protoc-gen-bitwise)
3. Replace route() with real message dispatch
4. Implement HandshakeHandler with ECDSA auth-chain validation
5. Wire SnapshotBoard interest management
6. Implement STATE_FULL / STATE_DELTA fan-out

**Defer Phase 7 to a separate task** after enet protocol is solid.

---

## Summary of Wire Incompatibilities Fixed

| Issue | catalyrst Current | Upstream/Real ENet | Fix Location |
|-------|------|----------|---|
| peerID packing | Simple u16 (no session bits) | `peer_id & 0xFFF \| session << 12` | protocol.rs ProtocolHeader::encode/decode |
| sentTime field | Always 4 bytes | 2-4 bytes variable (SENT_TIME_FLAG) | protocol.rs ProtocolHeader struct + encode/decode |
| Header size | Fixed 4 bytes | 2 or 4 bytes | Same |
| session_id tracking | None | Per-peer u8 [0-15] | peer.rs + host.rs |
| connectID echo | Hardcoded 0 (FATAL) | Client's value echoed in VERIFY_CONNECT | host.rs handshake |
| MTU range | No validation | [576, 4096] clamp | protocol.rs + host.rs |
| CRC32 trailer | Not supported | Optional (currently unused in Pulse) | Deferred |
| SendFragment parse | Returns error | Skip to next command | protocol.rs parse_command() |
| SendUnsequenced parse | Returns error | Skip to next command | protocol.rs parse_command() |

---

## Testing Strategy

1. **Unit tests for ProtocolHeader:** 2-byte/4-byte roundtrip, session bit extraction
2. **Unit tests for ConnectBody:** connectID echo, MTU clamping
3. **Integration test:** Mock ENet connect handshake (CONNECT → VERIFY_CONNECT → ACK)
4. **Wire test:** Capture real ENet-CSharp (Pulse server) datagram, parse with new code
5. **Interop test:** Connect catalyrst-enet client to real Pulse server, verify handshake completes

---

## Gotchas & Risk Factors

1. **Byte order:** All ENet fields are big-endian (network byte order). Existing catalyrst tests use hardcoded values (0x1234, 0xABCD); ensure they roundtrip correctly after bit-packing changes.

2. **Session ID lifetime:** Session IDs are per-connection, not global. When a peer reconnects, it gets a new session ID. Must not assume stale session IDs are invalid — they're used to drop old duplicate packets in flight.

3. **connectID validation is security-critical:** Hardcoding to 0 means any packet claiming to be from a CONNECT→VERIFY_CONNECT pair will match. Fix this first before any interop testing.

4. **sentTime wrapping:** 16-bit milliseconds wrap every 65.536 seconds. Not a problem for short-lived sessions, but don't assume monotonicity across long periods.

5. **Fragmentation complexity:** If fragmentation is required, expect 10–20% code size growth and a reassembly buffer per peer. Consider FFI-binding the native ENet library instead (Pulse/ENet-CSharp already does this in C#).

6. **Quantization codec (Pulse concern):** The protobuf quantization (BitReader/BitWriter) is separate from the ENet wire format. Defer until Phase 7.

### Gotchas
1. **connectID hardcoded to 0 (SECURITY-CRITICAL):** This is why the handshake fails against real ENet clients. The client sends a random connectID in CONNECT; the server must echo it back in VERIFY_CONNECT. Any other value is rejected. Fixing this unblocks all downstream Pulse work.

2. **peerID field now carries session bits (SILENT DATA LOSS RISK):** The handshake extracts peerID using simple `u16::from_be_bytes()` then ignores the upper nibble. Old datagrams with session bits set will parse with the session bits included in the peer ID, causing lookup failures. Tests must verify that the masking `peer_id & 0xFFF` correctly extracts the 12-bit index.

3. **sentTime flag packing (WIRE FORMAT INCOMPATIBILITY):** If an old client sends 4-byte headers to a new server that expects a 2-byte header with optional sentTime flag, the sentTime will be misinterpreted as the next command's commandByte. This causes command parsing to fail silently. Mitigated by: (a) never mixing old/new code in production, (b) version gates if needed.

4. **MTU clamping [576, 4096] must happen before use:** If the remote sends MTU=5000 (out of range), catalyrst must clamp before fragmenting. Otherwise fragmentation payloads exceed the negotiated window_size, causing reassembly failures or buffer overflows.

5. **ConnectBody.connectID is NOT auto-generated:** The server must store the client's incoming connectID and echo it. The current code initializes it to 0 in host.rs:enet_verify_connect, which is the bug.

6. **Fragmentation NOT YET IMPLEMENTED:** SendFragment/SendUnreliableFragment commands are not parsed; parse_command() will return DecodedCommand::Other(SendFragment). Attempting to send large messages will fail silently. Add a comment warning that payloads must fit in one datagram (MTU - overhead).

7. **Unreliable channel sequence numbers (u16) wrap every ~65k packets:** On ch1 (unreliable sequenced), a gap in sequence numbers triggers a RESYNC_REQUEST. Wrapping is expected; the client must handle it. No special handling needed in enet.rs, but Pulse's SnapshotBoard.rs must not assume sequence numbers are monotonic forever.

8. **ENet's "unsequenced" flag (0x40) bypasses both reliable and sequenced semantics:** A packet sent with UNSEQUENCED is delivered at most once, in any order, and never retransmitted. Pulse does not use this for control; it's there for high-frequency data that tolerates loss. Ensure tests cover all three flag combinations (None, Reliable, Unsequenced).

9. **CRC32 checksum trailer (optional):** Some ENet stacks support optional checksumming; Pulse does not enable it. If a future Pulse version enables it, datagrams will have a 4-byte CRC32 appended after all commands. Parsing must not fail if a trailer is present; we can ignore it for now.

10. **Peer state machine races:** Between ENet detecting a timeout and catalyrst's graceful teardown, a stale peer might send a command. The header will have an old session_id; masking to the 12-bit index might collide with a new peer on the same slot. Test that session_id mismatch causes the datagram to be silently dropped (ENet should do this on the receiver side, but verify).

## Pulse bitwise-quantized protobuf codec

### Wire spec
## WIRE FORMAT SPECIFICATION

### Core Quantization Algorithm (Quantize.cs:21-32)
```
Encode(value: float, min: float, max: float, bits: int) → uint32:
  steps = (1 << bits) - 1
  t = clamp((value - min) / (max - min), 0.0, 1.0)
  return round(t * steps)

Decode(encoded: uint32, min: float, max: float, bits: int) → float:
  steps = (1 << bits) - 1
  return (encoded / steps) * (max - min) + min
```

### Message: PlayerStateDeltaTier0 (PulseServer.cs:607-1300, PulseServer.Bitwise.cs:10-142)
**Protobuf message type** carrying optional fields with field_mask presence semantics (not standard protobuf required/optional — the plugin emits a presence bitmask).

**Fields:**
1. `subject_id` (uint32, field#1): varint, no quantization
2. `baseline_seq` (uint32, field#2): varint, no quantization
3. `new_seq` (uint32, field#3): varint, no quantization
4. `server_tick` (uint32, field#4): varint, no quantization
5. `parcel_index` (int32, field#5): optional (presence bit 0 in _hasBits0, PulseServer.cs:738), fixed sint32 wire encoding
6. `position_x` (uint32, field#6): optional (presence bit 1), **quantized float [0.0, 16.0] in 8 bits** — step ≈0.0627451
7. `position_y` (uint32, field#7): optional (presence bit 2), **quantized float [0.0, 200.0] in 13 bits** — step ≈0.024417
8. `position_z` (uint32, field#8): optional (presence bit 3), **quantized float [0.0, 16.0] in 8 bits** — step ≈0.0627451
9. `velocity_x` (uint32, field#9): optional (presence bit 4), **quantized float [-50.0, 50.0] in 8 bits** — step ≈0.392157
10. `velocity_y` (uint32, field#10): optional (presence bit 5), **quantized float [-50.0, 50.0] in 8 bits** — step ≈0.392157
11. `velocity_z` (uint32, field#11): optional (presence bit 6), **quantized float [-50.0, 50.0] in 8 bits** — step ≈0.392157
12. `rotation_y` (uint32, field#12): optional (presence bit 7), **quantized float [0.0, 360.0] in 7 bits** — step ≈2.83465
13. `movement_blend` (uint32, field#13): optional (presence bit 8), **quantized float [0.0, 3.0] in 5 bits** — step ≈0.0967742
14. `slide_blend` (uint32, field#14): optional (presence bit 9), **quantized float [0.0, 1.0] in 4 bits** — step ≈0.0666667
15. `head_yaw` (uint32, field#15): optional (presence bit 10), **quantized float [0.0, 360.0] in 7 bits** — step ≈2.83465
16. `head_pitch` (uint32, field#16): optional (presence bit 11), **quantized float [0.0, 180.0] in 6 bits** — step ≈2.85714
17. `state_flags` (uint32, field#17): optional (presence bit 12), varint uint32
18. `glide_state` (GlideState enum, field#18): optional (presence bit 13), values: PROP_CLOSED=0, OPENING_PROP=1, GLIDING=2, CLOSING_PROP=3, varint
19. `jump_count` (int32, field#19): optional (presence bit 14), varint sint32
20. `point_at_x` (uint32, field#20): optional (presence bit 15), **quantized float [-3000.0, 3000.0] in 17 bits** — step ≈0.0457767
21. `point_at_y` (uint32, field#21): optional (presence bit 16), **quantized float [0.0, 200.0] in 7 bits** — step ≈1.5748
22. `point_at_z` (uint32, field#22): optional (presence bit 17), **quantized float [-3000.0, 3000.0] in 17 bits** — step ≈0.0457767

**Presence bitmask (_hasBits0):** 32-bit field (PulseServer.cs:614, 642, 740, 768, 798, 828, 856, 884, 912, 938, 964, 990, 1016, 1044, 1072, 1164, 1192, 1220). Each optional field bit tests via `(_hasBits0 & N) != 0`.

**Wire encoding:**
- Standard protobuf tag-value pairs written via `CodedOutputStream`
- quantized uint32 fields: tag (varint) + value (varint)
- optional fields omitted if not present
- presence tracked via bitmask on read

### Options/Attributes (Options.cs)
**QuantizedFloatOptions extension** (field#50001 on `FieldOptions`):
- `min` (float, field#1): lower bound of quantization range
- `max` (float, field#2): upper bound of quantization range
- `bits` (uint32, field#3): bit width for quantization (2–32 typical)

**BitPackedOptions extension** (field#50002 on `FieldOptions`):
- `bits` (uint32, field#1): bit width for fixed-width integer packing (not variable-bit varint, but the plugin may use this for future binary-compact layouts)

### Related Messages (QuantizationExample.Bitwise.cs:10-78)
Same quantization pattern for example types:
- **PositionDelta:** dx, dy, dz each [-100.0, 100.0] in 16 bits (step ≈0.0030518)
- **PlayerInput:** moveX, moveZ [-1.0, 1.0] in 8 bits; yaw [-180.0, 180.0] in 12 bits (step ≈0.0879121)
- **AvatarStateSnapshot:** x,z [-4096.0, 4096.0] in 16 bits; y [-256.0, 256.0] in 14 bits (step ≈0.0312519); pitch [-90.0, 90.0] in 10 bits (step ≈0.175953); yaw [-180.0, 180.0] in 12 bits

### Field Encoding Details (PulseServer.cs lines 30-56)
From protobuf descriptorData base64 decode (PulseServer.cs:25–88), the `.proto` source annotations:
```
Field 6 (position_x): tag 0x0A bit pattern 0x0A (field 1, wire type 2) but actual proto field tag = (6 << 3) | 0 = 48 (varint)
  with quantized option: [(decentraland.common.quantized) = { min: 0.0, max: 16.0, bits: 8 }]
  Actual C# line 36 in descriptorData shows: "EiQKCnBvc2l0aW9uX3gYBiABKA1CC4q1GAcVAACAQRgISAGIAQE"
  Decoded: position_x field number 6, type 5 (fixed32? no, it's varint 1=sint, so it's uint32 type 5 = fixed32 or packed)
  Decimal "Ii1GAcVAACAQRgI" → field tag = 48 (0x30), value type is uint32 (varint variant).
```

**Critically:** quantized fields are **still transmitted as standard protobuf varints**, not as fixed-bit fields. The quantization algorithm produces a uint32 (which may fit in fewer bits after quantization, but is encoded as a full varint on the wire per protobuf spec). The "bitwise" plugin generates the quantization/dequantization glue code, but the wire format is standard protobuf varint for integers.

### Field Tags (from PulseServer.cs WriteTo lines 246–1300)
- Field 1 (subject_id): raw tag 8 (field#1 << 3 | wire type 0=varint)
- Field 2 (baseline_seq): raw tag 16
- Field 3 (new_seq): raw tag 24
- Field 4 (server_tick): raw tag 32
- Field 5 (parcel_index): raw tag 40 (sint32 ZigZag encoding: (value << 1) ^ (value >> 31))
- Field 6–22: tags 48, 56, 64, 72, 80, 88, 96, 104, 112, 120, 128, 136, 144, 152, 160, 168, 176

### FieldOptions Presence Encoding
**Standard protobuf optional:** Each optional field in `.proto` maps to a presence bit in `_hasBits0`. On write, only serialize if `HasXxx` property returns true (e.g., `HasPositionX` checks bit 1). On read, set the presence bit when the tag is encountered (PulseServer.cs ~840). **No explicit presence field on the wire; presence is implicit in tag occurrence.**

### Accessor Pattern (PulseServer.Bitwise.cs)
For each quantized field, the generated partial class adds:
```csharp
public float {FieldName}Quantized
{
  get => {FieldName}_cache ??= Quantize.Decode({FieldName}, min, max, bits);
  set { {FieldName}_cache = value; {FieldName} = Quantize.Encode(value, min, max, bits); }
}
```
The raw uint32 field stores the encoded integer; the `Quantized` accessor handles encode/decode on-demand. Example (PulseServer.Bitwise.cs:13–17):
```
public float PositionXQuantized
{
  get => _positionX ??= Quantize.Decode(PositionX, 0.0f, 16.0f, 8);
  set { _positionX = value; PositionX = Quantize.Encode(value, 0.0f, 16.0f, 8); }
}
```

### Rust plan
## RUST IMPLEMENTATION PLAN FOR BYTE-EXACT COMPATIBILITY

### 1. Core Quantization Module (`quantize.rs`)
Implement exact float quantization matching Quantize.cs:21–32:
```
pub fn encode(value: f32, min: f32, max: f32, bits: u32) → u32:
  steps = (1u32 << bits) - 1
  t = ((value - min) / (max - min)).max(0.0).min(1.0)
  (t * (steps as f32)).round() as u32

pub fn decode(encoded: u32, min: f32, max: f32, bits: u32) → f32:
  steps = (1u32 << bits) - 1
  (encoded as f32 / steps as f32) * (max - min) + min
```
Critical: Use **exact same rounding mode** as C# `MathF.Round()` (banker's rounding / round-to-nearest-even). Verify with reference test vectors.

### 2. Protobuf Message Codec (`messages.rs` or `pulse.rs`)
Define `PlayerStateDeltaTier0` struct matching PulseServer.cs:607–1300:
- Fields 1–4: standard varint uint32 (subject_id, baseline_seq, new_seq, server_tick)
- Field 5: optional sint32 (parcel_index) — ZigZag encoding: `(x << 1) ^ (x >> 31)` on encode, `(x >> 1) ^ -(x & 1)` on decode
- Fields 6–22: optional uint32 with quantization metadata stored per field
- Presence tracking: **mimic protobuf3 optional semantics** — 18 optional fields = 18 presence bits (or use a bitfield, or a separate presence mask u32)

**Wire format encoder/decoder:**
```rust
impl PlayerStateDeltaTier0 {
  pub fn encode_to_bytes(&self) → Vec<u8>:
    Build varint/zigzag/quantized values field-by-field
    Write field tag + value using standard protobuf wire format
    Omit fields with presence bit unset
    
  pub fn decode_from_bytes(data: &[u8]) → Result<PlayerStateDeltaTier0>:
    Read tags and values in order
    Dequantize uint32 fields using quantize::decode
    Set presence bits as fields are encountered
}
```

**Quantized field metadata table:**
```rust
const QUANTIZED_FIELDS: &[FieldMetadata] = &[
  FieldMetadata { field_num: 6, bits: 8, min: 0.0, max: 16.0 },    // position_x
  FieldMetadata { field_num: 7, bits: 13, min: 0.0, max: 200.0 },  // position_y
  FieldMetadata { field_num: 8, bits: 8, min: 0.0, max: 16.0 },    // position_z
  FieldMetadata { field_num: 9, bits: 8, min: -50.0, max: 50.0 },  // velocity_x
  FieldMetadata { field_num: 10, bits: 8, min: -50.0, max: 50.0 }, // velocity_y
  FieldMetadata { field_num: 11, bits: 8, min: -50.0, max: 50.0 }, // velocity_z
  FieldMetadata { field_num: 12, bits: 7, min: 0.0, max: 360.0 },  // rotation_y
  FieldMetadata { field_num: 13, bits: 5, min: 0.0, max: 3.0 },    // movement_blend
  FieldMetadata { field_num: 14, bits: 4, min: 0.0, max: 1.0 },    // slide_blend
  FieldMetadata { field_num: 15, bits: 7, min: 0.0, max: 360.0 },  // head_yaw
  FieldMetadata { field_num: 16, bits: 6, min: 0.0, max: 180.0 },  // head_pitch
  FieldMetadata { field_num: 20, bits: 17, min: -3000.0, max: 3000.0 }, // point_at_x
  FieldMetadata { field_num: 21, bits: 7, min: 0.0, max: 200.0 },  // point_at_y
  FieldMetadata { field_num: 22, bits: 17, min: -3000.0, max: 3000.0 }, // point_at_z
];
```

### 3. Varint & ZigZag Codecs
Implement standard protobuf wire format:
```rust
pub fn encode_varint(value: u64) → Vec<u8> { /* standard protobuf */ }
pub fn decode_varint(data: &[u8]) → (u64, &[u8]) { /* read continuation bytes */ }

pub fn encode_zigzag(value: i32) → u32 { (value as u32) << 1 ^ ((value as u32) >> 31) }
pub fn decode_zigzag(value: u32) → i32 { ((value >> 1) as i32) ^ -((value & 1) as i32) }
```

### 4. Field Tag Handling
```rust
// Tag format: (field_number << 3) | wire_type
// wire_type: 0=varint, 1=fixed64, 2=length-delimited, 5=fixed32
const fn tag(field_num: u32, wire_type: u32) → u32 {
  (field_num << 3) | wire_type
}

// For PlayerStateDeltaTier0:
// Fields 1–4, 17, 19: varint (wire type 0)
// Field 5: sint32 → varint (wire type 0, but with ZigZag encode)
// Fields 6–16, 18, 20–22: uint32 → varint (wire type 0, post-quantize)
```

### 5. Presence Tracking
Option A: Bitfield (most compact, matches C# `_hasBits0`):
```rust
struct PlayerStateDeltaTier0 {
  // Required fields
  subject_id: u32,
  baseline_seq: u32,
  new_seq: u32,
  server_tick: u32,
  
  // Optional fields with presence bits
  parcel_index: Option<i32>,  // Or use i32 + separate bool, or use bit 0 of u32 presence mask
  position_x: Option<u32>,    // bit 1
  position_y: Option<u32>,    // bit 2
  position_z: Option<u32>,    // bit 3
  // ... etc
  
  // Presence mask (or use bitfield macro)
  presence: u32,  // bits 0–17 track which optionals are set
}
```

Option B: Use Rust `Option<T>` directly (simpler, slightly larger footprint):
```rust
struct PlayerStateDeltaTier0 {
  subject_id: u32,
  // ... required
  parcel_index: Option<i32>,
  position_x: Option<u32>,
  // ... optionals
}
```
Encode: only write tag+value if `is_some()`; Decode: only set if tag encountered.

### 6. Test Harness
Create round-trip tests **byte-for-byte** against C# reference:
```rust
#[test]
fn test_round_trip_player_state_delta_tier0() {
  // Encode a PlayerStateDeltaTier0 with known field values
  let msg = PlayerStateDeltaTier0 { 
    subject_id: 42, 
    position_x: Some(100),  // Encoded as quantized value
    // ... 
  };
  let bytes = msg.encode_to_bytes();
  
  // Decode it back
  let decoded = PlayerStateDeltaTier0::decode_from_bytes(&bytes)?;
  assert_eq!(msg, decoded);
  
  // Compare bytes against C# serialized form (reference vector)
  assert_eq!(bytes, EXPECTED_BYTES);
}
```

### 7. Integration with catalyrst-enet
- `ENetChannel` wrapper provides send/recv on unreliable sequenced channel (ch1 for STATE_DELTA)
- Pass `PlayerStateDeltaTier0::encode_to_bytes()` result to `enet_send_packet()`
- On receive, parse via `PlayerStateDeltaTier0::decode_from_bytes()`
- Decode float fields using `Quantize::decode()` accessor when application layer reads them

### 8. Compatibility Checklist
- [ ] Quantize encode/decode matches C# MathF.Round behavior
- [ ] Varint encoding produces identical byte sequences to protobuf spec
- [ ] ZigZag encoding for sint32 fields correct
- [ ] Optional field presence bits track correctly (set on encode, checked on decode)
- [ ] Quantized field metadata table complete and correct
- [ ] Tag encoding matches field numbers in PulseServer.proto
- [ ] Enums (GlideState) encoded as varint integers (0–3)
- [ ] Round-trip encode/decode produces identical bytes (byte-for-byte, not just semantic equivalence)
- [ ] Test vectors derived from actual C# serialized messages pass Rust decoder and re-serialize to same bytes

### Gotchas
### Critical Gotchas

1. **Quantization Rounding Mode:** C# uses `MathF.Round()` which defaults to **banker's rounding** (round-half-to-even). Rust's `round()` also uses this, but verify with edge cases like 0.5 steps. Test: `Quantize.Encode(0.5 * (max - min) + min, ...)` must produce `(2^bits - 1) / 2`.

2. **Float Precision:** Quantization ranges like [-3000, 3000] in 17 bits produce tiny step sizes (~0.046m). Accumulated rounding error across encode→decode→encode can diverge. Use test vectors from the C# code itself (PulseServer.Bitwise.cs lines 14–122) to validate exact byte sequences.

3. **Presence Bits in Protobuf3 Optional:** Standard protobuf3 **does not** track presence on the wire — a field with value 0 is indistinguishable from a missing field. But Decentraland's custom plugin appears to preserve presence via a field_mask or separate presence field. Verify by examining C# parser: look for `HasXxx` property check in `WriteTo()` (e.g., PulseServer.cs line 768 `if ((_hasBits0 & 2) != 0)`). On the wire, a `position_x` field tag will only appear if `HasPositionX` is true, **even if the value is 0 after quantization**. This is non-standard and must be replicated exactly.

4. **Enum Wire Format:** `GlideState` is encoded as a varint (0–3 for the four states), not as a fixed enum code. The enum values are: PROP_CLOSED=0, OPENING_PROP=1, GLIDING=2, CLOSING_PROP=3 (PulseShared.cs:67–72). Encode as simple varint tag (field 18 → tag 144) + varint value (0–3).

5. **ZigZag for Signed Ints:** Field 5 (`parcel_index`) and field 19 (`jump_count`) are `int32` (signed). Protobuf encodes signed as ZigZag only if the `.proto` explicitly declares `sint32`, not `int32`. Verify from PulseServer.cs: parcel_index is declared as `int parcelIndex_` (C#), which maps to `sint32` in the proto descriptor (see base64 line 31 in PulseServer.cs: "EhkKDHBhcmNlbF9pbmRleBgFIAEoBUgAiAEB" decodes to field 5, type 5 = sint32). Do apply ZigZag. For jump_count (field 19), check similarly — if `sint32`, apply ZigZag; if just `int32`, transmit as signed varint (which extends the sign bit to 64 bits in protobuf, a potential issue for negative values). **Verify against actual C# serialized bytes.**

6. **Field Absence vs. Zero:** A quantized field with encoded value 0 is **still present** if its presence bit is set. On decode, do not skip a field just because the value is 0 — check the tag presence, set the presence bit, and populate the field.

7. **Byte Order:** All protobuf integers are little-endian in varint encoding (LSB first). No alignment padding. Varints are variable-length, not fixed.

8. **Comparison Test Vectors:** The C# Quantize.cs (lines 21–32) is the reference. Create test vectors in Rust by calling C# code (via FFI or by hand-computing reference outputs), then verify Rust produces identical bytes.

9. **Missing Field Initialization:** In catalyrst-pulse, when a `PlayerStateDeltaTier0` is parsed from wire, unset optional fields must remain `None` (or `Option::None`). Do **not** fill them with default values — this changes the wire encoding on re-serialization.

10. **Floating-Point Literal Precision:** In Quantize tests, use exactly the same float literals as PulseServer.Bitwise.cs (e.g., `0.0f`, `16.0f`, not `16` or `16.0d`). Rust will infer these as `f32` in context, but be explicit: `16.0_f32`.

11. **Protobuf Tag/Wire-Type Calculation:** Field tag = `(field_number << 3) | wire_type`. For varint fields, `wire_type = 0`. For fixed32, `wire_type = 5`. For length-delimited (e.g., strings), `wire_type = 2`. All quantized uint32 fields are varint, so tags are: field 6 → (6 << 3) | 0 = 48, field 7 → 56, etc. Verify this against actual C# output (WriteTo method raw tags).

12. **Array Allocation on Encode:** Varint encoding produces variable-length bytes (1–5 for a u32). When encoding a message, estimate the max size (each uint32 can be up to 5 bytes + tag 1 byte), then use a Vec or fixed buffer. Do not allocate a 256-byte buffer for every field; use incremental push or a BufWriter.

13. **Reference Bytes Test:** Capture actual bytes from C# `PlayerStateDeltaTier0.ToByteArray()` (via protobuf's internal serialization) for a known instance, store in Rust as a hex string or raw bytes, and assert that Rust's encoder produces the same bytes. This is the gold standard for "byte-exact" compatibility.

## Pulse Protocol (catalyrst-pulse) - Exact Wire Specification

### Wire spec

# PULSE WIRE PROTOCOL SPECIFICATION
## Overview
Pulse uses standard protobuf3 wire format with custom quantization extensions for spatial data.
The protocol has two top-level message envelopes:
- ClientMessage: oneof with field tags 1-7 (C->S)
- ServerMessage: oneof with field tags 1-9 (S->C)

## ENUMERATIONS
### PlayerAnimationFlags (uint32 bitmask in state_flags)
- NONE = 0
- GROUNDED = 1
- LONG_JUMP = 2
- LONG_FALL = 4
- FALLING = 8
- STUNNED = 16
- HEAD_YAW = 32
- HEAD_PITCH = 64
- POINTING_AT = 128

### GlideState (varint enum)
- PROP_CLOSED = 0
- OPENING_PROP = 1
- GLIDING = 2
- CLOSING_PROP = 3

### EmoteStopReason (varint enum)
- COMPLETED = 0
- CANCELLED = 1

## CLIENT MESSAGE ENVELOPE (oneof tag field number = -1, actual message type determined by oneof case)
Tag: (field_number << 3) | wire_type
- 0x0a (tag=1, wire=2): HandshakeRequest
- 0x12 (tag=2, wire=2): PlayerStateInput
- 0x1a (tag=3, wire=2): ResyncRequest
- 0x22 (tag=4, wire=2): ProfileVersionAnnouncement
- 0x2a (tag=5, wire=2): EmoteStart
- 0x32 (tag=6, wire=2): EmoteStop
- 0x3a (tag=7, wire=2): TeleportRequest

### HandshakeRequest (tag=1)
- Field 1: auth_chain (bytes, tag=0x0a): protobuf length-delimited
- Field 2: profile_version (int32, tag=0x10): protobuf sint32 (zigzag-encoded varint)
- Field 3: initial_state (PlayerInitialState, optional, tag=0x1a): protobuf message
  Location: PulseClient.cs:123-355

### PlayerInitialState (nested message)
- Field 1: state (PlayerState, tag=0x0a): nested message
- Field 2: emote_id (string, optional, tag=0x12): UTF-8, length-delimited
- Field 3: emote_duration_ms (uint32, optional, tag=0x18): varint
- Field 4: emote_start_offset_ms (uint32, optional, tag=0x20): varint
- Field 5: realm (string, tag=0x2a): UTF-8, length-delimited (non-optional; "" is wire-empty)
  Location: PulseClient.cs:357-725

### ProfileVersionAnnouncement (tag=4)
- Field 1: version (int32, tag=0x08): sint32 varint
  Location: PulseClient.cs:727-927

### PlayerStateInput (tag=2)
- Field 1: state (PlayerState, tag=0x0a): nested message
  Location: PulseClient.cs:929-1137

### ResyncRequest (tag=3)
- Field 1: subject_id (uint32, tag=0x08): varint
- Field 2: known_seq (uint32, tag=0x10): varint
  Location: PulseClient.cs:1139-1272

### EmoteStart (tag=5)
- Field 1: emote_id (string, tag=0x0a): UTF-8, length-delimited
- Field 2: duration_ms (uint32, optional, tag=0x10): varint
- Field 3: player_state (PlayerState, tag=0x1a): nested message
- Field 4: mask (int32, optional, tag=0x20): sint32 varint
  Location: PulseClient.cs:1274-1565 (estimated)

### EmoteStop (tag=6)
- (empty message, tag=0x32 is only the envelope tag)
  Location: PulseClient.cs (estimated around 1567)

### TeleportRequest (tag=7)
- Field 1: parcel_index (int32, tag=0x08): sint32 varint
- Field 2: position (Vector3, tag=0x12): nested message with 3x float32 fixed
- Field 3: realm (string, tag=0x1a): UTF-8, length-delimited
  Location: PulseClient.cs (estimated around 1600)

## PLAYERSTATE (FULL, UNQUANTIZED)
Used in PlayerStateInput, PlayerInitialState, PlayerStateFull, EmoteStarted, EmoteStopped, TeleportPerformed
- Field 1: parcel_index (int32, tag=0x08): sint32 varint
- Field 2: position (Vector3, tag=0x12): message { float x=1(0x0d=fixed32), y=2(0x15=fixed32), z=3(0x1d=fixed32) }
- Field 3: velocity (Vector3, tag=0x1a): message { float x=1, y=2, z=3 }
- Field 4: rotation_y (float, tag=0x25): fixed32 (IEEE 754)
- Field 5: movement_blend (float, tag=0x2d): fixed32
- Field 6: slide_blend (float, tag=0x35): fixed32
- Field 7: head_yaw (float, optional, tag=0x3d): fixed32
- Field 8: head_pitch (float, optional, tag=0x45): fixed32
- Field 9: state_flags (uint32, tag=0x48): varint
- Field 10: glide_state (GlideState, tag=0x50): varint enum
- Field 11: jump_count (int32, tag=0x58): sint32 varint
- Field 12: point_at (Vector3, optional, tag=0x62): message
  Location: PulseShared.cs:77-743

## PLAYERSTATE DELTA TIER0 (QUANTIZED STATE DELTA)
Compressed version of player state for frequent network transmission
All optional fields are guarded by presence check (_hasBits0 bit mask)

REQUIRED FIELDS (always present):
- Field 1: subject_id (uint32, tag=0x08): varint
- Field 2: baseline_seq (uint32, tag=0x10): varint - prior seq for loss detection
- Field 3: new_seq (uint32, tag=0x18): varint - current seq
- Field 4: server_tick (uint32, tag=0x20): varint - server frame counter

QUANTIZED OPTIONAL FIELDS:
- Field 5: parcel_index (int32, optional, tag=0x28): sint32 varint [omitted if unchanged]
- Field 6: position_x (uint32, optional, tag=0x30): varint, quantized
  QUANTIZATION: min=0, max=16, bits=8 → maps [0.0, 16.0) to [0, 256)
  Formula: quantized = (float_val - 0) * 256 / 16 = float_val * 16
  Inverse: float = quantized / 16
- Field 7: position_y (uint32, optional, tag=0x38): varint, quantized
  QUANTIZATION: min=0, max=200, bits=13 → maps [0.0, 200.0) to [0, 8192)
  Formula: quantized = (float_val - 0) * 8192 / 200 = float_val * 40.96
  Inverse: float = quantized / 40.96
- Field 8: position_z (uint32, optional, tag=0x40): varint, quantized
  QUANTIZATION: min=0, max=16, bits=8 → maps [0.0, 16.0) to [0, 256)
  Formula: quantized = float_val * 16
  Inverse: float = quantized / 16
- Field 9: velocity_x (uint32, optional, tag=0x48): varint, quantized
  QUANTIZATION: min=-50, max=50, bits=8 → maps [-50.0, 50.0) to [0, 256)
  Formula: quantized = (float_val - (-50)) * 256 / 100 = (float_val + 50) * 2.56
  Inverse: float = quantized / 2.56 - 50
- Field 10: velocity_y (uint32, optional, tag=0x50): varint, quantized
  QUANTIZATION: min=-50, max=50, bits=8
  Formula: quantized = (float_val + 50) * 2.56
  Inverse: float = quantized / 2.56 - 50
- Field 11: velocity_z (uint32, optional, tag=0x58): varint, quantized
  QUANTIZATION: min=-50, max=50, bits=8
  Formula: quantized = (float_val + 50) * 2.56
  Inverse: float = quantized / 2.56 - 50
- Field 12: rotation_y (uint32, optional, tag=0x60): varint, quantized
  QUANTIZATION: min=0, max=360.0, bits=7 → maps [0, 360) to [0, 128)
  Formula: quantized = (float_val - 0) * 128 / 360 = float_val / 2.8125
  Inverse: float = quantized * 2.8125
- Field 13: movement_blend (uint32, optional, tag=0x68): varint, quantized
  QUANTIZATION: min=0, max=3, bits=5 → maps [0, 3) to [0, 32)
  Formula: quantized = float_val / 3 * 32 = float_val * 10.667
  Inverse: float = quantized * 3 / 32 = quantized * 0.09375
- Field 14: slide_blend (uint32, optional, tag=0x70): varint, quantized
  QUANTIZATION: min=0, max=1, bits=4 → maps [0, 1) to [0, 16)
  Formula: quantized = float_val * 16
  Inverse: float = quantized / 16
- Field 15: head_yaw (uint32, optional, tag=0x78): varint, quantized
  QUANTIZATION: min=0, max=360.0, bits=7
  Formula: quantized = float_val / 2.8125
  Inverse: float = quantized * 2.8125
- Field 16: head_pitch (uint32, optional, tag=0x80): varint, quantized
  QUANTIZATION: min=0, max=360.0, bits=7
  Formula: quantized = float_val / 2.8125
  Inverse: float = quantized * 2.8125
- Field 17: state_flags (uint32, optional, tag=0x88): varint
- Field 18: glide_state (GlideState, optional, tag=0x90): varint enum
- Field 19: jump_count (int32, optional, tag=0x98): sint32 varint
- Field 20: point_at_x (uint32, optional, tag=0xa0): varint, quantized
  QUANTIZATION: min=-3000.0, max=3000.0, bits=17 → maps [-3000, 3000) to [0, 131072)
  Formula: quantized = (float_val - (-3000)) * 131072 / 6000 = (float_val + 3000) * 21.845333
  Inverse: float = quantized / 21.845333 - 3000
- Field 21: point_at_y (uint32, optional, tag=0xa8): varint, quantized
  QUANTIZATION: min=0.0, max=200.0, bits=7 → maps [0, 200) to [0, 128)
  Formula: quantized = float_val * 128 / 200 = float_val * 0.64
  Inverse: float = quantized / 0.64 = quantized * 1.5625
- Field 22: point_at_z (uint32, optional, tag=0xb0): varint, quantized
  QUANTIZATION: min=-3000.0, max=3000.0, bits=17
  Formula: quantized = (float_val + 3000) * 21.845333
  Inverse: float = quantized / 21.845333 - 3000
  Location: PulseServer.cs:606-1705

## SERVER MESSAGE ENVELOPE (oneof)
- 0x0a (tag=1, wire=2): HandshakeResponse
- 0x12 (tag=2, wire=2): PlayerStateFull
- 0x1a (tag=3, wire=2): PlayerStateDeltaTier0
- 0x22 (tag=4, wire=2): PlayerJoined
- 0x2a (tag=5, wire=2): PlayerLeft
- 0x32 (tag=6, wire=2): PlayerProfileVersionsAnnounced
- 0x3a (tag=7, wire=2): EmoteStarted
- 0x42 (tag=8, wire=2): EmoteStopped
- 0x4a (tag=9, wire=2): TeleportPerformed

### HandshakeResponse (tag=1)
- Field 1: success (bool, tag=0x08): wire type 0 (varint), value 0 or 1
- Field 2: error (string, optional, tag=0x12): UTF-8, length-delimited
  Location: PulseServer.cs:122-369

### PlayerStateFull (tag=2)
- Field 1: subject_id (uint32, tag=0x08): varint
- Field 2: sequence (uint32, tag=0x10): varint
- Field 3: server_tick (uint32, tag=0x18): varint
- Field 4: state (PlayerState, tag=0x22): nested full state message
  Location: PulseServer.cs:1934-2102

### PlayerJoined (tag=4)
- Field 1: user_id (string, tag=0x0a): UTF-8, length-delimited
- Field 2: profile_version (int32, tag=0x10): sint32 varint
- Field 3: state (PlayerStateFull, tag=0x1a): nested full state
  Location: PulseServer.cs:2254-2480

### PlayerLeft (tag=5)
- Field 1: subject_id (uint32, tag=0x08): varint
  Location: PulseServer.cs:2536-2620 (estimated)

### PlayerProfileVersionsAnnounced (tag=6)
- Field 1: subject_id (uint32, tag=0x08): varint
- Field 2: version (int32, tag=0x10): sint32 varint
  Location: PulseServer.cs:371-604

### EmoteStarted (tag=7)
- Field 1: subject_id (uint32, tag=0x08): varint
- Field 2: sequence (uint32, tag=0x10): varint
- Field 3: server_tick (uint32, tag=0x18): varint
- Field 4: emote_id (string, tag=0x22): UTF-8, length-delimited
- Field 5: player_state (PlayerState, tag=0x2a): nested full state
  Location: PulseServer.cs (estimated tag area 3000+)

### EmoteStopped (tag=8)
- Field 1: subject_id (uint32, tag=0x08): varint
- Field 2: server_tick (uint32, tag=0x10): varint
- Field 3: reason (EmoteStopReason, tag=0x18): varint enum
- Field 4: sequence (uint32, tag=0x20): varint
- Field 5: player_state (PlayerState, tag=0x2a): nested full state
  Location: PulseServer.cs (estimated tag area 3200+)

### TeleportPerformed (tag=9)
- Field 1: subject_id (uint32, tag=0x08): varint
- Field 2: sequence (uint32, tag=0x10): varint
- Field 3: server_tick (uint32, tag=0x18): varint
- Field 4: state (PlayerState, tag=0x22): nested full state
  Location: PulseServer.cs (estimated tag area 3400+)

## PROTOBUF WIRE FORMAT DETAILS
- Varint (wire type 0): LEB128 encoding (little-endian variable-length)
- Fixed32 (wire type 5): 4 bytes, little-endian IEEE 754 float
- Length-delimited (wire type 2): varint length + bytes
- Message (wire type 2): embedded message as length-delimited
- Enum (wire type 0): same as varint

## ONEOF SEMANTICS
Both ClientMessage and ServerMessage use protobuf3 oneof, meaning:
- Only one field variant can be present per message
- Parsing: discriminate by reading the field tag from wire
- Encoding: write field tag, length (if applicable), and value
- No explicit type marker; type is implicit in the field number

## FILE REFERENCES
All specifications derive from the auto-generated C# code:
- decentraland/Pulse/src/Protocol/Generated/PulseClient.cs
- decentraland/Pulse/src/Protocol/Generated/PulseServer.cs
- decentraland/Pulse/src/Protocol/Generated/PulseShared.cs


### Rust plan

# RUST IMPLEMENTATION PLAN FOR BYTE-EXACT PULSE CODEC

## ARCHITECTURE
Split into two crates: `catalyrst-pulse-codec` (serialization) and `catalyrst-pulse` (high-level API)

### 1. QUANTIZATION MODULE (catalyrst-pulse-codec/src/quantization.rs)
- Implement const quantization specs as struct definitions with bit widths and ranges
- Define trait for quantize/dequantize pairs for each field
- Use lookup tables or bit manipulation for max performance

Struct: `QuantizationSpec {
  min: f32,
  max: f32,
  bits: u32,
  _scale: f32,  // precomputed: (1u32 << bits) / (max - min)
  _offset: f32, // precomputed: min
}`

For each delta field:
- position_x: spec!(0.0, 16.0, 8)
- position_y: spec!(0.0, 200.0, 13)
- position_z: spec!(0.0, 16.0, 8)
- velocity_*: spec!(-50.0, 50.0, 8)
- rotation_y/head_yaw/head_pitch: spec!(0.0, 360.0, 7)
- movement_blend: spec!(0.0, 3.0, 5)
- slide_blend: spec!(0.0, 1.0, 4)
- point_at_*: spec!(-3000.0, 3000.0, 17) [x,z] or spec!(0.0, 200.0, 7) [y]

### 2. MESSAGE STRUCTURES (catalyrst-pulse-codec/src/messages.rs)
Define Rust structs mirroring protobuf exactly:

```rust
pub enum ClientMessage {
  Handshake(HandshakeRequest),
  Input(PlayerStateInput),
  Resync(ResyncRequest),
  ProfileAnnouncement(ProfileVersionAnnouncement),
  EmoteStart(EmoteStart),
  EmoteStop(EmoteStop),
  Teleport(TeleportRequest),
}

pub enum ServerMessage {
  Handshake(HandshakeResponse),
  PlayerStateFull(PlayerStateFull),
  PlayerStateDelta(PlayerStateDeltaTier0),
  PlayerJoined(PlayerJoined),
  PlayerLeft(PlayerLeft),
  ProfileVersionAnnounced(PlayerProfileVersionsAnnounced),
  EmoteStarted(EmoteStarted),
  EmoteStopped(EmoteStopped),
  Teleported(TeleportPerformed),
}

pub struct PlayerStateDeltaTier0 {
  pub subject_id: u32,
  pub baseline_seq: u32,
  pub new_seq: u32,
  pub server_tick: u32,
  
  // Optional fields (track presence)
  pub parcel_index: Option<i32>,
  pub position_x: Option<u32>,  // quantized
  pub position_y: Option<u32>,
  pub position_z: Option<u32>,
  pub velocity_x: Option<u32>,
  pub velocity_y: Option<u32>,
  pub velocity_z: Option<u32>,
  pub rotation_y: Option<u32>,
  pub movement_blend: Option<u32>,
  pub slide_blend: Option<u32>,
  pub head_yaw: Option<u32>,
  pub head_pitch: Option<u32>,
  pub state_flags: Option<u32>,
  pub glide_state: Option<GlideState>,
  pub jump_count: Option<i32>,
  pub point_at_x: Option<u32>,
  pub point_at_y: Option<u32>,
  pub point_at_z: Option<u32>,
}
```

### 3. CODEC IMPLEMENTATION (catalyrst-pulse-codec/src/codec.rs)
Implement trait `PulseCodec` with byte-level encoding/decoding:

```rust
pub trait PulseCodec {
  fn encode_to_vec(&self) -> Vec<u8>;
  fn decode_from_slice(data: &[u8]) -> Result<Self, DecodeError> where Self: Sized;
}
```

For each message type, hand-write encoding that:
- Writes field tags (field_number << 3 | wire_type) as varint
- Handles optional fields (presence check before writing)
- Writes quantized uint32s as varints (not fixed bytes)
- Encodes nested messages recursively
- Uses correct wire types: 0 (varint), 2 (length-delimited), 5 (fixed32)

Key functions:
- `encode_varint(u64) -> Vec<u8>` - LEB128 encode
- `decode_varint(&mut &[u8]) -> Result<u64>` - LEB128 decode
- `encode_tag(field: u32, wire_type: u32) -> Vec<u8>`
- `encode_string(s: &str) -> Vec<u8>`
- `encode_message<T: PulseCodec>(t: &T) -> Vec<u8>`
- `encode_float32(f: f32) -> [u8; 4]`

### 4. COMPLIANCE TESTS (catalyrst-pulse-codec/tests/)
For each message type, use golden byte strings from C# implementation:
- Create a C# serialization harness that outputs hex
- Roundtrip test: Rust encode -> C# decode, C# encode -> Rust decode
- Byte-exact comparison against reference C# output
- Quantization precision tests (ensure no precision loss beyond quantization spec)

### 5. HANDSHAKE/ENVELOPE FRAMING (catalyrst-pulse/src/framing.rs)
Implement ENet framing if required:
- See audit #45 (catalyrst-enet)
- Wrap ClientMessage/ServerMessage in ENet datagram header
- Reliability flags, sequence numbers, channel ID

### 6. HIGH-LEVEL API (catalyrst-pulse/src/lib.rs)
Expose:
- `PulseClient` struct with send_* methods for each message type
- `PulseServer` struct with message listeners
- Helper to convert full `PlayerState` <-> `PlayerStateDeltaTier0`
- Dequantization helpers to convert quantized u32 back to f32

## CRITICAL IMPLEMENTATION DETAILS

### Delta Encoding
When building `PlayerStateDeltaTier0`:
1. Compare current state to prior state
2. Only include fields that changed
3. Quantize changed floats to u32 using quantization specs
4. Write field tags only for present optional fields
5. Set presence bits correctly so encoder knows what to write

### Varint Encoding
Must match protobuf LEB128 exactly:
- Values 0-127 → single byte
- Values 128-16383 → 2 bytes, continuation bit set on first
- Values >= 16384 → 3+ bytes
- Signed integers use zigzag: (n << 1) ^ (n >> 31) for sint32/sint64

### Float Handling
- PlayerState uses fixed32 (4 bytes, IEEE 754)
- Encode as little-endian: `f32::to_le_bytes()`
- PlayerStateDeltaTier0 uses quantized u32 (variable-length varint)

### Tag Encoding
For oneof discriminators:
- Tag value = (field_number << 3) | wire_type
- ClientMessage tag=1 uses 0x0a (field 1, wire_type 2)
- ServerMessage tag=1 uses 0x0a (field 1, wire_type 2)
- Tag itself is encoded as varint

## TESTING STRATEGY
1. Unit tests: Each quantization spec roundtrips within spec tolerance
2. Integration tests: Encode sample messages, compare hex against C# reference
3. Fuzz tests: Random message values, ensure no panics
4. Compatibility tests: Cross-compile with catalyrst-enet, verify handshake flow
5. Regression tests: Capture wire dumps from live Pulse server, verify decode

## DEPENDENCIES
- `prost` (optional, if we want to also support prost-generated code)
- `bytes` (for efficient byte handling)
- No external codec libraries: hand-write for byte-exact control

## DELIVERABLES
- catalyrst-pulse-codec crate with PulseCodec trait implementations
- catalyrst-pulse crate with high-level API and framing
- 100% wire-compatible with Decentraland Pulse C# implementation
- All tests passing against golden byte sequences from upstream


### Gotchas

1. QUANTIZATION PRECISION: The `bits` field in quantization specs defines the binary precision, not decimal. E.g., position_y uses 13 bits = 8192 levels over [0, 200), so each step is 200/8192 ≈ 0.0244m. Must NOT round to nearest decimal place; use binary quantization formula exactly.

2. SIGNED INTEGERS IN DELTA: Fields like `parcel_index`, `head_pitch`, `jump_count`, `head_yaw` in PlayerStateDeltaTier0 use sint32 encoding (zigzag), NOT raw signed ints. The C# code checks `input.ReadInt32()` which uses varint-decoded signed int (zigzag on wire). Tag for sint32 is still wire_type 0.

3. OPTIONAL FIELD ENCODING: In PlayerStateDeltaTier0, a field is only written to wire if it has changed. The C# code tracks this via `_hasBits0` mask (bit flags 1-18). When encoding, check presence before writing tag. When decoding, track which fields were seen to reconstruct _hasBits0. Missing optional fields default to 0/empty.

4. QUANTIZED FIELDS ARE STILL VARINT: Even though position_x/y/z etc. are quantized to smaller bit widths (8-17 bits), they are encoded as uint32 varint on the wire. The bit width constraint is semantic (for dequantization precision), not a wire format constraint. A quantized value that happens to be 300 gets encoded as normal varint, not truncated to 8 bits.

5. ONEOF DISCRIMINATOR IS IMPLICIT: There is no explicit type/variant tag in the oneof. The discriminator is the field tag itself. When decoding a ClientMessage, read the first field tag; its field_number (1-7) tells you which variant. When encoding, write the message with its field tag (e.g., tag 1 for Handshake).

6. NESTED MESSAGE SEMANTICS: PlayerState appears in multiple contexts:
   - Full state in PlayerStateInput, PlayerStateFull, EmoteStarted, etc.: UNQUANTIZED floats
   - Implied in PlayerStateDeltaTier0: fields are QUANTIZED uint32s, NOT nested PlayerState message
   These are NOT the same wire format. Delta is a separate flattened message, not PlayerState wrapped.

7. STRING ENCODING: All strings (auth_chain as bytes, user_id, emote_id, realm, error) use length-delimited encoding: varint length + UTF-8 bytes. auth_chain is binary data (bytes), others are UTF-8 strings.

8. VECTOR3 ENCODING: Vector3 (position, velocity, point_at) is a nested message with three float32 fields. Each float32 is fixed wire_type 5 (4 bytes little-endian). The message itself is wire_type 2 (length-delimited), so: tag 0x0a (field 2 in parent) + varint length (typically 12) + 12 bytes of field tags + float values.

9. ENUM VALUES: GlideState and EmoteStopReason are varint-encoded enums. Default values (0) are typically omitted from encoding (proto3 default value suppression). When parsing, missing enum field implies value 0.

10. FLOATING POINT CLAMPING: Quantization formulas assume input is within [min, max]. The C# protoc extensions likely clamp out-of-range floats. When implementing dequantization in Rust, clamp to ensure no NaN/Inf from bad quantized values. When quantizing, clamp client-side input before quantizing to catch bugs early.

11. BASELINE_SEQ SEMANTICS: baseline_seq is the last acknowledged seq before this delta. new_seq is the current seq. The gap (new_seq - baseline_seq - 1) indicates how many intermediate inputs were not included in this delta. This is NOT an optional field; it's always present and used for packet loss detection. GOTCHA: Don't confuse it with state_flags (presence field).

12. PROFESSIONALIZATION FOR PRODUCTION: The prior catalyrst implementation (audits #28/#29/#30) was rejected because it was an approximation. This new impl must be BYTE-EXACT. Use property-based testing, wire protocol fuzz testing, and always roundtrip against C# reference. Set up continuous integration to re-validate against upstream Pulse server golden payloads monthly.


## Pulse Auth Handshake + Server Session Semantics (ENet/UDP + Protobuf)

### Wire spec

=== AUTHENTICATION HANDSHAKE FLOW ===

1. CLIENT SIDE: Pre-Auth Admission
   - ENet Connect event arrives at server (ENetHostedService.cs:204)
   - Server checks PreAuthAdmission quota (PreAuthAdmission.cs:42-68)
   - Two caps: global PreAuthBudget + per-IP MaxConcurrentPreAuthPerIP
   - PeerIndex allocated from pool (ENetHostedService.cs:206)
   - Peer moves to PENDING_AUTH state (PeerConnectionState.cs:11)

2. HANDSHAKE MESSAGE STRUCTURE (PulseClient.cs line 2407-2434)
   ClientMessage.handshake (field #1, tag 10, wire bytes 0x0A XX...)
   └─ HandshakeRequest (PulseClient.cs:77-200)
      ├─ auth_chain (field 1): bytes (Signed Fetch JSON string as UTF-8 bytes)
      │  Format: "{\"type\":\"SIGNER\",\"payload\":\"0xADDR\",\"signature\":\"\"}
      │           {\"type\":\"ECDSA_EPHEMERAL\",\"payload\":\"PURPOSE\\nEphemeral address: 0xADDR\\nExpiration: ISO8601\",\"signature\":\"...HEX...\"}
      │           {\"type\":\"...\",\"payload\":\"connect:/:TIMESTAMP:METADATA\",\"signature\":\"...\"}"
      ├─ profile_version (field 2): int32 (0 = no profile version declared)
      └─ initial_state (field 3, optional): PlayerInitialState (PulseClient.cs:237-370)
         ├─ state: PlayerState (PulseShared.cs:78-300, nested full player state)
         │  ├─ parcel_index: int32 (1D parcel ID in realm coordinate system)
         │  ├─ position: Vector3 (global position, meters)
         │  ├─ velocity: Vector3 (m/s)
         │  ├─ rotation_y: float (yaw, radians)
         │  ├─ movement_blend: float [0.0, 3.0] (animation blend)
         │  ├─ slide_blend: float [0.0, 1.0]
         │  ├─ head_yaw: float [0.0, 360.0] (optional, has-bit tracked)
         │  ├─ head_pitch: float [0.0, 180.0] (optional)
         │  ├─ state_flags: uint32 (PlayerAnimationFlags bitmask, PulseShared.cs:55-65)
         │  │  Flags: NONE(0), GROUNDED(1), LONG_JUMP(2), LONG_FALL(4), FALLING(8), STUNNED(16), HEAD_YAW(32), HEAD_PITCH(64), POINTING_AT(128)
         │  ├─ glide_state: enum GlideState [0-3] (PROP_CLOSED, OPENING_PROP, GLIDING, CLOSING_PROP)
         │  ├─ jump_count: int32
         │  └─ point_at: Vector3 (optional, ray-cast target)
         ├─ emote_id: string (optional, has-bit tracked)
         ├─ emote_duration_ms: uint32 (optional, has-bit tracked)
         └─ emote_start_offset_ms: uint32 (optional, has-bit tracked)

3. AUTH CHAIN VALIDATION (AuthChainValidator.cs:22-84)
   a) Parse x-identity-timestamp header (unix milliseconds)
   b) Verify freshness: |NOW - timestamp| < 60000ms (MAX_TIMESTAMP_SKEW_MS)
   c) Parse auth_chain JSON into AuthLink[] (type, payload, signature)
   d) First link must be SIGNER with empty signature + 0x address
   e) Intermediate ECDSA_EPHEMERAL links (0..n) with signed payload format:
      PURPOSE\nEphemeral address: 0xADDR\nExpiration: ISO8601-UTC
   f) Final link signed by last ephemeral (or wallet if no ephemeral chain)
   g) Final payload MUST equal BuildSignedFetchPayload("connect", "/", timestamp, metadata)
      = "connect:/:UNIX_MS:METADATA" (all lowercase, SignedFetch.cs:8)
   h) Return AuthChainValidationResult with UserAddress (normalized lowercase)

4. HANDSHAKE ACCEPTANCE (HandshakeHandler.cs:38-223)
   a) Check not already AUTHENTICATED (single-shot per slot)
   b) Check HandshakeAttemptPolicy (per-peer throttle)
   c) Parse JSON headers, extract x-identity-timestamp + x-identity-metadata
   d) Validate timestamp freshness (step 3b)
   e) Validate auth chain crypto + expiration
   f) Check platform BanList (IsBanned)
   g) Check ReplayPolicy (wallet, timestamp) pair not in cache
   h) Validate InitialState fields (FieldValidator.ValidateHandshakeInitialState)
   i) Create PeerState with wallet, set AUTHENTICATED
   j) Call PreAuthAdmission.ReleaseOnPromotion(peerIndex) → frees both global + per-IP quotas
   k) Check IdentityBoard for duplicate wallet → disconnect older peer with DUPLICATE_SESSION
   l) Call identityBoard.Set(peerIndex, walletId) → atomic Volatile.Write (IdentityBoard.cs:18-22)
   m) Call snapshotBoard.SetActive(peerIndex)
   n) Seed initial snapshot if InitialState provided (PeerSnapshotPublisher.PublishFromPlayerState)
   o) Send HandshakeResponse { success: true }

5. ON HANDSHAKE FAILURE
   Send HandshakeResponse { success: false, error: "message" }
   Reasons: invalid JSON, malformed chain, expired ephemeral, invalid signature, banned,
            stale/future timestamp, replay detected, invalid field
   State: PENDING_AUTH (not transitioned) OR PENDING_DISCONNECT if already AUTHENTICATED

=== SERVER MESSAGE TYPES (ServerMessage, field oneof) ===
(PulseServer.cs line 73-86 tags 10, 18, 26, 34, 42, 50, 58)

1. handshake (field 1, tag 10): HandshakeResponse
   ├─ success: bool
   └─ error: string (only if success=false)

2. player_state_full (field 2, tag 18): PlayerStateFull
   ├─ subject_id: uint32 (other peer's PeerIndex value, 0-based)
   ├─ sequence: uint32 (snapshot Seq)
   ├─ server_tick: uint32 (monotonic tick counter)
   └─ state: PlayerState (complete frozen state at that tick)

3. player_state_delta (field 3, tag 26): PlayerStateDeltaTier0
   ├─ subject_id: uint32
   ├─ baseline_seq: uint32 (Seq of last known snapshot by observer)
   ├─ new_seq: uint32 (Seq of current snapshot)
   ├─ server_tick: uint32
   └─ Optional quantized fields (has-bit per field, PulseServer.Bitwise.cs:10-122):
      ├─ parcel_index: int32 (if changed)
      ├─ position_x: uint32 → PositionXQuantized float [0.0, 16.0] @ 8 bits
      ├─ position_y: uint32 → PositionYQuantized float [0.0, 200.0] @ 13 bits
      ├─ position_z: uint32 → PositionZQuantized float [0.0, 16.0] @ 8 bits
      ├─ velocity_x: uint32 → VelocityXQuantized float [-50.0, 50.0] @ 8 bits
      ├─ velocity_y: uint32 → VelocityYQuantized float [-50.0, 50.0] @ 8 bits
      ├─ velocity_z: uint32 → VelocityZQuantized float [-50.0, 50.0] @ 8 bits
      ├─ rotation_y: uint32 → RotationYQuantized float [0.0, 360.0] @ 7 bits
      ├─ movement_blend: uint32 → MovementBlendQuantized float [0.0, 3.0] @ 5 bits
      ├─ slide_blend: uint32 → SlideBlendQuantized float [0.0, 1.0] @ 4 bits
      ├─ head_yaw: uint32 → HeadYawQuantized float [0.0, 360.0] @ 7 bits
      ├─ head_pitch: uint32 → HeadPitchQuantized float [0.0, 180.0] @ 6 bits
      ├─ state_flags: uint32 (PlayerAnimationFlags bitmask, if changed)
      ├─ glide_state: enum (0-3, if changed)
      ├─ jump_count: int32 (if changed)
      ├─ point_at_x: uint32 → PointAtXQuantized float [-3000.0, 3000.0] @ 17 bits
      ├─ point_at_y: uint32 → PointAtYQuantized float [0.0, 200.0] @ 7 bits
      └─ point_at_z: uint32 → PointAtZQuantized float [-3000.0, 3000.0] @ 17 bits

   Quantization: Quantize.Encode(value, min, max, bits) = round((clamp(value) - min) / (max - min) * ((1 << bits) - 1))
                 Quantize.Decode(encoded, min, max, bits) = (encoded / ((1 << bits) - 1)) * (max - min) + min

4. player_joined (field 4, tag 34): PlayerJoined
   ├─ user_id: string (wallet address, not PeerIndex!)
   ├─ profile_version: int32
   └─ state: PlayerStateFull { subject_id, sequence, server_tick, state }

5. player_left (field 5, tag 42): PlayerLeft
   └─ subject_id: uint32

6. player_profile_version_announced (field 6, tag 50): PlayerProfileVersionsAnnounced
   ├─ subject_id: uint32
   └─ version: int32

7. emote_started (field 7, tag 58): EmoteStarted
   ├─ subject_id: uint32
   ├─ sequence: uint32 (snapshot Seq when emote started)
   ├─ server_tick: uint32
   ├─ emote_id: string
   └─ player_state: PlayerState (frozen at emote start)

8. emote_stopped (field 8, tag 66): EmoteStopped
   ├─ subject_id: uint32
   ├─ server_tick: uint32
   ├─ reason: enum EmoteStopReason [COMPLETED=0, CANCELLED=1]
   ├─ sequence: uint32
   └─ player_state: PlayerState (state after stop)

9. teleported (field 9, tag 74): TeleportPerformed
   ├─ subject_id: uint32
   ├─ sequence: uint32
   ├─ server_tick: uint32
   └─ state: PlayerState

=== SNAPSHOT LEDGER & CARRY-FORWARD (SnapshotBoard.cs:44-94) ===

Each peer has a ring buffer of PeerSnapshot records (Snapshot.cs:44-84):
- Seq: uint32 (monotonic, increments on each publish)
- ServerTick: uint32 (server simulation tick when published)
- Parcel, GlobalPosition, LocalPosition, Velocity, RotationY
- Animation fields (JumpCount, MovementBlend, SlideBlend, HeadYaw, HeadPitch, PointAt, AnimationFlags, GlideState)
- IsTeleport: bool (marks teleport event)
- Emote: EmoteState? (null = no emote activity)
  ├─ EmoteId: string
  ├─ StartSeq: uint32 (Seq of real EmoteStart event)
  ├─ StartTick: uint32
  ├─ DurationMs: uint32?
  └─ StopReason: EmoteStopReason?
- Realm: string? (AoI partition)
- LastTeleportSeq: uint32 (Seq of most recent teleport)

Publish Invariants (SnapshotBoard.Publish):
1. Emote carry-forward: if emote==null, inherit from previous slot (handles stop marker consumption)
2. Realm carry-forward: if realm==null, inherit from previous
3. LastTeleportSeq carry-forward: if not IsTeleport, inherit; else set to current Seq
4. Seqlock: versions[index] incremented to odd (write start), then to even (write complete) with MemoryBarrier()
5. Ring index: snapshot stored at (Seq % ringCapacity), enabling RESYNC delta calculation

Interest Management:
- Player_joined sent when peer enters AoI (realm changes + parcel proximity)
- PlayerStateFull sent as baseline snapshot after join
- PlayerStateDelta sent for subsequent state changes (subset of fields with has-bits)
- Player_left sent when peer exits AoI

=== CLIENT MESSAGE TYPES (ClientMessage, field oneof) ===
(PulseClient.cs line 2407-2434 tags 10, 18, 26, 34, 42, 50, 58)

1. handshake (field 1, tag 10): HandshakeRequest
2. input (field 2, tag 18): PlayerStateInput { state: PlayerState }
3. resync (field 3, tag 26): ResyncRequest
   ├─ subject_id: uint32 (peer to resync)
   └─ known_seq: uint32 (last Seq received by client)
4. profile_announcement (field 4, tag 34): ProfileVersionAnnouncement { version: int32 }
5. emote_start (field 5, tag 42): EmoteStart
   ├─ emote_id: string
   ├─ duration_ms: uint32 (optional, has-bit)
   └─ player_state: PlayerState (optional)
6. emote_stop (field 6, tag 50): EmoteStop (no fields)
7. teleport (field 7, tag 58): TeleportRequest
   ├─ parcel_index: int32
   ├─ position: Vector3 (local position in parcel)
   └─ realm: string

=== TRANSPORT LAYER (ENetHostedService.cs) ===

Channels (ENetChannel in wrapper):
- RELIABLE (channel 0): HandshakeRequest/Response, TeleportRequest, auth-critical
- UNRELIABLE_SEQUENCED (channel 1): PlayerStateInput, movement data
- UNRELIABLE_UNSEQUENCED (channel 2): non-critical emotes, profile updates

Packet Format:
- Raw protobuf bytes (no framing header) wrapped in ENet packet
- ENet handles UDP framing, retransmission, sequencing per channel
- Max receive buffer: configurable (default 8192 bytes)

Disconnect Reasons (DisconnectReason.cs:5-95):
- 0: NONE
- 1: GRACEFUL (server shutdown)
- 2: AUTH_TIMEOUT (PENDING_AUTH deadline)
- 3: AUTH_FAILED (handshake validation failed)
- 4: DUPLICATE_SESSION (wallet already connected)
- 5: BANNED (platform ban)
- 6: SERVER_FULL (PeerIndex pool exhausted)
- 7: PRE_AUTH_IP_LIMIT_EXHAUSTED (per-IP quota)
- 8: PRE_AUTH_BUDGET_EXHAUSTED (global pre-auth quota)
- 9: INPUT_RATE_EXCEEDED (movement too fast)
- 10: DISCRETE_EVENT_RATE_EXCEEDED (emote/teleport spam)
- 11: INVALID_INPUT_FIELD (bad PlayerStateInput)
- 12: INVALID_EMOTE_FIELD (bad EmoteStart)
- 13: INVALID_TELEPORT_FIELD (bad TeleportRequest)
- 14: HANDSHAKE_REPLAY_REJECTED (replay cache hit)
- 15: INVALID_HANDSHAKE_FIELD (bad initial state)
- 16: PACKET_CORRUPTED (parse failure or oversized)

Connection Lifecycle:
1. ENet Connect → PeerIndex allocated, PENDING_AUTH, PreAuthAdmission.TryAdmit()
2. ClientMessage.handshake received → HandshakeHandler validates, calls PreAuthAdmission.ReleaseOnPromotion(), state → AUTHENTICATED
3. Gameplay: ClientMessage.input/emote_start/emote_stop/teleport received
4. ServerMessage snapshots sent (full baseline, then deltas)
5. ENet Disconnect/Timeout → TeardownPeerSlot() called, PreAuthAdmission.ReleaseOnDisconnect(), IdentityBoard.Remove(), state → DISCONNECTING

PacketMode values (PacketMode.cs:3-8):
- 0: RELIABLE (ENet channel 0)
- 1: UNRELIABLE_SEQUENCED (ENet channel 1)
- 2: UNRELIABLE_UNSEQUENCED (ENet channel 2)

=== THREADING MODEL ===

ENet Thread (RunLoop, one dedicated thread):
- Single writer to connectedPeers, slotToPeerIndex
- Reads from outgoingChannel (lock-free, unbounded)
- Calls messagePipe.OnDataReceived() (fast parse, queues to incomingChannel)

Worker Threads (PeersManager simulation):
- Single writer per peer slot (each worker owns a shard)
- Reads from snapshotBoard via seqlock (lock-free)
- Multiple concurrent readers during simulation
- Writes to outgoingChannel (messages queued for ENet to send)

Thread-Safety Guarantees:
- IdentityBoard: Volatile.Read/Write per slot (no locking, atomic)
- SnapshotBoard: seqlock per slot (readers spin on version even/odd)
- PreAuthAdmission: single Lock protects per-IP + global counters
- MessagePipe: channels are lock-free (SingleWriter/SingleReader where applicable)

=== RESYNC/INTEREST RECONCILIATION ===

ResyncRequest flow (ResyncRequestHandler.cs):
1. Client sends ResyncRequest { subject_id, known_seq }
2. Server looks up subject's snapshot ring (SnapshotBoard)
3. If known_seq still in ring → calculate delta from known_seq to latest
4. If known_seq evicted → send PlayerStateFull as baseline
5. Ring capacity determines max catch-up distance (typical 128-256 snapshots)

=== CRITICAL WIRE DETAILS ===

Protobuf Field Numbering:
- ClientMessage: handshake(1), input(2), resync(3), profile_announcement(4), emote_start(5), emote_stop(6), teleport(7)
- ServerMessage: handshake(1), player_state_full(2), player_state_delta(3), player_joined(4), player_left(5), player_profile_version_announced(6), emote_started(7), emote_stopped(8), teleported(9)
- PlayerStateDeltaTier0: subject_id(1), baseline_seq(2), new_seq(3), server_tick(4), parcel_index(5), position_x(6), position_y(7), position_z(8), velocity_x(9), velocity_y(10), velocity_z(11), rotation_y(12), movement_blend(13), slide_blend(14), head_yaw(15), head_pitch(16), state_flags(17), glide_state(18), jump_count(19), point_at_x(20), point_at_y(21), point_at_z(22)

Varints: uint32/int32 fields encoded as protobuf varints (not fixed32)
Floats: float (32-bit IEEE754) encoded as fixed32 (not varint-encoded)
Quantized uint32: encoded as varints then decoded via Quantize.Decode(bits, min, max)
Strings: UTF-8, length-prefixed protobuf format
Enums: encoded as varints (their numeric value)
Oneof: only one field set per message; tag-based discrimination


### Rust plan

=== CATALYRST-ENET (Transport Layer) ===

1. STRUCT DEFINITIONS
   - PeerIndex: newtype u32 (0-based, opaque handle)
   - DisconnectReason: u8 enum (0-16, as specified in DisconnectReason.cs)
   - PacketMode: u8 enum (RELIABLE=0, UNRELIABLE_SEQUENCED=1, UNRELIABLE_UNSEQUENCED=2)
   - PeerConnectionState: u8 enum (NONE=0, PENDING_AUTH=1, AUTHENTICATED=2, PENDING_DISCONNECT=3, DISCONNECTING=4)
   - MessagePacket: ref struct wrapping ReadOnlySpan<byte> + PeerIndex (zero-copy)
   - OutgoingMessage: struct with To: PeerIndex, PacketMode, Message: ServerMessage
   - ConnectedPeers: HashMap<PeerIndex, ENetPeer> (ENet thread exclusive)
   - SlotToPeerIndex: HashMap<u32, PeerIndex> (ENet slot_id → logical PeerIndex)

2. ENET INTEGRATION
   - Bind libenet-csharp via FFI or use ENet.rs crate (if available)
   - Host.Create(address="[::]:PORT", concurrentCap, channelLimit=3)
   - Service loop: host.Service(timeout_ms=1, &event)
   - CheckEvents() drain loop for buffered packets
   - Packet.Create(span: &[u8], mode: PacketMode) → send via peer.Send()
   - EventType::Connect/Disconnect/Timeout/Receive → dispatch to handlers
   - Timeout tracking per peer (PeerTimeoutMs option)

3. CONNECTION HANDLING
   - On Connect: TryAllocate PeerIndex from allocator, TryAdmit via PreAuthAdmission
   - On Disconnect/Timeout: TeardownPeerSlot() → remove from maps, mark for cleanup
   - FlushOutgoing(): drain outgoing message channel, send each to ENet
   - RecordCorruption(): increment per-peer/slot corruption counters (hardening)

4. MESSAGE PARSING
   - OnDataReceived(packet: MessagePacket) → parse ClientMessage via protobuf
   - On parse error: record corruption, return false (caller decides disconnect)
   - On parse success: queue to incomingChannel with (PeerIndex, ClientMessage)
   - Single-threaded parsing on ENet thread (zero-copy via ReadOnlySpan)

5. BYTE-EXACT COMPATIBILITY
   - Protobuf serialization: use prost or equivalent Rust protobuf lib
   - Varint encoding: standard protobuf varint (little-endian, MSB continuation)
   - Fixed32 for floats: stored as little-endian bytes in protobuf wire format
   - Oneof field: tag-based discrimination (field number determines which arm set)
   - Optional fields with has-bits: track in bitmask (same as C# version)
   - String encoding: length-prefixed UTF-8 bytes

=== CATALYRST-PULSE (Auth + Snapshot Lifecycle) ===

1. PRE-AUTH ADMISSION
   - Struct PreAuthAdmission { per_ip_counts: HashMap<IpAddr, usize>, in_flight: usize, lock: Mutex<()> }
   - TryAdmit(peerIndex: PeerIndex, ip: IpAddr) → Result<(), AdmitError>
     - Check per-ip count < MaxConcurrentPreAuthPerIP → AdmitError::IpLimitExhausted
     - Check in_flight < GlobalPreAuthBudget → AdmitError::BudgetExhausted
     - Increment per_ip_counts[ip] + 1, in_flight + 1 under lock
   - ReleaseOnPromotion(peerIndex) → decrement both under lock (auth succeeded)
   - ReleaseOnDisconnect(peerIndex) → decrement both if peer was in-flight (auth failed/timeout)

2. AUTH CHAIN VALIDATION
   - Struct AuthLink { type: String, payload: String, signature: String }
   - Parse auth chain JSON (x-identity-timestamp, x-identity-metadata headers)
   - Freshness check: |now - timestamp_ms| < 60_000
   - Validate SIGNER link: empty signature, 0x address, normalized
   - For each ECDSA_EPHEMERAL: parse 3-line payload, verify sig by current authority
   - Final link: verify against currentAuthority, payload == "connect:/:TIMESTAMP:METADATA"
   - Return { user_address, current_authority_address, chain }

3. IDENTITY BOARD
   - Struct IdentityBoard { wallets_by_peer_ids: Vec<Option<String>>, peer_ids_by_wallets: HashMap<String, PeerIndex> }
   - Concurrent reads via Arc<Mutex<>>
   - Set(peer_index, wallet_id) → write to vec[peer_index], update HashMap
   - GetWalletIdByPeerIndex(peer_index) → read vec[peer_index]
   - TryGetPeerIndexByWallet(wallet_id) → read HashMap
   - Remove(peer_index) → clear vec[peer_index], remove from HashMap

4. SNAPSHOT BOARD
   - Struct PeerSnapshot { seq, server_tick, parcel, position, velocity, rotation_y, ... emote, realm, last_teleport_seq }
   - Ring buffer per peer (pre-allocated Vec<PeerSnapshot>)
   - Seqlock per peer: versions[peer_index] (odd = write in progress)
   - Publish(peer_index, snapshot) → increment version to odd, store in ring[seq % capacity], increment to even with MemoryBarrier
   - Carry-forward logic: inherit emote/realm from previous slot if not set
   - GetLatestSeq(peer_index) → read lastSeqs[peer_index] under seqlock
   - ReadWithSeqlock(peer_index) → retry loop on version parity

5. HANDSHAKE ACCEPTANCE
   - Parse HandshakeRequest: extract auth_chain bytes, profile_version, initial_state
   - Call AuthChainValidator::Validate() → get AuthChainValidationResult
   - Check timestamp freshness again (defense-in-depth)
   - Check ReplayPolicy: (wallet, timestamp) pair not in cache
   - Check BanList: wallet not banned
   - Validate InitialState fields (FieldValidator)
   - Create PeerState { wallet_id, connection_state: AUTHENTICATED }
   - Call PreAuthAdmission::ReleaseOnPromotion()
   - Call IdentityBoard::Set(peer_index, wallet_id)
   - Call SnapshotBoard::SetActive(peer_index) → mark peer as active in AoI
   - Publish initial snapshot from InitialState
   - Send HandshakeResponse { success: true }

6. INTEREST MANAGEMENT / AoI
   - Realm-based partitioning: each peer has realm (string)
   - On TeleportRequest: update realm, call SnapshotBoard with IsTeleport=true
   - PlayerJoined: sent when peer enters observer's realm+proximity
   - PlayerStateFull: baseline snapshot (subject_id as uint32 = PeerIndex value)
   - PlayerStateDelta: incremental updates, quantized fields only sent if changed (has-bits)
   - PlayerLeft: sent when peer exits realm

7. SNAPSHOT PUBLISHING
   - PeerSnapshotPublisher: converts internal PeerSnapshot to server message types
   - PublishFromPlayerState(peer_index, input_state, emote) → PlayerStateFull or PlayerStateDelta
   - PublishTeleport(peer_index, position, realm) → set IsTeleport=true, update realm
   - EmoteStarted/EmoteStopped: transition emote state in snapshot
   - Seq increments on each publish (uint32, wrapping)

8. SERVER MESSAGES
   - ServerMessage (oneof): handshake, player_state_full, player_state_delta, player_joined, player_left, player_profile_version_announced, emote_started, emote_stopped, teleported
   - HandshakeResponse { success: bool, error: Option<String> }
   - PlayerStateFull { subject_id: u32, sequence: u32, server_tick: u32, state: PlayerState }
   - PlayerStateDeltaTier0 { subject_id, baseline_seq, new_seq, server_tick, [optional quantized fields] }
   - Quantize::Encode(value: f32, min, max, bits) → u32 (protobuf varint)
   - Quantize::Decode(encoded: u32, min, max, bits) → f32

9. RESYNC HANDLING
   - ResyncRequest { subject_id: u32, known_seq: u32 }
   - Lookup subject in SnapshotBoard ring
   - If known_seq in ring: calculate delta from known_seq to latest
   - If known_seq evicted: send PlayerStateFull as baseline
   - Emit series of PlayerStateDelta or single Full

10. THREADING
    - Arc<Mutex<>> for shared state (PreAuthAdmission, IdentityBoard, SnapshotBoard)
    - Seqlock for SnapshotBoard reads (lock-free for readers during simulation)
    - Message channels: crossbeam::channel::unbounded (or tokio::sync::mpsc::unbounded)
    - ENet thread: dedicated thread, flushes outgoing channel
    - Worker threads: sharded per-realm, write to outgoing channel (thread-safe)

11. PACKET MODES & CHANNELS
    - Map PacketMode enum to ENet channel ID (0, 1, 2)
    - RELIABLE: channel 0 (enet_packet_create with RELIABLE flag)
    - UNRELIABLE_SEQUENCED: channel 1 (sequenced flag)
    - UNRELIABLE_UNSEQUENCED: channel 2 (unsequenced flag)

12. DISCONNECT FLOW
    - transport.Disconnect(peer_index, reason) → queue to outgoing channel
    - ENet thread processes: peer.Disconnect(reason as u32)
    - On ENet Disconnect event: TeardownPeerSlot()
      - decrement PreAuthAdmission if in-flight
      - remove from IdentityBoard
      - remove from SnapshotBoard
      - mark PeerIndex for reallocation (grace period)
    - Send disconnect reason as reason code

=== EXACT PROTOBUF COMPATIBILITY ===

1. Message Descriptors (from PulseClient.cs, PulseServer.cs base64 descriptors):
   - Decode base64 descriptor data from C# reflection
   - Regenerate from .proto source (or infer from generated code)
   - Use prost-build or protobuf-codegen to generate Rust equivalents

2. Field Encoding:
   - Varint fields (uint32, int32, uint64): standard protobuf varint (little-endian, MSB continuation)
   - String fields: length-prefixed UTF-8 (varint length, then bytes)
   - Message fields: length-prefixed (varint length, then nested message bytes)
   - Optional fields: field set if has-bit is true (no presence tracking in wire format, implicit)
   - Oneof fields: only one set; wire format determined by active field tag
   - Enums: encoded as varint (numeric value)
   - Floats: fixed32 IEEE754 little-endian (NOT varint-encoded)
   - Quantized uint32: varint-encoded uint32, decoded via Quantize::Decode()

3. Quantization Function (Quantize.cs:21-33):
   - Encode: steps = (1 << bits) - 1; t = clamp((value - min) / (max - min), 0, 1); (t * steps).round() as u32
   - Decode: (encoded as f32 / steps) * (max - min) + min
   - Bit widths per field: position_x(8), position_y(13), position_z(8), velocity_x(8), velocity_y(8), velocity_z(8), rotation_y(7), movement_blend(5), slide_blend(4), head_yaw(7), head_pitch(6), point_at_x(17), point_at_y(7), point_at_z(17)

4. Vector3 Format (from Vectors.cs imported by pulse_shared.proto):
   - x, y, z: float (fixed32 each in protobuf)
   - No quantization at Vector3 level; individual components quantized for deltas

5. Testing: compare byte-for-byte serialization of test messages against C# version
   - Use property-based testing or fixed test vectors
   - Verify round-trip: serialize Rust → deserialize in C# → matches original

=== DELIVERY MODEL ===

1. Reliable (RELIABLE channel, HandshakeRequest/Response):
   - ENet retransmits until ack received (built-in)
   - Ordered delivery within channel
   - Used for auth-critical messages

2. Unreliable-Sequenced (channel 1, movement input):
   - ENet discards out-of-order packets
   - Keeps only latest in sequence
   - No retransmit (fire-and-forget)
   - Used for high-frequency state updates

3. Unreliable-Unsequenced (channel 2, emotes/profile):
   - No ordering guarantee, no discard of old packets
   - Best-effort delivery
   - Used for low-frequency events

=== HANDSHAKE REPLAY PROTECTION ===

1. ReplayPolicy: LRU cache of (wallet_address, timestamp_ms) pairs
2. On handshake: check if (wallet, timestamp) in cache
3. If hit: reject with HANDSHAKE_REPLAY_REJECTED
4. If miss: add to cache, set TTL to slightly > MAX_TIMESTAMP_SKEW_MS (e.g., 65 seconds)
5. Evict oldest entries on capacity overflow

=== FIELD VALIDATION (FieldValidator) ===

1. PlayerInitialState:
   - parcel_index: must be in valid range (realm-specific)
   - position: all components finite, not NaN
   - velocity: all components finite, not NaN
   - emote_id: length limit (e.g., <= 256 bytes)
   - emote_duration_ms: must not exceed max (e.g., 600,000 ms = 10 min)

2. PlayerStateInput:
   - Same checks as InitialState

3. EmoteStart:
   - emote_id: length limit
   - duration_ms: max duration
   - player_state: validate state fields

4. TeleportRequest:
   - parcel_index: valid range
   - position: finite values
   - realm: non-empty, length limit (e.g., <= 256 bytes)

=== METRICS & LOGGING ===

1. Per-peer counters:
   - packets_received, bytes_received
   - packets_sent, bytes_sent
   - send_failures
   - corrupted_packets

2. Connection metrics:
   - active_peers, peers_connected, peers_disconnected
   - pre_auth_in_flight, pre_auth_refused, pre_auth_ip_limit_refused
   - banned_refused, handshake_rejected_count

3. Message type counters:
   - per message type (ClientMessage discriminant)
   - per server message type

4. Logging:
   - INFO: peer connected, authenticated, disconnected
   - DEBUG: detailed event flow (connection state changes)
   - WARN: errors (handshake failures, corruption)


### Gotchas

CRITICAL GOTCHAS FOR BYTE-EXACT COMPATIBILITY:

1. HANDSHAKE AUTHENTICATION CHAIN PARSING
   - Must parse JSON from auth_chain bytes (UTF-8 string)
   - Chain is array of objects: [{"type":"...", "payload":"...", "signature":"..."}]
   - Signature validation requires secp256k1 ECDSA recovery (NOT just verification)
   - Must normalize wallet addresses to lowercase 0x-prefixed hex before comparison
   - Ephemeral payload format is EXACT: 3 lines with specific prefixes
     "PURPOSE\nEphemeral address: 0xADDR\nExpiration: ISO8601-UTC"
   - Final payload format: "connect:/:UNIX_MS:METADATA" (all lowercase)
   - Timestamp freshness check is ±60 seconds EXACTLY (60000ms), not milliseconds but boundaries exact

2. QUANTIZATION PRECISION
   - Quantize.Encode() uses MathF.Round(), not truncation
   - Clamp BEFORE scaling, not after
   - Different bit widths per field: position_y is 13 bits (0-200m), position_x is 8 bits (0-16m per parcel)
   - Point-at fields use 17 bits (large range, -3000 to 3000)
   - Step size varies per field (e.g., position_x step ≈ 0.0627451, must match exactly on round-trip)

3. SNAPSHOT RING BUFFER & SEQLOCK
   - Ring index is Seq % ringCapacity (NOT a circular pointer increment)
   - Seq wraps at u32::MAX (after 4 billion snapshots)
   - Seqlock must use MemoryBarrier() (full fence) between incrementing to odd and to even
   - Readers retry if they see an odd version (write in progress)
   - Emote carry-forward: if emote.EmoteId == null AND previous emote has stop marker → result is null
   - Realm carry-forward: if realm == null, inherit from previous; if IsTeleport, set explicitly

4. ONEOF FIELD DISCRIMINATION (ServerMessage & ClientMessage)
   - Protobuf oneof: only ONE field per message can be set
   - Field number determines tag: field 1 = tag 10, field 2 = tag 18, etc.
   - Discriminant in Rust: enum-like, not bitmask
   - Deserialization: first message with matching tag "wins", others are ignored
   - Serialization: only serialized field is written

5. OPTIONAL FIELDS & HAS-BITS
   - Optional fields in proto3 syntax have has-bit tracking (despite "proto3 has no has-bits" rule)
   - has-bit bitmask in _hasBits0 (one int for multiple optional fields)
   - On deserialization: presence is explicit (field was in wire, not default value)
   - On serialization: omit if default value AND no has-bit set
   - PlayerStateDeltaTier0 has 18 optional fields; bitmask tracks which are present

6. DISCONNECT REASON ENCODING
   - DisconnectReason is u8 enum (0-16)
   - Sent to ENet as uint32 to peer.Disconnect(reason_as_u32)
   - Must match exact enum values (no fuzz tolerance)

7. STRING ENCODING IN PROTOBUF
   - auth_chain: raw UTF-8 bytes, length-prefixed in protobuf message
   - Must not decode auth_chain from UTF-8 inside handshake; it's a JSON string encoded as bytes
   - realm: UTF-8 string, length limit (must validate, not just pass through)
   - emote_id: UTF-8 string, length limit

8. PACKET CHANNEL MAPPING
   - RELIABLE = ENet channel 0 (packet flag: 1 << 0)
   - UNRELIABLE_SEQUENCED = ENet channel 1 (packet flag: ordered)
   - UNRELIABLE_UNSEQUENCED = ENet channel 2 (packet flag: 0)
   - ENet channels are 0-indexed; must use correct channel ID when calling peer.Send()

9. INTEREST MANAGEMENT TIMING
   - PlayerJoined MUST include full PlayerStateFull snapshot (not delta)
   - PlayerLeft MUST be sent BEFORE peer is removed from IdentityBoard (observer needs time to react)
   - PlayerStateFull: subject_id is PeerIndex value as u32 (NOT wallet address)
   - Baseline snapshot selection for resync: if known_seq outside ring, send Full; else compute delta

10. FLOATING POINT NaN/INF CHECKS
    - Validate all incoming float fields are finite (not NaN, not ±Infinity)
    - Invalid float serialization will fail protobuf deserialization (NaN/Inf have specific IEEE754 bit patterns)
    - Must check AFTER deserialization before using value

11. CONCURRENT MODIFICATION INVARIANT
    - IdentityBoard: after Set(), TryGetPeerIndexByWallet() must return the same PeerIndex
    - SnapshotBoard: after Publish(), all readers must see carry-forward values for fields not explicitly set
    - PreAuthAdmission: ReleaseOnPromotion() and ReleaseOnDisconnect() must never double-release (idempotent)

12. WINDOWS-STYLE vs UNIX TIMESTAMP
    - x-identity-timestamp is unix milliseconds (not seconds)
    - Freshness window: |NOW - timestamp_ms| < 60000
    - ISO8601 expiration in ephemeral payload must parse with timezone (assumed UTC)

13. PROTOBUF PARSER STRICTNESS
    - Unknown fields: must NOT error, must pass through to UnknownFieldSet
    - Duplicate oneof fields: last one wins (ignore earlier instances)
    - Truncated messages: error (not "incomplete" acceptance)
    - Field size limits: varint can be up to 10 bytes; if larger, error

14. REPLICA CONSISTENCY
    - All replicas must agree on SnapshotBoard ring contents (if synchronized)
    - Seq MUST increment monotonically (never backwards)
    - Timestamp in InitialState is PART OF SIGNED PAYLOAD, must be preserved
    - Different observers may see different deltas based on their last known Seq

15. BITWISE PLUGIN METADATA
    - Quantization ranges and bit widths are HARDCODED in generated .Bitwise.cs (not configurable)
    - If source .proto changes (min/max/bits), ALL clients must rebuild (protocol version bump)
    - Mismatched quantization between client/server causes divergence (not detected at runtime)

16. CLOCK SKEW TOLERANCE
    - 60-second window is EXACT (not 59.9, not 60.1)
    - Clock skew beyond window is treated as replay/future attack (disconnect)
    - No clock synchronization protocol; clients must have accurate system time

17. PARCEL INDEX VALIDITY
    - Must validate parcel_index is in valid range for target realm
    - Invalid parcel: disconnect with INVALID_INPUT_FIELD or INVALID_TELEPORT_FIELD
    - Parcel 0 is valid; no special "uninitialized" value

18. EMOTE STOP REASON ENUM
    - COMPLETED = 0 (timer expired server-side)
    - CANCELLED = 1 (client sent EmoteStop for looping emote)
    - Client receives reason to know if emote finished naturally or was cut short

19. MESSAGE ORDERING WITHIN CHANNEL
    - RELIABLE channel: messages arrive in order (ENet guarantees)
    - UNRELIABLE_SEQUENCED: client sees last message in sequence (older ones discarded)
    - Implication: if client sends two PlayerStateInput rapidly, server may see only the latest
    - Must not assume all inputs are received

20. PROFILE VERSION SEMANTICS
    - profile_version in HandshakeRequest is sent by client (0 if no profile)
    - player_profile_version_announced in ServerMessage notifies observers of version changes
    - NOT validated against server's stored version; purely informational
    - Can be 0 or positive int32 (no semantic meaning to server)


## Synthesis / ordered plan

Descriptor decode is definitive. For `parcel_index`: bytes `18 05 20 01 28 05 48 00 88 01 01` decode as field 5 (`18 05`), label optional (`20 01`), **type `28 05` = type 5 = TYPE_INT32**. Same for `jump_count`: `28 05` = TYPE_INT32. So both are plain `int32` (varint, sign-extended for negatives), confirming `WriteInt32` and **refuting the spec's zigzag/sint32 claim**. The `48 xx 88 01 01` is the oneof_index + proto3_optional marker.

I now have everything needed. The spec's entire Phase 1-6 "ENet wire protocol in Rust" is based on a false premise — upstream Pulse uses native ENet via FFI, so byte-exactness at the ENet datagram layer is automatic if catalyrst links the same native lib, and the only byte-exact surface that matters is the **protobuf payload** carried in ENet packets. I'll write the corrected plan.

Here is the synthesized implementation plan.

---

# Catalyrst-ENet + Catalyrst-Pulse → Byte-Exact with Upstream Pulse: Implementation Plan

## 0. Ground-truth corrections to the input spec (verified against source — read before trusting the JSON)

The supplied subsystem JSON is substantially wrong about the ENet layer and the integer encodings. Verified findings, with file evidence:

1. **Upstream Pulse does NOT implement the ENet wire protocol in C#.** `Pulse/src/DCLPulse.Transport.Shared/Runtime/ENet.cs` is a pure **P/Invoke wrapper** over the native `enet` shared library (SoftwareGuy/ENet-CSharp fork, `Library.version = 2.4.9`). Every primitive (`enet_host_create`, `enet_peer_send`, `enet_packet_create`, `enet_host_service`) is `[DllImport(nativeLibrary)]`. There is **no managed `ProtocolHeader`, no peerID bit-packing, no managed CONNECT/VERIFY_CONNECT/ConnectBody, no connectID echo logic** in C#. The spec's cited line ranges are fabricated: `ENet.cs:72-86` is the `ENetAddress`/`Address` struct; `ENet.cs:123-186` is `Packet.Create` overloads; `ENet.cs:713-718` is `SetChecksumCallback` (a P/Invoke shim, not a CRC implementation).
   - **Consequence:** "byte-exact ENet datagram framing" is **not a Rust-level concern**. The ENet UDP datagram format (peerID session bits, sentTime flag, CONNECT handshake, ACKs, fragmentation, throttle, CRC) is produced by the **native C library**, identically on both ends, as long as catalyrst links a compatible `libenet`. Re-implementing the ENet datagram protocol in Rust (the spec's Phase 1–6) is the **wrong target** for byte-parity and a large interop risk.
   - **The only wire surface that must be byte-exact is the protobuf application payload** placed inside `enet_packet_create(...)`, on channels 0/1/2 with the right `PacketFlags`.

2. **`parcel_index` and `jump_count` are `int32`, NOT `sint32`/zigzag.** Generated code uses `output.WriteInt32(ParcelIndex)` / `WriteInt32(JumpCount)` (`PulseServer.cs:1339,1395`; `PulseShared.cs:379,419`). Descriptor bytes confirm `TYPE_INT32` (`28 05`) for both `parcel_index` (field 5) and `jump_count` (field 19). The spec repeatedly says "sint32 ZigZag" — **that is wrong** and would produce different bytes for any negative value (protobuf `int32` sign-extends negatives to a 10-byte varint; zigzag does not). Use plain int32 varint with sign-extension.

3. **`profile_version`/`version` are `int32`** (`WriteInt32(Version)`, `PulseServer.cs:491,509`) — again plain int32, not sint32.

4. **Quantize uses `MathF.Round` = round-half-to-even** (`Quantize.cs:25`). Rust `f32::round()` is round-half-**away-from-zero** — a mismatch. Must replicate banker's rounding explicitly. `steps = (1u32 << bits) - 1` (`Quantize.cs:23`).

5. **proto3 `optional`** semantics (the `88 01 01` proto3_optional marker in the descriptor + `_hasBits0`) mean a present field is written **only if its has-bit is set**, and on the wire it is just a normal tag+value with no extra presence marker. Default-valued-but-present fields are still emitted. `Option<T>` in Rust models this exactly.

6. **Channels & flags** (`ENetChannel.cs`, `PacketMode.cs`): ch0 = Reliable, ch1 = Unreliable-Sequenced (`PacketFlags.None`), ch2 = Unreliable-Unsequenced (`PacketFlags.Unsequenced`). The native lib supplies sequencing.

**Strategic pivot:** Treat catalyrst-enet as a **thin FFI/transport binding to native libenet** (parity comes for free from the shared C lib), and put 100% of byte-exactness effort into a new **protobuf codec** matching the generated C#. The spec's homegrown `catalyrst-enet/src/protocol.rs` (managed ENet datagram parser) and `catalyrst-pulse/src/protocol.rs` (5-tag codec) are both dead ends and should be deleted, not extended.

---

## 1. Dependency order (corrected)

```
(A) catalyrst-enet transport binding  ── parity is provided by native libenet; verify only
        │   (channels 0/1/2 + PacketFlags; no Rust datagram parser)
        ▼
(B) Quantize module ───────────────────── pure fn, zero deps, byte-exactness root
        │
        ▼
(C) Protobuf codec (prost-generated types + Quantize glue)
        │   PlayerState (full, fixed32 floats), PlayerStateDeltaTier0 (quantized varints),
        │   ClientMessage/ServerMessage oneof envelopes
        ▼
(D) Auth handshake + session semantics (auth-chain validate, identity board,
            snapshot board, pre-auth admission, disconnect reasons)
        ▼
(E) Server loop wiring (route(), interest mgmt, resync) over (A)+(C)+(D)
```

Rationale: (B) is the smallest verifiable unit and everything quantized depends on it; (C) cannot be byte-checked without (B); (D) consumes (C)'s message types; (A) is independent and can proceed in parallel but must exist before any live interop test.

---

## 2. Concrete per-crate steps

### Crate A — `catalyrst-enet` (transport binding, parity = native lib)

**Delete** `src/protocol.rs`, `src/peer.rs`'s hand-rolled header/session logic, and the managed datagram parser in `src/host.rs`. They re-implement what `libenet` already does and will diverge.

**Add** an FFI binding to native ENet (SoftwareGuy/ENet-CSharp's C library, version `2.4.9`). Two acceptable routes:
- Use the existing Rust crate `rusty_enet` (pure-Rust port of lsalzman ENet) **only if** its on-wire format matches the SoftwareGuy fork — must be validated by capture (see §4.5). The fork has extensions (`enet_host_set_max_duplicate_peers`, larger `maxPacketSize`, CRC64); confirm defaults match.
- Or `bindgen` + link the same `libenet.so` Pulse uses (lowest interop risk; guarantees identical datagrams). Build it from the ENet-CSharp `Native/` sources.

Key types/functions (mirror the C# wrapper surface, which is the contract Pulse uses):
```rust
pub struct Host { /* *mut ENetHost */ }
pub struct EnetPeer { id: u32, /* *mut ENetPeer */ }
pub enum EventKind { Connect, Disconnect, Receive, Timeout }
pub enum PacketMode { Reliable = 0, UnreliableSequenced = 1, UnreliableUnsequenced = 2 }

impl Host {
    pub fn create(addr: SocketAddr, peer_limit: usize, channel_limit: u8 /* =3 */) -> Host;
    pub fn service(&mut self, timeout_ms: u32) -> Option<Event>;     // enet_host_service
    pub fn check_events(&mut self) -> Option<Event>;                  // enet_host_check_events
    pub fn flush(&mut self);
}
impl EnetPeer {
    pub fn send(&self, channel: u8, data: &[u8], mode: PacketMode) -> i32; // enet_peer_send + create
    pub fn disconnect(&self, reason: u32);                                 // DisconnectReason as u32
}
```
PacketMode → (channel, PacketFlags) mapping (must match `PacketMode.cs`/`ENetChannel.cs`):
| Mode | channel | flags |
|---|---|---|
| Reliable | 0 | `Reliable (1<<0)` |
| UnreliableSequenced | 1 | `None (0)` |
| UnreliableUnsequenced | 2 | `Unsequenced (1<<1)` |

`DisconnectReason` enum (0..16) ported verbatim from `DisconnectReason.cs`.

**No byte-exact unit tests live here** — the wire bytes are produced by C. Parity test for (A) is a live capture diff (§4.5).

### Crate B — `catalyrst-pulse-codec/src/quantize.rs` (new crate, or module in catalyrst-pulse)

Port `Quantize.cs:21-33` exactly, **with banker's rounding**:
```rust
pub fn encode(value: f32, min: f32, max: f32, bits: u32) -> u32 {
    let steps = (1u32 << bits) - 1;
    let t = ((value - min) / (max - min)).clamp(0.0, 1.0);
    round_half_to_even(t * steps as f32) as u32   // NOT f32::round()
}
pub fn decode(encoded: u32, min: f32, max: f32, bits: u32) -> f32 {
    let steps = (1u32 << bits) - 1;
    encoded as f32 / steps as f32 * (max - min) + min
}
```
`round_half_to_even`: `let r = x.round(); if (x - x.trunc()).abs() == 0.5 { /* pick even */ } else { r }` — or compute via `(x).round_ties_even()` (stable since Rust 1.77; confirm toolchain, else hand-roll). All literals `f32` (`0.0_f32`).

Per-field spec table (verified against `PulseServer.Bitwise.cs`, exact min/max/bits):
```
position_x  0.0  16.0   8    velocity_x  -50.0 50.0  8    head_yaw    0.0 360.0 7
position_y  0.0  200.0  13   velocity_y  -50.0 50.0  8    head_pitch  0.0 180.0 6
position_z  0.0  16.0   8    velocity_z  -50.0 50.0  8    point_at_x -3000 3000 17
rotation_y  0.0  360.0  7    movement_blend 0.0 3.0 5    point_at_y  0.0 200.0 7
slide_blend 0.0  1.0    4                                  point_at_z -3000 3000 17
```
(Note: spec body text says `head_pitch` is 7 bits/360 in one place and 6 bits/180 in another — the Bitwise.cs ground truth is **6 bits, [0,180]**.)

### Crate C — `catalyrst-pulse-codec` protobuf codec

**Use prost** (0.11, already in-tree). Two options; pick **prost-build from `.proto`** if the proto sources can be fetched (they are NOT in this Pulse checkout — only generated C#; `decentraland/protocol` repo does not contain pulse protos either). If sources are unavailable, **hand-write `prost::Message` structs** with field numbers transcribed from the verified `WriteTo` tags below.

Field/tag table (verified from `PulseServer.cs` WriteRawTag + descriptor):

`PlayerStateDeltaTier0`:
- 1 subject_id `uint32` (tag 8), 2 baseline_seq `uint32` (16), 3 new_seq `uint32` (24), 4 server_tick `uint32` (32) — all required (written when `!= 0`)
- 5 parcel_index **`int32` optional** (40), 6–16 quantized `uint32` optional (48,56,64,72,80,88,96,104,112,120,128·1), 17 state_flags `uint32` (136·1), 18 glide_state `enum` (144·1, via `WriteEnum`), 19 jump_count **`int32` optional** (152·1), 20–22 point_at `uint32` optional (160·1,168·1,176·1)

`PlayerState` (full, unquantized): 1 parcel_index `int32`, 2 position `Vector3` msg, 3 velocity `Vector3` msg, 4 rotation_y `float`(fixed32, tag 37), 5 movement_blend float, 6 slide_blend float, 7 head_yaw float(opt), 8 head_pitch float(opt), 9 state_flags `uint32`, 10 glide_state enum, 11 jump_count `int32`, 12 point_at Vector3(opt). `Vector3` = 3× `float` fixed32.

`ClientMessage` oneof tags 1–7; `ServerMessage` oneof tags 1–9 (per spec, consistent with generated envelopes).

Codec implementation: with prost-derived `Message`, encoding is automatic and byte-exact **provided** each field has the correct prost type:
- `uint32` → `#[prost(uint32, tag="N")]`
- `int32` → `#[prost(int32, ...)]`  ← **NOT `sint32`**; this is the crux fix
- `float` (full state) → `#[prost(float, ...)]` (prost emits fixed32 LE — matches `WriteFloat`)
- proto3 optional → `Option<...>` with `#[prost(..., optional, tag="N")]`
- enums → `#[prost(enumeration="GlideState", ...)]`
- oneof → `#[prost(oneof="...", tags="1,2,...")]`

Then a thin **quantization glue layer** (mirrors `*.Bitwise.cs` accessors): converts app-level `f32` ↔ stored quantized `u32` via Crate B. Keep the stored field as `u32` (what goes on the wire) and provide `position_x_quantized()` getters/setters exactly like the C# `PositionXQuantized` partial.

### Crate D — `catalyrst-pulse` auth + session

Port from the C# session layer:
- `AuthChainValidator` ← `DCLAuth`/`AuthChainValidator.cs`: reuse existing `catalyrst-*/src/auth_chain.rs` + `catalyrst-fed/src/sig.rs` (has `signer()`/`verify()` over secp256k1). Enforce: `±60_000 ms` timestamp skew, SIGNER first link, ECDSA_EPHEMERAL chain, final payload `connect:/:<unix_ms>:<metadata>`, lowercase-normalized address.
- `PreAuthAdmission` (global budget + per-IP cap), `IdentityBoard` (wallet↔peer, duplicate-session eviction), `SnapshotBoard` (ring buffer + seqlock + emote/realm/teleport carry-forward), `ReplayPolicy` (LRU of (wallet,timestamp)), `HandshakeHandler` flow (15-step accept).
- `DisconnectReason` shared with Crate A.

### Crate E — server wiring

Replace `catalyrst-pulse/src/protocol.rs` (delete the 5-tag codec) and implement `route()`: decode `ClientMessage` via Crate C, dispatch, publish snapshots, fan out `PlayerStateFull`/`PlayerStateDeltaTier0` on the correct channels, handle `ResyncRequest`.

---

## 3. Upstream C# files to port from

| Rust target | Upstream file (all under `Pulse/src/`) |
|---|---|
| `quantize.rs` | `Protocol/Generated/Quantize.cs` |
| quantization glue (per-field min/max/bits) | `Protocol/Generated/PulseServer.Bitwise.cs`, `QuantizationExample.Bitwise.cs` |
| `ServerMessage`, `PlayerStateFull`, `PlayerStateDeltaTier0`, `PlayerJoined/Left`, `EmoteStarted/Stopped`, `TeleportPerformed`, `HandshakeResponse` | `Protocol/Generated/PulseServer.cs` |
| `ClientMessage`, `HandshakeRequest`, `PlayerInitialState`, `PlayerStateInput`, `ResyncRequest`, `EmoteStart/Stop`, `TeleportRequest`, `ProfileVersionAnnouncement` | `Protocol/Generated/PulseClient.cs` |
| `PlayerState`, `Vector3`, `GlideState`, `EmoteStopReason`, `PlayerAnimationFlags` | `Protocol/Generated/PulseShared.cs`, `Vectors.cs` |
| transport binding, channels, packet modes | `DCLPulse.Transport.Shared/Runtime/ENet.cs` (FFI surface), `ENetChannel.cs`, `PacketMode.cs`, `DisconnectReason.cs` |
| auth handshake, boards, admission | `DCLAuth/*` (AuthChainValidator), `DCLPulse/*` (HandshakeHandler, SnapshotBoard, IdentityBoard, PreAuthAdmission, PeerConnectionState) — locate exact paths under `Pulse/src/DCLPulse/` |
| native ENet C source (if building libenet) | ENet-CSharp `Native/` (lsalzman fork @ 2.4.9) |

The `QuantizationExample.*` types are reference vectors only — handy for cross-checking Crate B but not part of the live protocol.

---

## 4. Byte-parity unit tests (encode known message → compare bytes)

### 4.1 Quantize golden vectors (Crate B)
Derive expected `u32` by hand from `steps=(1<<bits)-1`, banker's rounding:
```rust
#[test] fn q_position_x_midpoint() {
    // [0,16] @ 8 bits, steps=255; value 8.0 → t=0.5 → 0.5*255=127.5 → round-even → 128
    assert_eq!(quantize::encode(8.0, 0.0, 16.0, 8), 128);
}
#[test] fn q_clamp_and_bounds() {
    assert_eq!(quantize::encode(-5.0, 0.0, 16.0, 8), 0);
    assert_eq!(quantize::encode(99.0, 0.0, 16.0, 8), 255);
}
#[test] fn q_velocity_zero_centered() {
    // [-50,50]@8, steps=255; value 0 → t=0.5 → 127.5 → even → 128
    assert_eq!(quantize::encode(0.0, -50.0, 50.0, 8), 128);
}
```
The `127.5 → 128` (even) vs `f32::round()` → `128` happens to agree here; add a vector that exposes the divergence, e.g. a field/value where `t*steps` lands on `x.5` with `x` even (rounds down under banker's, up under away-from-zero) and assert the banker's result, matching C# `MathF.Round`.

### 4.2 PlayerStateDeltaTier0 byte-exact (Crate C) — the headline test
```rust
#[test] fn delta_bytes_match_csharp() {
    let m = PlayerStateDeltaTier0 {
        subject_id: 42, baseline_seq: 7, new_seq: 8, server_tick: 100,
        parcel_index: Some(-3),            // int32 negative → 10-byte varint, NOT zigzag
        position_x: Some(128), ..default
    };
    // Expected: tag 8,42 | tag16,7 | tag24,8 | tag32,100 |
    //           tag40, int32(-3)= FD FF FF FF FF FF FF FF FF 01 |
    //           tag48, 128 = 80 01
    assert_eq!(m.encode_to_vec(), hex!("082a 1007 1808 2064 28 fdffffffffffffffff01 30 8001"));
}
```
The `parcel_index = -3` case is the **regression test that proves the int32-not-sint32 fix**: zigzag would give `28 05`, plain int32 gives the 10-byte sign-extended varint above.

### 4.3 PlayerState full float fixed32 (Crate C)
```rust
#[test] fn player_state_rotation_fixed32() {
    let s = PlayerState { rotation_y: 1.0, ..default };
    // field 4 float → tag 37 (0x25), then 1.0f LE = 00 00 80 3F
    assert!(s.encode_to_vec().windows(5).any(|w| w == hex!("25 0000803f")));
}
```

### 4.4 Envelope oneof + handshake
```rust
#[test] fn server_handshake_ok() {
    let env = ServerMessage::Handshake(HandshakeResponse{ success:true, error:None });
    // tag1 wire2 = 0x0a, len, inner: field1 bool true = 08 01
    assert_eq!(env.encode_to_vec(), hex!("0a02 0801"));
}
```

### 4.5 Live-capture diff (Crate A + integration) — the only ENet-layer parity check
Capture real datagrams from the C# Pulse server (tcpdump on the UDP port during a Unity-client handshake), and from catalyrst-pulse, and diff: CONNECT/VERIFY_CONNECT/ACK bytes (produced by native lib) and the protobuf payloads (Crate C). If using `rusty_enet`, this is where a fork-vs-upstream datagram mismatch surfaces — if it does, switch to linking `libenet`.

### 4.6 Cross-decode (strongest)
Feed `m.encode_to_vec()` into the C# `PlayerStateDeltaTier0.Parser.ParseFrom`, re-serialize via C# `.ToByteArray()`, assert identical bytes; and vice-versa. Store the C# outputs as committed hex fixtures so CI needs no .NET.

---

## 5. Loop-sized increments (each independently mergeable + testable)

1. **L1 — Quantize (Crate B).** ~80 LOC + golden tests (4.1). Banker's-rounding divergence test. *No deps; mergeable alone.*
2. **L2 — ENet FFI binding skeleton (Crate A).** Delete homegrown `protocol.rs`; bind `host_create/service/check_events/peer_send/disconnect`; channel+flag mapping; `DisconnectReason`. Smoke test: bring up a host, accept one native connect. *Parallel to L1.*
3. **L3 — Codec: PlayerStateDeltaTier0 only (Crate C).** prost struct with correct `int32`/`uint32`/optional types + quantization glue. Tests 4.2, plus the `-3` int32 regression. *Depends L1.*
4. **L4 — Codec: PlayerState/Vector3 + enums (Crate C).** fixed32 floats. Test 4.3. *Depends L3.*
5. **L5 — Codec: ClientMessage + ServerMessage envelopes + remaining server/client messages.** oneof. Tests 4.4. *Depends L3/L4.*
6. **L6 — Cross-decode fixtures (4.6).** Generate C# golden hex for all message types; wire into CI. *Depends L3–L5.*
7. **L7 — Auth handshake (Crate D part 1):** AuthChainValidator (reuse `catalyrst-fed::sig` + existing `auth_chain.rs`), ReplayPolicy, freshness. Unit tests with a known auth-chain fixture.
8. **L8 — Session boards (Crate D part 2):** PreAuthAdmission, IdentityBoard, SnapshotBoard (ring + seqlock + carry-forward). Unit tests for carry-forward invariants & duplicate-session eviction.
9. **L9 — Server wiring (Crate E):** delete `pulse/src/protocol.rs`; implement `route()` over L2+L5+L7+L8; full-baseline-then-delta fan-out; ResyncRequest.
10. **L10 — Live interop:** wire-capture diff (4.5) + drive a real Unity/refclient handshake against catalyrst-pulse; confirm `HandshakeResponse{success:true}` and that the client renders a remote avatar from deltas.

Increments L1–L6 are the byte-exact core and can land before any of the session/transport work; L7–L10 make it a live server.

---

## Key relevant file paths

Catalyrst (to modify/replace):
- `crates/catalyrst-enet/src/protocol.rs` — **delete** (re-implements native ENet)
- `crates/catalyrst-enet/src/{host.rs,peer.rs}` — replace managed datagram logic with native FFI binding
- `crates/catalyrst-pulse/src/protocol.rs` — **delete** (homegrown 5-tag codec)
- `crates/catalyrst-pulse/src/{lib.rs,server.rs}` — re-implement `route()` over new codec
- New crate `catalyrst-pulse-codec` (quantize + prost messages) — prost 0.11 precedent: `crates/catalyrst-archipelago/Cargo.toml`, `catalyrst-social-rpc/Cargo.toml`
- Reuse `crates/catalyrst-fed/src/sig.rs` (secp256k1 `signer()`/`verify()`) and any `crates/*/src/auth_chain.rs` for handshake validation.

Upstream ground truth:
- `decentraland/Pulse/src/Protocol/Generated/{Quantize.cs,PulseServer.cs,PulseServer.Bitwise.cs,PulseClient.cs,PulseShared.cs,Vectors.cs}`
- `decentraland/Pulse/src/DCLPulse.Transport.Shared/Runtime/{ENet.cs,ENetChannel.cs,PacketMode.cs,DisconnectReason.cs}` (ENet.cs is FFI-only)
- `decentraland/Pulse/src/{DCLAuth,DCLPulse}/` (auth + session layer — port from here)

**Load-bearing corrections vs the input spec:** (1) do **not** re-implement the ENet datagram protocol in Rust — bind/link native libenet; ENet byte-parity is the C library's job. (2) `parcel_index`, `jump_count`, `profile_version` are **`int32`, not `sint32`/zigzag** — verified via `WriteInt32` and descriptor `TYPE_INT32` (`28 05`); the `-3` parcel_index byte sequence is the definitive regression test. (3) Quantize must use **round-half-to-even** to match `MathF.Round`, not Rust `f32::round()`.
