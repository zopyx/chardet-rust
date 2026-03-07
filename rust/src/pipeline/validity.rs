//! Stage 2a: Byte sequence validity filtering.
//!
//! This module filters candidate encodings by validating that the input data
//! contains byte sequences that are structurally valid for each encoding.
//! Encodings that would produce errors when decoding are eliminated early,
//! reducing the candidate pool for expensive statistical analysis.
//!
//! # Validation Strategy
//!
//! - **UTF-8**: Already validated earlier, pass through
//! - **ASCII**: Check all bytes < 0x80
//! - **CJK Multi-byte**: Validate specific byte sequence rules
//! - **Single-byte**: Assume valid (statistical analysis will differentiate)

use crate::registry::EncodingInfo;

/// Filter candidates to only those where data decodes without errors.
///
/// This function eliminates encodings that are structurally incompatible
/// with the input data. For example, if the data contains invalid Shift_JIS
/// sequences, Shift_JIS is removed from the candidate list.
///
/// # Arguments
///
/// * `data` - The byte sequence to validate
/// * `candidates` - The list of candidate encodings to filter
///
/// # Returns
///
/// A vector of encodings that are structurally valid for the data.
///
/// # Algorithm
///
/// 1. Empty data: return all candidates
/// 2. For each candidate, apply encoding-specific validation
/// 3. Collect candidates that pass validation
///
/// # Examples
///
/// ```
/// use _chardet_rs::pipeline::validity::filter_by_validity;
/// use _chardet_rs::registry::{REGISTRY, get_candidates};
/// use _chardet_rs::enums::EncodingEra;
///
/// // Valid ASCII data
/// let data = b"Hello, World!";
/// let candidates = get_candidates(EncodingEra::All);
/// let valid = filter_by_validity(data, &candidates);
/// assert!(!valid.is_empty());
/// ```
pub fn filter_by_validity<'a>(
    data: &[u8],
    candidates: &[&'a EncodingInfo],
) -> Vec<&'a EncodingInfo> {
    if data.is_empty() {
        return candidates.to_vec();
    }

    candidates
        .iter()
        .filter(|enc| is_valid_for_encoding(data, enc))
        .copied()
        .collect()
}

/// Check if data is valid for a given encoding.
///
/// # Arguments
///
/// * `data` - The byte sequence to validate
/// * `enc` - The encoding to validate against
///
/// # Returns
///
/// `true` if the data is structurally valid for the encoding.
fn is_valid_for_encoding(data: &[u8], enc: &EncodingInfo) -> bool {
    // For most encodings, we rely on structural validation.
    // For UTF-8, we've already validated it earlier in the pipeline.

    match enc.name {
        "utf-8" | "utf-8-sig" => {
            // UTF-8 validation is done separately
            true
        }
        "ascii" => data.iter().all(|&b| b < 0x80),
        _ if enc.is_multibyte => {
            // For CJK encodings, we do basic structural checks
            is_valid_multibyte(data, enc.name)
        }
        _ => {
            // For single-byte encodings, most byte sequences are technically valid
            // We rely on statistical analysis to determine the correct one
            true
        }
    }
}

/// Validate data against multi-byte encoding rules.
///
/// # Arguments
///
/// * `data` - The byte sequence to validate
/// * `encoding` - The encoding name
///
/// # Returns
///
/// `true` if the data follows the encoding's structural rules.
fn is_valid_multibyte(data: &[u8], encoding: &str) -> bool {
    match encoding {
        "shift_jis_2004" | "cp932" => is_valid_shift_jis(data),
        "euc-jis-2004" | "euc-jp" => is_valid_euc_jp(data),
        "euc-kr" | "cp949" => is_valid_euc_kr(data),
        "gb18030" | "gb2312" | "gbk" => is_valid_gb18030(data),
        "big5hkscs" | "big5" => is_valid_big5(data),
        "johab" => is_valid_johab(data),
        "hz-gb-2312" => is_valid_hz(data),
        _ => true, // Unknown multibyte - let it through for statistical scoring
    }
}

/// Validate Shift_JIS byte sequences.
///
/// # Shift_JIS Structure
///
/// - ASCII: 0x00-0x7F
/// - Single-byte katakana: 0xA1-0xDF
/// - Lead bytes: 0x81-0x9F, 0xE0-0xFC
/// - Trail bytes: 0x40-0x7E, 0x80-0xFC
///
/// # Validation Rules
///
/// 1. Track valid vs invalid sequences
/// 2. Require at least 2 valid sequences
/// 3. Allow up to 30% invalid sequences (for truncated data)
fn is_valid_shift_jis(data: &[u8]) -> bool {
    let mut valid_pairs = 0;
    let mut valid_single = 0;
    let mut invalid_sequences = 0;

    let mut i = 0;
    while i < data.len() {
        let b = data[i];
        if b < 0x80 {
            i += 1;
            continue;
        }

        // Single-byte half-width katakana: 0xA0-0xDF (valid in CP932)
        if (0xA0..=0xDF).contains(&b) {
            valid_single += 1;
            i += 1;
            continue;
        }

        // Lead bytes: 0x81-0x9F, 0xE0-0xFC (includes 0xF0-0xFC for CP932 extended)
        if (0x81..=0x9F).contains(&b) || (0xE0..=0xFC).contains(&b) {
            if i + 1 >= data.len() {
                // Truncated at end - count as valid
                valid_pairs += 1;
                break;
            }
            let trail = data[i + 1];
            // Trail bytes: 0x40-0x7E, 0x80-0xFC
            if (0x40..=0x7E).contains(&trail) || (0x80..=0xFC).contains(&trail) {
                valid_pairs += 1;
                i += 2;
            } else if trail == 0x0A || trail == 0x0D {
                // Line break after lead - might be split character
                invalid_sequences += 1;
                i += 1; // Only advance by 1 to check the control char next
            } else {
                // Invalid trail byte
                invalid_sequences += 1;
                i += 2;
            }
        } else {
            // This is a high byte that's not a valid lead byte or single-byte char
            // (could be a trail byte in a valid sequence that we started mid-way)
            invalid_sequences += 1;
            i += 1;
        }
    }

    // Need at least some valid sequences to consider it valid Shift_JIS
    let total_valid = valid_pairs + valid_single;
    if total_valid < 2 {
        return false;
    }

    // Allow up to 30% invalid sequences
    let total = total_valid + invalid_sequences;
    if total > 0 && (invalid_sequences as f64) / (total as f64) > 0.30 {
        return false;
    }

    true
}

/// Validate EUC-JP byte sequences.
///
/// # EUC-JP Structure
///
/// - ASCII: 0x00-0x7F
/// - SS2 (half-width katakana): 0x8E + 0xA1-0xDF
/// - SS3 (JIS X 0212): 0x8F + 0xA1-0xFE + 0xA1-0xFE
/// - Two-byte JIS X 0208: 0xA1-0xFE + 0xA1-0xFE
fn is_valid_euc_jp(data: &[u8]) -> bool {
    let mut i = 0;
    while i < data.len() {
        let b = data[i];
        if b < 0x80 {
            i += 1;
            continue;
        }
        if b == 0x8E {
            // SS2: half-width katakana
            if i + 1 >= data.len() {
                return true;
            }
            if !(0xA1..=0xDF).contains(&data[i + 1]) {
                return false;
            }
            i += 2;
        } else if b == 0x8F {
            // SS3: JIS X 0212
            if i + 2 >= data.len() {
                return true;
            }
            if !(0xA1..=0xFE).contains(&data[i + 1]) || !(0xA1..=0xFE).contains(&data[i + 2]) {
                return false;
            }
            i += 3;
        } else if (0xA1..=0xFE).contains(&b) {
            // Two-byte sequence
            if i + 1 >= data.len() {
                return true;
            }
            if !(0xA1..=0xFE).contains(&data[i + 1]) {
                return false;
            }
            i += 2;
        } else {
            return false;
        }
    }
    true
}

/// Validate EUC-KR byte sequences.
///
/// # EUC-KR Structure
///
/// - ASCII: 0x00-0x7F
/// - Two-byte KS X 1001: 0xA1-0xFE + 0xA1-0xFE
fn is_valid_euc_kr(data: &[u8]) -> bool {
    let mut i = 0;
    while i < data.len() {
        let b = data[i];
        if b < 0x80 {
            i += 1;
            continue;
        }
        if (0xA1..=0xFE).contains(&b) {
            if i + 1 >= data.len() {
                return true;
            }
            if !(0xA1..=0xFE).contains(&data[i + 1]) {
                return false;
            }
            i += 2;
        } else {
            return false;
        }
    }
    true
}

/// Validate GB18030 byte sequences.
///
/// # GB18030 Structure
///
/// - ASCII: 0x00-0x7F
/// - Two-byte GBK: 0x81-0xFE + 0x40-0xFE
/// - Four-byte: 0x81-0xFE + 0x30-0x39 + 0x81-0xFE + 0x30-0x39
fn is_valid_gb18030(data: &[u8]) -> bool {
    let mut i = 0;
    while i < data.len() {
        let b = data[i];
        if b < 0x80 {
            i += 1;
            continue;
        }
        if (0x81..=0xFE).contains(&b) {
            if i + 1 >= data.len() {
                return true;
            }
            let b2 = data[i + 1];
            if (0x30..=0x39).contains(&b2) {
                // 4-byte sequence
                if i + 3 >= data.len() {
                    return true;
                }
                if !(0x81..=0xFE).contains(&data[i + 2]) || !(0x30..=0x39).contains(&data[i + 3]) {
                    return false;
                }
                i += 4;
            } else if (0x40..=0xFE).contains(&b2) {
                // 2-byte sequence
                i += 2;
            } else {
                return false;
            }
        } else {
            return false;
        }
    }
    true
}

/// Validate Big5 byte sequences.
///
/// # Big5 Structure
///
/// - ASCII: 0x00-0x7F
/// - Lead bytes: 0xA1-0xF9
/// - Trail bytes: 0x40-0x7E, 0xA1-0xFE
fn is_valid_big5(data: &[u8]) -> bool {
    let mut i = 0;
    while i < data.len() {
        let b = data[i];
        if b < 0x80 {
            i += 1;
            continue;
        }
        if (0xA1..=0xF9).contains(&b) {
            if i + 1 >= data.len() {
                return true;
            }
            let trail = data[i + 1];
            if !((0x40..=0x7E).contains(&trail) || (0xA1..=0xFE).contains(&trail)) {
                return false;
            }
            i += 2;
        } else {
            return false;
        }
    }
    true
}

/// Validate Johab byte sequences.
///
/// # Johab Structure
///
/// - ASCII: 0x00-0x7F
/// - Lead bytes: 0x84-0xD3, 0xD8-0xDE, 0xE0-0xF9
/// - Trail bytes: 0x31-0x7E, 0x81-0xFE
fn is_valid_johab(data: &[u8]) -> bool {
    let mut i = 0;
    while i < data.len() {
        let b = data[i];
        if b < 0x80 {
            i += 1;
            continue;
        }
        // Lead: 0x84-0xD3, 0xD8-0xDE, 0xE0-0xF9
        if (0x84..=0xD3).contains(&b) || (0xD8..=0xDE).contains(&b) || (0xE0..=0xF9).contains(&b) {
            if i + 1 >= data.len() {
                return true;
            }
            let trail = data[i + 1];
            // Trail: 0x31-0x7E, 0x81-0xFE
            if !((0x31..=0x7E).contains(&trail) || (0x81..=0xFE).contains(&trail)) {
                return false;
            }
            i += 2;
        } else {
            return false;
        }
    }
    true
}

/// Validate HZ-GB-2312 byte sequences.
///
/// # HZ-GB-2312 Structure
///
/// - ASCII mode: bytes < 0x80 (except ~)
/// - GB mode: `~{` enters, `~}` exits
/// - In GB mode: pairs of bytes in 0x21-0x7E
/// - Escaped tilde: `~~` represents literal ~
fn is_valid_hz(data: &[u8]) -> bool {
    // HZ-GB-2312 uses ~{ and ~} to shift in/out of GB mode
    // In GB mode, characters are pairs of bytes in 0x21-0x7E
    let mut in_gb_mode = false;
    let mut i = 0;

    while i < data.len() {
        if data[i] == b'~' {
            if i + 1 < data.len() {
                if data[i + 1] == b'{' {
                    in_gb_mode = true;
                    i += 2;
                    continue;
                } else if data[i + 1] == b'}' {
                    in_gb_mode = false;
                    i += 2;
                    continue;
                } else if data[i + 1] == b'~' {
                    // Escaped tilde
                    i += 2;
                    continue;
                }
            }
        }

        if in_gb_mode {
            // In GB mode, expect pairs in 0x21-0x7E
            if !(0x21..=0x7E).contains(&data[i]) {
                return false;
            }
        } else if data[i] > 0x7F {
            return false;
        }

        i += 1;
    }

    true
}
