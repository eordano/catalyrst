use prost::Message;

use crate::decentraland::pulse::{PlayerState, PlayerStateDeltaTier0, TeleportRequest};
use crate::quantize;

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

    pub fn encode(&self, value: f32) -> u32 {
        quantize::encode(value, self.min, self.max, self.bits)
    }

    pub fn decode(&self, encoded: u32) -> f32 {
        quantize::decode(encoded, self.min, self.max, self.bits)
    }

    pub const fn max_code(&self) -> u32 {
        (1u32 << self.bits) - 1
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PowerQuantSpec {
    pub max: f32,
    pub pow: f32,
    pub bits: u32,
}

impl PowerQuantSpec {
    pub const fn new(max: f32, pow: f32, bits: u32) -> Self {
        Self { max, pow, bits }
    }

    pub fn encode(&self, value: f32) -> u32 {
        quantize::encode_power(value, self.max, self.pow, self.bits)
    }

    pub fn decode(&self, encoded: u32) -> f32 {
        quantize::decode_power(encoded, self.max, self.pow, self.bits)
    }

    pub const fn max_code(&self) -> u32 {
        (1u32 << self.bits) - 1
    }
}

pub mod spec {
    use super::{PowerQuantSpec, QuantSpec};

    pub const POSITION_X: QuantSpec = QuantSpec::new(0.0, 16.0, 8);
    pub const POSITION_Y: QuantSpec = QuantSpec::new(0.0, 200.0, 13);
    pub const POSITION_Z: QuantSpec = QuantSpec::new(0.0, 16.0, 8);
    pub const VELOCITY_X: PowerQuantSpec = PowerQuantSpec::new(50.0, 2.0, 8);
    pub const VELOCITY_Y: PowerQuantSpec = PowerQuantSpec::new(50.0, 2.0, 8);
    pub const VELOCITY_Z: PowerQuantSpec = PowerQuantSpec::new(50.0, 2.0, 8);
    pub const ROTATION_Y: QuantSpec = QuantSpec::new(0.0, 360.0, 7);
    pub const MOVEMENT_BLEND: QuantSpec = QuantSpec::new(0.0, 3.0, 5);
    pub const SLIDE_BLEND: QuantSpec = QuantSpec::new(0.0, 1.0, 4);
    pub const HEAD_YAW: QuantSpec = QuantSpec::new(0.0, 360.0, 7);
    pub const HEAD_PITCH: QuantSpec = QuantSpec::new(0.0, 360.0, 7);
    pub const POINT_AT_X: QuantSpec = QuantSpec::new(-3000.0, 3000.0, 17);
    pub const POINT_AT_Y: QuantSpec = QuantSpec::new(0.0, 200.0, 7);
    pub const POINT_AT_Z: QuantSpec = QuantSpec::new(-3000.0, 3000.0, 17);
}

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

macro_rules! quantized_plain_accessor {
    ($field:ident, $set:ident, $get:ident, $spec:expr) => {
        #[doc = concat!("Set `", stringify!($field), "` from a float, quantizing per its proto spec.")]
        pub fn $set(&mut self, value: f32) {
            self.$field = $spec.encode(value);
        }
        #[doc = concat!("Read `", stringify!($field), "` as a float.")]
        pub fn $get(&self) -> f32 {
            $spec.decode(self.$field)
        }
    };
}

impl PlayerState {
    quantized_plain_accessor!(position_x, set_position_x_f, position_x_f, spec::POSITION_X);
    quantized_plain_accessor!(position_y, set_position_y_f, position_y_f, spec::POSITION_Y);
    quantized_plain_accessor!(position_z, set_position_z_f, position_z_f, spec::POSITION_Z);
    quantized_plain_accessor!(velocity_x, set_velocity_x_f, velocity_x_f, spec::VELOCITY_X);
    quantized_plain_accessor!(velocity_y, set_velocity_y_f, velocity_y_f, spec::VELOCITY_Y);
    quantized_plain_accessor!(velocity_z, set_velocity_z_f, velocity_z_f, spec::VELOCITY_Z);
    quantized_plain_accessor!(rotation_y, set_rotation_y_f, rotation_y_f, spec::ROTATION_Y);
    quantized_plain_accessor!(
        movement_blend,
        set_movement_blend_f,
        movement_blend_f,
        spec::MOVEMENT_BLEND
    );
    quantized_plain_accessor!(
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

    // The server relays raw codes verbatim, so an out-of-range code must be rejected here.
    pub fn are_quantized_fields_in_range(&self) -> bool {
        self.position_x <= spec::POSITION_X.max_code()
            && self.position_y <= spec::POSITION_Y.max_code()
            && self.position_z <= spec::POSITION_Z.max_code()
            && self.velocity_x <= spec::VELOCITY_X.max_code()
            && self.velocity_y <= spec::VELOCITY_Y.max_code()
            && self.velocity_z <= spec::VELOCITY_Z.max_code()
            && self.rotation_y <= spec::ROTATION_Y.max_code()
            && self.movement_blend <= spec::MOVEMENT_BLEND.max_code()
            && self.slide_blend <= spec::SLIDE_BLEND.max_code()
            && self.head_yaw.is_none_or(|v| v <= spec::HEAD_YAW.max_code())
            && self
                .head_pitch
                .is_none_or(|v| v <= spec::HEAD_PITCH.max_code())
            && self
                .point_at_x
                .is_none_or(|v| v <= spec::POINT_AT_X.max_code())
            && self
                .point_at_y
                .is_none_or(|v| v <= spec::POINT_AT_Y.max_code())
            && self
                .point_at_z
                .is_none_or(|v| v <= spec::POINT_AT_Z.max_code())
    }
}

impl TeleportRequest {
    quantized_plain_accessor!(position_x, set_position_x_f, position_x_f, spec::POSITION_X);
    quantized_plain_accessor!(position_y, set_position_y_f, position_y_f, spec::POSITION_Y);
    quantized_plain_accessor!(position_z, set_position_z_f, position_z_f, spec::POSITION_Z);

    pub fn are_quantized_fields_in_range(&self) -> bool {
        self.position_x <= spec::POSITION_X.max_code()
            && self.position_y <= spec::POSITION_Y.max_code()
            && self.position_z <= spec::POSITION_Z.max_code()
    }
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

    #[test]
    fn position_delta_wire_bytes() {
        let m = PositionDelta {
            dx: 1,
            dy: 0,
            dz: 0,
            entity_id: 0,
            sequence: 300,
        };

        assert_eq!(m.encode_to_vec(), vec![0x08, 0x01, 0x28, 0xAC, 0x02]);
    }

    #[test]
    fn quantized_field_roundtrips_on_wire() {
        let mut m = PositionDelta::default();
        m.set_dx(0.0);
        assert_eq!(m.dx, 32768);
        let bytes = m.encode_to_vec();
        let back = PositionDelta::decode(&bytes[..]).unwrap();
        assert_eq!(back.dx, 32768);
        assert!(back.dx_f().abs() < 0.01);
    }

    #[test]
    fn teleport_request_wire_bytes() {
        let m = TeleportRequest {
            parcel_index: 1,
            position_x: 128,
            position_y: 0,
            position_z: 3,
            realm: "x".into(),
        };

        assert_eq!(
            m.encode_to_vec(),
            vec![0x08, 0x01, 0x10, 0x80, 0x01, 0x20, 0x03, 0x2A, 0x01, 0x78]
        );
    }

    #[test]
    fn teleport_request_new_shape_roundtrips() {
        let mut m = TeleportRequest {
            parcel_index: 7,
            realm: "realm-a".into(),
            ..Default::default()
        };
        m.set_position_x_f(8.0);
        m.set_position_y_f(100.0);
        m.set_position_z_f(15.0);
        assert!(m.are_quantized_fields_in_range());

        let back = TeleportRequest::decode(&m.encode_to_vec()[..]).unwrap();
        assert_eq!(back, m);
        assert_eq!(back.realm, "realm-a");
        assert!((back.position_x_f() - 8.0).abs() < 0.04);
        assert!((back.position_y_f() - 100.0).abs() < 0.02);
        assert!((back.position_z_f() - 15.0).abs() < 0.04);
    }

    #[test]
    fn handshake_request_wire_bytes() {
        use crate::decentraland::pulse::HandshakeRequest;
        let m = HandshakeRequest {
            auth_chain: vec![0xAB],
            profile_version: 5,
            initial_state: None,
            protocol_features: 0,
        };

        assert_eq!(m.encode_to_vec(), vec![0x0A, 0x01, 0xAB, 0x10, 0x05]);
    }

    #[test]
    fn delta_quantized_accessors_set_presence_and_roundtrip() {
        let mut d = PlayerStateDeltaTier0::default();

        assert_eq!(d.position_x_f(), None);

        d.set_position_x_f(8.0);
        assert_eq!(d.position_x, Some(128));
        assert!((d.position_x_f().unwrap() - 8.031373).abs() < 1e-4);

        let bytes = d.encode_to_vec();
        let back = PlayerStateDeltaTier0::decode(&bytes[..]).unwrap();
        assert_eq!(back.position_x, Some(128));
    }

    #[test]
    fn delta_power_velocity_zero_is_code_zero_and_endpoints_split_by_sign() {
        let mut d = PlayerStateDeltaTier0::default();

        d.set_velocity_x_f(0.0);
        assert_eq!(d.velocity_x, Some(0));

        d.set_velocity_x_f(-50.0);
        assert_eq!(d.velocity_x, Some(255));
        d.set_velocity_x_f(50.0);
        assert_eq!(d.velocity_x, Some(254));
    }

    #[test]
    fn power_velocity_small_negative_magnitude_stays_single_varint_byte() {
        let mut s = PlayerState::default();
        s.set_velocity_x_f(-0.05);
        let code = s.velocity_x;
        assert_eq!(code, 9, "small magnitude, sign in LSB");
        assert!(code < 0x80, "single varint byte regardless of sign");
        assert_eq!(s.encode_to_vec(), vec![0x28, 0x09]);
    }

    #[test]
    fn head_pitch_spec_matches_upstream_bitwise() {
        assert_eq!(spec::HEAD_PITCH, QuantSpec::new(0.0, 360.0, 7));
        assert_eq!(spec::HEAD_PITCH, spec::HEAD_YAW);
        let mut d = PlayerStateDeltaTier0::default();

        d.set_head_pitch_f(360.0);
        assert_eq!(d.head_pitch, Some(127));
        d.set_head_pitch_f(0.0);
        assert_eq!(d.head_pitch, Some(0));
    }

    #[test]
    fn player_state_quantized_accessors_match_delta_grid() {
        let mut s = PlayerState::default();
        s.set_position_x_f(8.0);
        assert_eq!(s.position_x, 128);
        s.set_rotation_y_f(180.0);
        assert_eq!(s.rotation_y, 64);
        s.set_head_yaw_f(360.0);
        assert_eq!(s.head_yaw, Some(127));

        let mut d = PlayerStateDeltaTier0::default();
        d.set_position_x_f(8.0);
        assert_eq!(d.position_x, Some(s.position_x));
    }

    #[test]
    fn delta_point_at_17bit_endpoints() {
        let mut d = PlayerStateDeltaTier0::default();

        d.set_point_at_x_f(-3000.0);
        assert_eq!(d.point_at_x, Some(0));
        d.set_point_at_x_f(3000.0);
        assert_eq!(d.point_at_x, Some(131071));
    }

    #[test]
    fn delta_full_wire_bytes_with_quantized_field() {
        let mut d = PlayerStateDeltaTier0 {
            subject_id: 1,
            ..Default::default()
        };
        d.set_slide_blend_f(1.0);
        assert_eq!(d.slide_blend, Some(15));

        assert_eq!(d.encode_to_vec(), vec![0x08, 0x01, 0x70, 0x0F]);
    }
}
