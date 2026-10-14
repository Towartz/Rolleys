use crate::jid::{jid_decode, FullJid};
use crate::node::{BinaryNode, BinaryNodeContent};
use crate::tokens::*;
use crate::BinaryError;

pub struct BinaryEncoder {
    buffer: Vec<u8>,
}

impl Default for BinaryEncoder {
    fn default() -> Self {
        Self::new()
    }
}

impl BinaryEncoder {
    pub fn new() -> Self {
        Self {
            buffer: vec![0], // Initial 0 prefix byte matching Baileys
        }
    }

    pub fn encode(node: &BinaryNode) -> Result<Vec<u8>, BinaryError> {
        let mut encoder = Self::new();
        encoder.encode_node(node)?;
        Ok(encoder.buffer)
    }

    fn push_byte(&mut self, byte: u8) {
        self.buffer.push(byte);
    }

    fn push_bytes(&mut self, bytes: &[u8]) {
        self.buffer.extend_from_slice(bytes);
    }

    fn push_int16(&mut self, value: u16) {
        self.buffer.push((value >> 8) as u8);
        self.buffer.push(value as u8);
    }

    fn push_int20(&mut self, value: u32) {
        self.buffer.push(((value >> 16) & 0x0F) as u8);
        self.buffer.push(((value >> 8) & 0xFF) as u8);
        self.buffer.push((value & 0xFF) as u8);
    }

    fn push_int32(&mut self, value: u32) {
        self.buffer.extend_from_slice(&value.to_be_bytes());
    }

    fn write_byte_length(&mut self, length: usize) {
        if length >= (1 << 20) {
            self.push_byte(TAG_BINARY_32);
            self.push_int32(length as u32);
        } else if length >= 256 {
            self.push_byte(TAG_BINARY_20);
            self.push_int20(length as u32);
        } else {
            self.push_byte(TAG_BINARY_8);
            self.push_byte(length as u8);
        }
    }

    fn write_string_raw(&mut self, s: &str) {
        let bytes = s.as_bytes();
        self.write_byte_length(bytes.len());
        self.push_bytes(bytes);
    }

    fn pack_nibble(c: char) -> Result<u8, BinaryError> {
        match c {
            '-' => Ok(10),
            '.' => Ok(11),
            '\0' => Ok(15),
            '0'..='9' => Ok(c as u8 - b'0'),
            _ => Err(BinaryError::InvalidNibbleChar(c)),
        }
    }

    fn pack_hex(c: char) -> Result<u8, BinaryError> {
        match c {
            '0'..='9' => Ok(c as u8 - b'0'),
            'A'..='F' => Ok(10 + (c as u8 - b'A')),
            'a'..='f' => Ok(10 + (c as u8 - b'a')),
            '\0' => Ok(15),
            _ => Err(BinaryError::InvalidHexChar(c)),
        }
    }

    fn write_packed_bytes(&mut self, s: &str, is_nibble: bool) -> Result<(), BinaryError> {
        if s.len() > TAG_PACKED_MAX as usize {
            return Err(BinaryError::StringTooLarge(s.len()));
        }

        self.push_byte(if is_nibble { TAG_NIBBLE_8 } else { TAG_HEX_8 });

        let mut rounded_length = s.len().div_ceil(2);
        if !s.len().is_multiple_of(2) {
            rounded_length |= 128;
        }
        self.push_byte(rounded_length as u8);

        let chars: Vec<char> = s.chars().collect();
        let half_len = chars.len() / 2;
        for i in 0..half_len {
            let v1 = if is_nibble {
                Self::pack_nibble(chars[2 * i])?
            } else {
                Self::pack_hex(chars[2 * i])?
            };
            let v2 = if is_nibble {
                Self::pack_nibble(chars[2 * i + 1])?
            } else {
                Self::pack_hex(chars[2 * i + 1])?
            };
            self.push_byte((v1 << 4) | v2);
        }

        if !chars.len().is_multiple_of(2) {
            let v1 = if is_nibble {
                Self::pack_nibble(chars[chars.len() - 1])?
            } else {
                Self::pack_hex(chars[chars.len() - 1])?
            };
            let v2 = if is_nibble {
                Self::pack_nibble('\0')?
            } else {
                Self::pack_hex('\0')?
            };
            self.push_byte((v1 << 4) | v2);
        }

        Ok(())
    }

    fn is_nibble(s: &str) -> bool {
        if s.is_empty() || s.len() > TAG_PACKED_MAX as usize {
            return false;
        }
        s.chars()
            .all(|c| c.is_ascii_digit() || c == '-' || c == '.')
    }

    fn is_hex(s: &str) -> bool {
        if s.is_empty() || s.len() > TAG_PACKED_MAX as usize {
            return false;
        }
        s.chars().all(|c| c.is_ascii_hexdigit())
    }

    fn write_jid(&mut self, jid: &FullJid) -> Result<(), BinaryError> {
        if let Some(device) = jid.device {
            self.push_byte(TAG_AD_JID);
            self.push_byte(jid.domain_type.unwrap_or(0));
            self.push_byte(device);
            self.write_string(&jid.user)?;
        } else {
            self.push_byte(TAG_JID_PAIR);
            if jid.user.is_empty() {
                self.push_byte(TAG_LIST_EMPTY);
            } else {
                self.write_string(&jid.user)?;
            }
            self.write_string(&jid.server)?;
        }
        Ok(())
    }

    pub fn write_string(&mut self, s: &str) -> Result<(), BinaryError> {
        if s.is_empty() {
            self.write_string_raw(s);
            return Ok(());
        }

        if let Some(token_ref) = find_token(s) {
            match token_ref {
                TokenRef::Single(idx) => {
                    self.push_byte(idx);
                }
                TokenRef::Double { dict, index } => {
                    self.push_byte(TAG_DICTIONARY_0 + dict);
                    self.push_byte(index);
                }
            }
        } else if Self::is_nibble(s) {
            self.write_packed_bytes(s, true)?;
        } else if Self::is_hex(s) {
            self.write_packed_bytes(s, false)?;
        } else if let Some(jid) = jid_decode(s) {
            self.write_jid(&jid)?;
        } else {
            self.write_string_raw(s);
        }

        Ok(())
    }

    fn write_list_start(&mut self, list_size: usize) {
        if list_size == 0 {
            self.push_byte(TAG_LIST_EMPTY);
        } else if list_size < 256 {
            self.push_byte(TAG_LIST_8);
            self.push_byte(list_size as u8);
        } else {
            self.push_byte(TAG_LIST_16);
            self.push_int16(list_size as u16);
        }
    }

    fn encode_node(&mut self, node: &BinaryNode) -> Result<(), BinaryError> {
        let has_content = node.content.is_some();
        let list_size = 1 + (node.attrs.len() * 2) + if has_content { 1 } else { 0 };

        self.write_list_start(list_size);
        self.write_string(&node.tag)?;

        // Sort attrs for deterministic wire format
        let mut sorted_attrs: Vec<(&String, &String)> = node.attrs.iter().collect();
        sorted_attrs.sort_by(|a, b| a.0.cmp(b.0));

        for (k, v) in sorted_attrs {
            self.write_string(k)?;
            self.write_string(v)?;
        }

        if let Some(content) = &node.content {
            match content {
                BinaryNodeContent::Nodes(children) => {
                    self.write_list_start(children.len());
                    for child in children {
                        self.encode_node(child)?;
                    }
                }
                BinaryNodeContent::Bytes(bytes) => {
                    self.write_byte_length(bytes.len());
                    self.push_bytes(bytes);
                }
                BinaryNodeContent::String(s) => {
                    self.write_string(s)?;
                }
            }
        }

        Ok(())
    }
}
