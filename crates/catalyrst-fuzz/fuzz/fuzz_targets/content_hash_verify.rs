#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if data.len() < 2 {
        let _ = catalyrst_hashing::hash_bytes(data);
        let _ = catalyrst_hashing::hash_bytes_v1(data);
        return;
    }
    let split = (data[0] as usize) % data.len();
    let (head, body) = data.split_at(split);
    let claimed = std::str::from_utf8(head).unwrap_or("");
    let _ = catalyrst_hashing::verify_hash(body, claimed);

    let _ = catalyrst_hashing::hash_bytes(body);
    let _ = catalyrst_hashing::hash_bytes_v1(body);
});
