//! Model loading and bigram scoring utilities.
//!
//! This module provides high-level utilities for language model operations,
//! including checking if an encoding has language variants, inferring language
//! from encoding names, and scoring data against language models.

use crate::registry::REGISTRY;

/// List of encodings that have language-specific variants in the bigram models.
///
/// These encodings support multiple languages and require statistical scoring
/// to determine the most likely language for a given text sample.
const VARIANT_ENCODINGS: &[&str] = &[
    "utf-8",
    "utf8",
    "windows-1252",
    "iso-8859-1",
    "iso-8859-15",
    "windows-1251",
    "koi8-r",
    "iso-8859-5",
    "windows-1250",
    "iso-8859-2",
    "big5",
    "big5hkscs",
    "gb18030",
    "gb2312",
    "shift_jis",
    "cp932",
    "euc-jp",
    "euc-jis-2004",
    "euc-kr",
    "cp949",
];

/// Check if the encoding has language variants that require statistical scoring.
///
/// Some encodings like UTF-8, Windows-1252, and various CJK encodings
/// are used for multiple languages. When detecting these encodings,
/// we need to perform additional language-specific analysis to determine
/// the actual language of the text.
///
/// # Arguments
///
/// * `encoding` - The encoding name to check
///
/// # Returns
///
/// `true` if the encoding has multiple language variants, `false` otherwise
///
/// # Examples
///
/// ```
/// use _chardet_rs::models::has_model_variants;
///
/// assert!(has_model_variants("utf-8"));
/// assert!(has_model_variants("windows-1252"));
/// assert!(!has_model_variants("windows-1255")); // Hebrew-only
/// ```
pub fn has_model_variants(encoding: &str) -> bool {
    VARIANT_ENCODINGS.contains(&encoding.to_lowercase().as_str())
}

/// Infer the primary language for a given encoding.
///
/// This is a convenience wrapper around [`crate::equivalences::infer_language`]
/// that maps encoding names to their most likely language code.
///
/// # Arguments
///
/// * `encoding` - The encoding name
///
/// # Returns
///
/// An ISO 639-1 language code (e.g., "en", "ru", "ja") or `None` if the
/// encoding is not associated with a specific language.
///
/// # Examples
///
/// ```
/// use _chardet_rs::models::infer_language;
///
/// assert_eq!(infer_language("windows-1251"), Some("ru"));
/// assert_eq!(infer_language("shift_jis"), Some("ja"));
/// assert_eq!(infer_language("utf-8"), None); // Multi-language
/// ```
pub fn infer_language(encoding: &str) -> Option<&'static str> {
    crate::equivalences::infer_language(encoding)
}

/// Score input data against language variants of an encoding.
///
/// This function attempts to determine the best-matching language for data
/// when using a multi-language encoding. For single-language encodings,
/// it returns that language immediately with maximum confidence.
///
/// # Arguments
///
/// * `data` - The byte sequence to analyze
/// * `encoding` - The encoding name to score against
///
/// # Returns
///
/// A tuple of `(confidence, language)` where:
/// - `confidence` is a score from 0.0 to 1.0
/// - `language` is an ISO 639-1 code or `None` if no match
///
/// # Algorithm
///
/// 1. Return (0.0, None) for empty data
/// 2. Look up encoding in registry
/// 3. For single-language encodings: return (1.0, language)
/// 4. For multi-language encodings: return first language with 0.5 confidence
///    (full implementation would use statistical models)
pub fn score_best_language(data: &[u8], encoding: &str) -> (f64, Option<&'static str>) {
    if data.is_empty() {
        return (0.0, None);
    }

    // Get the encoding info from registry
    if let Some(enc_info) = REGISTRY.get(encoding.to_lowercase().as_str()) {
        if enc_info.languages.is_empty() {
            return (0.0, None);
        }

        if enc_info.languages.len() == 1 {
            // Single-language encoding - return with maximum confidence
            return (1.0, Some(enc_info.languages[0]));
        }

        // Multi-language encoding - in the full implementation,
        // this would use statistical models to determine the best language
        // For now, return the first language with a moderate score
        return (0.5, Some(enc_info.languages[0]));
    }

    (0.0, None)
}
