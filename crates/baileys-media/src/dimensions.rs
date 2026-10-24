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

/// Prepares an image as a WhatsApp profile picture like Baileys' `generateProfilePicture`: a
/// centred square crop scaled to `size` x `size` pixels, encoded as JPEG (quality 50).
pub fn generate_profile_picture(image_bytes: &[u8], size: u32) -> Result<Vec<u8>, MediaError> {
    use image::codecs::jpeg::JpegEncoder;
    use image::ExtendedColorType;

    if image_bytes.is_empty() {
        return Err(MediaError::DecryptionFailed(
            "Empty image bytes for profile picture".to_string(),
        ));
    }
    let img = image::load_from_memory(image_bytes).map_err(|e| {
        MediaError::DecryptionFailed(format!("Failed to decode profile picture: {}", e))
    })?;

    let edge = img.width().min(img.height());
    let square = img.crop_imm(
        (img.width() - edge) / 2,
        (img.height() - edge) / 2,
        edge,
        edge,
    );
    let rgb = square
        .resize_exact(size, size, FilterType::Lanczos3)
        .to_rgb8();

    let mut out = Vec::new();
    JpegEncoder::new_with_quality(&mut out, 50)
        .encode(rgb.as_raw(), size, size, ExtendedColorType::Rgb8)
        .map_err(|e| {
            MediaError::EncryptionFailed(format!("Failed to encode profile picture: {}", e))
        })?;
    Ok(out)
}

/// Prepares the whole image as a WhatsApp profile picture, without cropping: the picture keeps its
/// aspect ratio and is only scaled down so its longest edge is at most `max_edge` pixels (smaller
/// images are not enlarged). Encoded as JPEG (quality 80).
pub fn generate_full_profile_picture(
    image_bytes: &[u8],
    max_edge: u32,
) -> Result<Vec<u8>, MediaError> {
    use image::codecs::jpeg::JpegEncoder;
    use image::ExtendedColorType;

    if image_bytes.is_empty() {
        return Err(MediaError::DecryptionFailed(
            "Empty image bytes for profile picture".to_string(),
        ));
    }
    let img = image::load_from_memory(image_bytes).map_err(|e| {
        MediaError::DecryptionFailed(format!("Failed to decode profile picture: {}", e))
    })?;

    let longest = img.width().max(img.height());
    let rgb = if longest > max_edge {
        // `resize` fits inside the box while keeping the aspect ratio.
        img.resize(max_edge, max_edge, FilterType::Lanczos3)
            .to_rgb8()
    } else {
        img.to_rgb8()
    };

    let (width, height) = (rgb.width(), rgb.height());
    let mut out = Vec::new();
    JpegEncoder::new_with_quality(&mut out, 80)
        .encode(rgb.as_raw(), width, height, ExtendedColorType::Rgb8)
        .map_err(|e| {
            MediaError::EncryptionFailed(format!("Failed to encode profile picture: {}", e))
        })?;
    Ok(out)
}

/// Reads a WebP canvas size from its VP8X, VP8 or VP8L header.
///
/// The `image` crate is built without WebP support, so stickers need their own parser.
pub fn webp_dimensions(bytes: &[u8]) -> Option<MediaDimensions> {
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WEBP" {
        return None;
    }
    let u8_at = |i: usize| bytes.get(i).copied().map(u32::from);
    let u16_at = |i: usize| Some(u8_at(i)? | (u8_at(i + 1)? << 8));
    let u24_at = |i: usize| Some(u16_at(i)? | (u8_at(i + 2)? << 16));
    let mut pos = 12usize;
    while pos + 8 <= bytes.len() {
        let tag = &bytes[pos..pos + 4];
        let len = u24_at(pos + 4)? as usize | ((u8_at(pos + 7)? as usize) << 24);
        let data = pos + 8;
        let found = match tag {
            b"VP8X" => Some((u24_at(data + 4)? + 1, u24_at(data + 7)? + 1)),
            b"VP8 "
                if u8_at(data + 3)? == 0x9d
                    && u8_at(data + 4)? == 0x01
                    && u8_at(data + 5)? == 0x2a =>
            {
                Some((u16_at(data + 6)? & 0x3fff, u16_at(data + 8)? & 0x3fff))
            }
            b"VP8L" if u8_at(data)? == 0x2f => {
                let bits = u24_at(data + 1)? | (u8_at(data + 4)? << 24);
                Some(((bits & 0x3fff) + 1, ((bits >> 14) & 0x3fff) + 1))
            }
            _ => None,
        };
        if let Some((width, height)) = found.filter(|(w, h)| *w > 0 && *h > 0) {
            return Some(MediaDimensions { width, height });
        }
        pos = data.checked_add(len)?.checked_add(len & 1)?;
    }
    None
}

#[cfg(test)]
mod tests {
    #[test]
    fn webp_dimensions_reads_extended_and_lossy_headers() {
        fn riff(chunks: &[u8]) -> Vec<u8> {
            let mut out = b"RIFF".to_vec();
            out.extend_from_slice(&((chunks.len() + 4) as u32).to_le_bytes());
            out.extend_from_slice(b"WEBP");
            out.extend_from_slice(chunks);
            out
        }
        let mut vp8x = b"VP8X".to_vec();
        vp8x.extend_from_slice(&10u32.to_le_bytes());
        vp8x.extend_from_slice(&[0x02, 0, 0, 0, 0xff, 0x01, 0x00, 0xff, 0x01, 0x00]);
        assert_eq!(
            webp_dimensions(&riff(&vp8x)),
            Some(MediaDimensions::new(512, 512))
        );

        let mut vp8 = b"VP8 ".to_vec();
        vp8.extend_from_slice(&10u32.to_le_bytes());
        vp8.extend_from_slice(&[0, 0, 0, 0x9d, 0x01, 0x2a, 0x2c, 0x01, 0xc8, 0x00]);
        assert_eq!(
            webp_dimensions(&riff(&vp8)),
            Some(MediaDimensions::new(300, 200))
        );

        assert_eq!(webp_dimensions(b"\x89PNG\r\n\x1a\n00000000"), None);
        assert_eq!(webp_dimensions(&riff(b"VP8X\xff\xff\xff\xff")), None);
    }

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

    #[test]
    fn profile_picture_is_a_centred_square_jpeg() {
        // 300x200 PNG: red left third, green middle third, blue right third.
        let mut img = image::RgbImage::new(300, 200);
        for (x, _, px) in img.enumerate_pixels_mut() {
            *px = match x / 100 {
                0 => image::Rgb([255, 0, 0]),
                1 => image::Rgb([0, 255, 0]),
                _ => image::Rgb([0, 0, 255]),
            };
        }
        let mut png = Vec::new();
        image::DynamicImage::ImageRgb8(img)
            .write_to(&mut Cursor::new(&mut png), ImageFormat::Png)
            .unwrap();

        let jpeg = generate_profile_picture(&png, 64).unwrap();
        assert_eq!(&jpeg[..3], &[0xFF, 0xD8, 0xFF], "JPEG magic");
        let dims = extract_image_dimensions(&jpeg).unwrap();
        assert_eq!((dims.width, dims.height), (64, 64));

        // The 200x200 centre of a 300x200 image is the green third plus slivers of its neighbours.
        let decoded = image::load_from_memory(&jpeg).unwrap().to_rgb8();
        let centre = decoded.get_pixel(32, 32);
        assert!(
            centre[1] > 200 && centre[0] < 60 && centre[2] < 60,
            "centre should be green: {centre:?}"
        );

        assert!(generate_profile_picture(&[], 64).is_err());
        assert!(generate_profile_picture(b"not an image", 64).is_err());
    }

    #[test]
    fn full_profile_picture_keeps_the_whole_image() {
        fn png(width: u32, height: u32) -> Vec<u8> {
            // Red left third, green middle, blue right third, so a crop would be visible.
            let mut img = image::RgbImage::new(width, height);
            for (x, _, px) in img.enumerate_pixels_mut() {
                *px = match x * 3 / width {
                    0 => image::Rgb([255, 0, 0]),
                    1 => image::Rgb([0, 255, 0]),
                    _ => image::Rgb([0, 0, 255]),
                };
            }
            let mut out = Vec::new();
            image::DynamicImage::ImageRgb8(img)
                .write_to(&mut Cursor::new(&mut out), ImageFormat::Png)
                .unwrap();
            out
        }

        // A small non-square image is neither cropped nor enlarged.
        let small = generate_full_profile_picture(&png(300, 200), 720).unwrap();
        assert_eq!(&small[..3], &[0xFF, 0xD8, 0xFF], "JPEG magic");
        let dims = extract_image_dimensions(&small).unwrap();
        assert_eq!((dims.width, dims.height), (300, 200));
        let decoded = image::load_from_memory(&small).unwrap().to_rgb8();
        let left = decoded.get_pixel(20, 100);
        let right = decoded.get_pixel(280, 100);
        assert!(
            left[0] > 200 && left[1] < 60,
            "left third still red: {left:?}"
        );
        assert!(
            right[2] > 200 && right[0] < 60,
            "right third still blue: {right:?}"
        );

        // A large image is scaled down to the longest edge, keeping its proportions.
        let large = generate_full_profile_picture(&png(2000, 1000), 720).unwrap();
        let dims = extract_image_dimensions(&large).unwrap();
        assert_eq!((dims.width, dims.height), (720, 360));

        assert!(generate_full_profile_picture(&[], 720).is_err());
        assert!(generate_full_profile_picture(b"not an image", 720).is_err());
    }
}
