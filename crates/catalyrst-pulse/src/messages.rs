//! Pulse protobuf messages, byte-exact with upstream.
//!
//! The `protoc-gen-bitwise` plugin does NOT produce a custom bitstream — the
//! generated `*.Bitwise.cs` files only add float↔uint32 accessors via `Quantize`.
//! The wire is plain protobuf (proto3), so the prost catalog in
//! [`crate::decentraland`] (byte-identical to Google.Protobuf) carries it;
//! quantized floats ride as `uint32` varint fields and are converted with
//! [`crate::quantize`].
//!
//! This module adds the float↔uint32 quantization accessors for every quantized
//! field of [`PlayerStateDeltaTier0`] — the Rust equivalent of
//! `PulseServer.Bitwise.cs`'s `…Quantized` properties — keyed off the exact
//! `(min, max, bits)` options declared in `pulse_server.proto`.

use prost::Message;

use crate::decentraland::pulse::PlayerStateDeltaTier0;
use crate::quantize;

/// The quantization range + precision of one field (mirrors the
/// `[(decentraland.common.quantized) = { min, max, bits }]` option in
/// `pulse_server.proto`). `min`/`max`/`bits` MUST match the proto byte-for-byte
/// or the decoded floats diverge from the Unity client.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct QuantSpec {
    pub min: f32,
    pub max: f32,
    pub bits: u32,
}

impl QuantSpec {
    pub const fn new(min: f32, max: f32, bits: u32) -> Self {
        Self { min, max, bits }
    }
    /// Float → quantized uint32 (`Quantize.Encode`).
    pub fn encode(&self, value: f32) -> u32 {
        quantize::encode(value, self.min, self.max, self.bits)
    }
    /// Quantized uint32 → float (`Quantize.Decode`).
    pub fn decode(&self, encoded: u32) -> f32 {
        quantize::decode(encoded, self.min, self.max, self.bits)
    }
}

/// Per-field quantization specs for [`PlayerStateDeltaTier0`], one constant per
/// quantized field, transcribed from `pulse_server.proto` (fields 6–16, 20–22).
pub mod spec {
    use super::QuantSpec;

    pub const POSITION_X: QuantSpec = QuantSpec::new(0.0, 16.0, 8);
    pub const POSITION_Y: QuantSpec = QuantSpec::new(0.0, 200.0, 13);
    pub const POSITION_Z: QuantSpec = QuantSpec::new(0.0, 16.0, 8);
    pub const VELOCITY_X: QuantSpec = QuantSpec::new(-50.0, 50.0, 8);
    pub const VELOCITY_Y: QuantSpec = QuantSpec::new(-50.0, 50.0, 8);
    pub const VELOCITY_Z: QuantSpec = QuantSpec::new(-50.0, 50.0, 8);
    pub const ROTATION_Y: QuantSpec = QuantSpec::new(0.0, 360.0, 7);
    pub const MOVEMENT_BLEND: QuantSpec = QuantSpec::new(0.0, 3.0, 5);
    pub const SLIDE_BLEND: QuantSpec = QuantSpec::new(0.0, 1.0, 4);
    pub const HEAD_YAW: QuantSpec = QuantSpec::new(0.0, 360.0, 7);
    // head_pitch is [0,180]@6, NOT [0,360]@7 like head_yaw. Sourced from the
    // upstream `HeadPitchQuantized` accessor in PulseServer.Bitwise.cs
    // (`Quantize.Decode(HeadPitch, 0.0f, 180.0f, 6)`), which is authoritative.
    pub const HEAD_PITCH: QuantSpec = QuantSpec::new(0.0, 180.0, 6);
    pub const POINT_AT_X: QuantSpec = QuantSpec::new(-3000.0, 3000.0, 17);
    pub const POINT_AT_Y: QuantSpec = QuantSpec::new(0.0, 200.0, 7);
    pub const POINT_AT_Z: QuantSpec = QuantSpec::new(-3000.0, 3000.0, 17);
}

/// Generates a paired float getter/setter for one quantized `Option<u32>` field
/// of [`PlayerStateDeltaTier0`]. `set_*(f32)` quantizes and marks the field
/// present; `*_f()` dequantizes (returns `None` when the field is absent on the
/// wire, preserving proto3 presence semantics).
macro_rules! quantized_accessor {
    ($field:ident, $set:ident, $get:ident, $spec:expr) => {
        #[doc = concat!("Set `", stringify!($field), "` from a float, quantizing per its proto spec.")]
        pub fn $set(&mut self, value: f32) {
            self.$field = Some($spec.encode(value));
        }
        #[doc = concat!("Read `", stringify!($field), "` as a float, or `None` if absent.")]
        pub fn $get(&self) -> Option<f32> {
            self.$field.map(|q| $spec.decode(q))
        }
    };
}

impl PlayerStateDeltaTier0 {
    quantized_accessor!(position_x, set_position_x_f, position_x_f, spec::POSITION_X);
    quantized_accessor!(position_y, set_position_y_f, position_y_f, spec::POSITION_Y);
    quantized_accessor!(position_z, set_position_z_f, position_z_f, spec::POSITION_Z);
    quantized_accessor!(velocity_x, set_velocity_x_f, velocity_x_f, spec::VELOCITY_X);
    quantized_accessor!(velocity_y, set_velocity_y_f, velocity_y_f, spec::VELOCITY_Y);
    quantized_accessor!(velocity_z, set_velocity_z_f, velocity_z_f, spec::VELOCITY_Z);
    quantized_accessor!(rotation_y, set_rotation_y_f, rotation_y_f, spec::ROTATION_Y);
    quantized_accessor!(
        movement_blend,
        set_movement_blend_f,
        movement_blend_f,
        spec::MOVEMENT_BLEND
    );
    quantized_accessor!(
        slide_blend,
        set_slide_blend_f,
        slide_blend_f,
        spec::SLIDE_BLEND
    );
    quantized_accessor!(head_yaw, set_head_yaw_f, head_yaw_f, spec::HEAD_YAW);
    quantized_accessor!(head_pitch, set_head_pitch_f, head_pitch_f, spec::HEAD_PITCH);
    quantized_accessor!(point_at_x, set_point_at_x_f, point_at_x_f, spec::POINT_AT_X);
    quantized_accessor!(point_at_y, set_point_at_y_f, point_at_y_f, spec::POINT_AT_Y);
    quantized_accessor!(point_at_z, set_point_at_z_f, point_at_z_f, spec::POINT_AT_Z);
}

/// Δ position relative to the last acked full snapshot (channel 1, unreliable).
/// quantization_example.proto: dx/dy/dz quantized [-100,100]@16, entity_id@20,
/// sequence@12 — all plain `uint32` on the wire.
#[derive(Clone, PartialEq, Message)]
pub struct PositionDelta {
    #[prost(uint32, tag = "1")]
    pub dx: u32,
    #[prost(uint32, tag = "2")]
    pub dy: u32,
    #[prost(uint32, tag = "3")]
    pub dz: u32,
    #[prost(uint32, tag = "4")]
    pub entity_id: u32,
    #[prost(uint32, tag = "5")]
    pub sequence: u32,
}

const POS_MIN: f32 = -100.0;
const POS_MAX: f32 = 100.0;
const POS_BITS: u32 = 16;

impl PositionDelta {
    pub fn set_dx(&mut self, v: f32) {
        self.dx = crate::quantize::encode(v, POS_MIN, POS_MAX, POS_BITS);
    }
    pub fn dx_f(&self) -> f32 {
        crate::quantize::decode(self.dx, POS_MIN, POS_MAX, POS_BITS)
    }
    pub fn set_dy(&mut self, v: f32) {
        self.dy = crate::quantize::encode(v, POS_MIN, POS_MAX, POS_BITS);
    }
    pub fn dy_f(&self) -> f32 {
        crate::quantize::decode(self.dy, POS_MIN, POS_MAX, POS_BITS)
    }
    pub fn set_dz(&mut self, v: f32) {
        self.dz = crate::quantize::encode(v, POS_MIN, POS_MAX, POS_BITS);
    }
    pub fn dz_f(&self) -> f32 {
        crate::quantize::decode(self.dz, POS_MIN, POS_MAX, POS_BITS)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Exact protobuf wire bytes — the production-real gate. proto3 omits
    /// default(0) fields; field N varint tag = (N<<3)|0.
    #[test]
    fn position_delta_wire_bytes() {
        let m = PositionDelta {
            dx: 1,
            dy: 0,
            dz: 0,
            entity_id: 0,
            sequence: 300,
        };
        // dx=1 -> tag (1<<3)=0x08, varint 1 -> 0x01
        // dy/dz/entity_id = 0 -> omitted (proto3)
        // sequence=300 -> tag (5<<3)=0x28, varint 300 -> 0xAC 0x02
        assert_eq!(m.encode_to_vec(), vec![0x08, 0x01, 0x28, 0xAC, 0x02]);
    }

    #[test]
    fn quantized_field_roundtrips_on_wire() {
        let mut m = PositionDelta::default();
        m.set_dx(0.0); // t=0.5 -> 0.5*65535=32767.5 -> banker's round -> 32768
        assert_eq!(m.dx, 32768);
        let bytes = m.encode_to_vec();
        let back = PositionDelta::decode(&bytes[..]).unwrap();
        assert_eq!(back.dx, 32768);
        assert!(back.dx_f().abs() < 0.01);
    }

    // Byte-parity on prost-GENERATED messages from the real .proto catalog.
    #[test]
    fn teleport_request_wire_bytes() {
        use crate::decentraland::pulse::TeleportRequest;
        let m = TeleportRequest {
            parcel_index: 1,
            position: None,
            realm: "x".into(),
        };
        // field1 int32 1 -> 08 01 ; field2 (position) None omitted ; field3 string "x" -> 1A 01 78
        assert_eq!(m.encode_to_vec(), vec![0x08, 0x01, 0x1A, 0x01, 0x78]);
    }

    #[test]
    fn handshake_request_wire_bytes() {
        use crate::decentraland::pulse::HandshakeRequest;
        let m = HandshakeRequest {
            auth_chain: vec![0xAB],
            profile_version: 5,
            initial_state: None,
        };
        // field1 bytes -> 0A 01 AB ; field2 int32 5 -> 10 05 ; field3 None omitted
        assert_eq!(m.encode_to_vec(), vec![0x0A, 0x01, 0xAB, 0x10, 0x05]);
    }

    // ---- Quantized accessors on the generated PlayerStateDeltaTier0 ----

    #[test]
    fn delta_quantized_accessors_set_presence_and_roundtrip() {
        let mut d = PlayerStateDeltaTier0::default();
        // Unset quantized field reads as None (proto3 presence preserved).
        assert_eq!(d.position_x_f(), None);

        // position_x: [0,16]@8 -> steps=255. 8.0 -> t=0.5 -> 0.5*255=127.5 ->
        // banker's round -> 128.
        d.set_position_x_f(8.0);
        assert_eq!(d.position_x, Some(128));
        assert!((d.position_x_f().unwrap() - 8.031373).abs() < 1e-4);

        // The field is now present on the wire.
        let bytes = d.encode_to_vec();
        let back = PlayerStateDeltaTier0::decode(&bytes[..]).unwrap();
        assert_eq!(back.position_x, Some(128));
    }

    #[test]
    fn delta_signed_range_velocity_quantizes_at_midpoint() {
        let mut d = PlayerStateDeltaTier0::default();
        // velocity_x: [-50,50]@8 -> steps=255. 0.0 -> t=0.5 -> 127.5 -> 128.
        d.set_velocity_x_f(0.0);
        assert_eq!(d.velocity_x, Some(128));
        // endpoints are exact.
        d.set_velocity_x_f(-50.0);
        assert_eq!(d.velocity_x, Some(0));
        d.set_velocity_x_f(50.0);
        assert_eq!(d.velocity_x, Some(255));
    }

    #[test]
    fn head_pitch_spec_matches_upstream_bitwise() {
        // PulseServer.Bitwise.cs HeadPitchQuantized: Quantize.Decode(.., 0.0f, 180.0f, 6).
        assert_eq!(spec::HEAD_PITCH, QuantSpec::new(0.0, 180.0, 6));
        // ...and is distinct from head_yaw (0..360 @ 7), which is the bug we fixed.
        assert_ne!(spec::HEAD_PITCH, spec::HEAD_YAW);
        let mut d = PlayerStateDeltaTier0::default();
        // [0,180]@6 -> steps=63. 180.0 -> max code 63; 90.0 -> t=0.5 -> 31.5 -> 32.
        d.set_head_pitch_f(180.0);
        assert_eq!(d.head_pitch, Some(63));
        d.set_head_pitch_f(0.0);
        assert_eq!(d.head_pitch, Some(0));
    }

    #[test]
    fn delta_point_at_17bit_endpoints() {
        let mut d = PlayerStateDeltaTier0::default();
        // point_at_x: [-3000,3000]@17 -> steps=131071.
        d.set_point_at_x_f(-3000.0);
        assert_eq!(d.point_at_x, Some(0));
        d.set_point_at_x_f(3000.0);
        assert_eq!(d.point_at_x, Some(131071));
    }

    #[test]
    fn delta_full_wire_bytes_with_quantized_field() {
        // subject_id=1 (tag 0x08), new_seq=0 omitted, position_x quantized.
        let mut d = PlayerStateDeltaTier0 {
            subject_id: 1,
            ..Default::default()
        };
        d.set_slide_blend_f(1.0); // [0,1]@4 -> steps=15 -> 1.0*15 = 15
        assert_eq!(d.slide_blend, Some(15));
        // field 1 subject_id=1 -> 08 01 ; field 14 slide_blend=15 -> tag (14<<3)=0x70, varint 15 -> 0x0F
        assert_eq!(d.encode_to_vec(), vec![0x08, 0x01, 0x70, 0x0F]);
    }
}
