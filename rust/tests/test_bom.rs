//! BOM detection tests - Rust native implementation.
//!
//! These tests mirror the Python tests in tests/test_bom.py but run
//! directly against the Rust code without Python binding overhead.

use _chardet_rs::pipeline::bom::detect_bom;
use _chardet_rs::pipeline::DetectionResult;

#[test]
fn test_utf8_bom() {
    let data = b"\xef\xbb\xbfHello";
    let result = detect_bom(data);
    assert_eq!(
        result,
        Some(DetectionResult {
            encoding: Some("utf-8-sig".to_string()),
            confidence: 1.0,
            language: None,
        })
    );
}

#[test]
fn test_utf16_le_bom() {
    let data = b"\xff\xfeH\x00e\x00l\x00l\x00o\x00";
    let result = detect_bom(data);
    assert_eq!(
        result,
        Some(DetectionResult {
            encoding: Some("utf-16-le".to_string()),
            confidence: 1.0,
            language: None,
        })
    );
}

#[test]
fn test_utf16_be_bom() {
    let data = b"\xfe\xff\x00H\x00e\x00l\x00l\x00o";
    let result = detect_bom(data);
    assert_eq!(
        result,
        Some(DetectionResult {
            encoding: Some("utf-16-be".to_string()),
            confidence: 1.0,
            language: None,
        })
    );
}

#[test]
fn test_utf32_le_bom() {
    let data = b"\xff\xfe\x00\x00\x48\x00\x00\x00";
    let result = detect_bom(data);
    assert_eq!(
        result,
        Some(DetectionResult {
            encoding: Some("utf-32-le".to_string()),
            confidence: 1.0,
            language: None,
        })
    );
}

#[test]
fn test_utf32_be_bom() {
    let data = b"\x00\x00\xfe\xff\x00\x00\x00\x48";
    let result = detect_bom(data);
    assert_eq!(
        result,
        Some(DetectionResult {
            encoding: Some("utf-32-be".to_string()),
            confidence: 1.0,
            language: None,
        })
    );
}

#[test]
fn test_no_bom() {
    let data = b"Hello, world!";
    let result = detect_bom(data);
    assert_eq!(result, None);
}

#[test]
fn test_empty_input() {
    assert_eq!(detect_bom(b""), None);
}

#[test]
fn test_too_short_for_bom() {
    assert_eq!(detect_bom(b"\xef"), None);
    assert_eq!(detect_bom(b"\xef\xbb"), None);
}

#[test]
fn test_utf32_le_checked_before_utf16_le() {
    // UTF-32-LE BOM starts with \xff\xfe (same as UTF-16-LE) but has \x00\x00 after
    let data = b"\xff\xfe\x00\x00\x48\x00\x00\x00";
    let result = detect_bom(data);
    assert!(result.is_some());
    assert_eq!(result.unwrap().encoding, Some("utf-32-le".to_string()));
}

#[test]
fn test_utf32_le_bom_only() {
    // Bare UTF-32-LE BOM with no payload is valid (0 % 4 == 0)
    let result = detect_bom(b"\xff\xfe\x00\x00");
    assert!(result.is_some());
    assert_eq!(result.unwrap().encoding, Some("utf-32-le".to_string()));
}

#[test]
fn test_utf32_le_bom_falls_through_to_utf16_when_payload_not_aligned() {
    // FF FE 00 00 30 00 looks like UTF-32-LE BOM, but the remaining
    // 2 bytes are not a valid UTF-32 code unit (need multiple of 4).
    // Should fall through to UTF-16-LE BOM instead.
    let data = b"\xff\xfe\x00\x00\x30\x00";
    let result = detect_bom(data);
    assert!(result.is_some());
    assert_eq!(result.unwrap().encoding, Some("utf-16-le".to_string()));
}

#[test]
fn test_utf32_be_bom_falls_through_when_payload_not_aligned() {
    // Same logic for UTF-32-BE: payload must be a multiple of 4 bytes
    let data = b"\x00\x00\xfe\xff\x00\x48"; // 2-byte payload, not aligned
                                            // No UTF-16-BE fallback here (00 00 FE FF doesn't start with FE FF)
    assert_eq!(detect_bom(data), None);
}
