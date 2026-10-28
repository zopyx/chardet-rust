//! Stage 1a+: UTF-16/UTF-32 detection for data without a BOM.
//!
//! This stage runs after BOM detection and before binary detection: UTF-16
//! and UTF-32 text carries null-byte patterns that would otherwise make the
//! data look binary.  Mirrors `chardet/pipeline/utf1632.py`.

use crate::pipeline::{DetectionResult, DETERMINISTIC_CONFIDENCE};

/// How many bytes to sample for pattern analysis.
const SAMPLE_SIZE: usize = 4096;

/// Minimum bytes for reliable pattern detection.
const MIN_BYTES_UTF32: usize = 16;
const MIN_BYTES_UTF16: usize = 10;

/// Minimum positional null fraction for a UTF-16 candidate.  CJK-heavy text
/// can have as few as ~4.5% nulls, so the decode-and-quality check carries
/// the false-positive defence.
const UTF16_MIN_NULL_FRACTION: f64 = 0.03;

/// Minimum text-quality score when both endiannesses show null patterns.
const MIN_TEXT_QUALITY: f64 = 0.5;

/// Margin a weaker-null side must win by to override the null pattern.
const QUALITY_TIE_MARGIN: f64 = 0.05;

/// Minimum printable fraction for a decode to count as text.
const MIN_PRINTABLE_FRACTION: f64 = 0.7;

/// Positional null fraction below which a null-separator reading is possible.
const NULL_SEPARATOR_MAX_FRACTION: f64 = 0.75;

/// Bytes allowed besides a null in the null-separator guard: printable ASCII
/// plus the common whitespace controls.
fn is_null_separator_allowed(byte: u8) -> bool {
    byte == 0x00 || (0x20..=0x7E).contains(&byte) || matches!(byte, 0x09 | 0x0A | 0x0C | 0x0D)
}

fn is_null_separator_pattern(data: &[u8], null_fraction: f64) -> bool {
    if null_fraction >= NULL_SEPARATOR_MAX_FRACTION {
        return false;
    }
    data.iter().all(|&byte| is_null_separator_allowed(byte))
}

/// Detect UTF-32 or UTF-16 encoding from null-byte patterns.
pub fn detect_utf1632_patterns(data: &[u8]) -> Option<DetectionResult> {
    let sample = &data[..data.len().min(SAMPLE_SIZE)];
    if sample.len() < MIN_BYTES_UTF16 {
        return None;
    }

    if let Some(result) = check_utf32(sample) {
        return Some(result);
    }
    check_utf16(sample)
}

/// Decode UTF-16, tolerating a cut final unit and dropping unpaired surrogates.
#[allow(clippy::chunks_exact_to_as_chunks)] // pairs are read positionally; as_chunks would obscure the surrogate lookahead
fn decode_utf16_lenient(data: &[u8], big_endian: bool) -> String {
    let mut out = String::with_capacity(data.len() / 2);
    let mut units = data.chunks_exact(2);
    while let Some(unit) = units.next() {
        let value = if big_endian {
            u16::from_be_bytes([unit[0], unit[1]])
        } else {
            u16::from_le_bytes([unit[0], unit[1]])
        };
        if (0xD800..=0xDBFF).contains(&value) {
            if let Some(low) = units.next() {
                let low_value = if big_endian {
                    u16::from_be_bytes([low[0], low[1]])
                } else {
                    u16::from_le_bytes([low[0], low[1]])
                };
                if (0xDC00..=0xDFFF).contains(&low_value) {
                    let combined =
                        0x10000 + (((value as u32) - 0xD800) << 10) + ((low_value as u32) - 0xDC00);
                    if let Some(ch) = char::from_u32(combined) {
                        out.push(ch);
                    }
                }
            }
            continue;
        }
        if (0xDC00..=0xDFFF).contains(&value) {
            continue;
        }
        if let Some(ch) = char::from_u32(value as u32) {
            out.push(ch);
        }
    }
    out
}

/// Decode UTF-32, dropping out-of-range units.
fn decode_utf32_lenient(data: &[u8], big_endian: bool) -> Option<String> {
    let mut out = String::with_capacity(data.len() / 4);
    for unit in data.as_chunks::<4>().0 {
        let value = if big_endian {
            u32::from_be_bytes([unit[0], unit[1], unit[2], unit[3]])
        } else {
            u32::from_le_bytes([unit[0], unit[1], unit[2], unit[3]])
        };
        {
            let ch = char::from_u32(value)?;
            out.push(ch)
        }
    }
    Some(out)
}

fn check_utf32(data: &[u8]) -> Option<DetectionResult> {
    let trimmed_len = data.len() - (data.len() % 4);
    if trimmed_len < MIN_BYTES_UTF32 {
        return None;
    }
    let data = &data[..trimmed_len];
    let units = trimmed_len / 4;

    let be_first_null = (0..data.len()).step_by(4).filter(|&i| data[i] == 0).count();
    let be_second_null = (0..data.len())
        .step_by(4)
        .filter(|&i| data[i + 1] == 0)
        .count();

    if be_first_null == units && (be_second_null as f64) / (units as f64) > 0.5 {
        if let Some(text) = decode_utf32_lenient(data, true) {
            if looks_like_text(&text) {
                return Some(DetectionResult::new(
                    Some("utf-32-be"),
                    DETERMINISTIC_CONFIDENCE,
                    None,
                ));
            }
        }
    }

    let le_last_null = (3..data.len()).step_by(4).filter(|&i| data[i] == 0).count();
    let le_third_null = (2..data.len()).step_by(4).filter(|&i| data[i] == 0).count();

    if le_last_null == units && (le_third_null as f64) / (units as f64) > 0.5 {
        if let Some(text) = decode_utf32_lenient(data, false) {
            if looks_like_text(&text) {
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

fn check_utf16(data: &[u8]) -> Option<DetectionResult> {
    let mut sample_len = data.len().min(SAMPLE_SIZE);
    sample_len -= sample_len % 2;
    if sample_len < MIN_BYTES_UTF16 {
        return None;
    }
    let sample = &data[..sample_len];
    let units = (sample_len / 2) as f64;

    let be_null_count = (0..sample_len)
        .step_by(2)
        .filter(|&i| sample[i] == 0)
        .count();
    let le_null_count = (1..sample_len)
        .step_by(2)
        .filter(|&i| sample[i] == 0)
        .count();

    let be_fraction = be_null_count as f64 / units;
    let le_fraction = le_null_count as f64 / units;

    let le_qualified =
        le_fraction >= UTF16_MIN_NULL_FRACTION && !is_null_separator_pattern(sample, le_fraction);
    let be_qualified =
        be_fraction >= UTF16_MIN_NULL_FRACTION && !is_null_separator_pattern(sample, be_fraction);

    if !le_qualified && !be_qualified {
        return None;
    }

    // Sides are visited in null-signal order; a challenger must win by a clear
    // margin, so noisy near-ties keep the order the null pattern chose.
    let mut sides = [
        ("utf-16-le", le_fraction, le_qualified),
        ("utf-16-be", be_fraction, be_qualified),
    ];
    if be_fraction > le_fraction {
        sides.reverse();
    }

    let mut best_encoding: Option<&str> = None;
    let mut best_quality = -2.0f64;
    let mut best_qualified = false;
    let mut viable = 0usize;
    let mut qualified_side_decoded = false;

    for (encoding, _fraction, qualified) in sides {
        let text = decode_utf16_lenient(sample, encoding == "utf-16-be");
        if qualified {
            qualified_side_decoded = true;
        }
        if !looks_like_text(&text) {
            continue;
        }
        viable += 1;
        let quality = text_quality(&text);
        let margin = if viable > 1 { QUALITY_TIE_MARGIN } else { 0.0 };
        if quality > best_quality + margin {
            best_quality = quality;
            best_encoding = Some(encoding);
            best_qualified = qualified;
        }
    }

    let encoding = best_encoding?;

    let accepted = if best_qualified {
        viable == 1 || best_quality >= MIN_TEXT_QUALITY
    } else {
        qualified_side_decoded && best_quality >= MIN_TEXT_QUALITY
    };

    if accepted {
        return Some(DetectionResult::new(
            Some(encoding),
            DETERMINISTIC_CONFIDENCE,
            None,
        ));
    }
    None
}

/// Is the decoded sample mostly printable characters?
fn looks_like_text(text: &str) -> bool {
    if text.is_empty() {
        return false;
    }
    let sample: Vec<char> = text.chars().take(500).collect();
    let printable = sample.iter().filter(|&&c| is_printable(c)).count();
    (printable as f64) / (sample.len() as f64) > MIN_PRINTABLE_FRACTION
}

/// Approximate Unicode printability, as Python's `str.isprintable` decides it.
///
/// Not printable: separator categories (except the ASCII space), control
/// characters, and code points with no assigned category.  Rust's `char` API
/// has no general-category lookup, so "assigned" is approximated by the
/// alphabetic/punctuation/symbol ranges that carry text.
fn is_printable(c: char) -> bool {
    if c == ' ' || matches!(c, '\n' | '\r' | '\t') {
        return true;
    }
    if c.is_control() || c.is_whitespace() {
        return false;
    }
    if c.is_alphanumeric() || c.is_ascii_punctuation() {
        return true;
    }
    matches!(
        c as u32,
        0x00A0..=0x00BF
            | 0x2000..=0x2BFF
            | 0x2E00..=0x2E7F
            | 0x3000..=0x303F
            | 0xFE00..=0xFE6F
            | 0xFF00..=0xFFEF
    )
}

/// Approximate Unicode category `M*` for the combining-mark rejection rule.
fn is_combining_mark(c: char) -> bool {
    matches!(c as u32,
        0x0300..=0x036F | 0x0483..=0x0489 | 0x0591..=0x05BD | 0x0610..=0x061A
        | 0x064B..=0x065F | 0x0670 | 0x06D6..=0x06DC | 0x0900..=0x0903
        | 0x093A..=0x094F | 0x0951..=0x0957 | 0x0E31 | 0x0E34..=0x0E3A
        | 0x0E47..=0x0E4E | 0x1AB0..=0x1AFF | 0x1DC0..=0x1DFF | 0x20D0..=0x20F0
        | 0xFE20..=0xFE2F)
}

/// Score how much the decoded text looks like real human-readable content.
///
/// Base score is the letter ratio; ASCII letters carry an extra 0.5x weight
/// (the endianness signal), and a whitespace bonus applies to longer samples.
/// More than 10% control characters or 20% combining marks scores -1.0.
fn text_quality(text: &str) -> f64 {
    let sample: Vec<char> = text.chars().take(500).collect();
    let n = sample.len();
    if n == 0 {
        return -1.0;
    }

    let mut letters = 0usize;
    let mut marks = 0usize;
    let mut spaces = 0usize;
    let mut controls = 0usize;
    let mut ascii_letters = 0usize;

    for &c in &sample {
        if c.is_alphabetic() && !is_combining_mark(c) {
            letters += 1;
            if (c as u32) < 128 {
                ascii_letters += 1;
            }
        } else if is_combining_mark(c) {
            marks += 1;
        } else if c.is_whitespace() {
            spaces += 1;
        } else if c.is_control() {
            controls += 1;
        }
    }

    let n_f = n as f64;
    if (controls as f64) / n_f > 0.1 {
        return -1.0;
    }
    if (marks as f64) / n_f > 0.2 {
        return -1.0;
    }

    let mut score = (letters as f64) / n_f;
    score += ((ascii_letters as f64) / n_f) * 0.5;
    if n > 20 && spaces > 0 {
        score += 0.1;
    }
    score
}
