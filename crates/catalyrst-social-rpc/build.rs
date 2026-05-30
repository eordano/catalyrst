use std::io::Result;

fn main() -> Result<()> {
    println!("cargo:rerun-if-changed=proto");

    let proto_files = [
        "proto/decentraland/social_service/social_service_v2.proto",
        "proto/decentraland/social_service/errors.proto",
        "proto/decentraland/common/colors.proto",
    ];

    let mut config = prost_build::Config::new();
    config.service_generator(Box::new(dcl_rpc::codegen::RPCServiceGenerator::new()));

    config.compile_protos(&proto_files, &["proto"])?;

    Ok(())
}
