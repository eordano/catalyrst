use std::io::Result;

fn main() -> Result<()> {
    println!("cargo:rerun-if-changed=proto");

    let proto_files = [
        "proto/decentraland/kernel/comms/v3/archipelago.proto",
        "proto/decentraland/kernel/comms/v4/archipelago.proto",
        "proto/decentraland/common/auth.proto",
        "proto/decentraland/common/vectors.proto",
    ];

    let fds = protox::compile(proto_files, ["proto"]).map_err(protox_error)?;
    let mut config = prost_build::Config::new();
    config.btree_map([
        ".decentraland.kernel.comms.v4.Authenticate.resume",
        ".decentraland.kernel.comms.v4.Assignment.peers",
    ]);
    config.compile_fds(fds)
}

fn protox_error(e: protox::Error) -> std::io::Error {
    std::io::Error::other(format!("{e:?}"))
}
