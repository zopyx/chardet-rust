//! Stage 1d: UTF-8 structural validation.
//!
//! This module validates UTF-8 byte sequences according to the Unicode standard.
//! It checks for valid multi-byte sequences, rejects overlong encodings,
/// and ensures no invalid codepoints (surrogates, out-of-range) are present.
///
/// # UTF-8 Encoding Rules
///
/// | Code Point Range | Byte Sequence |
/// |-----------------|---------------|
/// | U+0000-U+007F | 0xxxxxxx |
/// | U+0080-U+07FF | 110xxxxx 10xxxxxx |
/// | U+0800-U+FFFF | 1110xxxx 10xxxxxx 10xxxxxx |
/// | U+10000-U+10FFFF | 11110xxx 10xxxxxx 10xxxxxx 10xxxxxx |

use super::DetectionResult;

/// Base confidence for UTF-8 detection.
///
/// This is the starting confidence when multi-byte sequences are found.
const BASE_CONFIDENCE: f64 = 0.80;

/// Maximum confidence for UTF-8 detection.
///
/// Reached when a significant portion of the text is multi-byte.
const MAX_CONFIDENCE: f64 = 0.99;

/// Scale factor for multi-byte ratio in confidence calculation.
///
/// Higher values cause confidence to ramp up faster with multi-byte content.
const MB_RATIO_SCALE: f64 = 6.0;

/// Validate UTF-8 byte structure and detect encoding.
///
/// This function performs strict UTF-8 validation according to RFC 3629:
/// - Validates continuation bytes
/// - Rejects overlong encodings
/// - Rejects UTF-16 surrogate halves (U+D800-U+DFFF)
/// - Rejects codepoints above U+10FFFF
///
/// # Arguments
///
/// * `data` - The byte sequence to validate
///
/// # Returns
///
/// - `Some(DetectionResult)` with "utf-8" encoding if valid multi-byte UTF-8
/// - `None` if data is pure ASCII or contains invalid UTF-8 sequences
///
/// # Confidence Calculation
///
/// Confidence scales with the proportion of multi-byte bytes:
/// ```text
/// confidence = BASE_CONFIDENCE + (MAX_CONFIDENCE - BASE_CONFIDENCE) * min(mb_ratio * 6, 1)
/// ```
///
/// Where `mb_ratio` is the fraction of bytes that are part of multi-byte sequences.
/// This gives higher confidence to text with more non-ASCII content.
///
/// # Validation Rules
///
/// ## 2-byte sequences (0xC2-0xDF)
/// - Leading byte: 0xC2-0xDF (0xC0-0xC1 are overlong and rejected)
/// - Continuation: 0x80-0xBF
///
/// ## 3-byte sequences (0xE0-0xEF)
/// - Leading byte: 0xE0-0xEF
/// - Continuation: 0x80-0xBF for both bytes
/// - Special cases:
///   - 0xE0: second byte must be >= 0xA0 (prevents overlong)
///   - 0xED: second byte must be <= 0x9F (prevents surrogates)
///
/// ## 4-byte sequences (0xF0-0xF4)
/// - Leading byte: 0xF0-0xF4 (0xF5-0xFF are invalid)
/// - Continuation: 0x80-0xBF for all three bytes
/// - Special cases:
///   - 0xF0: second byte must be >= 0x90 (prevents overlong)
///   - 0xF4: second byte must be <= 0x8F (prevents > U+10FFFF)
///
/// # Examples
///
/// ```
/// use _chardet_rs::pipeline::utf8::detect_utf8;
///
/// // Valid UTF-8 with multi-byte chars
/// let result = detect_utf8("Hello, 世界!".as_bytes());
/// assert!(result.is_some());
///
/// // Pure ASCII - returns None (handled by ASCII stage)
/// assert!(detect_utf8(b"Hello").is_none());
///
/// // Invalid UTF-8 (overlong encoding of '/')
/// assert!(detect_utf8(b"\xC0\xAF").is_none());
/// ```
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

        // ASCII byte - fast path
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

/// Get the expected length of a UTF-8 sequence from its leading byte.
///
/// # Arguments
///
/// * `byte` - The leading byte
///
/// # Returns
///
/// The expected sequence length (1-4), or 0 if the byte is invalid.
///
/// # Examples
///
/// ```
/// use _chardet_rs::pipeline::utf8::utf8_sequence_length;
///
/// assert_eq!(utf8_sequence_length(0x41), 1); // 'A'
/// assert_eq!(utf8_sequence_length(0xC3), 2); // Latin chars
/// assert_eq!(utf8_sequence_length(0xE4), 3); // CJK chars
/// assert_eq!(utf8_sequence_length(0xF0), 4); // Emoji, supplementary
/// assert_eq!(utf8_sequence_length(0x80), 0); // Invalid leading byte
/// ```
#[allow(dead_code)]
pub fn utf8_sequence_length(byte: u8) -> usize {
    match byte {
        0x00..=0x7F => 1,
        0xC2..=0xDF => 2,
        0xE0..=0xEF => 3,
        0xF0..=0xF4 => 4,
        _ => 0, // Invalid leading byte
    }
}

/// Check if a byte is a valid UTF-8 continuation byte.
///
/// # Arguments
///
/// * `byte` - The byte to check
///
/// # Returns
///
/// `true` if the byte is a valid continuation byte (0x80-0xBF).
#[inline]
#[allow(dead_code)]
pub fn is_continuation_byte(byte: u8) -> bool {
    (0x80..=0xBF).contains(&byte)
}
