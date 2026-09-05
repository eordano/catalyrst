use std::io::Result;

fn main() -> Result<()> {
    println!("cargo:rerun-if-changed=src/rpc_protocol/index.proto");
    let fds = protox::compile(["src/rpc_protocol/index.proto"], ["src"]).map_err(protox_error)?;
    prost_build::Config::new().compile_fds(fds)
}

fn protox_error(e: protox::Error) -> std::io::Error {
    std::io::Error::other(format!("{e:?}"))
}
