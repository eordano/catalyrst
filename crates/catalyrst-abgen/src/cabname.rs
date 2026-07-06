use crate::pathids::spooky_short;

pub fn shader_bundle_cab(target: &str) -> &'static str {
    let t = if target.is_empty() { "linux" } else { target };
    match t.to_lowercase().as_str() {
        "windows" => "CAB-51fbd4c9d0fb3e603fd599ac9f5d01e1",
        "mac" | "osx" => "CAB-5ba4993b7ea166819a0af9aec5b25b8c",
        "webgl" => "CAB-8cd96fef5eb754106e0087d8609a2406",
        _ => "CAB-b11b6af2a97300dee9faa763fb3805f6",
    }
}

fn cab_hash(bundle_name: &str) -> String {
    let lower = bundle_name.to_ascii_lowercase();
    let (h1, h2) = spooky_short(lower.as_bytes(), 0, 0);
    let mut buf = [0u8; 16];
    buf[0..8].copy_from_slice(&h1.to_le_bytes());
    buf[8..16].copy_from_slice(&h2.to_le_bytes());
    let mut s = String::with_capacity(32);
    for b in buf {
        s.push(char::from_digit((b >> 4) as u32, 16).unwrap());
        s.push(char::from_digit((b & 0xf) as u32, 16).unwrap());
    }
    s
}

pub fn cab_name(bundle_name: &str) -> String {
    for plat in ["linux", "windows", "mac", "osx", "webgl"] {
        if bundle_name == format!("scene_ignore_{plat}") {
            return shader_bundle_cab(plat).to_string();
        }
    }
    format!("CAB-{}", cab_hash(bundle_name))
}
