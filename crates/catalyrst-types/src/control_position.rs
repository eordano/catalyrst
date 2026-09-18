pub const SUBJECT: &str = "archipelago.control.position.v1";
pub const MAX_BYTES: usize = 640;

pub fn valid_realm(realm: &str) -> bool {
    !realm.is_empty() && realm.len() <= 256 && !realm.chars().any(char::is_control)
}

#[derive(Clone, Debug, PartialEq)]
pub struct ControlPosition {
    pub audience: String,
    pub wallet: String,
    pub session: String,
    pub epoch: u64,
    pub sequence: u64,
    pub realm: String,
    pub position: Option<[f32; 3]>,
}

impl ControlPosition {
    pub fn encode(&self) -> Option<Vec<u8>> {
        let wallet = crate::decode_hex_0x(&self.wallet).ok()?;
        let session = crate::decode_hex_0x(&self.session).ok()?;
        if wallet.len() != 20
            || session.len() != 20
            || self.epoch == 0
            || self.sequence == 0
            || self.audience.is_empty()
            || self.audience.len() > 256
            || self.realm.len() > 256
            || self.position.is_some_and(|position| {
                !valid_realm(&self.realm) || position.iter().any(|v| !v.is_finite())
            })
        {
            return None;
        }
        let mut out = Vec::with_capacity(MAX_BYTES);
        out.push(u8::from(self.position.is_some()));
        out.extend(wallet);
        out.extend(session);
        out.extend(self.epoch.to_le_bytes());
        out.extend(self.sequence.to_le_bytes());
        for value in [&self.audience, &self.realm] {
            out.extend((value.len() as u16).to_le_bytes());
            out.extend(value.as_bytes());
        }
        if let Some(position) = self.position {
            for value in position {
                out.extend(value.to_le_bytes());
            }
        }
        Some(out)
    }

    pub fn decode(mut bytes: &[u8]) -> Option<Self> {
        if bytes.len() > MAX_BYTES {
            return None;
        }
        fn take<'a>(bytes: &mut &'a [u8], size: usize) -> Option<&'a [u8]> {
            let value = bytes.get(..size)?;
            *bytes = &bytes[size..];
            Some(value)
        }
        fn address(bytes: &[u8]) -> String {
            let mut out = String::from("0x");
            for value in bytes {
                use std::fmt::Write;
                write!(out, "{value:02x}").unwrap();
            }
            out
        }
        fn string(bytes: &mut &[u8]) -> Option<String> {
            let size = u16::from_le_bytes(take(bytes, 2)?.try_into().ok()?) as usize;
            if size > 256 {
                return None;
            }
            std::str::from_utf8(take(bytes, size)?)
                .ok()
                .map(str::to_owned)
        }
        let present = take(&mut bytes, 1)?[0];
        if present > 1 {
            return None;
        }
        let wallet = address(take(&mut bytes, 20)?);
        let session = address(take(&mut bytes, 20)?);
        let epoch = u64::from_le_bytes(take(&mut bytes, 8)?.try_into().ok()?);
        let sequence = u64::from_le_bytes(take(&mut bytes, 8)?.try_into().ok()?);
        let audience = string(&mut bytes)?;
        let realm = string(&mut bytes)?;
        let position = if present == 1 {
            let mut values = [0.0; 3];
            for value in &mut values {
                *value = f32::from_le_bytes(take(&mut bytes, 4)?.try_into().ok()?);
            }
            Some(values)
        } else {
            None
        };
        if !bytes.is_empty() {
            return None;
        }
        let value = Self {
            audience,
            wallet,
            session,
            epoch,
            sequence,
            realm,
            position,
        };
        value.encode()?;
        Some(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn control_position_roundtrip_and_closed_frame_validation() {
        let mut value = ControlPosition {
            audience: "realm.example".into(),
            wallet: format!("0x{}", "01".repeat(20)),
            session: format!("0x{}", "02".repeat(20)),
            epoch: 1,
            sequence: 2,
            realm: "realm".into(),
            position: Some([1.0, 2.0, -3.0]),
        };
        let bytes = value.encode().unwrap();
        assert_eq!(ControlPosition::decode(&bytes), Some(value.clone()));
        for end in 0..bytes.len() {
            assert!(ControlPosition::decode(&bytes[..end]).is_none());
        }
        let mut trailing = bytes.clone();
        trailing.push(0);
        assert!(ControlPosition::decode(&trailing).is_none());
        value.position = None;
        assert_eq!(
            ControlPosition::decode(&value.encode().unwrap()),
            Some(value.clone())
        );
        value.epoch = 0;
        assert!(value.encode().is_none());
        value.epoch = 1;
        value.position = Some([f32::NAN, 0.0, 0.0]);
        assert!(value.encode().is_none());
    }

    #[test]
    fn realms_are_bounded_nonempty_and_contain_no_control_characters() {
        for realm in ["", "room\nname", "room\0name", &"x".repeat(257)] {
            assert!(!valid_realm(realm));
        }
        for realm in ["main", "scene:plaza", &"x".repeat(256)] {
            assert!(valid_realm(realm));
        }
    }
}
