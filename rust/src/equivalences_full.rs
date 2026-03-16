//! Encoding equivalences and legacy name remapping.
//!
//! This module defines:
//!
//! 1. **Directional supersets** for accuracy evaluation
//! 2. **Bidirectional equivalents** for UTF-16/UTF-32 variants
//! 3. **Preferred superset mapping** for the `should_rename_legacy` API option
//! 4. **Character-level equivalence** for decoding comparison

use once_cell::sync::Lazy;
use std::collections::{HashMap, HashSet};

/// Normalize encoding name for comparison.
pub fn normalize_encoding_name(name: &str) -> String {
    let normalized = name.to_lowercase().replace(['-', '_'], "");

    // Match Python's codecs.lookup() canonicalization for common IBM aliases.
    match normalized.as_str() {
        "ibm437" => "cp437".to_string(),
        "ibm850" => "cp850".to_string(),
        "ibm855" => "cp855".to_string(),
        "ibm858" => "cp858".to_string(),
        "ibm862" => "cp862".to_string(),
        "ibm863" => "cp863".to_string(),
        "ibm865" => "cp865".to_string(),
        _ => normalized,
    }
}

/// Directional superset relationships: detecting any of the supersets
/// when the expected encoding is the subset counts as correct.
fn build_supersets() -> HashMap<String, HashSet<String>> {
    let mut map: HashMap<String, HashSet<String>> = HashMap::new();

    let entries: &[(&str, &[&str])] = &[
        ("ascii", &["utf-8", "windows-1252"]),
        ("tis-620", &["iso-8859-11", "cp874"]),
        ("iso-8859-11", &["cp874"]),
        ("gb2312", &["gb18030"]),
        ("gbk", &["gb18030"]),
        ("big5", &["big5hkscs", "cp950"]),
        ("shift_jis", &["cp932", "shift_jis_2004"]),
        ("shift-jisx0213", &["shift_jis_2004"]),
        ("euc-jp", &["euc-jis-2004"]),
        ("euc-jisx0213", &["euc-jis-2004"]),
        ("euc-kr", &["cp949"]),
        ("cp037", &["cp500", "cp1140"]),
        ("cp1125", &["cp866"]),
        ("cp500", &["cp1140"]),
        ("cp775", &["windows-1257"]),
        ("cp858", &["cp437", "cp850"]),
        // ISO-2022-JP subsets
        (
            "iso-2022-jp",
            &["iso-2022-jp-2", "iso-2022-jp-2004", "iso-2022-jp-ext"],
        ),
        ("iso-2022-jp-1", &["iso-2022-jp-2", "iso-2022-jp-ext"]),
        ("iso-2022-jp-3", &["iso-2022-jp-2004"]),
        // ISO/Windows superset pairs
        ("iso-8859-1", &["windows-1252"]),
        ("iso-8859-2", &["windows-1250"]),
        ("iso-8859-4", &["windows-1257"]),
        ("iso-8859-5", &["windows-1251"]),
        ("iso-8859-6", &["windows-1256"]),
        ("iso-8859-7", &["windows-1253"]),
        ("iso-8859-8", &["windows-1255"]),
        ("iso-8859-9", &["windows-1254"]),
        ("iso-8859-13", &["windows-1257"]),
        ("iso-8859-14", &["windows-1252"]),
        ("iso-8859-15", &["windows-1252"]),
        ("iso-8859-16", &["windows-1250"]),
    ];

    for (subset, supersets) in entries {
        let norm_subset = normalize_encoding_name(subset);
        let norm_supersets: HashSet<String> = supersets
            .iter()
            .map(|s| normalize_encoding_name(s))
            .collect();
        map.insert(norm_subset, norm_supersets);
    }

    map
}

/// Preferred superset name for each encoding (used by should_rename_legacy).
fn build_preferred_superset() -> HashMap<String, String> {
    let entries: &[(&str, &str)] = &[
        ("ascii", "Windows-1252"),
        ("euc-kr", "CP949"),
        ("iso-8859-1", "Windows-1252"),
        ("iso-8859-2", "Windows-1250"),
        ("iso-8859-5", "Windows-1251"),
        ("iso-8859-6", "Windows-1256"),
        ("iso-8859-7", "Windows-1253"),
        ("iso-8859-8", "Windows-1255"),
        ("iso-8859-9", "Windows-1254"),
        ("iso-8859-11", "CP874"),
        ("iso-8859-13", "Windows-1257"),
        ("tis-620", "CP874"),
    ];

    entries
        .iter()
        .map(|(k, v)| (k.to_lowercase(), v.to_string()))
        .collect()
}

/// Bidirectional equivalent encoding groups.
fn build_bidirectional_groups() -> HashMap<String, HashSet<String>> {
    let groups: &[&[&str]] = &[
        &["utf-16", "utf-16-le", "utf-16-be"],
        &["utf-32", "utf-32-le", "utf-32-be"],
        &["iso-2022-jp-2", "iso-2022-jp-2004", "iso-2022-jp-ext"],
        &["cp037", "cp500", "cp1140"],
    ];

    let mut map = HashMap::new();
    for group in groups {
        let normalized: HashSet<String> =
            group.iter().map(|s| normalize_encoding_name(s)).collect();
        for name in *group {
            map.insert(normalize_encoding_name(name), normalized.clone());
        }
    }
    map
}

/// Bidirectional language equivalences.
fn build_language_equivalences() -> HashMap<String, HashSet<String>> {
    let groups: &[&[&str]] = &[
        &["sk", "cs"],             // Slovak / Czech
        &["uk", "ru", "bg", "be"], // East Slavic + Bulgarian
        &["ms", "id"],             // Malay / Indonesian
        &["no", "da", "sv"],       // Scandinavian
    ];

    let mut map = HashMap::new();
    for group in groups {
        let set: HashSet<String> = group.iter().map(|s| s.to_string()).collect();
        for code in *group {
            map.insert(code.to_string(), set.clone());
        }
    }
    map
}

/// Character pairs that are considered equivalent symbols.
fn build_equivalent_symbol_pairs() -> HashSet<(char, char)> {
    let pairs: &[(char, char)] = &[
        ('¤', '€'),
        ('€', '¤'),
        ('Á', '╡'),
        ('╡', 'Á'),
        ('€', 'ı'),
        ('ı', '€'),
        ('ű', 'ø'),
        ('ø', 'ű'),
    ];
    pairs.iter().cloned().collect()
}

// Static lazy-initialized lookup tables
static SUPERSETS: Lazy<HashMap<String, HashSet<String>>> = Lazy::new(build_supersets);
static PREFERRED_SUPERSET: Lazy<HashMap<String, String>> = Lazy::new(build_preferred_superset);
static BIDIRECTIONAL_GROUPS: Lazy<HashMap<String, HashSet<String>>> =
    Lazy::new(build_bidirectional_groups);
static LANGUAGE_EQUIVALENCES: Lazy<HashMap<String, HashSet<String>>> =
    Lazy::new(build_language_equivalences);
static EQUIVALENT_SYMBOL_PAIRS: Lazy<HashSet<(char, char)>> =
    Lazy::new(build_equivalent_symbol_pairs);

/// Apply legacy rename to get preferred superset name.
pub fn apply_legacy_rename(encoding: &str) -> String {
    PREFERRED_SUPERSET
        .get(&encoding.to_lowercase())
        .cloned()
        .unwrap_or_else(|| encoding.to_string())
}

/// Check whether two languages are equivalent.
pub fn is_language_equivalent(expected: &str, detected: &str) -> bool {
    if expected == detected {
        return true;
    }
    LANGUAGE_EQUIVALENCES
        .get(expected)
        .map(|set| set.contains(detected))
        .unwrap_or(false)
}

/// Check whether detected encoding is correct for expected encoding.
///
/// Acceptable means:
/// 1. Exact match (after normalization), OR
/// 2. Both belong to the same bidirectional byte-order group, OR
/// 3. Detected is a known superset of expected
pub fn is_correct(expected: Option<&str>, detected: Option<&str>) -> bool {
    // Handle None cases (binary files)
    let (expected, detected) = match (expected, detected) {
        (None, None) => return true,
        (None, Some(_)) => return false,
        (Some(_), None) => return false,
        (Some(e), Some(d)) => (e, d),
    };

    let norm_exp = normalize_encoding_name(expected);
    let norm_det = normalize_encoding_name(detected);

    // 1. Exact match
    if norm_exp == norm_det {
        return true;
    }

    // 2. Bidirectional (same byte-order group)
    if let Some(group) = BIDIRECTIONAL_GROUPS.get(&norm_exp) {
        if group.contains(&norm_det) {
            return true;
        }
    }

    // 3. Superset is acceptable (detected is a known superset of expected)
    if let Some(supersets) = SUPERSETS.get(&norm_exp) {
        if supersets.contains(&norm_det) {
            return true;
        }
    }

    false
}

/// Strip combining characters from text (NFKD normalization).
fn strip_combining(text: &str) -> String {
    // For now, just return the text as-is
    // Full Unicode normalization requires the unicode-normalization crate
    // which adds a dependency. We can add it if needed.
    text.to_string()
}

/// Check if two characters are functionally equivalent.
fn chars_equivalent(a: char, b: char) -> bool {
    if a == b {
        return true;
    }
    if EQUIVALENT_SYMBOL_PAIRS.contains(&(a, b)) {
        return true;
    }
    // Compare base letters after stripping combining marks
    // (simplified - full implementation needs unicode-normalization)
    strip_combining(&a.to_string()) == strip_combining(&b.to_string())
}

/// Check whether detected encoding produces functionally identical text to expected.
///
/// This function decodes the data with both encodings and compares the results,
/// allowing for character-level equivalence (e.g., ¤ ↔ €).
pub fn is_equivalent_detection(
    data: &[u8],
    expected: Option<&str>,
    detected: Option<&str>,
) -> bool {
    // Handle None cases
    let (expected, detected) = match (expected, detected) {
        (None, None) => return true,
        (None, Some(_)) => return false,
        (Some(_), None) => return false,
        (Some(e), Some(d)) => (e, d),
    };

    let norm_exp = normalize_encoding_name(expected);
    let norm_det = normalize_encoding_name(detected);

    // Already same encoding name
    if norm_exp == norm_det {
        return true;
    }

    // Try to decode with both encodings
    let text_exp = match encoding_rs::Encoding::for_label(norm_exp.as_bytes()) {
        Some(enc) => {
            let (cow, _, had_errors) = enc.decode(data);
            if had_errors {
                return false;
            }
            cow.into_owned()
        }
        None => return false,
    };

    let text_det = match encoding_rs::Encoding::for_label(norm_det.as_bytes()) {
        Some(enc) => {
            let (cow, _, had_errors) = enc.decode(data);
            if had_errors {
                return false;
            }
            cow.into_owned()
        }
        None => return false,
    };

    // Exact text match
    if text_exp == text_det {
        return true;
    }

    // Length mismatch means different text
    if text_exp.len() != text_det.len() {
        return false;
    }

    // Check character-level equivalence
    text_exp
        .chars()
        .zip(text_det.chars())
        .all(|(a, b)| chars_equivalent(a, b))
}

/// Check whether detected encoding is correct, trying is_correct first,
/// then falling back to is_equivalent_detection if needed.
pub fn is_acceptable_detection(
    data: &[u8],
    expected: Option<&str>,
    detected: Option<&str>,
) -> bool {
    is_correct(expected, detected) || is_equivalent_detection(data, expected, detected)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_encoding_name() {
        assert_eq!(normalize_encoding_name("UTF-8"), "utf8");
        assert_eq!(normalize_encoding_name("iso-8859-1"), "iso88591");
        assert_eq!(normalize_encoding_name("Shift_JIS"), "shiftjis");
    }

    #[test]
    fn test_is_correct_exact_match() {
        assert!(is_correct(Some("utf-8"), Some("utf-8")));
        assert!(is_correct(Some("UTF-8"), Some("utf8")));
    }

    #[test]
    fn test_is_correct_superset() {
        // ASCII is subset of UTF-8 and Windows-1252
        assert!(is_correct(Some("ascii"), Some("utf-8")));
        assert!(is_correct(Some("ascii"), Some("windows-1252")));
        // But not the reverse
        assert!(!is_correct(Some("utf-8"), Some("ascii")));
    }

    #[test]
    fn test_is_correct_bidirectional() {
        // UTF-16 variants are bidirectional equivalents
        assert!(is_correct(Some("utf-16"), Some("utf-16-le")));
        assert!(is_correct(Some("utf-16-le"), Some("utf-16-be")));
    }

    #[test]
    fn test_apply_legacy_rename() {
        assert_eq!(apply_legacy_rename("ascii"), "Windows-1252");
        assert_eq!(apply_legacy_rename("iso-8859-1"), "Windows-1252");
        assert_eq!(apply_legacy_rename("utf-8"), "utf-8"); // No rename
    }

    #[test]
    fn test_is_language_equivalent() {
        assert!(is_language_equivalent("sk", "cs"));
        assert!(is_language_equivalent("cs", "sk"));
        assert!(is_language_equivalent("ru", "uk"));
        assert!(!is_language_equivalent("en", "fr"));
    }
}
