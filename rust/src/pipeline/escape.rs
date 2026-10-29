//! Early detection of escape-sequence-based encodings (ISO-2022, HZ-GB-2312, UTF-7).
//!
//! This module detects character encodings that use escape sequences or
//! shift characters to switch between different character sets. These
//! encodings are distinctive and can be detected with high confidence
//! when their characteristic sequences are present.
//!
//! # Supported Encodings
//!
//! ## ISO-2022-JP Family
//! - `iso2022-jp-2`: Base ISO-2022-JP with JIS X 0208
//! - `iso2022-jp-2004`: Modern variant with JIS X 0213
//! - `iso2022-jp-ext`: Extended variant with half-width katakana
//!
//! ## ISO-2022-KR
//! - Korean variant using KS C 5601
//!
//! ## HZ-GB-2312
//! - Chinese encoding using `~{` and `~}` shift sequences
//!
//! ## UTF-7
//! - 7-bit safe Unicode using `+Base64-` sequences

use super::{DetectionResult, DETERMINISTIC_CONFIDENCE};
use crate::pipeline::decode_validity;
use crate::pipeline::EVIDENCE_CAP_BYTES;

/// Detect ISO-2022, HZ-GB-2312, and UTF-7 from escape/tilde/plus sequences.
///
/// This function looks for distinctive markers that indicate escape-sequence-based
/// encodings. These encodings are stateful - they switch between different
/// character sets using escape or shift sequences.
///
/// # Arguments
///
/// * `data` - The byte sequence to analyze
///
/// # Returns
///
/// - `Some(DetectionResult)` with encoding and confidence 0.95 if detected
/// - `None` if no escape-sequence encoding is found
///
/// # Detection Criteria
///
/// ## ISO-2022-JP
/// Requires ESC sequences:
/// - `ESC $ B` or `ESC $ @` (JIS X 0208)
/// - `ESC (J` (ASCII/JIS-Roman)
///
/// Variants:
/// - `iso2022-jp-2004`: Additional `ESC $(O` or `ESC $(P` (JIS X 0213)
/// - `iso2022-jp-ext`: Contains SI (0x0E) and SO (0x0F) markers
///
/// ## ISO-2022-KR
/// Requires: `ESC $ ) C` (KS C 5601 designation)
///
/// ## HZ-GB-2312
/// Requires:
/// - `~{` and `~}` markers
/// - Valid GB2312 byte pairs (0x21-0x7E) between markers
///
/// ## UTF-7
/// Requires:
/// - `+` followed by Base64 characters
/// - All bytes < 0x80 (7-bit clean)
/// - Valid UTF-16BE after decoding
///
/// # Examples
///
/// ```
/// use _chardet_rs::pipeline::escape::detect_escape_encoding;
///
/// // ISO-2022-JP (ESC $ B)
/// let iso2022 = b"\x1B$B$H$/$F".to_vec();
/// let result = detect_escape_encoding(&iso2022);
/// assert!(result.is_some());
///
/// // No escape sequences
/// assert!(detect_escape_encoding(b"Hello").is_none());
/// ```
pub fn detect_escape_encoding(data: &[u8]) -> Option<DetectionResult> {
    let has_esc = data.contains(&0x1B);
    let has_tilde = data.contains(&b'~');
    let has_plus = data.contains(&b'+');

    if !has_esc && !has_tilde && !has_plus {
        return None;
    }

    // Where a sequence may *begin* converges on the evidence cap; where it may
    // *end* is one further window on, so a sequence opening just inside the cap
    // is judged whole instead of cut by the boundary.
    let max_start = data.len().min(EVIDENCE_CAP_BYTES);
    let max_end = data.len().min(2 * EVIDENCE_CAP_BYTES);

    // ISO-2022 is a 7-bit encoding: its codecs reject any byte >= 0x80, so a
    // window carrying one is not ISO-2022 whatever designations it holds.
    let seven_bit = data.iter().all(|&b| b < 0x80);

    // ISO-2022-JP family: check for base ESC sequences, then classify variant.
    if has_esc && seven_bit {
        // Check for JIS X 0208 sequences
        if contains_subsequence(data, b"\x1b$B")
            || contains_subsequence(data, b"\x1b$@")
            || contains_subsequence(data, b"\x1b(J")
        {
            // JIS X 0213 designation -> modern Japanese branch
            if contains_subsequence(data, b"\x1b$(O") || contains_subsequence(data, b"\x1b$(P") {
                return Some(DetectionResult::new(
                    Some("iso2022_jp_2004"),
                    DETERMINISTIC_CONFIDENCE,
                    Some("ja"),
                ));
            }
            // Half-width katakana SI/SO markers (0x0E / 0x0F)
            if data.contains(&0x0E) && data.contains(&0x0F) {
                return Some(DetectionResult::new(
                    Some("iso2022_jp_ext"),
                    DETERMINISTIC_CONFIDENCE,
                    Some("ja"),
                ));
            }
            // Multinational designations or base codes -> broadest multinational
            return Some(DetectionResult::new(
                Some("iso2022_jp_2"),
                DETERMINISTIC_CONFIDENCE,
                Some("ja"),
            ));
        }

        // ISO-2022-KR: ESC sequence for KS C 5601
        if contains_subsequence(data, b"\x1b$)C") {
            return Some(DetectionResult::new(
                Some("iso2022_kr"),
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
        && decode_validity::decodes_without_error(data, "hz")
    {
        return Some(DetectionResult::new(
            Some("hz"),
            DETERMINISTIC_CONFIDENCE,
            Some("zh"),
        ));
    }

    // UTF-7: plus-sign shifts into Base64-encoded Unicode.
    //
    // 7-bit clean over the whole window, a valid shifted sequence from bounded
    // evidence, and the whole window must decode as UTF-7 — the decode gate is
    // what makes the answer true, since this stage claims 1.0 confidence.
    if has_plus
        && data.iter().all(|&b| b < 0x80)
        && has_valid_utf7_sequences(data, max_start, max_end)
        && utf7_window_decodes(data)
    {
        return Some(DetectionResult::new(
            Some("utf-7"),
            DETERMINISTIC_CONFIDENCE,
            None,
        ));
    }

    None
}

/// Check if data contains a specific byte subsequence.
///
/// # Arguments
///
/// * `data` - The haystack to search in
/// * `pattern` - The needle to search for
///
/// # Returns
///
/// `true` if the pattern appears anywhere in data.
fn contains_subsequence(data: &[u8], pattern: &[u8]) -> bool {
    if pattern.is_empty() || data.len() < pattern.len() {
        return false;
    }
    data.windows(pattern.len()).any(|window| window == pattern)
}

/// Validate HZ-GB-2312 regions contain valid GB2312 byte pairs.
///
/// HZ-GB-2312 uses `~{` to enter GB mode and `~}` to exit. In GB mode,
/// characters are encoded as pairs of bytes in the range 0x21-0x7E.
///
/// # Arguments
///
/// * `data` - The byte sequence to validate
///
/// # Returns
///
/// `true` if at least one `~{...~}` region contains valid GB2312 pairs.
fn has_valid_hz_regions(data: &[u8]) -> bool {
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
            && region.len().is_multiple_of(2)
            && region.iter().all(|&b| (0x21..=0x7E).contains(&b))
        {
            return true;
        }
        start = end + 2;
    }
}

/// Find the first occurrence of a byte subsequence.
///
/// # Arguments
///
/// * `data` - The haystack to search in
/// * `pattern` - The needle to search for
///
/// # Returns
///
/// The starting index of the first match, or `None` if not found.
fn find_subsequence(data: &[u8], pattern: &[u8]) -> Option<usize> {
    if pattern.is_empty() || data.len() < pattern.len() {
        return None;
    }
    data.windows(pattern.len())
        .position(|window| window == pattern)
}

/// Base64 alphabet used inside UTF-7 shifted sequences (+<Base64>-).
const B64_CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Check that *data* contains at least one valid UTF-7 shifted sequence.
///
/// A valid shifted sequence is ``+<base64 chars>`` terminated by either an
/// explicit ``-`` or any non-Base64 character (RFC 2152).  The base64 portion
/// must decode to valid UTF-16BE with zero padding bits.  ``+-`` is a literal
/// plus and is not counted.
///
/// *max_start* bounds where a shift may occur and *max_end* how far its base64
/// run may reach, so a run that begins inside the evidence window can finish
/// beyond it.  A run that outruns *max_end* is skipped rather than judged on
/// the part that fits.
fn has_valid_utf7_sequences(data: &[u8], max_start: usize, max_end: usize) -> bool {
    let mut start = 0usize;
    let limit = data.len().min(max_end);
    loop {
        let shift_pos = match data[start..limit].iter().position(|&b| b == b'+') {
            Some(offset) => start + offset,
            None => return false,
        };
        if shift_pos >= max_start {
            return false;
        }
        let mut pos = shift_pos + 1;

        // `+-` is a literal plus, not a shifted sequence.
        if pos < data.len() && data[pos] == b'-' {
            start = pos + 1;
            continue;
        }
        // Guard A: `+` as the first base64 character encodes PUA code points,
        // which real text never contains (`C++20`, `++row`).  Skip every
        // consecutive `+`.
        if pos < data.len() && data[pos] == b'+' {
            while pos < data.len() && data[pos] == b'+' {
                pos += 1;
            }
            start = pos;
            continue;
        }
        // Guard B: a `+` inside a base64 stream (PEM, MIME attachment) is not
        // a shift character.
        if is_embedded_in_base64(data, shift_pos) {
            start = pos;
            continue;
        }
        // Collect consecutive base64 characters.
        let mut i = pos;
        while i < limit && B64_CHARS.contains(&data[i]) {
            i += 1;
        }
        if i == limit && limit < data.len() && B64_CHARS.contains(&data[i]) {
            // The run outruns the bound — skip it rather than judge a cut run.
            start = i;
            continue;
        }
        let b64_len = i - pos;
        let b64_data = &data[pos..i];

        // Guard C: reject base64 blocks without an uppercase letter.  UTF-7
        // encodes UTF-16BE, whose high byte makes virtually every script
        // produce uppercase base64 characters; `+row`, `+100` are ASCII text.
        if b64_len >= 3 && !b64_data.iter().any(|b| b.is_ascii_uppercase()) {
            start = i;
            continue;
        }
        if b64_len >= 3 && is_valid_utf7_b64(b64_data) {
            // Guard D: a single-unit block must decode into a script range
            // where a lone shifted character plausibly occurs.
            if (b64_len * 6) / 16 == 1 && !plausible_lone_unit(single_unit(b64_data)) {
                start = i;
                continue;
            }
            return true;
        }
        start = pos.max(i);
    }
}

/// Decode the first (only) UTF-16 code unit of a single-unit block.
fn single_unit(b64_data: &[u8]) -> u16 {
    let value = (base64_decode(b64_data[0]).unwrap_or(0) as u32) << 12
        | (base64_decode(b64_data[1]).unwrap_or(0) as u32) << 6
        | base64_decode(b64_data[2]).unwrap_or(0) as u32;
    (value >> 2) as u16
}

/// Script ranges where a lone shifted character plausibly occurs.
fn plausible_lone_unit(unit: u16) -> bool {
    matches!(
        unit,
        0x0080..=0x07FF
            | 0x0E00..=0x0FFF
            | 0x2000..=0x2BFF
            | 0x3000..=0x30FF
            | 0x4E00..=0x9FFF
            | 0xAC00..=0xD7A3
            | 0xFF00..=0xFFEF
    )
}

/// Whole-window UTF-7 validity: 7-bit clean, and every shift well formed.
///
/// Mirrors what Python's `utf-7` codec accepts: a ``+`` must be followed by
/// ``-`` (literal plus) or by a base64 run of at least three characters that
/// decodes cleanly; a run cut by the end of data is tolerated.
pub fn utf7_window_decodes(data: &[u8]) -> bool {
    if data.iter().any(|&b| b >= 0x80) {
        return false;
    }
    let mut i = 0usize;
    while i < data.len() {
        if data[i] != b'+' {
            i += 1;
            continue;
        }
        let start = i + 1;
        if start >= data.len() {
            // A trailing `+` is not a legal shift.
            return false;
        }
        if data[start] == b'-' {
            i = start + 1;
            continue;
        }
        let mut end = start;
        while end < data.len() && B64_CHARS.contains(&data[end]) {
            end += 1;
        }
        let run = &data[start..end];
        if run.len() < 3 || !is_valid_utf7_b64(run) {
            return false;
        }
        i = end;
    }
    true
}

fn is_embedded_in_base64(data: &[u8], pos: usize) -> bool {
    // Return True if the '+' at pos is embedded in a base64 stream
    let mut count = 0;
    let mut i = pos;

    while i > 0 {
        i -= 1;
        let b = data[i];
        if b == 0x0A || b == 0x0D {
            // Skip newlines
            continue;
        }
        if B64_CHARS.contains(&b) || b == b'=' {
            count += 1;
            if count >= 4 {
                return true;
            }
        } else {
            break;
        }
    }

    count >= 4
}

/// Validate that Base64 content decodes to valid UTF-16BE.
///
/// UTF-7 requires that shifted sequences decode to valid UTF-16BE
/// without lone surrogates.
///
/// # Arguments
///
/// * `b64_bytes` - Base64-encoded content
///
/// # Returns
///
/// `true` if the content decodes to valid UTF-16BE.
fn is_valid_utf7_b64(b64_bytes: &[u8]) -> bool {
    let n = b64_bytes.len();
    let total_bits = n * 6;
    // The unused low bits of the last sextet must be zero.
    let padding_bits = total_bits % 16;
    if padding_bits > 0 {
        let last = u32::from(base64_decode(b64_bytes[n - 1]).unwrap_or(0));
        // `padding_bits` can reach 14, so the mask does not fit in a u8:
        // shifting a `u8` there overflows (debug builds panic, release builds
        // wrap to a 255 mask).  Python's fallback computes it without a width
        // limit, so mirror that with a u32.
        let mask = (1u32 << padding_bits) - 1;
        if last & mask != 0 {
            return false;
        }
    }
    let num_bytes = total_bits / 8;
    let mut raw = vec![0u8; num_bytes];
    let mut bit_buf: u32 = 0;
    let mut bit_count: u32 = 0;
    let mut out_idx = 0usize;
    for &c in b64_bytes {
        bit_buf = (bit_buf << 6) | base64_decode(c).unwrap_or(0) as u32;
        bit_count += 6;
        if bit_count >= 8 {
            bit_count -= 8;
            raw[out_idx] = ((bit_buf >> bit_count) & 0xFF) as u8;
            bit_buf &= (1 << bit_count) - 1;
            out_idx += 1;
        }
    }
    let mut prev_high = false;
    for chunk in raw.as_chunks::<2>().0 {
        let code_unit = ((chunk[0] as u16) << 8) | chunk[1] as u16;
        if (0xD800..=0xDBFF).contains(&code_unit) {
            if prev_high {
                return false;
            }
            prev_high = true;
        } else if (0xDC00..=0xDFFF).contains(&code_unit) {
            if !prev_high {
                return false;
            }
            prev_high = false;
        } else {
            if prev_high {
                return false;
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
