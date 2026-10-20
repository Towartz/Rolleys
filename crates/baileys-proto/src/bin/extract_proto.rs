use std::path::Path;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("[*] Pulling latest WAProto directly from live WhatsApp Web...");
    let dest = Path::new("crates/baileys-proto/proto/WAProto.proto");
    let summary = baileys_proto::extractor::pull_latest_waproto(Some(dest))
        .await
        .map_err(|e| format!("Extraction failed: {}", e))?;
    println!("[+] Successfully extracted schema!");
    println!("    Revision       : {}", summary.client_revision);
    println!("    Total entities : {}", summary.total_entities);
    println!("    Root entities  : {}", summary.root_entities);
    println!("    Schema size    : {} bytes", summary.schema_bytes);
    println!("    Output target  : {:?}", summary.output_file);
    Ok(())
}
