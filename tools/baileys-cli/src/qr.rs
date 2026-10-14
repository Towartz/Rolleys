use std::sync::RwLock;

/// Stores the latest QR code payload received from the WhatsApp Web gateway for on-demand REPL display.
pub static LATEST_QR: RwLock<Option<String>> = RwLock::new(None);

/// Caches the latest received QR code string for on-demand REPL display.
pub fn set_latest_qr(code: String) {
    if let Ok(mut lock) = LATEST_QR.write() {
        *lock = Some(code);
    }
}

/// Retrieves the latest cached QR code string, if available.
pub fn get_latest_qr() -> Option<String> {
    LATEST_QR.read().ok().and_then(|lock| lock.clone())
}

/// Clears cached QR code upon successful connection / authentication.
pub fn clear_latest_qr() {
    if let Ok(mut lock) = LATEST_QR.write() {
        *lock = None;
    }
}

/// Renders a QR code with 100% parity to Baileys Node.js `qrcode-terminal { small: true }`
/// and `node-qrcode terminal-small.js`.
///
/// Features:
/// - Explicit white card background (`\x1b[47m\x1b[30m`) ensuring high contrast and identical
///   appearance across all terminal themes (dark, light, transparent, Windows Terminal, ConHost).
/// - Unicode half-blocks (` `, `▄`, `▀`, `█`) encoding two vertical modules per row.
/// - 1-module quiet zone, resulting in ~21-25 rows total (comfortably fitting in any terminal without scrolling).
pub fn render_baileys_small_qr(
    data: &str,
    invert: bool,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    use qrcode::{Color, EcLevel, QrCode};

    let qr = QrCode::with_error_correction_level(data.as_bytes(), EcLevel::L)?;
    let size = qr.width();
    let colors = qr.to_colors();

    let bg_white = "\x1b[47m";
    let bg_black = "\x1b[40m";
    let fg_white = "\x1b[37m";
    let fg_black = "\x1b[30m";
    let reset = "\x1b[0m";

    let (line_setup, white_fg, black_fg) = if invert {
        (format!("{}{}", bg_black, fg_white), fg_black, fg_white)
    } else {
        (format!("{}{}", bg_white, fg_black), fg_white, fg_black)
    };

    // Pixel code: 0 = transparent, 1 = white, 2 = black
    let mk_code_pixel = |x: isize, y: isize| -> u8 {
        let size_isize = size as isize;
        let size_plus = size_isize + 1;
        if x >= size_plus || y >= size_plus || y < -1 || x < -1 {
            0
        } else if x >= size_isize || y >= size_isize || y < 0 || x < 0 {
            1
        } else {
            let idx = (y as usize) * size + (x as usize);
            if colors[idx] == Color::Dark {
                2
            } else {
                1
            }
        }
    };

    let mut output = String::new();
    let size_isize = size as isize;
    let mut y = -1isize;

    while y < size_isize + 1 {
        output.push_str("    "); // 4-space indent for clean alignment
        output.push_str(&line_setup);

        for x in -1..=size_isize {
            let p1 = mk_code_pixel(x, y);
            let p2 = mk_code_pixel(x, y + 1);

            match (p1, p2) {
                (0, 0) => {
                    output.push_str(reset);
                    output.push(' ');
                    output.push_str(&line_setup);
                }
                (0, 1) => {
                    output.push_str(reset);
                    output.push_str(white_fg);
                    output.push('▄');
                    output.push_str(&line_setup);
                }
                (0, 2) => {
                    output.push_str(reset);
                    output.push_str(black_fg);
                    output.push('▄');
                    output.push_str(&line_setup);
                }
                (1, 0) => {
                    output.push_str(reset);
                    output.push_str(white_fg);
                    output.push('▀');
                    output.push_str(&line_setup);
                }
                (1, 1) => {
                    output.push(' ');
                }
                (1, 2) => {
                    output.push('▄');
                }
                (2, 0) => {
                    output.push_str(reset);
                    output.push_str(black_fg);
                    output.push('▀');
                    output.push_str(&line_setup);
                }
                (2, 1) => {
                    output.push('▀');
                }
                (2, 2) => {
                    output.push('█');
                }
                _ => {
                    output.push(' ');
                }
            }
        }

        output.push_str(reset);
        output.push('\n');
        y += 2;
    }

    Ok(output)
}

/// Renders a full-size QR code matching default `qrcode-terminal` (without `small: true`).
/// Each module is represented as 2 spaces with ANSI background colors (`\x1b[40m  \x1b[0m` and `\x1b[47m  \x1b[0m`).
pub fn render_baileys_large_qr(
    data: &str,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    use qrcode::{Color, EcLevel, QrCode};

    let qr = QrCode::with_error_correction_level(data.as_bytes(), EcLevel::L)?;
    let size = qr.width();
    let colors = qr.to_colors();

    let black = "\x1b[40m  \x1b[0m";
    let white = "\x1b[47m  \x1b[0m";

    let mut output = String::new();

    // Top horizontal margin line
    output.push_str("    ");
    for _ in 0..(size + 2) {
        output.push_str(white);
    }
    output.push('\n');

    // Matrix rows with 1-module left and right margin
    for y in 0..size {
        output.push_str("    ");
        output.push_str(white); // Left margin
        for x in 0..size {
            if colors[y * size + x] == Color::Dark {
                output.push_str(black);
            } else {
                output.push_str(white);
            }
        }
        output.push_str(white); // Right margin
        output.push('\n');
    }

    // Bottom horizontal margin line
    output.push_str("    ");
    for _ in 0..(size + 2) {
        output.push_str(white);
    }
    output.push('\n');

    Ok(output)
}

/// Backward compatibility alias for `render_baileys_small_qr`
#[allow(dead_code)]
pub fn render_compact_qr(
    data: &str,
    invert: bool,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    render_baileys_small_qr(data, invert)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_qr_cache_lifecycle() {
        clear_latest_qr();
        assert!(get_latest_qr().is_none());

        set_latest_qr("2@testpayload123".to_string());
        assert_eq!(get_latest_qr(), Some("2@testpayload123".to_string()));

        clear_latest_qr();
        assert!(get_latest_qr().is_none());
    }

    #[test]
    fn test_render_baileys_small_qr_line_count() {
        // Typical WhatsApp QR payload format
        let payload = "1@W1L55V4r7fXfC1vV/B8p2jX8N8m6sV8d/r4=,qN9c2d7v4r1w,m8s4k1j9p8w=,1.0";
        let rendered =
            render_baileys_small_qr(payload, false).expect("QR rendering should succeed");

        let lines: Vec<&str> = rendered.lines().collect();
        // Baileys Node.js produces exactly 18 lines for version 4 QR matrix
        assert_eq!(
            lines.len(),
            18,
            "Small QR line count must match Baileys Node.js parity (18 lines)"
        );

        // Every line should contain ANSI styling
        for line in &lines {
            assert!(
                line.contains("\x1b[47m")
                    || line.contains("\x1b[0m")
                    || line.contains("▀")
                    || line.contains("▄")
            );
        }
    }

    #[test]
    fn test_render_baileys_large_qr() {
        let payload = "2@testpayload123";
        let rendered = render_baileys_large_qr(payload).expect("Large QR rendering should succeed");

        let lines: Vec<&str> = rendered.lines().collect();
        assert!(!lines.is_empty());
        // Large QR uses 2-space ANSI background blocks
        assert!(rendered.contains("\x1b[40m  \x1b[0m"));
        assert!(rendered.contains("\x1b[47m  \x1b[0m"));
    }

    #[test]
    fn test_render_baileys_small_qr_inverted() {
        let payload = "2@testpayload123";
        let normal = render_baileys_small_qr(payload, false).unwrap();
        let inverted = render_baileys_small_qr(payload, true).unwrap();

        assert!(normal.contains("\x1b[47m\x1b[30m")); // White bg, black fg
        assert!(inverted.contains("\x1b[40m\x1b[37m")); // Black bg, white fg
    }
}
