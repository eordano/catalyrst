// Generate Rust for the Pulse protobuf catalog from the vendored .proto files
// (copied from decentraland/protocol origin/quantization). The wire is plain
// proto3, so prost output is byte-identical to the upstream Google.Protobuf C#.
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
