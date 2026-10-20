//! WAProto schema inspection & management commands for REPL.

use std::sync::Arc;
use baileys_store::SqliteStore;
use baileys_proto::schema::SchemaInspector;
use prost::Message as ProstMessage;

async fn handle_proto_command(sub: &str, store: &Arc<SqliteStore>) {
    let parts: Vec<&str> = sub.split_whitespace().collect();
    if parts.is_empty() {
        print_proto_help();
        return;
    }

    match parts[0].to_lowercase().as_str() {
        "list" => {
            println!("\n=== Recognized High-Level Message Types (Rose-Baileys Intelligence) ===");
            let canonical = RecognizedMessageType::all_canonical();
            for (i, t) in canonical.iter().enumerate() {
                println!("  [{:>2}] {:<24} - {}", i + 1, t.as_str(), t.display_name());
            }

            let stats = SchemaInspector::global().stats();
            println!("\n=== WAProto.proto Schema In-Memory Index ===");
            println!("  Total Messages : {}", stats.total_messages);
            println!("  Total Enums    : {}", stats.total_enums);
            println!("  Total Fields   : {}", stats.total_fields);
            println!("  Schema Size    : {} KB\n", stats.schema_bytes / 1024);
        }

        "search" => {
            if parts.len() < 2 {
                println!("[-] Usage: proto search <query>");
                println!("    Example: proto search poll");
                println!("    Example: proto search interactive");
                return;
            }
            let query = parts[1..].join(" ");
            println!("[*] Searching WAProto schema for '{}'...", query);
            let matches = SchemaInspector::global().search(&query);
            if matches.is_empty() {
                println!(
                    "[-] No messages, enums, or fields found matching '{}'.",
                    query
                );
                return;
            }

            println!("\nFound {} match(es) in schema:", matches.len());
            for m in matches.iter().take(40) {
                match &m.item_type {
                    baileys_proto::schema::SchemaItemType::Message => {
                        println!("  [MSG]  {:<32} (Message Definition)", m.name);
                    }
                    baileys_proto::schema::SchemaItemType::Enum => {
                        println!("  [ENUM] {:<32} (Enum Definition)", m.name);
                    }
                    baileys_proto::schema::SchemaItemType::Field {
                        parent_message,
                        field_type,
                        field_number,
                    } => {
                        println!(
                            "  [FLD]  {}.{:<20} -> {:<15} #{}",
                            parent_message, m.name, field_type, field_number
                        );
                    }
                    baileys_proto::schema::SchemaItemType::EnumValue {
                        parent_enum,
                        number,
                    } => {
                        println!("  [VAL]  {}.{:<20} = {}", parent_enum, m.name, number);
                    }
                }
            }
            if matches.len() > 40 {
                println!(
                    "  ... and {} more matches truncated (be more specific).",
                    matches.len() - 40
                );
            }
            println!();
        }

        "view" => {
            if parts.len() < 2 {
                println!("[-] Usage: proto view <MessageType | EnumType>");
                println!("    Example: proto view PollCreationMessage");
                println!("    Example: proto view ReactionMessage");
                return;
            }
            let type_name = parts[1];
            if let Some(def) = SchemaInspector::global().get_definition(type_name) {
                println!("\n=== WAProto Definition: {} ===\n{}\n", type_name, def);
            } else {
                println!(
                    "[-] Type '{}' not found in WAProto.proto schema.",
                    type_name
                );
                println!(
                    "    Tip: Use 'proto search {}' to find related symbols.",
                    type_name
                );
            }
        }

        "inspect" => {
            if parts.len() < 2 {
                println!("[-] Usage: proto inspect <id | last>");
                println!("    Example: proto inspect last");
                println!("    Example: proto inspect 3EB0C4...");
                return;
            }
            let target = parts[1];
            let msg_res = if target.eq_ignore_ascii_case("last") {
                store.get_latest_message().await
            } else {
                store.get_message(target).await
            };

            match msg_res {
                Ok(Some(stored)) => {
                    println!("\n=== Inspecting Message [{}] ===", stored.id);
                    println!("  Chat JID  : {}", stored.chat_jid);
                    println!("  Sender JID: {}", stored.sender_jid);
                    println!("  From Me   : {}", stored.from_me);
                    println!("  Timestamp : {}", stored.timestamp);
                    println!("  Summary   : {}", stored.summary);
                    println!("  Bytes Size: {} bytes", stored.proto_bytes.len());

                    if stored.proto_bytes.is_empty() {
                        println!("[-] Raw Protobuf bytes are empty for this message record.");
                        return;
                    }

                    match <baileys_proto::proto::Message as ProstMessage>::decode(
                        &stored.proto_bytes[..],
                    ) {
                        Ok(raw_proto) => {
                            let unwrap_res = raw_proto.unwrap_wrappers();
                            println!("\n--- Wrapper Unwrapping & Auto-Recognition ---");
                            println!(
                                "  Canonical Type : {} ({})",
                                unwrap_res.real_type.as_str(),
                                unwrap_res.real_type.display_name()
                            );
                            if unwrap_res.wrappers.is_empty() {
                                println!("  Wrapper Chain  : None (Direct message payload)");
                            } else {
                                println!("  Wrapper Chain  : {}", unwrap_res.wrappers.join(" -> "));
                            }

                            if let Some(text) = raw_proto.extract_text() {
                                println!("  Extracted Text : \"{}\"", text);
                            }
                            if let Some(media) = raw_proto.extract_media_meta() {
                                println!(
                                    "  Media Metadata : Mime='{}', Size={} bytes, DirectPath='{}'",
                                    media.mimetype.as_deref().unwrap_or("none"),
                                    media.file_length,
                                    media.direct_path.as_deref().unwrap_or("none")
                                );
                            }
                            if let Some(quoted) = raw_proto.extract_quoted_message() {
                                println!(
                                    "  Quoted Message : Recognized as {:?}",
                                    quoted.recognize_type()
                                );
                            }

                            println!("\n--- Decoded JSON Structure ---");
                            let json_val = raw_proto.to_json_value();
                            match serde_json::to_string_pretty(&json_val) {
                                Ok(pretty) => println!("{}", pretty),
                                Err(e) => println!("[-] JSON format error: {}", e),
                            }
                            println!();
                        }
                        Err(e) => {
                            println!("[-] Failed to decode protobuf bytes into Message: {}", e);
                        }
                    }
                }
                Ok(None) => {
                    if target.eq_ignore_ascii_case("last") {
                        println!("[-] No messages found in the local SQLite database.");
                    } else {
                        println!(
                            "[-] Message with ID '{}' not found in SQLite store.",
                            target
                        );
                    }
                }
                Err(e) => {
                    println!("[-] Database error querying message: {}", e);
                }
            }
        }

        "pull" => {
            println!("[*] Pulling latest WAProto directly from live WhatsApp Web (Pure Rust AST Engine)...");
            let default_target = std::path::Path::new("crates/baileys-proto/proto/WAProto.proto");
            let target_path = if default_target.exists() {
                Some(default_target)
            } else {
                None
            };
            let start = std::time::Instant::now();
            match baileys_proto::extractor::pull_latest_waproto(target_path).await {
                Ok(res) => {
                    let dur = start.elapsed();
                    println!(
                        "[+] Successfully extracted WAProto schema in pure Rust! ({:.2?})",
                        dur
                    );
                    println!("    WA Revision   : {}", res.client_revision);
                    println!("    Root entities : {}", res.root_entities);
                    println!("    Total fields  : {}", res.total_entities);
                    println!("    Schema size   : {} KB", res.schema_bytes / 1024);
                    println!(
                        "    Output target : {}",
                        res.output_file
                            .as_deref()
                            .unwrap_or("In-Memory Schema (Verified)")
                    );
                    if res.output_file.is_some() {
                        println!(
                            "    To rebuild Rust proto bindings: cargo build -p baileys-proto"
                        );
                    }
                }
                Err(e) => {
                    eprintln!("[-] Schema extraction failed: {}", e);
                }
            }
        }

        _ => {
            println!("[-] Unknown proto subcommand: '{}'", parts[0]);
            print_proto_help();
        }
    }
}

fn print_proto_help() {
    println!("\n=== WAProto Intelligence Suite Commands ===");
    println!("   proto list                   List all 28 recognized types & schema statistics");
    println!("   proto search <query>         Search messages, enums, and fields in WAProto");
    println!("   proto view <TypeName>        Display schema definition for a message or enum");
    println!("   proto inspect <id | last>    Decode message, unwrap wrappers & show pretty JSON");
    println!(
        "   proto pull                   Pull fresh WAProto from live WhatsApp Web JS bundle\n"
    );
}


