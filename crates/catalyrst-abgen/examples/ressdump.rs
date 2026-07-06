use abgen::unity::bundle_file::{Bundle, FileContent};
use sha2::{Digest, Sha256};

fn main() {
    for path in std::env::args().skip(1) {
        let data = match std::fs::read(&path) {
            Ok(d) => d,
            Err(e) => {
                println!("{path}\tread-error\t{e}");
                continue;
            }
        };
        let bundle = match Bundle::load_bytes(&data) {
            Ok(b) => b,
            Err(e) => {
                println!("{path}\tparse-error\t{e:#}");
                continue;
            }
        };
        for f in &bundle.files {
            if let FileContent::Raw(bytes) = &f.content {
                if f.name.to_lowercase().ends_with(".ress") {
                    let hex: String = Sha256::digest(bytes)
                        .iter()
                        .map(|b| format!("{b:02x}"))
                        .collect();
                    println!("{path}\tresS\t{}\t{hex}", bytes.len());
                }
            }
        }
    }
}
