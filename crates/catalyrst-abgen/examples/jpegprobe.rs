use abgen::ffi;

fn main() {
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| format!("{}/testdata/gradient-16x16.jpg", env!("CARGO_MANIFEST_DIR")));
    println!(
        "TURBOJPEG_LIB={}",
        std::env::var("TURBOJPEG_LIB").unwrap_or_else(|_| "(unset)".into())
    );
    println!("turbojpeg_available: {}", ffi::turbojpeg_available());
    let jpeg = std::fs::read(&path).unwrap_or_else(|e| panic!("read {path}: {e}"));
    println!("fixture: {path} ({} bytes)", jpeg.len());

    match ffi::decode_jpeg_rgba_box(&jpeg) {
        Ok((rgba, w, h)) => {
            let sum: u64 = rgba.iter().map(|&b| b as u64).sum();
            println!(
                "libjpeg9c-first decode: OK {w}x{h} rgba_len={} byte_sum={sum}",
                rgba.len()
            );
        }
        Err(e) => println!("libjpeg9c-first decode: FAILED: {e:#}"),
    }
    match ffi::decode_jpeg_rgba(&jpeg) {
        Ok((rgba, w, h)) => {
            let sum: u64 = rgba.iter().map(|&b| b as u64).sum();
            println!(
                "turbojpeg decode: OK {w}x{h} rgba_len={} byte_sum={sum}",
                rgba.len()
            );
        }
        Err(e) => {
            println!("turbojpeg decode: FAILED: {e:#}");
            std::process::exit(1);
        }
    }
}
