use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Serde helper module to serialize and deserialize byte buffers matching Baileys BufferJSON:
/// `{ "type": "Buffer", "data": "<base64>" }`
#[derive(Serialize, Deserialize)]
struct BufferJsonObject {
    #[serde(rename = "type")]
    buf_type: String,
    data: String,
}

pub fn serialize<S>(bytes: &[u8], serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let obj = BufferJsonObject {
        buf_type: "Buffer".to_string(),
        data: STANDARD.encode(bytes),
    };
    obj.serialize(serializer)
}

pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
where
    D: Deserializer<'de>,
{
    let obj = BufferJsonObject::deserialize(deserializer)?;
    STANDARD.decode(&obj.data).map_err(serde::de::Error::custom)
}
