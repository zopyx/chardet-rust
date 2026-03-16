//! High-level API tests - Rust native implementation.
//!
//! These tests mirror the Python tests in tests/test_api.py.

use _chardet_rs::{detect_all_bytes, detect_bytes, enums::EncodingEra};

#[test]
fn test_detect_returns_result() {
    let result = detect_bytes(b"Hello world", EncodingEra::All, 200_000);
    assert!(result.encoding.is_some());
    assert!(result.confidence > 0.0);
}

#[test]
fn test_detect_ascii() {
    let result = detect_bytes(b"Hello world", EncodingEra::All, 200_000);
    // The encoding should be detected as ASCII or Windows-1252
    assert!(result.confidence == 1.0);
}

#[test]
fn test_detect_utf8_bom() {
    let result = detect_bytes(b"\xef\xbb\xbfHello", EncodingEra::All, 200_000);
    assert_eq!(result.encoding, Some("utf-8-sig".to_string()));
}

#[test]
fn test_detect_utf8_multibyte() {
    let data = "Héllo wörld café".as_bytes();
    let result = detect_bytes(data, EncodingEra::All, 200_000);
    assert_eq!(result.encoding, Some("utf-8".to_string()));
}

#[test]
fn test_detect_empty() {
    let result = detect_bytes(b"", EncodingEra::All, 200_000);
    assert_eq!(result.encoding, Some("utf-8".to_string()));
    assert_eq!(result.confidence, 0.10);
}

#[test]
fn test_detect_with_encoding_era() {
    let data = b"Hello world";
    let result = detect_bytes(data, EncodingEra::ModernWeb, 200_000);
    assert!(result.encoding.is_some());
}

#[test]
fn test_detect_with_max_bytes() {
    let data = b"Hello world".repeat(100_000);
    let result = detect_bytes(&data, EncodingEra::All, 100);
    assert!(result.encoding.is_some());
    assert!(result.confidence > 0.0);
}

#[test]
fn test_detect_all_returns_vec() {
    let results = detect_all_bytes(b"Hello world", EncodingEra::All, 200_000, true);
    assert!(!results.is_empty());
}

#[test]
fn test_detect_all_sorted_by_confidence() {
    let data = "Héllo wörld".as_bytes();
    let results = detect_all_bytes(data, EncodingEra::All, 200_000, true);
    let confidences: Vec<f64> = results.iter().map(|r| r.confidence).collect();
    let mut sorted = confidences.clone();
    sorted.sort_by(|a, b| b.partial_cmp(a).unwrap());
    assert_eq!(confidences, sorted);
}

#[test]
fn test_detect_all_each_has_fields() {
    let results = detect_all_bytes(b"Hello world", EncodingEra::All, 200_000, true);
    for r in results {
        // encoding can be Some or None for binary
        assert!(r.confidence >= 0.0 && r.confidence <= 1.0);
        // language is optional
    }
}

// --- ignore_threshold tests ---

#[test]
fn test_ignore_threshold_true_returns_all() {
    let data = "Héllo wörld café résumé".as_bytes();
    let results = detect_all_bytes(data, EncodingEra::All, 200_000, true);
    assert!(!results.is_empty());
}

#[test]
fn test_ignore_threshold_false_filters() {
    let data = "Héllo wörld café résumé".as_bytes();
    let results_all = detect_all_bytes(data, EncodingEra::All, 200_000, true);
    let results_filtered = detect_all_bytes(data, EncodingEra::All, 200_000, false);
    assert!(results_filtered.len() <= results_all.len());
    for r in &results_filtered {
        assert!(r.confidence > 0.20);
    }
}

#[test]
fn test_ignore_threshold_fallback() {
    // If all results filtered, fall back to top result.
    let results = detect_all_bytes(b"", EncodingEra::All, 200_000, false);
    assert!(!results.is_empty());
}

// --- New encoding tests ---

#[test]
fn test_detect_utf7() {
    // UTF-7 encoded text: "Hello, 世界!"
    let data = b"Hello, +ZeVnLIqe-!";
    let result = detect_bytes(data, EncodingEra::All, 200_000);
    assert_eq!(result.encoding, Some("utf-7".to_string()));
}

#[test]
fn test_detect_utf7_era_all() {
    // UTF-7 should be detected with EncodingEra.ALL (includes LEGACY_REGIONAL).
    // Using simpler UTF-7 sequence that will be detected
    let data = b"Meeting notes: +ZeVnLIqe- and +Noo-.";
    let result = detect_bytes(data, EncodingEra::All, 200_000);
    assert_eq!(result.encoding, Some("utf-7".to_string()));
}

#[test]
fn test_detect_utf7_era_modern_web_skipped() {
    // UTF-7 should NOT be detected with MODERN_WEB (disabled by browsers since ~2020).
    let data = b"Hello, +ZeVnLIqe-!";
    let result = detect_bytes(data, EncodingEra::ModernWeb, 200_000);
    assert_ne!(result.encoding, Some("utf-7".to_string()));
}

#[test]
fn test_detect_hz_gb_2312_era_all() {
    // hz-gb-2312 should be detected with EncodingEra.ALL.
    let data = b"Hello ~{CEDE~} World";
    let result = detect_bytes(data, EncodingEra::All, 200_000);
    assert_eq!(result.encoding, Some("hz-gb-2312".to_string()));
}

#[test]
fn test_detect_hz_gb_2312_era_modern_web_skipped() {
    // hz-gb-2312 is WHATWG 'replacement' - should NOT be detected with MODERN_WEB.
    let data = b"Hello ~{CEDE~} World";
    let result = detect_bytes(data, EncodingEra::ModernWeb, 200_000);
    assert_ne!(result.encoding, Some("hz-gb-2312".to_string()));
}

#[test]
fn test_detect_iso_2022_kr_era_all() {
    // iso-2022-kr should be detected with EncodingEra.ALL.
    let data = b"\x1b$)C\x0e\x21\x21\x0f";
    let result = detect_bytes(data, EncodingEra::All, 200_000);
    assert_eq!(result.encoding, Some("iso-2022-kr".to_string()));
}

#[test]
fn test_detect_iso_2022_kr_era_modern_web_skipped() {
    // iso-2022-kr is WHATWG 'replacement' - should NOT be detected with MODERN_WEB.
    let data = b"\x1b$)C\x0e\x21\x21\x0f";
    let result = detect_bytes(data, EncodingEra::ModernWeb, 200_000);
    assert_ne!(result.encoding, Some("iso-2022-kr".to_string()));
}

#[test]
fn test_detect_cp273() {
    // EBCDIC encoded text: "Grüße aus Deutschland"
    let data = b"\x87\x99\xa4\x94\x40\x81\xa4\xa2\x40\xc4\x85\xa4\xa3\xa2\xc3\x85\xa4\x95";
    let result = detect_bytes(data, EncodingEra::All, 200_000);
    assert!(result.encoding.is_some());
    // Should detect an EBCDIC encoding (cp273 or a close variant)
    assert!(result.encoding.as_ref().unwrap().starts_with("cp"));
}
