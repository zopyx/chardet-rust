//! Stage 1d: UTF-8 structural validation.

use super::DetectionResult;

// Confidence curve parameters for UTF-8 detection.
const BASE_CONFIDENCE: f64 = 0.80;
const MAX_CONFIDENCE: f64 = 0.99;
const MB_RATIO_SCALE: f64 = 6.0;

/// Validate UTF-8 byte structure.
///
/// Returns a result only if multi-byte sequences are found (pure ASCII
/// is handled by the ASCII stage).
pub fn detect_utf8(data: &[u8]) -> Option<DetectionResult> {
    if data.is_empty() {
        return None;
    }

    let mut i = 0;
    let length = data.len();
    let mut multibyte_sequences = 0;
    let mut multibyte_bytes = 0;

    while i < length {
        let byte = data[i];

        if byte < 0x80 {
            i += 1;
            continue;
        }

        // Determine expected sequence length from leading byte.
        // 0xC0-0xC1 are overlong 2-byte encodings of ASCII, so we start at 0xC2.
        let seq_len = if (0xC2..=0xDF).contains(&byte) {
            2
        } else if (0xE0..=0xEF).contains(&byte) {
            3
        } else if (0xF0..=0xF4).contains(&byte) {
            4
        } else {
            // Invalid start byte (0x80-0xC1, 0xF5-0xFF)
            return None;
        };

        // Truncated final sequence (e.g. from max_bytes slicing) — treat as
        // valid since the bytes seen so far are structurally correct.
        if i + seq_len > length {
            break;
        }

        // Validate continuation bytes (must be 0x80-0xBF)
        for j in 1..seq_len {
            if data[i + j] < 0x80 || data[i + j] > 0xBF {
                return None;
            }
        }

        // Reject overlong encodings and surrogates
        match seq_len {
            3 => {
                // 0xE0: second byte must be >= 0xA0 (prevents overlong 3-byte)
                if byte == 0xE0 && data[i + 1] < 0xA0 {
                    return None;
                }
                // 0xED: second byte must be <= 0x9F (prevents UTF-16 surrogates U+D800-U+DFFF)
                if byte == 0xED && data[i + 1] > 0x9F {
                    return None;
                }
            }
            4 => {
                // 0xF0: second byte must be >= 0x90 (prevents overlong 4-byte)
                if byte == 0xF0 && data[i + 1] < 0x90 {
                    return None;
                }
                // 0xF4: second byte must be <= 0x8F (prevents codepoints above U+10FFFF)
                if byte == 0xF4 && data[i + 1] > 0x8F {
                    return None;
                }
            }
            _ => {}
        }

        multibyte_sequences += 1;
        multibyte_bytes += seq_len;
        i += seq_len;
    }

    // Pure ASCII — let the ASCII detector handle it
    if multibyte_sequences == 0 {
        return None;
    }

    // Confidence scales with the proportion of multi-byte bytes in the data.
    let mb_ratio = multibyte_bytes as f64 / length as f64;
    let confidence_range = MAX_CONFIDENCE - BASE_CONFIDENCE;
    let confidence = MAX_CONFIDENCE
        .min(BASE_CONFIDENCE + confidence_range * (mb_ratio * MB_RATIO_SCALE).min(1.0));

    Some(DetectionResult::new(Some("utf-8"), confidence, None))
}
