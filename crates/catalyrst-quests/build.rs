use std::io::Result;

fn main() -> Result<()> {
    println!("cargo:rerun-if-changed=proto");

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
