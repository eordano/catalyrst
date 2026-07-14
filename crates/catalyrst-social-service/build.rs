use std::io::Result;

fn main() -> Result<()> {
    if std::env::var_os("CARGO_FEATURE_RPC").is_none() {
        return Ok(());
    }
    println!("cargo:rerun-if-changed=proto");

    let proto_files = [
        "proto/decentraland/social_service/social_service_v2.proto",
        "proto/decentraland/social_service/errors.proto",
        "proto/decentraland/common/colors.proto",
    ];

    let mut config = prost_build::Config::new();
    config.service_generator(Box::new(catalyrst_drpc::codegen::RPCServiceGenerator::new()));

    let fds = protox::compile(proto_files, ["proto"]).map_err(protox_error)?;
    config.compile_fds(fds)
}

fn protox_error(e: protox::Error) -> std::io::Error {
    std::io::Error::other(format!("{e:?}"))
}
