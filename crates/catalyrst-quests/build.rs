use std::io::Result;

fn main() -> Result<()> {
    println!("cargo:rerun-if-changed=proto");

    // Mirror decentraland/quests crates/protocol/build.rs:
    //   - serde Serialize/Deserialize on every message
    //   - `#[serde(rename_all = "camelCase")]` so the REST wire matches the
    //     protobuf-defined JSON shape (questId/creatorAddress/imageUrl/...)
    //   - the `definition` field is skipped when None so non-creator reads omit it
    // plus the dcl-rpc service generator for the QuestsService transport.
    let mut config = prost_build::Config::new();
    config.service_generator(Box::new(dcl_rpc::codegen::RPCServiceGenerator::new()));
    config
        .type_attribute(".", "#[derive(serde::Serialize, serde::Deserialize)]")
        .type_attribute(".", "#[serde(rename_all = \"camelCase\")]")
        .field_attribute(
            "definition",
            "#[serde(skip_serializing_if = \"Option::is_none\")]",
        )
        .compile_protos(&["proto/decentraland/quests/definitions.proto"], &["proto"])?;

    Ok(())
}
