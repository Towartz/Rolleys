//! Streaming Chunked Media Pipeline for WhatsApp Media.
//!
//! Provides zero-OOM streaming download & upload primitives with constant ~64KB memory usage:
//! - Streaming download with incremental AES-256-CBC decryption, trailing 10-byte MAC verification,
//!   and atomic temp-file swap (.part -> destination).
//! - Hybrid streaming upload with in-memory fast path (< 10 MB) and disk-spooling (.enc.tmp) streaming
//!   path (>= 10 MB) with auto-cleanup.

use futures_util::StreamExt;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use subtle::ConstantTimeEq;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_util::io::ReaderStream;
use tracing::debug;

use crate::{
    derive_media_keys, encrypt_media_bytes, upload_encrypted_media, upload_encrypted_media_body,
    MediaConnInfo, MediaError, MediaType, MediaUploadResult, DEFAULT_ORIGIN, DEFAULT_USER_AGENT,
};
use baileys_crypto::{Aes256CbcStreamDecryptor, Aes256CbcStreamEncryptor, HmacSha256, Mac};

/// Thread-safe media progress callback: (bytes_transferred, total_bytes, percentage_0_to_100).
pub type MediaProgressCallback = Arc<dyn Fn(u64, u64, f32) + Send + Sync + 'static>;

/// Constant 64 KB chunk buffer for streaming I/O.
pub const STREAM_CHUNK_SIZE: usize = 64 * 1024;

/// Threshold (10 MB) under which media is encrypted in RAM, and at/above which streaming disk spooling is used.
pub const STREAM_SPOOL_THRESHOLD_BYTES: u64 = 10 * 1024 * 1024;

/// Result metadata returned by streaming upload.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StreamingUploadResult {
    pub upload_res: MediaUploadResult,
    pub media_key: [u8; 32],
    pub file_sha256: [u8; 32],
    pub file_enc_sha256: [u8; 32],
    pub file_length: u64,
}

/// RAII guard that automatically cleans up temporary files on disk if dropped before being committed.
pub struct TempFileGuard {
    path: PathBuf,
    committed: bool,
}

impl TempFileGuard {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            committed: false,
        }
    }

    pub fn commit(mut self) {
        self.committed = true;
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempFileGuard {
    fn drop(&mut self) {
        if !self.committed && self.path.exists() {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

/// Streams and decrypts media directly from WhatsApp CDN to disk with ~64KB constant memory usage.
///
/// Verifies the trailing 10-byte truncated HMAC-SHA256 signature upon EOF before atomically renaming
/// the `.part` file to the final destination path. If corrupted or aborted, the partial file is deleted.
pub async fn download_media_streaming<P: AsRef<Path>>(
    download_url: &str,
    media_key: &[u8],
    media_type: MediaType,
    dest_path: P,
    progress: Option<MediaProgressCallback>,
) -> Result<PathBuf, MediaError> {
    let dest_path = dest_path.as_ref().to_path_buf();
    if let Some(parent) = dest_path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| MediaError::IoError(e.to_string()))?;
    }

    let file_stem = dest_path
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "download".to_string());
    let part_path = dest_path.with_file_name(format!("{}.part", file_stem));
    let guard = TempFileGuard::new(part_path.clone());

    let keys = derive_media_keys(media_key, media_type)?;

    let client = reqwest::Client::builder()
        .build()
        .map_err(|e| MediaError::HttpError(e.to_string()))?;

    let mut last_err = String::new();
    let mut response_opt = None;

    for attempt in 0..3 {
        if attempt > 0 {
            tokio::time::sleep(std::time::Duration::from_millis(400 * (1 << attempt))).await;
        }

        match client
            .get(download_url)
            .header("Origin", DEFAULT_ORIGIN)
            .header("User-Agent", DEFAULT_USER_AGENT)
            .send()
            .await
        {
            Ok(resp) => {
                if resp.status().is_success() {
                    response_opt = Some(resp);
                    break;
                } else {
                    let st = resp.status();
                    last_err = format!("Server returned status {}", st);
                    if st.as_u16() == 404 || st.as_u16() == 410 {
                        break;
                    }
                }
            }
            Err(e) => {
                last_err = format!("Failed to connect to media CDN: {}", e);
            }
        }
    }

    let mut response = response_opt.ok_or(MediaError::HttpError(last_err))?;
    let total_bytes = response.content_length().unwrap_or(0);

    let mut decryptor = Aes256CbcStreamDecryptor::new(&keys.cipher_key, &keys.iv)
        .map_err(|e| MediaError::DecryptionFailed(e.to_string()))?;

    let mut hmac_state = HmacSha256::new_from_slice(&keys.mac_key)
        .map_err(|e| MediaError::CryptoError(e.to_string()))?;
    hmac_state.update(&keys.iv);

    let mut out_file = tokio::fs::File::create(&part_path)
        .await
        .map_err(|e| MediaError::IoError(e.to_string()))?;

    let mut mac_tail_buf = Vec::with_capacity(STREAM_CHUNK_SIZE);
    let mut bytes_downloaded: u64 = 0;

    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|e| MediaError::HttpError(format!("Chunk download error: {}", e)))?
    {
        mac_tail_buf.extend_from_slice(&chunk);
        bytes_downloaded += chunk.len() as u64;

        // Retain trailing 10 bytes in mac_tail_buf for HMAC tag verification at EOF
        if mac_tail_buf.len() > 10 {
            let ct_len = mac_tail_buf.len() - 10;
            let ct_slice: Vec<u8> = mac_tail_buf.drain(..ct_len).collect();

            hmac_state.update(&ct_slice);
            let decrypted = decryptor
                .update(&ct_slice)
                .map_err(|e| MediaError::DecryptionFailed(e.to_string()))?;

            if !decrypted.is_empty() {
                out_file
                    .write_all(&decrypted)
                    .await
                    .map_err(|e| MediaError::IoError(e.to_string()))?;
            }
        }

        if let Some(ref cb) = progress {
            let pct = if total_bytes > 0 {
                ((bytes_downloaded as f64 / total_bytes as f64) * 100.0) as f32
            } else {
                0.0
            };
            cb(bytes_downloaded, total_bytes, pct.clamp(0.0, 99.9));
        }
    }

    if mac_tail_buf.len() != 10 {
        return Err(MediaError::PayloadTooShort);
    }

    // Authenticate trailing 10-byte truncated HMAC-SHA256
    let calculated_mac = hmac_state.finalize().into_bytes();
    if calculated_mac[..10].ct_eq(&mac_tail_buf).unwrap_u8() != 1 {
        return Err(MediaError::InvalidMacSignature);
    }

    // Finalize decryptor and write remaining block
    let final_plain = decryptor
        .finalize()
        .map_err(|e| MediaError::DecryptionFailed(e.to_string()))?;
    if !final_plain.is_empty() {
        out_file
            .write_all(&final_plain)
            .await
            .map_err(|e| MediaError::IoError(e.to_string()))?;
    }

    out_file
        .flush()
        .await
        .map_err(|e| MediaError::IoError(e.to_string()))?;
    drop(out_file);

    guard.commit();
    tokio::fs::rename(&part_path, &dest_path)
        .await
        .map_err(|e| MediaError::IoError(e.to_string()))?;

    if let Some(ref cb) = progress {
        cb(bytes_downloaded, bytes_downloaded, 100.0);
    }

    Ok(dest_path)
}

/// Uploads a media file using the hybrid streaming pipeline:
/// - Files < 10 MB: instant in-memory encryption & upload
/// - Files >= 10 MB: incremental AES-256-CBC spooling to .enc.tmp & streaming HTTP body upload
pub async fn upload_media_file_streaming<P: AsRef<Path>>(
    file_path: P,
    media_type: MediaType,
    media_conn: &MediaConnInfo,
    progress: Option<MediaProgressCallback>,
) -> Result<StreamingUploadResult, MediaError> {
    let file_path = file_path.as_ref();
    let meta = tokio::fs::metadata(file_path)
        .await
        .map_err(|e| MediaError::IoError(format!("Failed to stat media file: {}", e)))?;
    let file_length = meta.len();

    // Fast-path: In-memory for files < 10 MB
    if file_length < STREAM_SPOOL_THRESHOLD_BYTES {
        if let Some(ref cb) = progress {
            cb(0, file_length, 0.0);
        }

        let raw_bytes = tokio::fs::read(file_path)
            .await
            .map_err(|e| MediaError::IoError(e.to_string()))?;

        if let Some(ref cb) = progress {
            cb(file_length / 2, file_length, 50.0);
        }

        let enc = encrypt_media_bytes(&raw_bytes, None, media_type)?;
        let upload_res = upload_encrypted_media(
            &enc.encrypted_bytes,
            &enc.file_enc_sha256,
            media_type,
            media_conn,
        )
        .await?;

        if let Some(ref cb) = progress {
            cb(file_length, file_length, 100.0);
        }

        return Ok(StreamingUploadResult {
            upload_res,
            media_key: enc.media_key,
            file_sha256: enc.file_sha256,
            file_enc_sha256: enc.file_enc_sha256,
            file_length,
        });
    }

    // Heavy-path: Incremental spooling to .enc.tmp for files >= 10 MB
    let mut media_key = [0u8; 32];
    rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut media_key);

    let keys = derive_media_keys(&media_key, media_type)?;

    let spool_stem = format!(
        "{}.{}.enc.tmp",
        file_path.to_string_lossy(),
        rand::random::<u32>()
    );
    let spool_path = PathBuf::from(spool_stem);
    let guard = TempFileGuard::new(spool_path.clone());

    let mut encryptor = Aes256CbcStreamEncryptor::new(&keys.cipher_key, &keys.iv)
        .map_err(|e| MediaError::EncryptionFailed(e.to_string()))?;

    let mut hmac_state = HmacSha256::new_from_slice(&keys.mac_key)
        .map_err(|e| MediaError::CryptoError(e.to_string()))?;
    hmac_state.update(&keys.iv);

    let mut plain_hasher = Sha256::new();
    let mut enc_hasher = Sha256::new();

    let mut src_file = tokio::fs::File::open(file_path)
        .await
        .map_err(|e| MediaError::IoError(e.to_string()))?;
    let mut dst_file = tokio::fs::File::create(&spool_path)
        .await
        .map_err(|e| MediaError::IoError(e.to_string()))?;

    let mut buf = vec![0u8; STREAM_CHUNK_SIZE];
    let mut bytes_read: u64 = 0;

    // Stage 1: Stream Encrypt (0% -> 50% progress)
    loop {
        let n = src_file
            .read(&mut buf)
            .await
            .map_err(|e| MediaError::IoError(e.to_string()))?;
        if n == 0 {
            break;
        }

        plain_hasher.update(&buf[..n]);
        let ct = encryptor.update(&buf[..n]);
        if !ct.is_empty() {
            hmac_state.update(&ct);
            enc_hasher.update(&ct);
            dst_file
                .write_all(&ct)
                .await
                .map_err(|e| MediaError::IoError(e.to_string()))?;
        }

        bytes_read += n as u64;
        if let Some(ref cb) = progress {
            let pct = if file_length > 0 {
                ((bytes_read as f64 / file_length as f64) * 50.0) as f32
            } else {
                0.0
            };
            cb(bytes_read, file_length, pct.clamp(0.0, 50.0));
        }
    }

    let final_ct = encryptor.finalize();
    if !final_ct.is_empty() {
        hmac_state.update(&final_ct);
        enc_hasher.update(&final_ct);
        dst_file
            .write_all(&final_ct)
            .await
            .map_err(|e| MediaError::IoError(e.to_string()))?;
    }

    let full_mac = hmac_state.finalize().into_bytes();
    let mac_10 = &full_mac[..10];
    enc_hasher.update(mac_10);
    dst_file
        .write_all(mac_10)
        .await
        .map_err(|e| MediaError::IoError(e.to_string()))?;

    dst_file
        .flush()
        .await
        .map_err(|e| MediaError::IoError(e.to_string()))?;
    drop(dst_file);
    drop(src_file);

    let file_sha256: [u8; 32] = plain_hasher.finalize().into();
    let file_enc_sha256: [u8; 32] = enc_hasher.finalize().into();

    // Stage 2: Stream Upload spool file (50% -> 100% progress)
    let spool_file = tokio::fs::File::open(&spool_path)
        .await
        .map_err(|e| MediaError::IoError(e.to_string()))?;
    let spool_meta = spool_file
        .metadata()
        .await
        .map_err(|e| MediaError::IoError(e.to_string()))?;
    let spool_len = spool_meta.len();

    let reader_stream = ReaderStream::with_capacity(spool_file, STREAM_CHUNK_SIZE);

    let progress_cb = progress.clone();
    let mut bytes_uploaded: u64 = 0;

    let mapped_stream = reader_stream.map(move |item| {
        if let Ok(ref bytes) = item {
            bytes_uploaded += bytes.len() as u64;
            if let Some(ref cb) = progress_cb {
                let pct = if spool_len > 0 {
                    50.0 + ((bytes_uploaded as f64 / spool_len as f64) * 50.0) as f32
                } else {
                    50.0
                };
                cb(bytes_uploaded, spool_len, pct.clamp(50.0, 99.9));
            }
        }
        item
    });

    let body = reqwest::Body::wrap_stream(mapped_stream);

    debug!(
        "Streaming encrypted media upload for {} ({} bytes)",
        file_path.display(),
        spool_len
    );

    let upload_res =
        upload_encrypted_media_body(body, &file_enc_sha256, media_type, media_conn).await?;

    // Auto-cleanup spool file: guard dropped without commit removes the temporary .enc.tmp
    drop(guard);

    if let Some(ref cb) = progress {
        cb(spool_len, spool_len, 100.0);
    }

    Ok(StreamingUploadResult {
        upload_res,
        media_key,
        file_sha256,
        file_enc_sha256,
        file_length,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_temp_file_guard_cleanup() {
        let tmp_dir = std::env::temp_dir();
        let test_file = tmp_dir.join(format!("test_guard_{}.tmp", rand::random::<u64>()));

        // Test uncommitted guard deletes file on drop
        tokio::fs::write(&test_file, b"transient data")
            .await
            .unwrap();
        assert!(test_file.exists());
        {
            let _guard = TempFileGuard::new(test_file.clone());
        }
        assert!(
            !test_file.exists(),
            "Uncommitted TempFileGuard must remove file on drop"
        );

        // Test committed guard retains file
        tokio::fs::write(&test_file, b"persisted data")
            .await
            .unwrap();
        assert!(test_file.exists());
        {
            let guard = TempFileGuard::new(test_file.clone());
            guard.commit();
        }
        assert!(
            test_file.exists(),
            "Committed TempFileGuard must retain file"
        );
        let _ = tokio::fs::remove_file(&test_file).await;
    }

    #[test]
    fn test_streaming_media_encryption_and_decryption_equivalence() {
        let sample_payload = (0..200_000).map(|i| (i % 251) as u8).collect::<Vec<u8>>();
        let media_type = MediaType::Video;

        let mut media_key = [0u8; 32];
        media_key.copy_from_slice(&[0x7A; 32]);

        // 1. One-shot encryption baseline
        let oneshot_enc =
            encrypt_media_bytes(&sample_payload, Some(&media_key), media_type).unwrap();

        // 2. Incremental streaming encryption
        let keys = derive_media_keys(&media_key, media_type).unwrap();
        let mut encryptor = Aes256CbcStreamEncryptor::new(&keys.cipher_key, &keys.iv).unwrap();
        let mut hmac_state = HmacSha256::new_from_slice(&keys.mac_key).unwrap();
        hmac_state.update(&keys.iv);

        let mut plain_hasher = Sha256::new();
        let mut enc_hasher = Sha256::new();
        let mut streamed_encrypted_bytes = Vec::new();

        for chunk in sample_payload.chunks(STREAM_CHUNK_SIZE) {
            plain_hasher.update(chunk);
            let ct = encryptor.update(chunk);
            if !ct.is_empty() {
                hmac_state.update(&ct);
                enc_hasher.update(&ct);
                streamed_encrypted_bytes.extend_from_slice(&ct);
            }
        }

        let final_ct = encryptor.finalize();
        if !final_ct.is_empty() {
            hmac_state.update(&final_ct);
            enc_hasher.update(&final_ct);
            streamed_encrypted_bytes.extend_from_slice(&final_ct);
        }

        let full_mac = hmac_state.finalize().into_bytes();
        let mac_10 = &full_mac[..10];
        enc_hasher.update(mac_10);
        streamed_encrypted_bytes.extend_from_slice(mac_10);

        let stream_file_sha256: [u8; 32] = plain_hasher.finalize().into();
        let stream_file_enc_sha256: [u8; 32] = enc_hasher.finalize().into();

        assert_eq!(stream_file_sha256, oneshot_enc.file_sha256);
        assert_eq!(stream_file_enc_sha256, oneshot_enc.file_enc_sha256);
        assert_eq!(streamed_encrypted_bytes, oneshot_enc.encrypted_bytes);

        // 3. Incremental streaming decryption
        let mut decryptor = Aes256CbcStreamDecryptor::new(&keys.cipher_key, &keys.iv).unwrap();
        let mut dec_hmac_state = HmacSha256::new_from_slice(&keys.mac_key).unwrap();
        dec_hmac_state.update(&keys.iv);

        let mut mac_tail_buf = Vec::new();
        let mut streamed_decrypted_bytes = Vec::new();

        // Feed in irregular chunk sizes (e.g. 17,000 bytes)
        for chunk in streamed_encrypted_bytes.chunks(17_000) {
            mac_tail_buf.extend_from_slice(chunk);
            if mac_tail_buf.len() > 10 {
                let ct_len = mac_tail_buf.len() - 10;
                let ct_slice: Vec<u8> = mac_tail_buf.drain(..ct_len).collect();

                dec_hmac_state.update(&ct_slice);
                let decrypted = decryptor.update(&ct_slice).unwrap();
                streamed_decrypted_bytes.extend_from_slice(&decrypted);
            }
        }

        assert_eq!(mac_tail_buf.len(), 10);
        let calculated_mac = dec_hmac_state.finalize().into_bytes();
        assert_eq!(&calculated_mac[..10], &mac_tail_buf[..]);

        let final_plain = decryptor.finalize().unwrap();
        streamed_decrypted_bytes.extend_from_slice(&final_plain);

        assert_eq!(streamed_decrypted_bytes, sample_payload);
    }
}
