//! Audio container detection for voice notes and audio messages.
//!
//! WhatsApp voice notes are Ogg/Opus. A client that labels other audio (for example AAC in an MP4
//! container) as `audio/ogg; codecs=opus` makes other WhatsApp clients fail to play it, so the
//! mimetype is taken from the bytes themselves.

/// What the audio bytes actually are.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioFormat {
    OggOpus,
    OggOther,
    Mp4,
    Aac,
    Mp3,
    Wav,
    Flac,
    Amr,
    Unknown,
}

impl AudioFormat {
    /// The mimetype to send; unknown data keeps the voice-note default.
    pub fn mimetype(self) -> &'static str {
        match self {
            AudioFormat::OggOpus | AudioFormat::Unknown => "audio/ogg; codecs=opus",
            AudioFormat::OggOther => "audio/ogg",
            AudioFormat::Mp4 => "audio/mp4",
            AudioFormat::Aac => "audio/aac",
            AudioFormat::Mp3 => "audio/mpeg",
            AudioFormat::Wav => "audio/wav",
            AudioFormat::Flac => "audio/flac",
            AudioFormat::Amr => "audio/amr",
        }
    }
}

/// Offset of the first Ogg page's body (after the 27-byte header and the segment table).
fn ogg_first_body(bytes: &[u8]) -> Option<usize> {
    if bytes.len() < 28 || &bytes[..4] != b"OggS" {
        return None;
    }
    Some(27 + bytes[26] as usize)
}

/// Identifies the audio container from its first bytes.
pub fn detect_audio_format(bytes: &[u8]) -> AudioFormat {
    if let Some(body) = ogg_first_body(bytes) {
        return match bytes.get(body..body + 8) {
            Some(head) if head == b"OpusHead" => AudioFormat::OggOpus,
            _ => AudioFormat::OggOther,
        };
    }
    if bytes.len() >= 12 && &bytes[4..8] == b"ftyp" {
        return AudioFormat::Mp4;
    }
    if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WAVE" {
        return AudioFormat::Wav;
    }
    if bytes.starts_with(b"fLaC") {
        return AudioFormat::Flac;
    }
    if bytes.starts_with(b"#!AMR") {
        return AudioFormat::Amr;
    }
    if bytes.starts_with(b"ID3") {
        return AudioFormat::Mp3;
    }
    if bytes.len() >= 2 && bytes[0] == 0xFF {
        // ADTS (raw AAC) has layer bits 00; MP3 frames have a non-zero layer.
        if bytes[1] & 0xF6 == 0xF0 {
            return AudioFormat::Aac;
        }
        if bytes[1] & 0xE0 == 0xE0 && (bytes[1] >> 1) & 0x03 != 0 {
            return AudioFormat::Mp3;
        }
    }
    AudioFormat::Unknown
}

/// The mimetype to send: what the bytes are when recognisable, else the caller's hint, else the
/// voice-note default.
pub fn resolve_audio_mimetype(bytes: &[u8], hint: Option<&str>) -> String {
    match detect_audio_format(bytes) {
        AudioFormat::Unknown => hint
            .filter(|h| h.starts_with("audio/"))
            .unwrap_or(AudioFormat::Unknown.mimetype())
            .to_string(),
        known => known.mimetype().to_string(),
    }
}

/// Length in whole seconds (rounded up) of an Ogg/Opus stream, from its last page's granule
/// position and the pre-skip in `OpusHead`. `None` when it isn't Ogg/Opus or the length is unknown.
pub fn ogg_opus_duration_secs(bytes: &[u8]) -> Option<u32> {
    let body = ogg_first_body(bytes)?;
    if bytes.get(body..body + 8)? != b"OpusHead" {
        return None;
    }
    let pre_skip = u16::from_le_bytes([*bytes.get(body + 10)?, *bytes.get(body + 11)?]) as u64;
    // The last real page header: "OggS" with version 0, searched from the end.
    let last = (0..bytes.len().saturating_sub(27))
        .rev()
        .find(|&i| &bytes[i..i + 4] == b"OggS" && bytes[i + 4] == 0)?;
    let granule = u64::from_le_bytes(bytes[last + 6..last + 14].try_into().ok()?);
    if granule == u64::MAX || granule <= pre_skip {
        return None;
    }
    let samples = granule - pre_skip;
    Some(samples.div_ceil(48_000) as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ogg_page(header_type: u8, granule: u64, body: &[u8]) -> Vec<u8> {
        let mut page = b"OggS".to_vec();
        page.push(0);
        page.push(header_type);
        page.extend_from_slice(&granule.to_le_bytes());
        page.extend_from_slice(&[0u8; 12]); // serial, sequence, checksum
        page.push(1); // one segment
        page.push(body.len() as u8);
        page.extend_from_slice(body);
        page
    }

    fn opus_head(pre_skip: u16) -> Vec<u8> {
        let mut head = b"OpusHead".to_vec();
        head.extend_from_slice(&[1, 1]);
        head.extend_from_slice(&pre_skip.to_le_bytes());
        head.extend_from_slice(&[0x80, 0xBB, 0, 0, 0, 0, 0]); // 48000 Hz, gain 0, mapping 0
        head
    }

    fn opus_stream(pre_skip: u16, final_granule: u64) -> Vec<u8> {
        let mut data = ogg_page(0x02, 0, &opus_head(pre_skip));
        data.extend(ogg_page(0x00, 0, b"OpusTags....."));
        data.extend(ogg_page(0x04, final_granule, &[1, 2, 3, 4]));
        data
    }

    #[test]
    fn recognises_each_container() {
        assert_eq!(
            detect_audio_format(&opus_stream(312, 48_000)),
            AudioFormat::OggOpus
        );
        assert_eq!(
            detect_audio_format(&ogg_page(0x02, 0, b"\x01vorbis....")),
            AudioFormat::OggOther
        );
        assert_eq!(
            detect_audio_format(b"\0\0\0\x20ftypM4A \0\0\0\0"),
            AudioFormat::Mp4
        );
        assert_eq!(
            detect_audio_format(b"RIFF\0\0\0\0WAVEfmt "),
            AudioFormat::Wav
        );
        assert_eq!(
            detect_audio_format(b"fLaC\0\0\0\0\0\0\0\0"),
            AudioFormat::Flac
        );
        assert_eq!(
            detect_audio_format(b"#!AMR\n\0\0\0\0\0\0\0"),
            AudioFormat::Amr
        );
        assert_eq!(
            detect_audio_format(b"ID3\x04\0\0\0\0\0\0\0\0"),
            AudioFormat::Mp3
        );
        assert_eq!(
            detect_audio_format(&[0xFF, 0xFB, 0x90, 0x00]),
            AudioFormat::Mp3
        );
        assert_eq!(
            detect_audio_format(&[0xFF, 0xF1, 0x50, 0x80]),
            AudioFormat::Aac
        );
        assert_eq!(
            detect_audio_format(b"not audio at all"),
            AudioFormat::Unknown
        );
        assert_eq!(detect_audio_format(&[]), AudioFormat::Unknown);
    }

    #[test]
    fn mimetype_follows_the_bytes_not_the_caller() {
        let m4a = b"\0\0\0\x20ftypM4A \0\0\0\0";
        // AAC in MP4 must not be sent as Opus, whatever the caller says.
        assert_eq!(
            resolve_audio_mimetype(m4a, Some("audio/ogg; codecs=opus")),
            "audio/mp4"
        );
        assert_eq!(
            resolve_audio_mimetype(&opus_stream(312, 48_000), Some("audio/mp4")),
            "audio/ogg; codecs=opus"
        );
        // Unrecognised data uses a sensible hint, then the voice-note default.
        assert_eq!(
            resolve_audio_mimetype(b"???", Some("audio/x-custom")),
            "audio/x-custom"
        );
        assert_eq!(
            resolve_audio_mimetype(b"???", Some("video/mp4")),
            "audio/ogg; codecs=opus"
        );
        assert_eq!(
            resolve_audio_mimetype(b"???", None),
            "audio/ogg; codecs=opus"
        );
    }

    #[test]
    fn opus_duration_comes_from_the_last_granule_position() {
        assert_eq!(
            ogg_opus_duration_secs(&opus_stream(312, 312 + 48_000 * 5)),
            Some(5)
        );
        assert_eq!(
            ogg_opus_duration_secs(&opus_stream(312, 312 + 48_000 * 4 + 100)),
            Some(5)
        );
        assert_eq!(ogg_opus_duration_secs(&opus_stream(312, u64::MAX)), None);
        assert_eq!(ogg_opus_duration_secs(&opus_stream(312, 100)), None);
        assert_eq!(ogg_opus_duration_secs(b"\0\0\0\x20ftypM4A \0\0\0\0"), None);
        assert_eq!(ogg_opus_duration_secs(&[]), None);
    }
}
