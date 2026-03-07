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
    // UTF-32 variants must be checked before UTF-16
    (b"\x00\x00\xfe\xff", "utf-32-be"),
    (b"\xff\xfe\x00\x00", "utf-32-le"),
    // UTF-8 BOM
    (b"\xef\xbb\xbf", "utf-8-sig"),
    // UTF-16 variants
    (b"\xfe\xff", "utf-16-be"),
    (b"\xff\xfe", "utf-16-le"),
];

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
    for (bom_bytes, encoding) in BOMS {
        if data.starts_with(bom_bytes) {
            // UTF-32 BOMs overlap with UTF-16 BOMs (e.g. FF FE 00 00 starts
            // with the UTF-16-LE BOM FF FE). Validate that the payload after
            // a UTF-32 BOM is a valid number of UTF-32 code units (multiple of 4 bytes).
            if bom_bytes.len() == 4 {
                let payload_len = data.len() - bom_bytes.len();
                if payload_len % 4 != 0 {
                    // Not a valid UTF-32 length, skip this BOM
                    continue;
                }
            }
            return Some(DetectionResult::new(Some(encoding), 1.0, None));
        }
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
