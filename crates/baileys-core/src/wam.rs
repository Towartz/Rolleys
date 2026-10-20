//! WhatsApp Metrics (WAM) telemetry encoder and anti-fingerprinting wire builder.
//!
//! Emulates official WhatsApp Web `w:stats` binary telemetry batches:
//! - Wire-compatible V5 WAM buffer binary serialization
//! - Delta-encoded global attributes (commit time, stream ID, platform, browser)
//! - Variable-width integer compression and string length tagging
//! - Stanza generation for periodic `<iq to="s.whatsapp.net" type="set" xmlns="w:stats">` uploads

use baileys_binary::{BinaryNode, BinaryNodeContent};
use std::collections::HashMap;

/// WAM protocol version deployed on WhatsApp Web.
pub const WA_WAM_PROTOCOL_VERSION: u8 = 5;

/// Global attribute ID for the commit timestamp (seconds since Unix epoch).
pub const COMMIT_TIME_GLOBAL_ID: u16 = 47;

/// Global attribute ID for the per-session stream ID.
pub const STREAM_ID_GLOBAL_ID: u16 = 3543;

// Marker bits (lower nibble, bits 0..3)
const MARK_GLOBAL: u8 = 0x00;
const MARK_EVENT: u8 = 0x01;
const MARK_FIELD: u8 = 0x02;
const MARK_LAST: u8 = 0x04;
const MARK_EXTENDED_ID: u8 = 0x08;

// Value encoding bits (upper nibble, bits 4..7)
const ENC_NULL: u8 = 0x00;
const ENC_INT_ZERO: u8 = 0x10;
const ENC_INT_ONE: u8 = 0x20;
const ENC_INT8: u8 = 0x30;
const ENC_INT16: u8 = 0x40;
const ENC_INT32: u8 = 0x50;
const ENC_INT64: u8 = 0x60;
const ENC_FLOAT64: u8 = 0x70;
const ENC_STR_SHORT: u8 = 0x80;
const ENC_STR_MED: u8 = 0x90;
const ENC_STR_LONG: u8 = 0xa0;

const INT32_MIN: i64 = -2_147_483_648;
const INT32_UPPER_EXCLUSIVE: i64 = 2_147_483_648;

/// Low-level big-endian binary buffer writer for WAM frames.
#[derive(Debug, Default, Clone)]
pub struct WamBinaryWriter {
    buf: Vec<u8>,
}

impl WamBinaryWriter {
    pub fn new(capacity: usize) -> Self {
        Self {
            buf: Vec::with_capacity(capacity),
        }
    }

    #[inline]
    pub fn write_u8(&mut self, val: u8) {
        self.buf.push(val);
    }

    #[inline]
    pub fn write_u16(&mut self, val: u16) {
        self.buf.extend_from_slice(&val.to_be_bytes());
    }

    #[inline]
    pub fn write_u32(&mut self, val: u32) {
        self.buf.extend_from_slice(&val.to_be_bytes());
    }

    #[inline]
    pub fn write_i8(&mut self, val: i8) {
        self.buf.push(val as u8);
    }

    #[inline]
    pub fn write_i16(&mut self, val: i16) {
        self.buf.extend_from_slice(&val.to_be_bytes());
    }

    #[inline]
    pub fn write_i32(&mut self, val: i32) {
        self.buf.extend_from_slice(&val.to_be_bytes());
    }

    #[inline]
    pub fn write_i64(&mut self, val: i64) {
        self.buf.extend_from_slice(&val.to_be_bytes());
    }

    #[inline]
    pub fn write_f64(&mut self, val: f64) {
        self.buf.extend_from_slice(&val.to_be_bytes());
    }

    #[inline]
    pub fn write_bytes(&mut self, bytes: &[u8]) {
        self.buf.extend_from_slice(bytes);
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.buf.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        self.buf.clone()
    }
}

/// Dynamic value carried in a WAM global attribute or event field.
#[derive(Debug, Clone, PartialEq)]
pub enum WamValue {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
}

impl From<bool> for WamValue {
    fn from(v: bool) -> Self {
        Self::Bool(v)
    }
}

impl From<i64> for WamValue {
    fn from(v: i64) -> Self {
        Self::Int(v)
    }
}

impl From<i32> for WamValue {
    fn from(v: i32) -> Self {
        Self::Int(v as i64)
    }
}

impl From<u64> for WamValue {
    fn from(v: u64) -> Self {
        Self::Int(v as i64)
    }
}

impl From<f64> for WamValue {
    fn from(v: f64) -> Self {
        Self::Float(v)
    }
}

impl From<&str> for WamValue {
    fn from(v: &str) -> Self {
        Self::String(v.to_string())
    }
}

impl From<String> for WamValue {
    fn from(v: String) -> Self {
        Self::String(v)
    }
}

/// A resolved field inside a WAM event.
#[derive(Debug, Clone, PartialEq)]
pub struct WamField {
    pub id: u16,
    pub value: WamValue,
}

impl WamField {
    pub fn new<V: Into<WamValue>>(id: u16, value: V) -> Self {
        Self {
            id,
            value: value.into(),
        }
    }
}

/// Writes a TLV tag with ID and combined marker/encoding byte.
fn write_tag(writer: &mut WamBinaryWriter, id: u16, marker_with_encoding: u8) {
    if id < 256 {
        writer.write_u8(marker_with_encoding);
        writer.write_u8(id as u8);
    } else {
        writer.write_u8(marker_with_encoding | MARK_EXTENDED_ID);
        writer.write_u16(id);
    }
}

/// Writes an integer value packed into the smallest matching width class.
fn write_int(writer: &mut WamBinaryWriter, id: u16, marker: u8, value: i64) {
    if value == 0 {
        write_tag(writer, id, ENC_INT_ZERO | marker);
    } else if value == 1 {
        write_tag(writer, id, ENC_INT_ONE | marker);
    } else if (-128..128).contains(&value) {
        write_tag(writer, id, ENC_INT8 | marker);
        writer.write_i8(value as i8);
    } else if (-32_768..32_768).contains(&value) {
        write_tag(writer, id, ENC_INT16 | marker);
        writer.write_i16(value as i16);
    } else if (INT32_MIN..INT32_UPPER_EXCLUSIVE).contains(&value) {
        write_tag(writer, id, ENC_INT32 | marker);
        writer.write_i32(value as i32);
    } else {
        write_tag(writer, id, ENC_INT64 | marker);
        writer.write_i64(value);
    }
}

/// Writes a 64-bit IEEE-754 floating point value.
fn write_float(writer: &mut WamBinaryWriter, id: u16, marker: u8, value: f64) {
    write_tag(writer, id, ENC_FLOAT64 | marker);
    writer.write_f64(value);
}

/// Writes a string with short/medium/long length header.
fn write_string_value(writer: &mut WamBinaryWriter, id: u16, marker: u8, value: &str) {
    let bytes = value.as_bytes();
    let len = bytes.len();
    if len < 256 {
        write_tag(writer, id, ENC_STR_SHORT | marker);
        writer.write_u8(len as u8);
    } else if len < 65_536 {
        write_tag(writer, id, ENC_STR_MED | marker);
        writer.write_u16(len as u16);
    } else {
        write_tag(writer, id, ENC_STR_LONG | marker);
        writer.write_u32(len as u32);
    }
    writer.write_bytes(bytes);
}

/// Writes a global attribute TLV (marker 0x00).
pub fn write_global_attribute(writer: &mut WamBinaryWriter, id: u16, value: &WamValue) {
    let marker = MARK_GLOBAL;
    match value {
        WamValue::Null => write_tag(writer, id, ENC_NULL | marker),
        WamValue::Bool(b) => write_int(writer, id, marker, if *b { 1 } else { 0 }),
        WamValue::Int(i) => write_int(writer, id, marker, *i),
        WamValue::Float(f) => write_float(writer, id, marker, *f),
        WamValue::String(s) => write_string_value(writer, id, marker, s),
    }
}

/// Writes an event header TLV (marker 0x01) with sampling weight.
pub fn write_event_header(
    writer: &mut WamBinaryWriter,
    event_id: u16,
    weight: i64,
    has_fields: bool,
) {
    let marker = if has_fields {
        MARK_EVENT
    } else {
        MARK_EVENT | MARK_LAST
    };
    write_int(writer, event_id, marker, weight);
}

/// Writes a field TLV (marker 0x02). Sets MARK_LAST on the final field of an event.
pub fn write_field(writer: &mut WamBinaryWriter, field: &WamField, is_last: bool) {
    let marker = if is_last {
        MARK_FIELD | MARK_LAST
    } else {
        MARK_FIELD
    };
    match &field.value {
        WamValue::Null => write_tag(writer, field.id, ENC_NULL | marker),
        WamValue::Bool(b) => write_int(writer, field.id, marker, if *b { 1 } else { 0 }),
        WamValue::Int(i) => write_int(writer, field.id, marker, *i),
        WamValue::Float(f) => write_float(writer, field.id, marker, *f),
        WamValue::String(s) => write_string_value(writer, field.id, marker, s),
    }
}

/// A serialized WAM batch containing header, delta-encoded globals, and event records.
#[derive(Debug, Clone)]
pub struct WamBatch {
    writer: WamBinaryWriter,
    committed_globals: HashMap<u16, WamValue>,
    events_written: usize,
}

impl WamBatch {
    /// Creates a new WAM batch with the canonical V5 header.
    pub fn new(stream_id: u8, sequence_number: u16, channel_code: u8) -> Self {
        let mut writer = WamBinaryWriter::new(512);
        writer.write_bytes(b"WAM");
        writer.write_u8(WA_WAM_PROTOCOL_VERSION);
        writer.write_u8(stream_id);
        writer.write_u16(sequence_number);
        writer.write_u8(channel_code);

        Self {
            writer,
            committed_globals: HashMap::new(),
            events_written: 0,
        }
    }

    /// Sets a global attribute, emitting it only if its value changed since the last write.
    pub fn set_global<V: Into<WamValue>>(&mut self, id: u16, value: V) {
        let val = value.into();
        if self.committed_globals.get(&id) == Some(&val) {
            return;
        }
        write_global_attribute(&mut self.writer, id, &val);
        self.committed_globals.insert(id, val);
    }

    /// Appends an event record with its commit timestamp (seconds), event ID, weight, and fields.
    pub fn write_event(
        &mut self,
        commit_time_sec: u64,
        event_id: u16,
        weight: i64,
        fields: &[WamField],
    ) {
        write_global_attribute(
            &mut self.writer,
            COMMIT_TIME_GLOBAL_ID,
            &WamValue::Int(commit_time_sec as i64),
        );
        self.committed_globals
            .insert(COMMIT_TIME_GLOBAL_ID, WamValue::Int(commit_time_sec as i64));

        let has_fields = !fields.is_empty();
        write_event_header(&mut self.writer, event_id, weight, has_fields);

        for (i, field) in fields.iter().enumerate() {
            let is_last = i == fields.len() - 1;
            write_field(&mut self.writer, field, is_last);
        }

        self.events_written += 1;
    }

    pub fn events_count(&self) -> usize {
        self.events_written
    }

    pub fn has_events(&self) -> bool {
        self.events_written > 0
    }

    pub fn len(&self) -> usize {
        self.writer.len()
    }

    pub fn is_empty(&self) -> bool {
        self.writer.is_empty()
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        self.writer.to_bytes()
    }

    /// Wraps the serialized WAM batch into WhatsApp's official `<iq xmlns="w:stats"><add t="...">` stanza.
    pub fn build_iq_node(&self, timestamp_sec: u64) -> BinaryNode {
        BinaryNode {
            tag: "iq".to_string(),
            attrs: HashMap::from([
                ("to".to_string(), "s.whatsapp.net".to_string()),
                ("type".to_string(), "set".to_string()),
                ("xmlns".to_string(), "w:stats".to_string()),
            ]),
            content: Some(BinaryNodeContent::Nodes(vec![BinaryNode {
                tag: "add".to_string(),
                attrs: HashMap::from([("t".to_string(), timestamp_sec.to_string())]),
                content: Some(BinaryNodeContent::Bytes(self.to_bytes())),
            }])),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex_encode(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{:02x}", b)).collect()
    }

    fn encode_global_test<V: Into<WamValue>>(id: u16, val: V) -> String {
        let mut writer = WamBinaryWriter::new(16);
        write_global_attribute(&mut writer, id, &val.into());
        hex_encode(&writer.to_bytes())
    }

    #[test]
    fn test_binary_writer_primitives() {
        let mut writer = WamBinaryWriter::new(4);
        writer.write_u8(0x05);
        writer.write_u16(0x0102);
        writer.write_u32(0x0a0b0c0d);
        writer.write_i8(-1);
        writer.write_bytes(b"hi");
        assert_eq!(hex_encode(&writer.to_bytes()), "0501020a0b0c0dff6869");
    }

    #[test]
    fn test_wam_global_wire_classes() {
        assert_eq!(encode_global_test(5, 0i64), "1005");
        assert_eq!(encode_global_test(5, 1i64), "2005");
        assert_eq!(encode_global_test(5, 100i64), "300564");
        assert_eq!(encode_global_test(5, 300i64), "4005012c");
        assert_eq!(encode_global_test(5, "hi"), "8005026869");
        assert_eq!(encode_global_test(5, WamValue::Null), "0005");
        assert_eq!(encode_global_test(5, true), "2005");
        assert_eq!(encode_global_test(300, 1i64), "28012c");
    }

    #[test]
    fn test_wam_batch_serialization_exact_parity() {
        let mut batch = WamBatch::new(7, 1, 0);
        batch.write_event(100, 472, 1, &[WamField::new(1, 3i64)]);
        assert!(batch.has_events());
        assert_eq!(
            hex_encode(&batch.to_bytes()),
            "57414d0507000100302f642901d8360103"
        );
    }

    #[test]
    fn test_wam_delta_globals() {
        let mut batch = WamBatch::new(1, 1, 0);
        batch.set_global(3543, 42i64);
        let len1 = batch.len();
        batch.set_global(3543, 42i64);
        assert_eq!(batch.len(), len1, "Unchanged global must not be re-emitted");
        batch.set_global(3543, 99i64);
        assert!(batch.len() > len1, "Changed global must be re-emitted");
    }

    #[test]
    fn test_wam_iq_node_structure() {
        let mut batch = WamBatch::new(1, 1, 0);
        batch.write_event(1700000000, 100, 1, &[WamField::new(2, 42i64)]);
        let iq = batch.build_iq_node(1700000000);
        assert_eq!(iq.tag, "iq");
        assert_eq!(iq.attrs.get("xmlns").map(String::as_str), Some("w:stats"));
        assert_eq!(iq.attrs.get("type").map(String::as_str), Some("set"));
    }
}
