fn main() {
    println!("cargo:rerun-if-changed=proto");
    prost_build::compile_protos(
        &[
            "proto/decentraland/pulse/pulse_client.proto",
            "proto/decentraland/pulse/pulse_server.proto",
            // No pulse proto references Vector3 anymore, but the server still uses it
            // internally (global positions for AOI).
            "proto/decentraland/common/vectors.proto",
        ],
        &["proto"],
    )
    .expect("prost-build failed to compile pulse protos");
}
