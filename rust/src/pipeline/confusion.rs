//! Confusion group resolution for similar single-byte encodings.
//!
//! This module resolves ties between similar encodings that are difficult to
//! distinguish statistically. It uses language-specific distinguishing bytes
//! and encoding family preferences to break ties correctly.
//!
//! # Confusion Groups
//!
//! Many single-byte encodings overlap significantly in their byte mappings.
//! For example, ISO-8859-1 and Windows-1252 are nearly identical except for
//! the 0x80-0x9F range. This module uses distinguishing bytes to resolve
//! these ambiguities.
//!
//! # Resolution Strategy
//!
//! 1. Check for language-specific distinguishing bytes
//! 2. Apply encoding family preferences (Windows > ISO)
//! 3. Return reordered results

use super::DetectionResult;

/// Check if data contains bytes that distinguish Windows-1257 from Windows-1252
/// for Baltic languages (Lithuanian, Latvian, Estonian).
///
/// These bytes decode to different characters in Windows-1252 vs Windows-1257
/// and are commonly used in Baltic languages:
/// - 0xE0 = ą (1257) vs à (1252)
/// - 0xE8 = ė (1257) vs è (1252)
/// - 0xF0 = š (1257) vs ð (1252)
/// - 0xF8 = ų (1257) vs ø (1252)
/// - 0xFB = ū (1257) vs û (1252)
/// - 0xFE = ž (1257) vs þ (1252)
/// - 0xEB = ė (1257) vs ë (1252)
const BALTIC_DISTINGUISHING_BYTES: &[u8] = &[0xE0, 0xE8, 0xF0, 0xF8, 0xFB, 0xFE, 0xEB];

/// Check for Baltic distinguishing bytes.
///
/// # Arguments
///
/// * `data` - The byte sequence to check
///
/// # Returns
///
/// `true` if any Baltic distinguishing byte is present.
fn has_baltic_distinguishing_bytes(data: &[u8]) -> bool {
    data.iter()
        .any(|&b| BALTIC_DISTINGUISHING_BYTES.contains(&b))
}

/// Check if data contains bytes that distinguish KOI8-U from KOI8-R.
///
/// KOI8-U specific bytes for Ukrainian:
/// - 0xA4 = є (Ukrainian ye)
/// - 0xA6 = i (Ukrainian i)
/// - 0xA7 = ї (Ukrainian yi)
const KOI8U_DISTINGUISHING_BYTES: &[u8] = &[0xA4, 0xA6, 0xA7];

/// Check for KOI8-U distinguishing bytes.
///
/// # Arguments
///
/// * `data` - The byte sequence to check
///
/// # Returns
///
/// `true` if any KOI8-U distinguishing byte is present.
fn has_koi8u_distinguishing_bytes(data: &[u8]) -> bool {
    data.iter()
        .any(|&b| KOI8U_DISTINGUISHING_BYTES.contains(&b))
}

/// Check if data contains bytes that distinguish ISO-8859-16 from ISO-8859-1.
///
/// ISO-8859-16 specific bytes for South-Eastern European languages
/// (Romanian, Croatian, etc.):
/// - 0xA1 = Ą, 0xA2 = ą, 0xA3 = Ł, 0xA6 = Ș, 0xA9 = Œ
/// - 0xAA = ő, 0xAB = Ő, 0xAC = Ĳ, 0xB1 = ą, 0xB2 = Ł
/// - 0xB3 = ł, 0xB6 = ș, 0xB9 = œ, 0xBA = ő, 0xBB = ő, 0xBC = ĳ
const ISO8859_16_DISTINGUISHING_BYTES: &[u8] = &[
    0xA1, 0xA2, 0xA3, 0xA6, 0xA9, 0xAA, 0xAB, 0xAC, 0xB1, 0xB2, 0xB3, 0xB6, 0xB9, 0xBA, 0xBB, 0xBC,
];

/// Check for ISO-8859-16 distinguishing bytes.
///
/// # Arguments
///
/// * `data` - The byte sequence to check
///
/// # Returns
///
/// `true` if any ISO-8859-16 distinguishing byte is present.
fn has_iso8859_16_distinguishing_bytes(data: &[u8]) -> bool {
    data.iter()
        .any(|&b| ISO8859_16_DISTINGUISHING_BYTES.contains(&b))
}

/// Get the language from the top detection result.
///
/// # Arguments
///
/// * `results` - The detection results
///
/// # Returns
///
/// The ISO 639-1 language code of the top result, or `None`.
fn get_top_language(results: &[DetectionResult]) -> Option<&str> {
    results.first().and_then(|r| r.language.as_deref())
}

/// Resolve confusion between similar encodings in the top results.
///
/// This function examines the top detection results and reorders them
/// if there's a known confusion that can be resolved using distinguishing
/// bytes or encoding preferences.
///
/// # Arguments
///
/// * `data` - The original byte sequence
/// * `results` - The detection results from statistical scoring
///
/// # Returns
///
/// Potentially reordered results with confusions resolved.
///
/// # Resolution Rules
///
/// 1. **Baltic**: If top language is Baltic and distinguishing bytes present,
///    prefer Windows-1257 or ISO-8859-13
/// 2. **KOI8-U vs KOI8-R**: If Ukrainian bytes present, prefer KOI8-U
/// 3. **ISO-8859-16**: If distinguishing bytes present, prefer ISO-8859-16
/// 4. **Windows vs ISO**: Generally prefer Windows codepages over ISO equivalents
///
/// # Examples
///
/// ```
/// use _chardet_rs::pipeline::confusion::resolve_confusion_groups;
/// use _chardet_rs::pipeline::DetectionResult;
///
/// // Results with potential ISO-8859-1 vs Windows-1252 confusion
/// let results = vec![
///     DetectionResult::new(Some("iso-8859-1"), 0.9, Some("en")),
///     DetectionResult::new(Some("windows-1252"), 0.85, Some("en")),
/// ];
/// let resolved = resolve_confusion_groups(b"test", results);
/// // Windows-1252 should be preferred
/// ```
pub fn resolve_confusion_groups(
    data: &[u8],
    results: Vec<DetectionResult>,
) -> Vec<DetectionResult> {
    if results.len() < 2 {
        return results;
    }

    // Check for known confusion pairs and resolve if needed
    let top = &results[0];
    let second = &results[1];

    if let (Some(ref enc1), Some(ref enc2)) = (&top.encoding, &second.encoding) {
        // Special case: Baltic text with distinguishing bytes should use Windows-1257
        // This handles cases where iso-8859-1 and iso-8859-13 tie for top
        let top_lang = get_top_language(&results);
        let is_baltic_lang = matches!(top_lang, Some("lt") | Some("lv") | Some("et"));

        if is_baltic_lang && has_baltic_distinguishing_bytes(data) {
            // Find Windows-1257 or iso-8859-13 in the results and promote it
            for (i, result) in results.iter().enumerate() {
                if let Some(ref enc) = result.encoding {
                    if enc == "windows-1257" || enc == "Windows-1257" || enc == "iso-8859-13" {
                        if i != 0 {
                            let mut new_results = results.clone();
                            new_results.swap(0, i);
                            return new_results;
                        }
                        return results;
                    }
                }
            }
        }

        // Special case: KOI8-U vs KOI8-R confusion with Ukrainian bytes
        // If we have KOI8-U distinguishing bytes and both encodings are close,
        // prefer KOI8-U (the bytes are strong evidence of Ukrainian text)
        let is_koi8_confusion =
            (enc1 == "koi8-r" && enc2 == "koi8-u") || (enc1 == "koi8-u" && enc2 == "koi8-r");

        if is_koi8_confusion && has_koi8u_distinguishing_bytes(data) {
            // Prefer KOI8-U if it has Ukrainian-specific bytes
            // enc1 is top, enc2 is second - we want KOI8-U to be top
            if enc1 == "koi8-r" && enc2 == "koi8-u" {
                let mut new_results = results.clone();
                new_results.swap(0, 1);
                return new_results;
            }
        }

        // Special case: ISO-8859-16 vs ISO-8859-1 confusion
        // ISO-8859-16 is for South-Eastern European languages (Romanian, Croatian, etc.)
        let is_iso8859_16_confusion = (enc1 == "iso-8859-1" && enc2 == "iso-8859-16")
            || (enc1 == "iso-8859-16" && enc2 == "iso-8859-1");

        if is_iso8859_16_confusion && has_iso8859_16_distinguishing_bytes(data) {
            // Prefer ISO-8859-16 if it has distinguishing bytes
            // enc1 is top, enc2 is second - we want ISO-8859-16 to be top
            if enc1 == "iso-8859-1" && enc2 == "iso-8859-16" {
                let mut new_results = results.clone();
                new_results.swap(0, 1);
                return new_results;
            }
        }

        // Known confusion pairs - prefer Windows encodings over ISO equivalents
        // Note: We don't include mac-cyrillic here because it's not an ISO encoding,
        // and swapping it with windows-1251 can cause incorrect detection for files
        // that contain bytes valid in mac-cyrillic but not in windows-1251.
        let confusion_pairs: &[(&str, &str)] = &[
            ("iso-8859-1", "windows-1252"),
            ("iso-8859-2", "windows-1250"),
            ("iso-8859-4", "windows-1257"),
            ("iso-8859-5", "windows-1251"),
            ("iso-8859-6", "windows-1256"),
            ("iso-8859-7", "windows-1253"),
            ("iso-8859-8", "windows-1255"),
            ("iso-8859-9", "windows-1254"),
            ("iso-8859-13", "windows-1257"),
            ("koi8-r", "windows-1251"),
        ];

        for (a, b) in confusion_pairs {
            if (enc1 == *a && enc2 == *b) || (enc1 == *b && enc2 == *a) {
                // Prefer Windows encodings over ISO equivalents
                let winner = if enc1.starts_with("windows-") {
                    enc1
                } else {
                    enc2
                };
                if winner == enc2 {
                    // Swap results
                    let mut new_results = results.clone();
                    new_results.swap(0, 1);
                    return new_results;
                }
            }
        }
    }

    results
}
