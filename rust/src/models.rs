//! Model loading and bigram scoring utilities.

use crate::pipeline::DetectionResult;
use crate::registry::REGISTRY;

/// Check if the encoding has language variants.
pub fn has_model_variants(encoding: &str) -> bool {
    // Check if encoding has language variants in the model index
    matches!(
        encoding.to_lowercase().as_str(),
        "utf-8"
            | "utf8"
            | "windows-1252"
            | "iso-8859-1"
            | "iso-8859-15"
            | "windows-1251"
            | "koi8-r"
            | "iso-8859-5"
            | "windows-1250"
            | "iso-8859-2"
            | "big5"
            | "big5hkscs"
            | "gb18030"
            | "gb2312"
            | "shift_jis"
            | "cp932"
            | "euc-jp"
            | "euc-jis-2004"
            | "euc-kr"
            | "cp949"
    )
}

/// Infer language from encoding (simplified).
pub fn infer_language(encoding: &str) -> Option<&'static str> {
    crate::equivalences::infer_language(encoding)
}

/// Score data against all language variants of an encoding.
pub fn score_best_language(data: &[u8], encoding: &str) -> (f64, Option<&'static str>) {
    if data.is_empty() {
        return (0.0, None);
    }

    // Get the encoding info
    if let Some(enc_info) = REGISTRY.get(encoding.to_lowercase().as_str()) {
        if enc_info.languages.is_empty() {
            return (0.0, None);
        }

        if enc_info.languages.len() == 1 {
            // Single-language encoding
            return (1.0, Some(enc_info.languages[0]));
        }

        // Multi-language encoding - in the full implementation,
        // this would use statistical models to determine the best language
        // For now, return the first language with a moderate score
        return (0.5, Some(enc_info.languages[0]));
    }

    (0.0, None)
}
