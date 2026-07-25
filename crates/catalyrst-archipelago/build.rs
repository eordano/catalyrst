use std::io::Result;

fn main() -> Result<()> {
    println!("cargo:rerun-if-changed=proto");

    let proto_files = [
        "proto/decentraland/kernel/comms/v3/archipelago.proto",
        "proto/decentraland/common/vectors.proto",
    ];

    let mut config = prost_build::Config::new();
    config.compile_protos(&proto_files, &["proto"])?;

    Ok(())
}
