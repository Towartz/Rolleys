//! Media dimensions and thumbnail generation utilities for WhatsApp media and stories.

use crate::MediaError;
use image::imageops::FilterType;
use image::{ImageFormat, ImageReader};
use serde::{Deserialize, Serialize};
use std::io::Cursor;

/// Dimension metadata for an image or video.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MediaDimensions {
    pub width: u32,
    pub height: u32,
}

impl MediaDimensions {
    pub fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }

    /// WhatsApp considers media HD if either dimension is >= 1280 pixels.
    #[inline]
    pub fn is_hd(&self) -> bool {
        self.width >= 1280 || self.height >= 1280
    }
}

/// Extracts width and height of an image without decoding pixel buffers.
///
/// Reads file headers directly (e.g. SOF markers for JPEG, IHDR for PNG, VP8 chunks for WebP).
pub fn extract_image_dimensions(bytes: &[u8]) -> Option<MediaDimensions> {
    if bytes.is_empty() {
        return None;
    }

    let cursor = Cursor::new(bytes);
    let reader = ImageReader::new(cursor).with_guessed_format().ok()?;
    let (width, height) = reader.into_dimensions().ok()?;

    if width > 0 && height > 0 {
        Some(MediaDimensions { width, height })
    } else {
        None
    }
}

/// Generates a tiny inline JPEG thumbnail (typically 32px to 64px max edge) for instant story previews.
pub fn generate_micro_thumbnail(image_bytes: &[u8], max_edge: u32) -> Result<Vec<u8>, MediaError> {
    if image_bytes.is_empty() {
        return Err(MediaError::DecryptionFailed(
            "Empty image bytes for thumbnail generation".to_string(),
        ));
    }

    let img = image::load_from_memory(image_bytes).map_err(|e| {
        MediaError::DecryptionFailed(format!("Failed to decode image for micro-thumbnail: {}", e))
    })?;

    let (width, height) = (img.width(), img.height());
    let target = max_edge.max(16);

    let scaled = if width > target || height > target {
        img.resize(target, target, FilterType::Lanczos3)
    } else {
        img
    };

    let mut out_bytes = Vec::new();
    let mut cursor = Cursor::new(&mut out_bytes);
    scaled
        .write_to(&mut cursor, ImageFormat::Jpeg)
        .map_err(|e| {
            MediaError::EncryptionFailed(format!("Failed to encode micro JPEG thumbnail: {}", e))
        })?;

    Ok(out_bytes)
}

/// Scales image bytes appropriately for WhatsApp Status:
/// - `is_hd == false`: Downscales to max edge 1280px (standard quality).
/// - `is_hd == true`: Preserves high fidelity, capping max edge at 1920px (WhatsApp native HD status limit).
///
/// If the image already conforms to the dimensions, returns original bytes without redundant re-encoding.
pub fn scale_image_for_status(image_bytes: &[u8], is_hd: bool) -> Result<Vec<u8>, MediaError> {
    if image_bytes.is_empty() {
        return Err(MediaError::DecryptionFailed(
            "Empty image bytes".to_string(),
        ));
    }

    let max_edge = if is_hd { 1920 } else { 1280 };
    if let Some(dims) = extract_image_dimensions(image_bytes) {
        if dims.width <= max_edge && dims.height <= max_edge {
            return Ok(image_bytes.to_vec());
        }
    }

    let img = image::load_from_memory(image_bytes)
        .map_err(|e| MediaError::DecryptionFailed(format!("Failed to decode image: {}", e)))?;

    let scaled = img.resize(max_edge, max_edge, FilterType::Lanczos3);

    let mut out_bytes = Vec::new();
    let mut cursor = Cursor::new(&mut out_bytes);
    scaled
        .write_to(&mut cursor, ImageFormat::Jpeg)
        .map_err(|e| MediaError::EncryptionFailed(format!("Failed to encode JPEG: {}", e)))?;

    Ok(out_bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dimensions_is_hd() {
        let std_img = MediaDimensions::new(800, 600);
        assert!(!std_img.is_hd());

        let hd_width = MediaDimensions::new(1280, 720);
        assert!(hd_width.is_hd());

        let hd_height = MediaDimensions::new(1080, 1920);
        assert!(hd_height.is_hd());

        let full_hd = MediaDimensions::new(1920, 1080);
        assert!(full_hd.is_hd());
    }

    #[test]
    fn test_micro_thumbnail_generation() {
        // Create a small 100x100 RGB image in memory
        let img = image::RgbImage::new(100, 100);
        let mut jpeg_buf = Vec::new();
        let mut cursor = Cursor::new(&mut jpeg_buf);
        img.write_to(&mut cursor, ImageFormat::Jpeg).unwrap();

        let dims = extract_image_dimensions(&jpeg_buf);
        assert_eq!(dims, Some(MediaDimensions::new(100, 100)));

        let thumb = generate_micro_thumbnail(&jpeg_buf, 48).unwrap();
        assert!(!thumb.is_empty());

        let thumb_dims = extract_image_dimensions(&thumb);
        assert!(thumb_dims.is_some());
        let td = thumb_dims.unwrap();
        assert!(td.width <= 48 && td.height <= 48);
    }

    #[test]
    fn test_scale_image_for_status() {
        let img = image::RgbImage::new(200, 200);
        let mut jpeg_buf = Vec::new();
        let mut cursor = Cursor::new(&mut jpeg_buf);
        img.write_to(&mut cursor, ImageFormat::Jpeg).unwrap();

        // When image is small, it should return original bytes directly
        let scaled_hd = scale_image_for_status(&jpeg_buf, true).unwrap();
        assert_eq!(scaled_hd.len(), jpeg_buf.len());

        let scaled_sd = scale_image_for_status(&jpeg_buf, false).unwrap();
        assert_eq!(scaled_sd.len(), jpeg_buf.len());
    }
}
