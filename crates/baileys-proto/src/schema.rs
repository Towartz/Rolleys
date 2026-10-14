//! In-memory WAProto protobuf schema inspector and search engine.
//!
//! Parses and indexes `WAProto.proto` at compile time, enabling fast lookup,
//! fuzzy search across all message definitions, fields, numbers, and enums without
//! external runtime dependencies.

use std::collections::HashMap;
use std::sync::OnceLock;

pub const RAW_WAPROTO_SCHEMA: &str = include_str!("../proto/WAProto.proto");

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SchemaItemType {
    Message,
    Enum,
    Field {
        parent_message: String,
        field_type: String,
        field_number: u32,
    },
    EnumValue {
        parent_enum: String,
        number: i32,
    },
}

#[derive(Debug, Clone)]
pub struct SchemaSearchResult {
    pub name: String,
    pub item_type: SchemaItemType,
    pub snippet: String,
}

#[derive(Debug, Clone)]
pub struct SchemaStats {
    pub total_messages: usize,
    pub total_enums: usize,
    pub total_fields: usize,
    pub schema_bytes: usize,
}

pub struct SchemaInspector {
    messages: HashMap<String, String>,
    enums: HashMap<String, String>,
    field_index: Vec<(String, String, String, u32)>, // (field_name, field_type, parent_msg, number)
    enum_index: Vec<(String, String, i32)>,          // (value_name, parent_enum, number)
}

static INSTANCE: OnceLock<SchemaInspector> = OnceLock::new();

impl SchemaInspector {
    pub fn global() -> &'static Self {
        INSTANCE.get_or_init(Self::parse_embedded)
    }

    pub fn parse_embedded() -> Self {
        Self::parse(RAW_WAPROTO_SCHEMA)
    }

    pub fn parse(content: &str) -> Self {
        let mut messages = HashMap::new();
        let mut enums = HashMap::new();
        let mut field_index = Vec::new();
        let mut enum_index = Vec::new();

        let lines: Vec<&str> = content.lines().collect();

        // Stack to track nested containers: (name, is_enum, start_line, depth_at_start)
        let mut container_stack: Vec<(String, bool, usize, usize)> = Vec::new();
        let mut current_depth = 0;

        for (i, raw_line) in lines.iter().enumerate() {
            let line = raw_line.trim();
            if line.is_empty() || line.starts_with("//") {
                continue;
            }

            if line.starts_with("message ") && line.ends_with('{') {
                let raw_name = line
                    .trim_start_matches("message ")
                    .trim_end_matches('{')
                    .trim();
                let full_name = if let Some((parent, false, _, _)) = container_stack.last() {
                    format!("{}.{}", parent, raw_name)
                } else {
                    raw_name.to_string()
                };
                container_stack.push((full_name, false, i, current_depth));
                current_depth += line.chars().filter(|&c| c == '{').count();
                current_depth =
                    current_depth.saturating_sub(line.chars().filter(|&c| c == '}').count());
                continue;
            } else if line.starts_with("enum ") && line.ends_with('{') {
                let raw_name = line
                    .trim_start_matches("enum ")
                    .trim_end_matches('{')
                    .trim();
                let full_name = if let Some((parent, false, _, _)) = container_stack.last() {
                    format!("{}.{}", parent, raw_name)
                } else {
                    raw_name.to_string()
                };
                container_stack.push((full_name, true, i, current_depth));
                current_depth += line.chars().filter(|&c| c == '{').count();
                current_depth =
                    current_depth.saturating_sub(line.chars().filter(|&c| c == '}').count());
                continue;
            }

            let opens = line.chars().filter(|&c| c == '{').count();
            let closes = line.chars().filter(|&c| c == '}').count();
            current_depth += opens;

            // Check field or enum value
            if line.ends_with(';') {
                if let Some((curr_name, is_enum, _, _)) = container_stack.last() {
                    if *is_enum {
                        if let Some((v_name, v_num)) = Self::parse_enum_value(line) {
                            enum_index.push((v_name, curr_name.clone(), v_num));
                        }
                    } else if !line.starts_with("oneof ") {
                        if let Some((f_type, f_name, f_num)) = Self::parse_field_line(line) {
                            field_index.push((f_name, f_type, curr_name.clone(), f_num));
                        }
                    }
                }
            }

            current_depth = current_depth.saturating_sub(closes);

            // Check if any containers closed
            while let Some((_, _, _, depth_at_start)) = container_stack.last() {
                if current_depth <= *depth_at_start {
                    let (name, is_enum, start_idx, _) = container_stack.pop().unwrap();
                    let block = lines[start_idx..=i].join("\n");
                    let short_name = name.split('.').next_back().unwrap_or(&name).to_string();

                    if is_enum {
                        enums.insert(name.clone(), block.clone());
                        if short_name != name {
                            enums.entry(short_name).or_insert(block);
                        }
                    } else {
                        messages.insert(name.clone(), block.clone());
                        if short_name != name {
                            messages.entry(short_name).or_insert(block);
                        }
                    }
                } else {
                    break;
                }
            }
        }

        Self {
            messages,
            enums,
            field_index,
            enum_index,
        }
    }

    fn parse_field_line(line: &str) -> Option<(String, String, u32)> {
        // e.g.: "optional string conversation = 1;"
        // e.g.: "repeated uint32 options = 2;"
        let clean = line.trim_end_matches(';').trim();
        let (left, right) = clean.split_once('=')?;
        let num: u32 = right.trim().parse().ok()?;

        let tokens: Vec<&str> = left.split_whitespace().collect();
        if tokens.len() >= 2 {
            let f_name = tokens.last()?.to_string();
            let f_type = if tokens.len() >= 3 {
                format!("{} {}", tokens[0], tokens[1])
            } else {
                tokens[0].to_string()
            };
            Some((f_type, f_name, num))
        } else {
            None
        }
    }

    fn parse_enum_value(line: &str) -> Option<(String, i32)> {
        // e.g.: "CONNECTED = 0;"
        let clean = line.trim_end_matches(';').trim();
        let (name, right) = clean.split_once('=')?;
        let num: i32 = right.trim().parse().ok()?;
        Some((name.trim().to_string(), num))
    }

    pub fn stats(&self) -> SchemaStats {
        SchemaStats {
            total_messages: self.messages.len(),
            total_enums: self.enums.len(),
            total_fields: self.field_index.len(),
            schema_bytes: RAW_WAPROTO_SCHEMA.len(),
        }
    }

    pub fn get_message(&self, name: &str) -> Option<&str> {
        self.messages.get(name).map(|s| s.as_str())
    }

    pub fn get_enum(&self, name: &str) -> Option<&str> {
        self.enums.get(name).map(|s| s.as_str())
    }

    pub fn get_definition(&self, name: &str) -> Option<&str> {
        self.get_message(name).or_else(|| self.get_enum(name))
    }

    pub fn list_messages(&self) -> Vec<&str> {
        let mut list: Vec<&str> = self.messages.keys().map(|s| s.as_str()).collect();
        list.sort_unstable();
        list
    }

    pub fn list_enums(&self) -> Vec<&str> {
        let mut list: Vec<&str> = self.enums.keys().map(|s| s.as_str()).collect();
        list.sort_unstable();
        list
    }

    pub fn search(&self, query: &str) -> Vec<SchemaSearchResult> {
        let q = query.trim().to_lowercase();
        if q.is_empty() {
            return Vec::new();
        }

        let mut results = Vec::new();

        // 1. Match message names
        for (name, body) in &self.messages {
            if name.to_lowercase().contains(&q) {
                let first_line = body.lines().next().unwrap_or(name);
                results.push(SchemaSearchResult {
                    name: name.clone(),
                    item_type: SchemaItemType::Message,
                    snippet: first_line.to_string(),
                });
            }
        }

        // 2. Match enum names
        for (name, body) in &self.enums {
            if name.to_lowercase().contains(&q) {
                let first_line = body.lines().next().unwrap_or(name);
                results.push(SchemaSearchResult {
                    name: name.clone(),
                    item_type: SchemaItemType::Enum,
                    snippet: first_line.to_string(),
                });
            }
        }

        // 3. Match fields
        for (f_name, f_type, parent, num) in &self.field_index {
            if f_name.to_lowercase().contains(&q) {
                results.push(SchemaSearchResult {
                    name: format!("{}::{}", parent, f_name),
                    item_type: SchemaItemType::Field {
                        parent_message: parent.clone(),
                        field_type: f_type.clone(),
                        field_number: *num,
                    },
                    snippet: format!("{} {} = {}; (in {})", f_type, f_name, num, parent),
                });
            }
        }

        // 4. Match enum values
        for (v_name, parent, num) in &self.enum_index {
            if v_name.to_lowercase().contains(&q) {
                results.push(SchemaSearchResult {
                    name: format!("{}::{}", parent, v_name),
                    item_type: SchemaItemType::EnumValue {
                        parent_enum: parent.clone(),
                        number: *num,
                    },
                    snippet: format!("{} = {}; (in enum {})", v_name, num, parent),
                });
            }
        }

        results.sort_by(|a, b| a.name.cmp(&b.name));
        results
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_schema_parsing_and_stats() {
        let inspector = SchemaInspector::global();
        let stats = inspector.stats();

        assert!(
            stats.total_messages > 50,
            "Expected >50 messages, got {}",
            stats.total_messages
        );
        assert!(
            stats.total_enums > 10,
            "Expected >10 enums, got {}",
            stats.total_enums
        );
        assert!(
            stats.total_fields > 200,
            "Expected >200 fields, got {}",
            stats.total_fields
        );
    }

    #[test]
    fn test_schema_get_message_definition() {
        let inspector = SchemaInspector::global();

        let msg = inspector.get_message("Message");
        assert!(msg.is_some(), "Message should be found in WAProto");
        assert!(msg.unwrap().contains("conversation"));

        let web_msg = inspector.get_message("WebMessageInfo");
        assert!(web_msg.is_some());
        assert!(web_msg.unwrap().contains("key"));
    }

    #[test]
    fn test_schema_search() {
        let inspector = SchemaInspector::global();

        let poll_results = inspector.search("poll");
        assert!(
            !poll_results.is_empty(),
            "Search for 'poll' should yield results"
        );

        let conv_results = inspector.search("conversation");
        assert!(
            !conv_results.is_empty(),
            "Search for 'conversation' should yield results"
        );
    }
}
