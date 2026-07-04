fn main() {
    println!("cargo:rerun-if-changed=proto");
    prost_build::compile_protos(
        &[
            "proto/decentraland/pulse/pulse_client.proto",
            "proto/decentraland/pulse/pulse_server.proto",
        ],
        &["proto"],
    )
    .expect("prost-build failed to compile pulse protos");
}
