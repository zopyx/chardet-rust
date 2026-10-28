//! Stage 0c: Binary content detection.
//!
//! This module detects binary (non-text) content by:
//! 1. Checking for common binary file signatures (magic numbers)
//! 2. Analyzing the distribution of control characters and null bytes
//!
//! Binary detection runs after BOM and UTF-16/32 checks to avoid
//! misclassifying Unicode text with null bytes as binary.

/// Threshold for binary classification based on control character ratio.
///
/// If more than 1% of bytes are binary-indicating control characters,
/// the content is classified as binary.
const BINARY_THRESHOLD: f64 = 0.01;

/// Minimum fraction of high bytes among non-space bytes for EBCDIC text.
const EBCDIC_MIN_HIGH_FRACTION: f64 = 0.25;

/// Minimum fraction of EBCDIC word separators (space or HT) in the data.
const EBCDIC_MIN_SPACE_FRACTION: f64 = 0.02;

/// The EBCDIC space, horizontal-tab and newline bytes.
const EBCDIC_SPACE: u8 = 0x40;
const EBCDIC_HT: u8 = 0x05;
const EBCDIC_NL: u8 = 0x15;

/// Check for common binary file signatures (magic numbers).
///
/// Many binary file formats have distinctive byte signatures at the start
/// of the file. This function checks for the most common formats.
///
/// # Arguments
///
/// * `data` - The byte sequence to check
///
/// # Returns
///
/// `true` if the data starts with a known binary file signature.
///
/// # Supported Formats
///
/// | Format | Signature |
/// |--------|-----------|
/// | PNG | `\x89PNG` |
/// | GIF | `GIF8` |
/// | JPEG | `\xFF\xD8\xFF` |
/// | ZIP | `PK\x03\x04` |
/// | PDF | `%PDF` |
/// | RAR | `Rar!` |
/// | 7z | `7z\xBC\xAF` |
/// | MP3 | `ID3` |
/// | MP4 | `....ftyp` |
#[allow(dead_code)]
pub fn has_binary_signature(data: &[u8]) -> bool {
    if data.len() < 4 {
        return false;
    }

    // Check for common binary file signatures (magic numbers)
    match &data[..4] {
        // PNG: \x89 followed by "PNG"
        [0x89, 0x50, 0x4E, 0x47] => true,
        // GIF: "GIF8" (both 87a and 89a variants)
        [0x47, 0x49, 0x46, 0x38] => true,
        // JPEG: \xFF\xD8\xFF (SOI marker followed by APP0 or APP1)
        [0xFF, 0xD8, 0xFF, _] => true,
        // ZIP / Office documents (DOCX, XLSX, etc.)
        [0x50, 0x4B, 0x03, 0x04] => true,
        // PDF: "%PDF"
        [0x25, 0x50, 0x44, 0x46] => true,
        // RAR: "Rar!"
        [0x52, 0x61, 0x72, 0x21] => true,
        // 7z: "7z\xBC\xAF\x27\x1C"
        [0x37, 0x7A, 0xBC, 0xAF] => true,
        // MP3 with ID3 tag: "ID3"
        [0x49, 0x44, 0x33, _] => true,
        // MP4/QuickTime: first 4 bytes are size, then "ftyp"
        [_, _, _, _] if data.len() > 8 && &data[4..8] == b"ftyp" => true,
        _ => false,
    }
}

/// Determine if data appears to be binary (non-text) content.
///
/// This function uses a two-stage approach:
/// 1. First checks for binary file signatures (fast path)
/// 2. Then analyzes the distribution of control characters
///
/// # Arguments
///
/// * `data` - The byte sequence to analyze
/// * `max_bytes` - Maximum number of bytes to examine
///
/// # Returns
///
/// `true` if the content is classified as binary.
///
/// # Algorithm
///
/// 1. Check for binary file signatures first
/// 2. Count null bytes (0x00) - high proportion indicates binary
/// 3. Count binary-indicating control bytes (0x00-0x08, 0x0E-0x1A, 0x1C-0x1F)
///    - Excludes 0x09 (tab), 0x0A (LF), 0x0B (VT), 0x0C (FF), 0x0D (CR)
///    - Excludes 0x0E-0x0F (used by ISO-2022)
///    - Excludes 0x1B (ESC, used by ISO-2022)
/// 4. Classify as binary if control character ratio exceeds threshold
///
/// # Examples
///
/// ```
/// use _chardet_rs::pipeline::binary::is_binary;
///
/// // Text content
/// assert!(!is_binary(b"Hello, World!\n", 10000));
///
/// // Binary content (high null bytes)
/// let binary = vec![0x00, 0x01, 0x02, 0x03];
/// assert!(is_binary(&binary, 10000));
/// ```
pub fn is_binary(data: &[u8], max_bytes: usize) -> bool {
    let data = &data[..data.len().min(max_bytes)];
    if data.is_empty() {
        return false;
    }

    // Binary-indicator control bytes: 0x00-0x08 and 0x0E-0x1F.  Tab, newline,
    // vertical tab, form feed and carriage return are text, not indicators.
    let binary_count = data
        .iter()
        .filter(|&&b| b <= 0x08 || (0x0E..=0x1F).contains(&b))
        .count();
    if (binary_count as f64) / (data.len() as f64) <= BINARY_THRESHOLD {
        return false;
    }

    // Above the threshold — but EBCDIC text uses 0x05 (HT) and 0x15 (NL) as
    // whitespace, which are binary indicators in ASCII-compatible data.  If the
    // excess comes entirely from those two bytes and the data looks like
    // EBCDIC text, treat it as text.
    let ht_count = data.iter().filter(|&&b| b == EBCDIC_HT).count();
    let nl_count = data.iter().filter(|&&b| b == EBCDIC_NL).count();
    let hard_count = binary_count.saturating_sub(ht_count + nl_count);
    if (hard_count as f64) / (data.len() as f64) > BINARY_THRESHOLD {
        return true;
    }
    let space_count = data.iter().filter(|&&b| b == EBCDIC_SPACE).count() + ht_count;
    let non_space = data.len().saturating_sub(space_count);
    if non_space == 0 {
        return false;
    }
    let high_count = data.iter().filter(|&&b| b >= 0x80).count();
    if (high_count as f64) / (non_space as f64) < EBCDIC_MIN_HIGH_FRACTION {
        return true;
    }
    (space_count as f64) / (data.len() as f64) < EBCDIC_MIN_SPACE_FRACTION
}
