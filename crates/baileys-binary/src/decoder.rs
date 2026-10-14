use flate2::read::ZlibDecoder;
use std::collections::HashMap;
use std::io::Read;

use crate::jid::jid_encode;
use crate::node::{BinaryNode, BinaryNodeContent};
use crate::tokens::*;
use crate::BinaryError;

pub struct BinaryDecoder<'a> {
    data: &'a [u8],
    index: usize,
}

impl<'a> BinaryDecoder<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, index: 0 }
    }

    /// Decompresses buffer if required (first byte has bit 2 set), strips prefix byte, and decodes node.
    pub fn decode(buffer: &[u8]) -> Result<BinaryNode, BinaryError> {
        if buffer.is_empty() {
            return Err(BinaryError::UnexpectedEof);
        }

        let decompressed = if (buffer[0] & 2) != 0 {
            let mut decoder = ZlibDecoder::new(&buffer[1..]);
            let mut decomp = Vec::new();
            decoder
                .read_to_end(&mut decomp)
                .map_err(|e| BinaryError::DecompressionFailed(e.to_string()))?;
            decomp
        } else {
            buffer[1..].to_vec()
        };

        let mut decoder = BinaryDecoder::new(&decompressed);
        decoder.decode_node()
    }

    fn check_eos(&self, length: usize) -> Result<(), BinaryError> {
        if self.index + length > self.data.len() {
            Err(BinaryError::UnexpectedEof)
        } else {
            Ok(())
        }
    }

    fn next_byte(&mut self) -> Result<u8, BinaryError> {
        self.check_eos(1)?;
        let val = self.data[self.index];
        self.index += 1;
        Ok(val)
    }

    fn read_bytes(&mut self, n: usize) -> Result<&'a [u8], BinaryError> {
        self.check_eos(n)?;
        let slice = &self.data[self.index..self.index + n];
        self.index += n;
        Ok(slice)
    }

    fn read_int16(&mut self) -> Result<u16, BinaryError> {
        let b = self.read_bytes(2)?;
        Ok(u16::from_be_bytes([b[0], b[1]]))
    }

    fn read_int20(&mut self) -> Result<u32, BinaryError> {
        let b = self.read_bytes(3)?;
        Ok((((b[0] & 0x0F) as u32) << 16) | ((b[1] as u32) << 8) | (b[2] as u32))
    }

    fn read_int32(&mut self) -> Result<u32, BinaryError> {
        let b = self.read_bytes(4)?;
        Ok(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn unpack_hex(value: u8) -> Result<char, BinaryError> {
        match value {
            0..=9 => Ok((b'0' + value) as char),
            10..=15 => Ok((b'A' + value - 10) as char),
            _ => Err(BinaryError::InvalidHexByte(value)),
        }
    }

    fn unpack_nibble(value: u8) -> Result<char, BinaryError> {
        match value {
            0..=9 => Ok((b'0' + value) as char),
            10 => Ok('-'),
            11 => Ok('.'),
            15 => Ok('\0'),
            _ => Err(BinaryError::InvalidNibbleByte(value)),
        }
    }

    fn read_packed8(&mut self, tag: u8) -> Result<String, BinaryError> {
        let start_byte = self.next_byte()?;
        let num_pairs = (start_byte & 127) as usize;
        let mut out = String::with_capacity(num_pairs * 2);

        for _ in 0..num_pairs {
            let cur = self.next_byte()?;
            let high = (cur & 0xF0) >> 4;
            let low = cur & 0x0F;

            let c1 = if tag == TAG_NIBBLE_8 {
                Self::unpack_nibble(high)?
            } else {
                Self::unpack_hex(high)?
            };
            let c2 = if tag == TAG_NIBBLE_8 {
                Self::unpack_nibble(low)?
            } else {
                Self::unpack_hex(low)?
            };

            out.push(c1);
            out.push(c2);
        }

        // If top bit is 1, the last nibble is padding and should be stripped
        if (start_byte >> 7) != 0 && !out.is_empty() {
            out.pop();
        }

        Ok(out)
    }

    fn read_string_from_chars(&mut self, length: usize) -> Result<String, BinaryError> {
        let bytes = self.read_bytes(length)?;
        String::from_utf8(bytes.to_vec()).map_err(|e| BinaryError::InvalidUtf8(e.to_string()))
    }

    fn read_jid_pair(&mut self) -> Result<String, BinaryError> {
        let user_tag = self.next_byte()?;
        let user = self.read_string(user_tag)?;
        let server_tag = self.next_byte()?;
        let server = self.read_string(server_tag)?;

        if user.is_empty() {
            Ok(server)
        } else {
            Ok(format!("{}@{}", user, server))
        }
    }

    fn read_ad_jid(&mut self) -> Result<String, BinaryError> {
        let domain_type = self.next_byte()?;
        let device = self.next_byte()?;
        let user_tag = self.next_byte()?;
        let user = self.read_string(user_tag)?;

        let server = match domain_type {
            1 => "lid",
            128 => "hosted",
            129 => "hosted.lid",
            _ => "s.whatsapp.net",
        };

        Ok(jid_encode(&user, server, Some(device), None))
    }

    fn read_fb_jid(&mut self) -> Result<String, BinaryError> {
        let user_tag = self.next_byte()?;
        let user = self.read_string(user_tag)?;
        let device = self.read_int16()?;
        let server_tag = self.next_byte()?;
        let server = self.read_string(server_tag)?;
        Ok(format!("{}:{}@{}", user, device, server))
    }

    fn read_interop_jid(&mut self) -> Result<String, BinaryError> {
        let user_tag = self.next_byte()?;
        let user = self.read_string(user_tag)?;
        let device = self.read_int16()?;
        let integrator = self.read_int16()?;
        let before_index = self.index;
        let server = match self.next_byte().and_then(|tag| self.read_string(tag)) {
            Ok(s) => s,
            Err(_) => {
                self.index = before_index;
                "interop".to_string()
            }
        };
        Ok(format!("{}-{}:{}@{}", integrator, user, device, server))
    }

    pub fn read_string(&mut self, tag: u8) -> Result<String, BinaryError> {
        if tag == TAG_LIST_EMPTY {
            return Ok(String::new());
        }

        if (tag as usize) < SINGLE_BYTE_TOKENS.len() {
            if let Some(token) = get_single_byte_token(tag as usize) {
                return Ok(token.to_string());
            }
        }

        match tag {
            TAG_DICTIONARY_0..=244 => {
                let dict = (tag - TAG_DICTIONARY_0) as usize;
                let index = self.next_byte()? as usize;
                match get_double_byte_token(dict, index) {
                    Some(token) => Ok(token.to_string()),
                    None => {
                        tracing::warn!(dict, index, "Unknown double-byte token in dictionary, falling back to synthesized token");
                        Ok(format!("_tok_d{}_{}", dict, index))
                    }
                }
            }
            TAG_LIST_8 => {
                let size = self.next_byte()?;
                Err(BinaryError::UnexpectedListTag(size as usize))
            }
            TAG_BINARY_8 => {
                let len = self.next_byte()? as usize;
                self.read_string_from_chars(len)
            }
            TAG_BINARY_20 => {
                let len = self.read_int20()? as usize;
                self.read_string_from_chars(len)
            }
            TAG_BINARY_32 => {
                let len = self.read_int32()? as usize;
                self.read_string_from_chars(len)
            }
            TAG_NIBBLE_8 | TAG_HEX_8 => self.read_packed8(tag),
            TAG_JID_PAIR => self.read_jid_pair(),
            TAG_FB_JID => self.read_fb_jid(),
            TAG_INTEROP_JID => self.read_interop_jid(),
            TAG_AD_JID => self.read_ad_jid(),
            _ => {
                if tag < TAG_DICTIONARY_0 {
                    tracing::warn!(
                        tag,
                        "Unknown single-byte token, falling back to synthesized token"
                    );
                    Ok(format!("_tok_{}", tag))
                } else {
                    Err(BinaryError::UnknownTag(tag))
                }
            }
        }
    }

    fn read_list_size(&mut self, tag: u8) -> Result<usize, BinaryError> {
        match tag {
            TAG_LIST_EMPTY => Ok(0),
            TAG_LIST_8 => Ok(self.next_byte()? as usize),
            TAG_LIST_16 => Ok(self.read_int16()? as usize),
            _ => Err(BinaryError::UnexpectedListTag(tag as usize)),
        }
    }

    pub fn decode_node(&mut self) -> Result<BinaryNode, BinaryError> {
        let list_tag = self.next_byte()?;
        let list_size = self.read_list_size(list_tag)?;

        if list_size == 0 {
            return Err(BinaryError::EmptyList);
        }

        let tag_byte = self.next_byte()?;
        let tag = self.read_string(tag_byte)?;

        let num_attrs = (list_size - 1) / 2;
        let mut attrs = HashMap::with_capacity(num_attrs);

        for _ in 0..num_attrs {
            let key_byte = self.next_byte()?;
            let key = self.read_string(key_byte)?;
            let val_byte = self.next_byte()?;
            let val = self.read_string(val_byte)?;
            attrs.insert(key, val);
        }

        let mut content = None;
        if list_size % 2 == 0 {
            let content_tag = self.next_byte()?;
            match content_tag {
                TAG_LIST_EMPTY | TAG_LIST_8 | TAG_LIST_16 => {
                    let child_count = self.read_list_size(content_tag)?;
                    let mut children = Vec::with_capacity(child_count);
                    for _ in 0..child_count {
                        children.push(self.decode_node()?);
                    }
                    content = Some(BinaryNodeContent::Nodes(children));
                }
                TAG_BINARY_8 => {
                    let len = self.next_byte()? as usize;
                    content = Some(BinaryNodeContent::Bytes(self.read_bytes(len)?.to_vec()));
                }
                TAG_BINARY_20 => {
                    let len = self.read_int20()? as usize;
                    content = Some(BinaryNodeContent::Bytes(self.read_bytes(len)?.to_vec()));
                }
                TAG_BINARY_32 => {
                    let len = self.read_int32()? as usize;
                    content = Some(BinaryNodeContent::Bytes(self.read_bytes(len)?.to_vec()));
                }
                _ => {
                    let s = self.read_string(content_tag)?;
                    content = Some(BinaryNodeContent::String(s));
                }
            }
        }

        Ok(BinaryNode {
            tag,
            attrs,
            content,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decoder_fallback_double_byte_token() {
        // Tag 240 (dict 4) + index 10 (not present in current dictionaries)
        let data = vec![240, 10];
        let mut decoder = BinaryDecoder::new(&data);
        let tag = decoder.next_byte().unwrap();
        let s = decoder.read_string(tag).unwrap();
        assert_eq!(s, "_tok_d4_10");
    }

    #[test]
    fn test_decoder_fallback_single_byte_token() {
        // Tag 230 (if out of bounds or unused single-byte token)
        let tag_unmapped = 230u8;
        let data = vec![tag_unmapped];
        let mut decoder = BinaryDecoder::new(&data);
        let tag = decoder.next_byte().unwrap();
        let s = decoder.read_string(tag).unwrap();
        // Either resolved if in dictionary or fallback _tok_230
        assert!(!s.is_empty());
    }
}
