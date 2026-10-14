use std::env;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // If PROTOC is already specified in the environment (e.g. cross-compilation or custom toolchains),
    // respect it. Otherwise, fall back to protoc-bin-vendored.
    if env::var("PROTOC").is_err() {
        if let Ok(protoc_path) = protoc_bin_vendored::protoc_bin_path() {
            env::set_var("PROTOC", protoc_path);
        }
    }

    println!("cargo:rerun-if-changed=proto/WAProto.proto");

    let mut config = prost_build::Config::new();
    config.type_attribute(".", "#[derive(serde::Serialize, serde::Deserialize)]");
    config.compile_protos(&["proto/WAProto.proto"], &["proto/"])?;

    Ok(())
}
