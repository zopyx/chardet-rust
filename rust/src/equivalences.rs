//! Encoding equivalences and legacy name remapping.
//!
//! This module handles the mapping between legacy encoding names and their
//! modern equivalents, as well as inferring the primary language associated
//! with an encoding. This is important for maintaining compatibility with
//! chardet's API while providing modern encoding names.

use once_cell::sync::Lazy;
use std::collections::HashMap;

/// Preferred superset mapping for the `should_rename_legacy` API option.
///
/// Maps legacy encoding names to their modern superset equivalents.
/// Uses display-cased names to match chardet 6.x output (e.g., "Windows-1252").
///
/// The rationale for each mapping:
/// - ASCII → Windows-1252: Windows-1252 is a superset of ASCII
/// - ISO-8859-1 → Windows-1252: Windows-1252 adds useful characters in 0x80-0x9F
/// - ISO-8859-2/16 → Windows-1250: Better Windows support for Central European
/// - ISO-8859-5 → Windows-1251: Better Windows support for Cyrillic
/// - ISO-8859-6 → Windows-1256: Better Windows support for Arabic
/// - ISO-8859-7 → Windows-1253: Better Windows support for Greek
/// - ISO-8859-8 → Windows-1255: Better Windows support for Hebrew
/// - ISO-8859-9 → Windows-1254: Better Windows support for Turkish
/// - ISO-8859-11/13 → Windows-1257: Better Windows support for Baltic
/// - TIS-620 → CP874: Windows codepage for Thai
/// - EUC-KR → CP949: Unified Hangul Code is more common on Windows
static PREFERRED_SUPERSET: Lazy<HashMap<&'static str, &'static str>> = Lazy::new(|| {
    let mut map = HashMap::new();

    // ASCII is a subset of Windows-1252
    map.insert("ascii", "Windows-1252");

    // Korean: EUC-KR is largely superseded by CP949 (Unified Hangul Code)
    map.insert("euc-kr", "CP949");

    // Western European: ISO-8859-1 maps to Windows-1252 (superset with extra chars)
    map.insert("iso-8859-1", "Windows-1252");

    // Central European encodings
    map.insert("iso-8859-2", "Windows-1250");
    map.insert("iso-8859-16", "Windows-1250");

    // Cyrillic encodings
    map.insert("iso-8859-5", "Windows-1251");

    // Arabic encodings
    map.insert("iso-8859-6", "Windows-1256");

    // Greek encodings
    map.insert("iso-8859-7", "Windows-1253");

    // Hebrew encodings
    map.insert("iso-8859-8", "Windows-1255");

    // Turkish encodings
    map.insert("iso-8859-9", "Windows-1254");

    // Thai encodings
    map.insert("iso-8859-11", "CP874");
    map.insert("tis-620", "CP874");

    // Baltic encodings
    map.insert("iso-8859-13", "Windows-1257");
    map.insert("iso-8859-4", "Windows-1257");

    // Celtic encoding (rare, map to common Western)
    map.insert("iso-8859-14", "Windows-1252");

    map
});

/// Apply legacy-to-modern encoding name remapping.
///
/// This function implements the `should_rename_legacy` option from the chardet API.
/// When enabled, it maps legacy encoding names (like ISO-8859-1) to their modern
/// Windows equivalents (like Windows-1252) that are supersets of the original.
///
/// # Arguments
///
/// * `encoding` - The original encoding name
///
/// # Returns
///
/// The modernized encoding name if a mapping exists, otherwise the original name.
///
/// # Examples
///
/// ```
/// use _chardet_rs::equivalences::apply_legacy_rename;
///
/// assert_eq!(apply_legacy_rename("iso-8859-1"), "Windows-1252");
/// assert_eq!(apply_legacy_rename("utf-8"), "utf-8"); // No mapping
/// ```
pub fn apply_legacy_rename(encoding: &str) -> String {
    PREFERRED_SUPERSET
        .get(encoding.to_lowercase().as_str())
        .map(|&s| s.to_string())
        .unwrap_or_else(|| encoding.to_string())
}

/// Infer the primary language associated with an encoding.
///
/// Returns the ISO 639-1 language code most commonly associated with the given
/// encoding. For multi-language encodings (like UTF-8 or Windows-1252), returns
/// the most representative language or None.
///
/// # Arguments
///
/// * `encoding` - The encoding name (case-insensitive)
///
/// # Returns
///
/// An ISO 639-1 two-letter language code (e.g., "en", "ru", "ja") or `None`
/// if the encoding is not language-specific.
///
/// # Language Coverage
///
/// | Language | Code | Encodings |
/// |----------|------|-----------|
/// | Chinese | zh | Big5, GB18030, GB2312, GBK, HZ-GB-2312 |
/// | Japanese | ja | CP932, Shift_JIS, EUC-JP, ISO-2022-JP |
/// | Korean | ko | CP949, EUC-KR, Johab, ISO-2022-KR |
/// | Russian | ru | Windows-1251, KOI8-R, ISO-8859-5 |
/// | Greek | el | Windows-1253, ISO-8859-7 |
/// | Hebrew | he | Windows-1255, ISO-8859-8 |
/// | Arabic | ar | Windows-1256, ISO-8859-6 |
/// | Turkish | tr | Windows-1254, ISO-8859-9 |
/// | ... | ... | ... |
///
/// # Examples
///
/// ```
/// use _chardet_rs::equivalences::infer_language;
///
/// assert_eq!(infer_language("shift_jis"), Some("ja"));
/// assert_eq!(infer_language("koi8-r"), Some("ru"));
/// assert_eq!(infer_language("utf-8"), None);
/// ```
pub fn infer_language(encoding: &str) -> Option<&'static str> {
    let encoding_lower = encoding.to_lowercase();

    // Single-language encodings - map encoding name to language code
    match encoding_lower.as_str() {
        // Chinese encodings
        "big5" | "big5hkscs" | "gb18030" | "gb2312" | "gbk" | "hz-gb-2312" => Some("zh"),

        // Japanese encodings
        "cp932" | "shift_jis" | "shift-jis" | "shift_jis_2004" | "euc-jp" | "euc-jis-2004"
        | "iso-2022-jp" | "iso2022-jp-2" | "iso2022-jp-2004" | "iso2022-jp-ext" => Some("ja"),

        // Korean encodings
        "cp949" | "euc-kr" | "iso-2022-kr" | "johab" => Some("ko"),

        // Thai encodings
        "cp874" | "windows-874" | "tis-620" => Some("th"),

        // Central European (primarily Polish)
        "windows-1250" | "cp1250" | "iso-8859-2" | "mac-latin2" | "cp852" => Some("pl"),

        // Cyrillic encodings (primarily Russian)
        "windows-1251" | "cp1251" | "koi8-r" | "koi8-u" | "mac-cyrillic" | "iso-8859-5"
        | "cp855" | "cp866" => Some("ru"),

        // Western European (primarily English)
        "windows-1252" | "cp1252" | "mac-roman" | "hp-roman8" | "iso-8859-1" | "iso-8859-15"
        | "cp437" | "cp850" | "cp858" => Some("en"),

        // Greek encodings
        "windows-1253" | "cp1253" | "iso-8859-7" | "mac-greek" | "cp737" | "cp869" => Some("el"),

        // Turkish encodings
        "windows-1254" | "cp1254" | "iso-8859-9" | "mac-turkish" | "cp857" => Some("tr"),

        // Hebrew encodings
        "windows-1255" | "cp1255" | "iso-8859-8" | "cp856" | "cp862" | "cp424" => Some("he"),

        // Arabic encodings
        "windows-1256" | "cp1256" | "iso-8859-6" | "cp720" | "cp864" | "cp1006" => Some("ar"),

        // Baltic encodings (Estonian, Latvian, Lithuanian)
        "windows-1257" | "cp1257" | "iso-8859-13" | "iso-8859-4" | "iso-8859-10" | "cp775" => {
            Some("et")
        }

        // Vietnamese encodings
        "windows-1258" | "cp1258" => Some("vi"),

        // Other specific language encodings
        "iso-8859-3" => Some("eo"), // Esperanto, Maltese, Turkish
        "mac-iceland" | "cp861" => Some("is"), // Icelandic
        "iso-8859-14" => Some("cy"), // Welsh
        "koi8-t" => Some("tg"),     // Tajik
        "kz-1048" | "ptcp154" => Some("kk"), // Kazakh
        "cp860" => Some("pt"),      // Portuguese
        "cp863" => Some("fr"),      // French Canadian
        "cp865" => Some("da"),      // Danish/Norwegian
        "cp1125" => Some("uk"),     // Ukrainian
        "cp500" | "cp1140" | "cp273" | "cp1026" => Some("de"), // German
        "cp875" => Some("el"),      // Greek (alternative)

        // Multi-language or unknown
        _ => None,
    }
}
