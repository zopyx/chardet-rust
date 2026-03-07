//! Stage 0b: UTF-16/UTF-32 detection for data without BOM.
//!
//! This module detects UTF-16 and UTF-32 encoded text even when no BOM is present.
//! It analyzes null-byte patterns and validates the resulting codepoints to
//! distinguish text from binary data.
//!
//! # Detection Strategy
//!
//! ## UTF-32 Detection
//! - BE: First two bytes of each 4-byte unit are null for BMP characters
//! - LE: Last two bytes of each 4-byte unit are null for BMP characters
//! - Validates codepoints are valid Unicode (<= U+10FFFF)
//! - Checks that decoded text looks like text (>70% printable)
//!
//! ## UTF-16 Detection
//! - Counts null bytes in even vs odd positions
//! - ASCII text in UTF-16-LE has nulls at odd positions
//! - ASCII text in UTF-16-BE has nulls at even positions
//! - Validates surrogate pairs (no lone surrogates)

use super::{DetectionResult, DETERMINISTIC_CONFIDENCE};
use crate::pipeline::binary;

/// Sample size for UTF-16/32 pattern analysis.
///
/// We limit analysis to the first 4KB to avoid scanning large files.
const SAMPLE_SIZE: usize = 4096;

/// Minimum bytes needed for reliable UTF-32 detection.
///
/// Need at least 4 full code units (16 bytes) for pattern analysis.
const MIN_BYTES_UTF32: usize = 16;

/// Minimum bytes needed for reliable UTF-16 detection.
///
/// Need at least 5 full code units (10 bytes) for pattern analysis.
const MIN_BYTES_UTF16: usize = 10;

/// Minimum fraction of null bytes in expected positions for UTF-16.
///
/// For UTF-16 with ASCII text, we expect null bytes in every other position.
/// This threshold allows for some non-ASCII characters while still detecting
/// the encoding.
const UTF16_MIN_NULL_FRACTION: f64 = 0.03;

/// Detect UTF-16 or UTF-32 encoding from null-byte patterns.
///
/// This function analyzes null-byte distribution to detect UTF-16 and UTF-32
/// encodings even without a BOM. It also validates that the decoded content
/// looks like text (not binary data with coincidental patterns).
///
/// # Arguments
///
/// * `data` - The byte sequence to analyze
///
/// # Returns
///
/// - `Some(DetectionResult)` with encoding (utf-16-be, utf-16-le, utf-32-be, utf-32-le)
///   and confidence 0.95 if detected
/// - `None` if no UTF-16/32 pattern is detected
///
/// # Algorithm
///
/// 1. Skip binary files (they may have null bytes but shouldn't be detected as text)
/// 2. Try UTF-32 detection first (more specific pattern)
/// 3. Fall back to UTF-16 detection
///
/// # Examples
///
/// ```
/// use _chardet_rs::pipeline::utf1632::detect_utf1632_patterns;
///
/// // UTF-16-LE pattern (ASCII with nulls after each char)
/// let data: Vec<u8> = b"A\x00B\x00C\x00".to_vec();
/// let result = detect_utf1632_patterns(&data);
/// // May detect as UTF-16-LE depending on pattern strength
///
/// // Too short
/// assert!(detect_utf1632_patterns(b"AB").is_none());
/// ```
pub fn detect_utf1632_patterns(data: &[u8]) -> Option<DetectionResult> {
    let sample = &data[..data.len().min(SAMPLE_SIZE)];

    if sample.len() < MIN_BYTES_UTF16 {
        return None;
    }

    // Skip binary files (GIF, PNG, PDF, etc.) - they may have null bytes
    // but should not be detected as UTF-16/32
    if binary::has_binary_signature(sample) {
        return None;
    }

    // Check UTF-32 first (more specific pattern)
    if let Some(result) = check_utf32(sample) {
        return Some(result);
    }

    // Then check UTF-16
    check_utf16(sample)
}

/// Check for UTF-32 encoding patterns.
///
/// UTF-32 encodes each character as a 4-byte value. For BMP characters
/// (U+0000-U+FFFF), this means two null bytes per character.
///
/// # Arguments
///
/// * `data` - The byte sequence to analyze
///
/// # Returns
///
/// Detection result if UTF-32 pattern is found and validated.
///
/// # UTF-32-BE Detection
/// - First byte of each 4-byte unit is 0x00 for BMP
/// - Second byte is often 0x00 for BMP
///
/// # UTF-32-LE Detection
/// - Fourth byte of each 4-byte unit is 0x00 for BMP
/// - Third byte is often 0x00 for BMP
fn check_utf32(data: &[u8]) -> Option<DetectionResult> {
    // Trim to a multiple of 4 bytes
    let trimmed_len = data.len() - (data.len() % 4);
    if trimmed_len < MIN_BYTES_UTF32 {
        return None;
    }
    let data = &data[..trimmed_len];
    let num_units = trimmed_len / 4;

    // UTF-32-BE: first byte of each 4-byte unit must be 0x00
    let be_first_null: usize = (0..data.len()).step_by(4).filter(|&i| data[i] == 0).count();
    // Second byte is 0x00 for BMP characters
    let be_second_null: usize = (0..data.len())
        .step_by(4)
        .filter(|&i| data[i + 1] == 0)
        .count();

    if be_first_null == num_units && (be_second_null as f64) / (num_units as f64) > 0.5 {
        // Try to decode as UTF-32-BE
        let chunks: Vec<u32> = data
            .chunks_exact(4)
            .map(|c| {
                ((c[0] as u32) << 24) | ((c[1] as u32) << 16) | ((c[2] as u32) << 8) | (c[3] as u32)
            })
            .collect();

        // Check if all code points are valid Unicode
        if chunks.iter().all(|&cp| cp <= 0x10FFFF) {
            if looks_like_text(&chunks) {
                return Some(DetectionResult::new(
                    Some("utf-32-be"),
                    DETERMINISTIC_CONFIDENCE,
                    None,
                ));
            }
        }
    }

    // UTF-32-LE: last byte of each 4-byte unit must be 0x00
    let le_last_null: usize = (3..data.len()).step_by(4).filter(|&i| data[i] == 0).count();
    // Third byte is 0x00 for BMP characters
    let le_third_null: usize = (2..data.len()).step_by(4).filter(|&i| data[i] == 0).count();

    if le_last_null == num_units && (le_third_null as f64) / (num_units as f64) > 0.5 {
        // Try to decode as UTF-32-LE
        let chunks: Vec<u32> = data
            .chunks_exact(4)
            .map(|c| {
                (c[0] as u32) | ((c[1] as u32) << 8) | ((c[2] as u32) << 16) | ((c[3] as u32) << 24)
            })
            .collect();

        // Check if all code points are valid Unicode
        if chunks.iter().all(|&cp| cp <= 0x10FFFF) {
            if looks_like_text(&chunks) {
                return Some(DetectionResult::new(
                    Some("utf-32-le"),
                    DETERMINISTIC_CONFIDENCE,
                    None,
                ));
            }
        }
    }

    None
}

/// Check for UTF-16 encoding patterns.
///
/// UTF-16 encodes BMP characters as 2 bytes. For ASCII text, this means
/// one null byte per character (either before or after the ASCII byte).
///
/// # Arguments
///
/// * `data` - The byte sequence to analyze
///
/// # Returns
///
/// Detection result if UTF-16 pattern is found and validated.
///
/// # UTF-16-BE Detection
/// - Null bytes appear at even positions for ASCII text
/// - Pattern: [0x00, 'A', 0x00, 'B', ...]
///
/// # UTF-16-LE Detection
/// - Null bytes appear at odd positions for ASCII text
/// - Pattern: ['A', 0x00, 'B', 0x00, ...]
fn check_utf16(data: &[u8]) -> Option<DetectionResult> {
    let sample_len = data.len() - (data.len() % 2);
    if sample_len < MIN_BYTES_UTF16 {
        return None;
    }
    let data = &data[..sample_len];
    let num_units = sample_len / 2;

    // Count null bytes in even positions (UTF-16-BE high byte for ASCII)
    let be_null_count: usize = (0..sample_len).step_by(2).filter(|&i| data[i] == 0).count();
    // Count null bytes in odd positions (UTF-16-LE high byte for ASCII)
    let le_null_count: usize = (1..sample_len).step_by(2).filter(|&i| data[i] == 0).count();

    let be_frac = be_null_count as f64 / num_units as f64;
    let le_frac = le_null_count as f64 / num_units as f64;

    let mut candidates: Vec<(&str, f64)> = Vec::new();
    if le_frac >= UTF16_MIN_NULL_FRACTION {
        candidates.push(("utf-16-le", le_frac));
    }
    if be_frac >= UTF16_MIN_NULL_FRACTION {
        candidates.push(("utf-16-be", be_frac));
    }

    if candidates.is_empty() {
        return None;
    }

    // If only one candidate, validate and return
    if candidates.len() == 1 {
        let (encoding, _) = candidates[0];
        if validate_utf16(data, encoding == "utf-16-be") {
            return Some(DetectionResult::new(
                Some(encoding),
                DETERMINISTIC_CONFIDENCE,
                None,
            ));
        }
        return None;
    }

    // Both candidates matched - pick the one with higher null fraction
    let (best_encoding, _) = if le_frac > be_frac {
        ("utf-16-le", le_frac)
    } else {
        ("utf-16-be", be_frac)
    };

    if validate_utf16(data, best_encoding == "utf-16-be") {
        Some(DetectionResult::new(
            Some(best_encoding),
            DETERMINISTIC_CONFIDENCE,
            None,
        ))
    } else {
        None
    }
}

/// Validate UTF-16 data for proper surrogate pair usage.
///
/// # Arguments
///
/// * `data` - The byte sequence to validate
/// * `is_be` - True if big-endian, false if little-endian
///
/// # Returns
///
/// `true` if the data contains valid UTF-16 sequences:
/// - No lone surrogates (U+D800-U+DFFF must appear in valid pairs)
/// - No consecutive high surrogates
/// - No high surrogate at end without following low surrogate
fn validate_utf16(data: &[u8], is_be: bool) -> bool {
    // Decode to 16-bit code units
    let units: Vec<u16> = data
        .chunks_exact(2)
        .map(|c| {
            if is_be {
                ((c[0] as u16) << 8) | (c[1] as u16)
            } else {
                (c[0] as u16) | ((c[1] as u16) << 8)
            }
        })
        .collect();

    // Check for valid surrogate pairs
    let mut prev_high = false;
    for unit in &units {
        if (0xD800..=0xDBFF).contains(unit) {
            // High surrogate
            if prev_high {
                return false; // Consecutive high surrogates
            }
            prev_high = true;
        } else if (0xDC00..=0xDFFF).contains(unit) {
            // Low surrogate
            if !prev_high {
                return false; // Lone low surrogate
            }
            prev_high = false;
        } else {
            if prev_high {
                return false; // High surrogate not followed by low
            }
        }
    }

    !prev_high // Should not end with unpaired high surrogate
}

/// Check if decoded codepoints look like text.
///
/// This heuristic filters out binary data that happens to have valid
/// UTF-16/32 patterns but doesn't represent text.
///
/// # Arguments
///
/// * `codepoints` - The decoded Unicode codepoints
///
/// # Returns
///
/// `true` if more than 70% of codepoints are printable text.
///
/// # Excluded Characters
///
/// - Control characters (except tab, LF, CR)
/// - Surrogates (U+D800-U+DFFF)
/// - Private use areas (U+E000-U+F8FF, etc.)
fn looks_like_text(codepoints: &[u32]) -> bool {
    if codepoints.is_empty() {
        return false;
    }

    let sample_len = codepoints.len().min(500);
    let sample = &codepoints[..sample_len];

    let printable: usize = sample
        .iter()
        .filter(|&&cp| {
            // Valid Unicode codepoint
            if cp > 0x10FFFF {
                return false;
            }
            // Control characters (except common whitespace)
            if cp < 0x20 && cp != 0x09 && cp != 0x0A && cp != 0x0D {
                return false;
            }
            // High surrogates (invalid standalone)
            if (0xD800..=0xDFFF).contains(&cp) {
                return false;
            }
            // Private use areas (unlikely to be meaningful text)
            if (0xE000..=0xF8FF).contains(&cp)
                || (0xF0000..=0xFFFFD).contains(&cp)
                || (0x100000..=0x10FFFD).contains(&cp)
            {
                return false;
            }
            true
        })
        .count();

    (printable as f64) / (sample_len as f64) > 0.7
}
