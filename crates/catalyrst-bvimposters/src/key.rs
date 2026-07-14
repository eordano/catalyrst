pub const MAX_LEVEL: u8 = 5;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TileKey {
    pub level: u8,
    pub x: i32,
    pub y: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ImposterKey {
    pub tile: TileKey,
    pub crc: u32,
}

impl TileKey {
    pub fn new(level: u8, x: i32, y: i32) -> Option<Self> {
        if level > MAX_LEVEL {
            return None;
        }
        if (x >> level) << level != x || (y >> level) << level != y {
            return None;
        }
        Some(Self { level, x, y })
    }

    pub fn label(&self) -> String {
        format!("{}/{},{}", self.level, self.x, self.y)
    }
}

impl ImposterKey {
    pub fn new(level: u8, x: i32, y: i32, crc: u32) -> Option<Self> {
        if crc == 0 {
            return None;
        }
        Some(Self {
            tile: TileKey::new(level, x, y)?,
            crc,
        })
    }

    pub fn zip_name(&self) -> String {
        format!("{},{}.{}.zip", self.tile.x, self.tile.y, self.crc)
    }

    pub fn spec_member_name(&self) -> String {
        format!("{},{}-spec.json", self.tile.x, self.tile.y)
    }
}

fn parse_coords_crc(stem: &str) -> Option<(i32, i32, u32)> {
    let (coords, crc_str) = stem.rsplit_once('.')?;
    let crc: u32 = crc_str.parse().ok()?;
    let (x_str, y_str) = coords.split_once(',')?;
    let x: i32 = x_str.parse().ok()?;
    let y: i32 = y_str.parse().ok()?;
    Some((x, y, crc))
}

pub fn parse_zip_request(level_seg: &str, file: &str) -> Option<ImposterKey> {
    let level: u8 = level_seg.parse().ok()?;
    let stem = file.strip_suffix(".zip")?;
    let (x, y, crc) = parse_coords_crc(stem)?;
    ImposterKey::new(level, x, y, crc)
}

pub fn parse_spec_request(level_seg: &str, file: &str) -> Option<ImposterKey> {
    let level: u8 = level_seg.parse().ok()?;
    let stem = file.strip_suffix("-spec.json")?;
    let (x, y, crc) = parse_coords_crc(stem)?;
    ImposterKey::new(level, x, y, crc)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_negative_coordinates() {
        let key = parse_zip_request("0", "-64,-128.123.zip").unwrap();
        assert_eq!(key.tile.x, -64);
        assert_eq!(key.tile.y, -128);
        assert_eq!(key.crc, 123);
    }

    #[test]
    fn parses_level_aligned_tiles() {
        assert!(parse_zip_request("3", "-64,-128.123.zip").is_some());
        assert!(parse_zip_request("5", "-32,96.1.zip").is_some());
    }

    #[test]
    fn rejects_unaligned_tiles() {
        assert!(parse_zip_request("1", "1,0.123.zip").is_none());
        assert!(parse_zip_request("2", "0,-2.123.zip").is_none());
        assert!(parse_zip_request("5", "-31,0.123.zip").is_none());
    }

    #[test]
    fn rejects_crc_zero() {
        assert!(parse_zip_request("0", "0,100.0.zip").is_none());
    }

    #[test]
    fn rejects_crc_out_of_bounds() {
        assert!(parse_zip_request("0", "0,100.4294967295.zip").is_some());
        assert!(parse_zip_request("0", "0,100.4294967296.zip").is_none());
        assert!(parse_zip_request("0", "0,100.-1.zip").is_none());
    }

    #[test]
    fn rejects_level_out_of_range() {
        assert!(parse_zip_request("6", "0,0.123.zip").is_none());
        assert!(parse_zip_request("x", "0,0.123.zip").is_none());
    }

    #[test]
    fn rejects_malformed_names() {
        assert!(parse_zip_request("0", "0,100.123").is_none());
        assert!(parse_zip_request("0", "0.100.123.zip").is_none());
        assert!(parse_zip_request("0", "a,b.123.zip").is_none());
    }

    #[test]
    fn parses_spec_request() {
        let key = parse_spec_request("0", "0,100.3504527830-spec.json").unwrap();
        assert_eq!(key.crc, 3504527830);
        assert_eq!(key.spec_member_name(), "0,100-spec.json");
    }

    #[test]
    fn tile_label_shape() {
        let tile = TileKey::new(2, -64, -128).unwrap();
        assert_eq!(tile.label(), "2/-64,-128");
    }
}
