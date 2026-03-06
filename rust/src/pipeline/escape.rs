//! Early detection of escape-sequence-based encodings (ISO-2022, HZ-GB-2312, UTF-7).

use super::{DetectionResult, DETERMINISTIC_CONFIDENCE};

/// Detect ISO-2022, HZ-GB-2312, and UTF-7 from escape/tilde/plus sequences.
pub fn detect_escape_encoding(data: &[u8]) -> Option<DetectionResult> {
    let has_esc = data.contains(&0x1B);
    let has_tilde = data.contains(&b'~');
    let has_plus = data.contains(&b'+');

    if !has_esc && !has_tilde && !has_plus {
        return None;
    }

    // ISO-2022-JP family: check for base ESC sequences, then classify variant.
    if has_esc {
        // Check for JIS X 0208 sequences
        if contains_subsequence(data, b"\x1b$B")
            || contains_subsequence(data, b"\x1b$@")
            || contains_subsequence(data, b"\x1b(J")
        {
            // JIS X 0213 designation -> modern Japanese branch
            if contains_subsequence(data, b"\x1b$(O") || contains_subsequence(data, b"\x1b$(P") {
                return Some(DetectionResult::new(
                    Some("iso2022-jp-2004"),
                    DETERMINISTIC_CONFIDENCE,
                    Some("ja"),
                ));
            }
            // Half-width katakana SI/SO markers (0x0E / 0x0F)
            if data.contains(&0x0E) && data.contains(&0x0F) {
                return Some(DetectionResult::new(
                    Some("iso2022-jp-ext"),
                    DETERMINISTIC_CONFIDENCE,
                    Some("ja"),
                ));
            }
            // Multinational designations or base codes -> broadest multinational
            return Some(DetectionResult::new(
                Some("iso2022-jp-2"),
                DETERMINISTIC_CONFIDENCE,
                Some("ja"),
            ));
        }

        // ISO-2022-KR: ESC sequence for KS C 5601
        if contains_subsequence(data, b"\x1b$)C") {
            return Some(DetectionResult::new(
                Some("iso-2022-kr"),
                DETERMINISTIC_CONFIDENCE,
                Some("ko"),
            ));
        }
    }

    // HZ-GB-2312: tilde escapes for GB2312
    // Require valid GB2312 byte pairs (0x21-0x7E range) between ~{ and ~} markers.
    if has_tilde
        && contains_subsequence(data, b"~{")
        && contains_subsequence(data, b"~}")
        && has_valid_hz_regions(data)
    {
        return Some(DetectionResult::new(
            Some("hz-gb-2312"),
            DETERMINISTIC_CONFIDENCE,
            Some("zh"),
        ));
    }

    // UTF-7: plus-sign shifts into Base64-encoded Unicode.
    // UTF-7 is a 7-bit encoding: every byte must be in 0x00-0x7F.
    if has_plus && data.iter().all(|&b| b < 0x80) && has_valid_utf7_sequences(data) {
        return Some(DetectionResult::new(
            Some("utf-7"),
            DETERMINISTIC_CONFIDENCE,
            None,
        ));
    }

    None
}

fn contains_subsequence(data: &[u8], pattern: &[u8]) -> bool {
    if pattern.is_empty() || data.len() < pattern.len() {
        return false;
    }
    data.windows(pattern.len()).any(|window| window == pattern)
}

fn has_valid_hz_regions(data: &[u8]) -> bool {
    // Check that at least one ~{...~} region contains valid GB2312 byte pairs.
    let mut start = 0;
    loop {
        let begin = find_subsequence(&data[start..], b"~{");
        if begin.is_none() {
            return false;
        }
        let begin = start + begin.unwrap();
        let end = find_subsequence(&data[begin + 2..], b"~}");
        if end.is_none() {
            return false;
        }
        let end = begin + 2 + end.unwrap();
        let region = &data[begin + 2..end];

        // Must be non-empty, even length, and all bytes in GB2312 range
        if region.len() >= 2
            && region.len() % 2 == 0
            && region.iter().all(|&b| (0x21..=0x7E).contains(&b))
        {
            return true;
        }
        start = end + 2;
    }
}

fn find_subsequence(data: &[u8], pattern: &[u8]) -> Option<usize> {
    if pattern.is_empty() || data.len() < pattern.len() {
        return None;
    }
    data.windows(pattern.len())
        .position(|window| window == pattern)
}

// Base64 alphabet used inside UTF-7 shifted sequences (+<Base64>-)
const B64_CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn has_valid_utf7_sequences(data: &[u8]) -> bool {
    let mut start = 0;
    let mut high_confidence_count = 0;

    loop {
        let shift_pos = data[start..].iter().position(|&b| b == b'+');
        if shift_pos.is_none() {
            break;
        }
        let shift_pos = start + shift_pos.unwrap();
        let pos = shift_pos + 1; // skip the '+'

        if pos >= data.len() {
            break;
        }

        // +- is a literal plus, not a shifted sequence
        if data[pos] == b'-' {
            start = pos + 1;
            continue;
        }

        // Guard: if the '+' is embedded in a base64 stream, it's not real UTF-7
        if is_embedded_in_base64(data, shift_pos) {
            start = pos;
            continue;
        }

        // Collect consecutive Base64 characters
        let mut i = pos;
        while i < data.len() && B64_CHARS.contains(&data[i]) {
            i += 1;
        }
        let b64_len = i - pos;
        let b64_content = &data[pos..i];

        // Check what comes after the base64 sequence
        let next_char = if i < data.len() { Some(data[i]) } else { None };
        let has_explicit_terminator = next_char == Some(b'-');
        let has_implicit_terminator = next_char.map_or(true, |c| !B64_CHARS.contains(&c));

        // A valid UTF-7 sequence must:
        // 1. Have valid base64 content (decodes to valid UTF-16BE)
        // 2. Have at least 2 characters
        if b64_len >= 2 && is_valid_utf7_b64(b64_content) {
            if has_explicit_terminator {
                // Explicit terminator is strong evidence of UTF-7
                // One such sequence is enough to declare UTF-7
                return true;
            } else if has_implicit_terminator {
                // For implicit terminators, we need to be more careful
                // Long sequences (6+ chars = 2+ code units) with common chars are likely UTF-7
                if b64_len >= 6 {
                    if let Some(decoded) = decode_first_utf7_char(b64_content) {
                        if is_common_unicode_char(decoded) {
                            // Check if there's a second code unit and if it's also common
                            if let Some(second) = decode_second_utf7_char(b64_content) {
                                if is_common_unicode_char(second) {
                                    // Long sequence with 2 common chars is likely UTF-7
                                    return true;
                                }
                            }
                        }
                    }
                }
                // For shorter sequences, accumulate count
                else if b64_len >= 2 {
                    if let Some(decoded) = decode_first_utf7_char(b64_content) {
                        if is_common_unicode_char(decoded) {
                            high_confidence_count += 1;
                            if high_confidence_count >= 2 {
                                return true;
                            }
                        }
                    }
                }
            }
        }

        start = i;
    }

    false
}

/// Check if a Unicode code point is in a commonly-used block
fn is_common_unicode_char(code_unit: u16) -> bool {
    match code_unit {
        // Basic Latin (ASCII)
        0x0000..=0x007F => true,
        // Latin-1 Supplement
        0x0080..=0x00FF => true,
        // Latin Extended-A
        0x0100..=0x017F => true,
        // Latin Extended-B
        0x0180..=0x024F => true,
        // IPA Extensions
        0x0250..=0x02AF => true,
        // Spacing Modifier Letters
        0x02B0..=0x02FF => true,
        // Combining Diacritical Marks
        0x0300..=0x036F => true,
        // Greek and Coptic
        0x0370..=0x03FF => true,
        // Cyrillic
        0x0400..=0x04FF => true,
        // Cyrillic Supplement
        0x0500..=0x052F => true,
        // Armenian
        0x0530..=0x058F => true,
        // Hebrew
        0x0590..=0x05FF => true,
        // Arabic
        0x0600..=0x06FF => true,
        // Syriac
        0x0700..=0x074F => true,
        // Arabic Supplement
        0x0750..=0x077F => true,
        // Thaana
        0x0780..=0x07BF => true,
        // Devanagari (Hindi, etc.)
        0x0900..=0x097F => true,
        // Bengali and Assamese
        0x0980..=0x09FF => true,
        // Gurmukhi (Punjabi)
        0x0A00..=0x0A7F => true,
        // Gujarati
        0x0A80..=0x0AFF => true,
        // Oriya
        0x0B00..=0x0B7F => true,
        // Tamil
        0x0B80..=0x0BFF => true,
        // Telugu
        0x0C00..=0x0C7F => true,
        // Kannada
        0x0C80..=0x0CFF => true,
        // Malayalam
        0x0D00..=0x0D7F => true,
        // Sinhala
        0x0D80..=0x0DFF => true,
        // Thai
        0x0E00..=0x0E7F => true,
        // Lao
        0x0E80..=0x0EFF => true,
        // Tibetan
        0x0F00..=0x0FFF => true,
        // Myanmar
        0x1000..=0x109F => true,
        // Georgian
        0x10A0..=0x10FF => true,
        // Hangul Jamo
        0x1100..=0x11FF => true,
        // Latin Extended Additional
        0x1E00..=0x1EFF => true,
        // Greek Extended
        0x1F00..=0x1FFF => true,
        // General Punctuation
        0x2000..=0x206F => true,
        // Superscripts and Subscripts
        0x2070..=0x209F => true,
        // Currency Symbols
        0x20A0..=0x20CF => true,
        // Combining Diacritical Marks for Symbols
        0x20D0..=0x20FF => true,
        // Letterlike Symbols
        0x2100..=0x214F => true,
        // Number Forms
        0x2150..=0x218F => true,
        // Arrows
        0x2190..=0x21FF => true,
        // Mathematical Operators
        0x2200..=0x22FF => true,
        // Miscellaneous Technical
        0x2300..=0x23FF => true,
        // Enclosed Alphanumerics
        0x2460..=0x24FF => true,
        // Box Drawing
        0x2500..=0x257F => true,
        // Block Elements
        0x2580..=0x259F => true,
        // Geometric Shapes
        0x25A0..=0x25FF => true,
        // Miscellaneous Symbols
        0x2600..=0x26FF => true,
        // Dingbats
        0x2700..=0x27BF => true,
        // CJK Symbols and Punctuation
        0x3000..=0x303F => true,
        // Hiragana
        0x3040..=0x309F => true,
        // Katakana
        0x30A0..=0x30FF => true,
        // Bopomofo
        0x3100..=0x312F => true,
        // Hangul Compatibility Jamo
        0x3130..=0x318F => true,
        // Kanbun
        0x3190..=0x319F => true,
        // Bopomofo Extended
        0x31A0..=0x31BF => true,
        // CJK Strokes
        0x31C0..=0x31EF => true,
        // Katakana Phonetic Extensions
        0x31F0..=0x31FF => true,
        // Enclosed CJK Letters and Months
        0x3200..=0x32FF => true,
        // CJK Compatibility
        0x3300..=0x33FF => true,
        // CJK Unified Ideographs Extension A
        0x3400..=0x4DBF => true,
        // CJK Unified Ideographs
        0x4E00..=0x9FFF => true,
        // Hangul Syllables
        0xAC00..=0xD7AF => true,
        // CJK Compatibility Ideographs
        0xF900..=0xFAFF => true,
        // Arabic Presentation Forms-A (not commonly used in text)
        0xFB50..=0xFDFF => false,
        // Arabic Presentation Forms-B
        0xFE70..=0xFEFF => false,
        // Halfwidth and Fullwidth Forms
        0xFF00..=0xFFEF => true,
        // High Surrogates
        0xD800..=0xDBFF => false,
        // Low Surrogates
        0xDC00..=0xDFFF => false,
        // Private Use Area
        0xE000..=0xF8FF => false,
        // Default: not common
        _ => false,
    }
}

/// Decode the first UTF-16 code unit from UTF-7 base64 content
fn decode_first_utf7_char(b64_bytes: &[u8]) -> Option<u16> {
    let n = b64_bytes.len();
    let total_bits = n * 6;
    let num_bytes = total_bits / 8;

    if num_bytes < 2 {
        return None;
    }

    // Decode just enough bits for the first code unit (16 bits)
    let mut bit_buf = 0u32;
    let mut bit_count = 0;
    let mut raw = Vec::new();

    for &c in b64_bytes {
        let val = base64_decode(c).unwrap_or(0);
        bit_buf = (bit_buf << 6) | val as u32;
        bit_count += 6;
        if bit_count >= 8 {
            bit_count -= 8;
            raw.push(((bit_buf >> bit_count) & 0xFF) as u8);
        }
        if raw.len() >= 2 {
            break;
        }
    }

    if raw.len() >= 2 {
        Some(((raw[0] as u16) << 8) | (raw[1] as u16))
    } else {
        None
    }
}

/// Decode the second UTF-16 code unit from UTF-7 base64 content
fn decode_second_utf7_char(b64_bytes: &[u8]) -> Option<u16> {
    let n = b64_bytes.len();
    let total_bits = n * 6;
    let num_bytes = total_bits / 8;

    // Need at least 4 bytes to decode 2 code units
    if num_bytes < 4 {
        return None;
    }

    // Decode enough bits for the second code unit (bits 16-31)
    let mut bit_buf = 0u32;
    let mut bit_count = 0;
    let mut raw = Vec::new();

    for &c in b64_bytes {
        let val = base64_decode(c).unwrap_or(0);
        bit_buf = (bit_buf << 6) | val as u32;
        bit_count += 6;
        if bit_count >= 8 {
            bit_count -= 8;
            raw.push(((bit_buf >> bit_count) & 0xFF) as u8);
        }
        if raw.len() >= 4 {
            break;
        }
    }

    if raw.len() >= 4 {
        Some(((raw[2] as u16) << 8) | (raw[3] as u16))
    } else {
        None
    }
}

fn is_embedded_in_base64(data: &[u8], pos: usize) -> bool {
    // Return True if the '+' at pos is embedded in a base64 stream
    let mut count = 0;
    let mut i = pos.saturating_sub(1);

    while i > 0 {
        let b = data[i];
        if b == 0x0A || b == 0x0D {
            // Skip newlines
            if i == 0 {
                break;
            }
            i -= 1;
            continue;
        }
        if B64_CHARS.contains(&b) || b == b'=' {
            count += 1;
            if i == 0 {
                break;
            }
            i -= 1;
        } else {
            break;
        }
    }

    count >= 4
}

fn is_valid_utf7_b64(b64_bytes: &[u8]) -> bool {
    // Check if base64 bytes decode to valid UTF-16BE.
    // Note: Unlike standard base64, UTF-7 doesn't require padding bits to be zero.
    let n = b64_bytes.len();
    let total_bits = n * 6;

    // Decode to raw bytes and validate as UTF-16BE
    let num_bytes = total_bits / 8;
    let mut raw = Vec::with_capacity(num_bytes);
    let mut bit_buf = 0u32;
    let mut bit_count = 0;

    for &c in b64_bytes {
        let val = base64_decode(c).unwrap_or(0);
        bit_buf = (bit_buf << 6) | val as u32;
        bit_count += 6;
        if bit_count >= 8 {
            bit_count -= 8;
            raw.push(((bit_buf >> bit_count) & 0xFF) as u8);
        }
    }

    // Validate UTF-16BE: check for lone surrogates
    let mut prev_high = false;
    for chunk in raw.chunks_exact(2) {
        let code_unit = ((chunk[0] as u16) << 8) | (chunk[1] as u16);

        if (0xD800..=0xDBFF).contains(&code_unit) {
            // High surrogate
            if prev_high {
                return false; // Consecutive high surrogates
            }
            prev_high = true;
        } else if (0xDC00..=0xDFFF).contains(&code_unit) {
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

    !prev_high
}

fn base64_decode(c: u8) -> Option<u8> {
    match c {
        b'A'..=b'Z' => Some(c - b'A'),
        b'a'..=b'z' => Some(c - b'a' + 26),
        b'0'..=b'9' => Some(c - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}
