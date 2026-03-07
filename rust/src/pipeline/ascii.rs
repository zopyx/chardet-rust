//! Stage 1c: Pure ASCII detection.
//!
//! This module provides fast detection of pure ASCII content. ASCII text
//! is very common and can be detected quickly with a single pass through
//! the data. When detected, it returns immediately with maximum confidence.

use super::DetectionResult;

/// Detect pure ASCII content.
///
/// Checks if all bytes in the data are valid ASCII characters:
/// - Tab (0x09), newline (0x0A), carriage return (0x0D)
/// - Printable ASCII (0x20-0x7E, space through tilde)
///
/// This is an optimization - ASCII is extremely common and can be detected
/// very quickly, avoiding more expensive multi-byte encoding analysis.
///
/// # Arguments
///
/// * `data` - The byte sequence to analyze
///
/// # Returns
///
/// - `Some(DetectionResult)` with encoding "ascii" and confidence 1.0 if all
///   bytes are valid ASCII
/// - `None` if the data contains non-ASCII bytes (>= 0x80) or is empty
///
/// # Note
///
/// This function returns `None` for empty data because empty data could be
/// any encoding. The pipeline will fall back to UTF-8 with low confidence
/// for empty input.
///
/// # Examples
///
/// ```
/// use _chardet_rs::pipeline::ascii::detect_ascii;
///
/// // Pure ASCII text
/// let result = detect_ascii(b"Hello, World!");
/// assert!(result.is_some());
/// assert_eq!(result.unwrap().encoding.as_deref(), Some("ascii"));
///
/// // Contains high byte
/// let result = detect_ascii(b"Hello, \xC3\xA9!"); // UTF-8 for "é"
/// assert!(result.is_none());
///
/// // Empty data
/// assert!(detect_ascii(b"").is_none());
/// ```
pub fn detect_ascii(data: &[u8]) -> Option<DetectionResult> {
    if data.is_empty() {
        return None;
    }

    // Check if all bytes are allowed ASCII:
    // - Tab (0x09), newline (0x0A), carriage return (0x0D)
    // - Printable ASCII (0x20-0x7E)
    let is_ascii = data
        .iter()
        .all(|&b| b == 0x09 || b == 0x0A || b == 0x0D || (b >= 0x20 && b <= 0x7E));

    if is_ascii {
        Some(DetectionResult::new(Some("ascii"), 1.0, None))
    } else {
        None
    }
}

/// Check if a byte is valid ASCII whitespace.
///
/// # Arguments
///
/// * `b` - The byte to check
///
/// # Returns
///
/// `true` if the byte is ASCII whitespace (tab, LF, CR, space, FF, VT).
#[inline]
#[allow(dead_code)]
pub fn is_ascii_whitespace(b: u8) -> bool {
    matches!(b, 0x09 | 0x0A | 0x0D | 0x20 | 0x0B | 0x0C)
}

/// Check if a byte is printable ASCII.
///
/// # Arguments
///
/// * `b` - The byte to check
///
/// # Returns
///
/// `true` if the byte is printable ASCII (0x20-0x7E).
#[inline]
#[allow(dead_code)]
pub fn is_printable_ascii(b: u8) -> bool {
    (0x20..=0x7E).contains(&b)
}
