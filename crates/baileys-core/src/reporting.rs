//! WhatsApp Reporting Tokens v2 Engine.
//!
//! Generates anti-spam & abuse reporting tokens attached to outgoing message stanzas.
//! Performs deterministic Protobuf wire field filtering and HMAC-SHA256 calculation
//! matching WhatsApp Web protocol (`Baileys/src/Utils/reporting-utils.ts`).

use crate::CoreError;
use baileys_binary::BinaryNode;
use baileys_crypto::{hkdf_sha256, hmac_sha256};
use std::collections::HashMap;

pub const ENC_SECRET_REPORT_TOKEN: &str = "Report Token";

#[derive(Debug, Clone)]
pub struct ReportingField {
    pub f: u32,
    pub m: bool,
    pub s: Vec<ReportingField>,
}

#[derive(Debug, Clone, Default)]
pub struct CompiledReportingField {
    pub m: bool,
    pub children: HashMap<u32, CompiledReportingField>,
}

fn build_reporting_field_schema() -> HashMap<u32, CompiledReportingField> {
    fn r(f: u32, m: bool, s: Vec<ReportingField>) -> ReportingField {
        ReportingField { f, m, s }
    }

    let fields = vec![
        r(1, false, vec![]),
        r(
            3,
            false,
            vec![
                r(2, false, vec![]),
                r(3, false, vec![]),
                r(8, false, vec![]),
                r(11, false, vec![]),
                r(17, false, vec![r(21, false, vec![]), r(22, false, vec![])]),
                r(25, false, vec![]),
            ],
        ),
        r(
            4,
            false,
            vec![
                r(1, false, vec![]),
                r(16, false, vec![]),
                r(17, false, vec![r(21, false, vec![]), r(22, false, vec![])]),
            ],
        ),
        r(
            5,
            false,
            vec![
                r(3, false, vec![]),
                r(4, false, vec![]),
                r(5, false, vec![]),
                r(16, false, vec![]),
                r(17, false, vec![r(21, false, vec![]), r(22, false, vec![])]),
            ],
        ),
        r(
            6,
            false,
            vec![
                r(1, false, vec![]),
                r(17, false, vec![r(21, false, vec![]), r(22, false, vec![])]),
                r(30, false, vec![]),
            ],
        ),
        r(
            7,
            false,
            vec![
                r(2, false, vec![]),
                r(7, false, vec![]),
                r(10, false, vec![]),
                r(17, false, vec![r(21, false, vec![]), r(22, false, vec![])]),
                r(20, false, vec![]),
            ],
        ),
        r(
            8,
            false,
            vec![
                r(2, false, vec![]),
                r(7, false, vec![]),
                r(9, false, vec![]),
                r(17, false, vec![r(21, false, vec![]), r(22, false, vec![])]),
                r(21, false, vec![]),
            ],
        ),
        r(
            9,
            false,
            vec![
                r(2, false, vec![]),
                r(6, false, vec![]),
                r(7, false, vec![]),
                r(13, false, vec![]),
                r(17, false, vec![r(21, false, vec![]), r(22, false, vec![])]),
                r(20, false, vec![]),
            ],
        ),
        r(
            12,
            false,
            vec![
                r(1, false, vec![]),
                r(2, false, vec![]),
                r(14, true, vec![]),
                r(15, false, vec![]),
            ],
        ),
        r(
            18,
            false,
            vec![
                r(6, false, vec![]),
                r(16, false, vec![]),
                r(17, false, vec![r(21, false, vec![]), r(22, false, vec![])]),
            ],
        ),
        r(
            26,
            false,
            vec![
                r(4, false, vec![]),
                r(5, false, vec![]),
                r(8, false, vec![]),
                r(13, false, vec![]),
                r(17, false, vec![r(21, false, vec![]), r(22, false, vec![])]),
            ],
        ),
        r(
            28,
            false,
            vec![
                r(1, false, vec![]),
                r(2, false, vec![]),
                r(4, false, vec![]),
                r(5, false, vec![]),
                r(6, false, vec![]),
                r(7, false, vec![r(21, false, vec![]), r(22, false, vec![])]),
            ],
        ),
        r(37, true, vec![r(1, true, vec![])]),
        r(
            49,
            false,
            vec![
                r(2, false, vec![]),
                r(3, false, vec![r(1, false, vec![]), r(2, false, vec![])]),
                r(5, false, vec![r(21, false, vec![]), r(22, false, vec![])]),
                r(8, false, vec![r(1, false, vec![]), r(2, false, vec![])]),
            ],
        ),
        r(53, true, vec![r(1, true, vec![])]),
        r(55, true, vec![r(1, true, vec![])]),
        r(58, true, vec![r(1, true, vec![])]),
        r(59, true, vec![r(1, true, vec![])]),
        r(
            60,
            false,
            vec![
                r(2, false, vec![]),
                r(3, false, vec![r(1, false, vec![]), r(2, false, vec![])]),
                r(5, false, vec![r(21, false, vec![]), r(22, false, vec![])]),
                r(8, false, vec![r(1, false, vec![]), r(2, false, vec![])]),
            ],
        ),
        r(
            64,
            false,
            vec![
                r(2, false, vec![]),
                r(3, false, vec![r(1, false, vec![]), r(2, false, vec![])]),
                r(5, false, vec![r(21, false, vec![]), r(22, false, vec![])]),
                r(8, false, vec![r(1, false, vec![]), r(2, false, vec![])]),
            ],
        ),
        r(
            66,
            false,
            vec![
                r(2, false, vec![]),
                r(6, false, vec![]),
                r(7, false, vec![]),
                r(13, false, vec![]),
                r(17, false, vec![r(21, false, vec![]), r(22, false, vec![])]),
                r(20, false, vec![]),
            ],
        ),
        r(74, true, vec![r(1, true, vec![])]),
        r(87, true, vec![r(1, true, vec![])]),
        r(
            88,
            false,
            vec![
                r(1, false, vec![]),
                r(2, false, vec![r(1, false, vec![])]),
                r(3, false, vec![r(21, false, vec![]), r(22, false, vec![])]),
            ],
        ),
        r(92, true, vec![r(1, true, vec![])]),
        r(93, true, vec![r(1, true, vec![])]),
        r(94, true, vec![r(1, true, vec![])]),
    ];

    compile_fields(&fields)
}

fn compile_fields(fields: &[ReportingField]) -> HashMap<u32, CompiledReportingField> {
    let mut map = HashMap::new();
    for f in fields {
        let children = if f.s.is_empty() {
            HashMap::new()
        } else {
            compile_fields(&f.s)
        };
        map.insert(f.f, CompiledReportingField { m: f.m, children });
    }
    map
}

const WIRE_VARINT: u32 = 0;
const WIRE_FIXED64: u32 = 1;
const WIRE_BYTES: u32 = 2;
const WIRE_FIXED32: u32 = 5;

fn decode_varint(data: &[u8], offset: usize) -> Option<(u64, usize)> {
    let mut value: u64 = 0;
    let mut shift = 0;
    let mut bytes = 0;

    while offset + bytes < data.len() {
        let current = data[offset + bytes];
        value |= ((current & 0x7f) as u64) << shift;
        bytes += 1;

        if (current & 0x80) == 0 {
            return Some((value, bytes));
        }

        shift += 7;
        if shift > 64 {
            return None;
        }
    }
    None
}

fn encode_varint(mut value: u64) -> Vec<u8> {
    let mut parts = Vec::new();
    while value > 0x7f {
        parts.push(((value & 0x7f) as u8) | 0x80);
        value >>= 7;
    }
    parts.push(value as u8);
    parts
}

struct FieldSlice {
    num: u32,
    bytes: Vec<u8>,
}

pub fn extract_reporting_token_content(
    data: &[u8],
    cfg: &HashMap<u32, CompiledReportingField>,
) -> Option<Vec<u8>> {
    let mut out: Vec<FieldSlice> = Vec::new();
    let mut i = 0;

    while i < data.len() {
        let (tag_val, tag_bytes) = decode_varint(data, i)?;
        let field_num = (tag_val >> 3) as u32;
        let wire_type = (tag_val & 0x7) as u32;

        let field_start = i;
        i += tag_bytes;

        let field_cfg = cfg.get(&field_num);

        match wire_type {
            WIRE_VARINT => {
                let (_, val_bytes) = decode_varint(data, i)?;
                let end = i + val_bytes;
                if end > data.len() {
                    return None;
                }
                if field_cfg.is_some() {
                    out.push(FieldSlice {
                        num: field_num,
                        bytes: data[field_start..end].to_vec(),
                    });
                }
                i = end;
            }
            WIRE_FIXED64 => {
                let end = i + 8;
                if end > data.len() {
                    return None;
                }
                if field_cfg.is_some() {
                    out.push(FieldSlice {
                        num: field_num,
                        bytes: data[field_start..end].to_vec(),
                    });
                }
                i = end;
            }
            WIRE_FIXED32 => {
                let end = i + 4;
                if end > data.len() {
                    return None;
                }
                if field_cfg.is_some() {
                    out.push(FieldSlice {
                        num: field_num,
                        bytes: data[field_start..end].to_vec(),
                    });
                }
                i = end;
            }
            WIRE_BYTES => {
                let (len_val, len_bytes) = decode_varint(data, i)?;
                let val_start = i + len_bytes;
                let val_end = val_start + (len_val as usize);
                if val_end > data.len() {
                    return None;
                }

                if let Some(cfg_entry) = field_cfg {
                    if cfg_entry.m || !cfg_entry.children.is_empty() {
                        let sub = extract_reporting_token_content(
                            &data[val_start..val_end],
                            &cfg_entry.children,
                        )?;
                        if !sub.is_empty() {
                            let mut combined = Vec::new();
                            combined.extend(encode_varint(tag_val));
                            combined.extend(encode_varint(sub.len() as u64));
                            combined.extend(sub);
                            out.push(FieldSlice {
                                num: field_num,
                                bytes: combined,
                            });
                        }
                    } else {
                        out.push(FieldSlice {
                            num: field_num,
                            bytes: data[field_start..val_end].to_vec(),
                        });
                    }
                }
                i = val_end;
            }
            _ => return None,
        }
    }

    if out.is_empty() {
        return Some(Vec::new());
    }

    out.sort_by_key(|f| f.num);
    let mut result = Vec::new();
    for item in out {
        result.extend(item.bytes);
    }
    Some(result)
}

pub fn generate_msg_secret_key(
    modification_type: &str,
    orig_msg_id: &str,
    orig_msg_sender: &str,
    modification_sender: &str,
    orig_msg_secret: &[u8],
) -> Result<[u8; 32], CoreError> {
    let mut use_case_secret = Vec::new();
    use_case_secret.extend_from_slice(orig_msg_id.as_bytes());
    use_case_secret.extend_from_slice(orig_msg_sender.as_bytes());
    use_case_secret.extend_from_slice(modification_sender.as_bytes());
    use_case_secret.extend_from_slice(modification_type.as_bytes());

    let derived = hkdf_sha256(orig_msg_secret, 32, None, Some(&use_case_secret))
        .map_err(|e| CoreError::Crypto(format!("Failed to derive msg secret key: {}", e)))?;

    let mut key = [0u8; 32];
    key.copy_from_slice(&derived);
    Ok(key)
}

/// Computes the 16-byte reporting token and wraps it in a `<reporting><reporting_token v="2">{token}</reporting_token></reporting>` BinaryNode.
pub fn get_message_reporting_token(
    msg_protobuf_bytes: &[u8],
    msg_id: &str,
    from_jid: &str,
    to_jid: &str,
    msg_secret: &[u8],
) -> Result<Option<BinaryNode>, CoreError> {
    if msg_secret.is_empty() || msg_id.is_empty() {
        return Ok(None);
    }

    let schema = build_reporting_field_schema();
    let filtered_content = match extract_reporting_token_content(msg_protobuf_bytes, &schema) {
        Some(c) if !c.is_empty() => c,
        _ => return Ok(None),
    };

    let reporting_secret = generate_msg_secret_key(
        ENC_SECRET_REPORT_TOKEN,
        msg_id,
        from_jid,
        to_jid,
        msg_secret,
    )?;

    let hmac = hmac_sha256(&filtered_content, &reporting_secret)
        .map_err(|e| CoreError::Crypto(format!("Failed to calculate reporting HMAC: {}", e)))?;

    let token_16 = &hmac[..16];

    let reporting_token_node = BinaryNode::new("reporting_token")
        .with_attr("v", "2")
        .with_bytes_content(token_16.to_vec());

    let reporting_node = BinaryNode::new("reporting").with_child(reporting_token_node);

    Ok(Some(reporting_node))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_varint_encode_decode() {
        let values = vec![0, 1, 127, 128, 300, 16384, u32::MAX as u64];
        for v in values {
            let enc = encode_varint(v);
            let (dec, bytes) = decode_varint(&enc, 0).expect("Varint decode failed");
            assert_eq!(dec, v);
            assert_eq!(bytes, enc.len());
        }
    }

    #[test]
    fn test_reporting_token_generation() {
        let msg_id = "3EB0FF1122334455";
        let from_jid = "1234567890@s.whatsapp.net";
        let to_jid = "0987654321@s.whatsapp.net";
        let msg_secret = [42u8; 32];

        // Simulate simple protobuf with field 1 (conversation)
        let mut proto_bytes = Vec::new();
        proto_bytes.extend(encode_varint(((1 << 3) | WIRE_BYTES) as u64)); // field 1, WIRE_BYTES
        let text = b"Hello, world!";
        proto_bytes.extend(encode_varint(text.len() as u64));
        proto_bytes.extend_from_slice(text);

        let node = get_message_reporting_token(&proto_bytes, msg_id, from_jid, to_jid, &msg_secret)
            .expect("Reporting token generation failed")
            .expect("Node should not be None");

        assert_eq!(node.tag, "reporting");
        assert_eq!(node.get_children().len(), 1);

        let child = &node.get_children()[0];
        assert_eq!(child.tag, "reporting_token");
        assert_eq!(child.get_attr("v"), Some("2"));
        assert_eq!(child.content_as_bytes().unwrap().len(), 16);
    }
}
