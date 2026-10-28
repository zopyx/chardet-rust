//! Stage 0a: BOM (Byte Order Mark) detection.
//!
//! This module detects Unicode Byte Order Marks at the start of data.
//! BOM detection is the first stage in the pipeline because it's
//! deterministic - if a valid BOM is present, the encoding is known
//! with certainty.
//!
//! # BOM Formats
//!
//! | BOM Bytes | Encoding | Size |
//! |-----------|----------|------|
//! | `EF BB BF` | UTF-8 | 3 bytes |
//! | `FE FF` | UTF-16-BE | 2 bytes |
//! | `FF FE` | UTF-16-LE | 2 bytes |
//! | `00 00 FE FF` | UTF-32-BE | 4 bytes |
//! | `FF FE 00 00` | UTF-32-LE | 4 bytes |

use super::DetectionResult;

/// Table of BOM signatures sorted by length (longest first).
///
/// This ordering is important because UTF-32 BOMs overlap with UTF-16 BOMs:
/// - UTF-32-LE starts with `FF FE 00 00`, which begins with the UTF-16-LE BOM `FF FE`
/// - UTF-32-BE starts with `00 00 FE FF`, which doesn't overlap with UTF-16-BE
///
/// By checking longer BOMs first, we correctly identify UTF-32 even when
/// the first bytes match a UTF-16 BOM.
const BOMS: &[(&[u8], &str)] = &[
    // UTF-32 variants must be checked before UTF-16: the UTF-32-LE BOM
    // starts with the UTF-16-LE BOM.
    (b"\x00\x00\xfe\xff", "utf-32"),
    (b"\xff\xfe\x00\x00", "utf-32"),
    (b"\xef\xbb\xbf", "utf-8-sig"),
    (b"\xfe\xff", "utf-16"),
    (b"\xff\xfe", "utf-16"),
    // UTF-7 signatures: U+FEFF encoded in UTF-7.  All four are ASCII bytes,
    // so without these marks a signed UTF-7 file reads as plain ASCII.
    (b"+/v8", "utf-7"),
    (b"+/v9", "utf-7"),
    (b"+/v+", "utf-7"),
    (b"+/v/", "utf-7"),
];

/// UTF-32 marks, which must have a payload that is a whole number of units.
const UTF32_BOMS: &[&[u8]] = &[b"\x00\x00\xfe\xff", b"\xff\xfe\x00\x00"];

/// Whether the bytes after a mark decode under the encoding it names.
///
/// The mark names the encoding, but the answer is only true if the body
/// decodes under it: a lone surrogate after a UTF-16 mark, or cp1252 bytes
/// after a UTF-8 mark, would otherwise come back at 1.0 and fail in the
/// caller's own decode.  A cut final unit is tolerated, as the input is
/// usually a prefix of a larger whole.
fn body_decodes(body: &[u8], encoding: &str, big_endian: bool) -> bool {
    match encoding {
        "utf-8-sig" => match std::str::from_utf8(body) {
            Ok(_) => true,
            // Tolerate a truncated multi-byte sequence at the very end.
            Err(err) => err.error_len().is_none(),
        },
        "utf-16" => utf16_body_is_sound(body, big_endian),
        "utf-32" => utf32_body_is_sound(body, big_endian),
        // UTF-7 marks are ordinary ASCII (`+/v8/src/api.cc` in a diff), so the
        // mark only counts when the whole buffer decodes as UTF-7.
        // UTF-7 is decided over the whole buffer below, not on the body.
        "utf-7" => true,
        _ => true,
    }
}

#[allow(clippy::chunks_exact_to_as_chunks)] // pairs are read positionally; as_chunks would obscure the surrogate lookahead
fn utf16_body_is_sound(body: &[u8], big_endian: bool) -> bool {
    if !body.len().is_multiple_of(2) {
        // A cut final byte is tolerable only as a truncated tail.
        return body.len() > 1;
    }
    let units = body.chunks_exact(2);
    let mut pending_high = false;
    for unit in units {
        let value = if big_endian {
            u16::from_be_bytes([unit[0], unit[1]])
        } else {
            u16::from_le_bytes([unit[0], unit[1]])
        };
        if pending_high {
            if !(0xDC00..=0xDFFF).contains(&value) {
                return false;
            }
            pending_high = false;
            continue;
        }
        if (0xD800..=0xDBFF).contains(&value) {
            pending_high = true;
        } else if (0xDC00..=0xDFFF).contains(&value) {
            return false;
        }
    }
    // An unpaired high surrogate at the end is a cut pair, not corruption.
    true
}

fn utf32_body_is_sound(body: &[u8], big_endian: bool) -> bool {
    // A partial final unit is a window cut, not corruption: only whole units
    // carry evidence, and the caller's `input_truncated` decides whether the
    // mark is admitted at all.  The BOM decides the byte order, so a unit that
    // is only valid when read the other way round is corruption, not a match.
    body.as_chunks::<4>().0.iter().all(|unit| {
        let value = if big_endian {
            u32::from_be_bytes([unit[0], unit[1], unit[2], unit[3]])
        } else {
            u32::from_le_bytes([unit[0], unit[1], unit[2], unit[3]])
        };
        value <= 0x10FFFF && !(0xD800..=0xDFFF).contains(&value)
    })
}

/// Detect Byte Order Mark at the start of data.
///
/// Checks if the data starts with a recognized Unicode BOM. If found,
/// returns the corresponding encoding with 100% confidence.
///
/// # Arguments
///
/// * `data` - The byte sequence to check
///
/// # Returns
///
/// - `Some(DetectionResult)` with encoding and confidence 1.0 if BOM found
/// - `None` if no BOM is detected
///
/// # UTF-32 Validation
///
/// For UTF-32 BOMs, this function validates that the remaining data
/// after the BOM is a valid length (multiple of 4 bytes). This prevents
/// misdetecting data that happens to start with those bytes but isn't
/// actually valid UTF-32.
///
/// # Examples
///
/// ```
/// use _chardet_rs::pipeline::bom::detect_bom;
///
/// // UTF-8 BOM
/// let result = detect_bom(b"\xEF\xBB\xBFHello");
/// assert_eq!(result.unwrap().encoding.as_deref(), Some("utf-8-sig"));
///
/// // UTF-16-LE BOM
/// let result = detect_bom(b"\xFF\xFEH\x00e\x00l\x00l\x00o\x00");
/// assert_eq!(result.unwrap().encoding.as_deref(), Some("utf-16-le"));
///
/// // No BOM
/// assert!(detect_bom(b"Hello").is_none());
/// ```
pub fn detect_bom(data: &[u8]) -> Option<DetectionResult> {
    detect_bom_window(data, false)
}

/// Detect a BOM, knowing whether the examination window ends before the input.
///
/// * `input_truncated` — the window was cut by the caller, so a partial final
///   UTF-32 unit is acceptable instead of disqualifying the mark.
pub fn detect_bom_window(data: &[u8], input_truncated: bool) -> Option<DetectionResult> {
    for (bom_bytes, encoding) in BOMS {
        if !data.starts_with(bom_bytes) {
            continue;
        }

        let body = &data[bom_bytes.len()..];

        // UTF-32 marks overlap with UTF-16 marks (FF FE 00 00 starts with the
        // UTF-16-LE mark FF FE); require a whole number of UTF-32 units.
        if UTF32_BOMS.contains(bom_bytes) && !body.len().is_multiple_of(4) && !input_truncated {
            continue;
        }

        // The mark names the encoding, but the body has to decode under it.
        // A UTF-16 mark decides the byte order: FE FF is big-endian, FF FE
        // little-endian, so surrogates must be read in that order.
        let big_endian = bom_bytes.starts_with(&[0x00, 0x00, 0xFE, 0xFF])
            || bom_bytes.starts_with(&[0xFE, 0xFF]);
        // A UTF-7 mark is ordinary ASCII text ("+/v8/src/api.cc" in a diff), so
        // it only counts when the whole buffer decodes as UTF-7 — a body check
        // would accept the mark and hand back ASCII as UTF-7.
        if *encoding == "utf-7" && !crate::pipeline::escape::utf7_window_decodes(data) {
            continue;
        }
        if !body_decodes(body, encoding, big_endian) {
            // A relaxed UTF-32 match may be UTF-16 starting with NUL; give the
            // shorter mark its turn before giving up.
            if input_truncated && UTF32_BOMS.contains(bom_bytes) && !data.len().is_multiple_of(4) {
                continue;
            }
            return None;
        }

        return Some(DetectionResult::new(Some(encoding), 1.0, None));
    }
    None
}

/// Get the size of a BOM in bytes for a given encoding.
///
/// # Arguments
///
/// * `encoding` - The encoding name
///
/// # Returns
///
/// The BOM size in bytes, or 0 if the encoding doesn't use a BOM.
///
/// # Examples
///
/// ```
/// use _chardet_rs::pipeline::bom::bom_size;
///
/// assert_eq!(bom_size("utf-8-sig"), 3);
/// assert_eq!(bom_size("utf-16-be"), 2);
/// assert_eq!(bom_size("utf-32-be"), 4);
/// assert_eq!(bom_size("utf-8"), 0);
/// ```
#[allow(dead_code)]
pub fn bom_size(encoding: &str) -> usize {
    match encoding.to_lowercase().as_str() {
        "utf-8-sig" | "utf-8-bom" => 3,
        "utf-16-be" | "utf-16-le" => 2,
        "utf-32-be" | "utf-32-le" => 4,
        _ => 0,
    }
}

/// Strip the BOM from data if present.
///
/// # Arguments
///
/// * `data` - The byte sequence that may start with a BOM
///
/// # Returns
///
/// A byte slice with the BOM removed, or the original data if no BOM.
///
/// # Examples
///
/// ```
/// use _chardet_rs::pipeline::bom::strip_bom;
///
/// let data = b"\xEF\xBB\xBFHello";
/// assert_eq!(strip_bom(data), b"Hello");
///
/// let data = b"Hello";
/// assert_eq!(strip_bom(data), b"Hello");
/// ```
#[allow(dead_code)]
pub fn strip_bom(data: &[u8]) -> &[u8] {
    for (bom_bytes, _) in BOMS {
        if data.starts_with(bom_bytes) {
            return &data[bom_bytes.len()..];
        }
    }
    data
}
