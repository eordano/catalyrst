fn main() {
    println!("cargo:rerun-if-changed=proto");
    let fds = protox::compile(
        [
            "proto/decentraland/pulse/pulse_client.proto",
            "proto/decentraland/pulse/pulse_server.proto",
            // No pulse proto references Vector3 anymore, but the server still uses it
            // internally (global positions for AOI).
            "proto/decentraland/common/vectors.proto",
        ],
        ["proto"],
    )
    .unwrap_or_else(|e| panic!("protox failed to compile pulse protos: {e:?}"));
    prost_build::Config::new()
        .compile_fds(fds)
        .expect("prost-build failed to compile pulse protos");
}
