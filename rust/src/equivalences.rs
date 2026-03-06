//! Encoding equivalences and legacy name remapping.

use std::collections::HashMap;
use once_cell::sync::Lazy;

/// Preferred superset mapping for the should_rename_legacy API option.
/// Uses display-cased names to match chardet 6.x output (e.g., "Windows-1252").
static PREFERRED_SUPERSET: Lazy<HashMap<&'static str, &'static str>> = Lazy::new(|| {
    let mut map = HashMap::new();
    map.insert("ascii", "Windows-1252");
    map.insert("euc-kr", "CP949");
    map.insert("iso-8859-1", "Windows-1252");
    map.insert("iso-8859-2", "Windows-1250");
    map.insert("iso-8859-5", "Windows-1251");
    map.insert("iso-8859-6", "Windows-1256");
    map.insert("iso-8859-7", "Windows-1253");
    map.insert("iso-8859-8", "Windows-1255");
    map.insert("iso-8859-9", "Windows-1254");
    map.insert("iso-8859-11", "CP874");
    map.insert("iso-8859-13", "Windows-1257");
    map.insert("iso-8859-14", "Windows-1252");
    map.insert("iso-8859-15", "Windows-1252");
    map.insert("iso-8859-16", "Windows-1250");
    map.insert("tis-620", "CP874");
    map
});

/// Apply legacy renaming to an encoding name.
pub fn apply_legacy_rename(encoding: &str) -> String {
    PREFERRED_SUPERSET
        .get(encoding.to_lowercase().as_str())
        .map(|&s| s.to_string())
        .unwrap_or_else(|| encoding.to_string())
}

/// Infer language from encoding name.
pub fn infer_language(encoding: &str) -> Option<&'static str> {
    let encoding_lower = encoding.to_lowercase();
    
    // Single-language encodings
    match encoding_lower.as_str() {
        "big5" | "big5hkscs" | "gb18030" | "gb2312" | "gbk" | "hz-gb-2312" => Some("zh"),
        "cp932" | "shift_jis" | "shift-jis" | "shift_jis_2004" | "euc-jp" | "euc-jis-2004" | "iso-2022-jp" | "iso2022-jp-2" | "iso2022-jp-2004" | "iso2022-jp-ext" => Some("ja"),
        "cp949" | "euc-kr" | "iso-2022-kr" | "johab" => Some("ko"),
        "cp874" | "windows-874" | "tis-620" => Some("th"),
        "windows-1250" | "cp1250" => Some("pl"), // Most common
        "windows-1251" | "cp1251" | "koi8-r" | "koi8-u" | "mac-cyrillic" | "iso-8859-5" => Some("ru"),
        "windows-1252" | "cp1252" => Some("en"),
        "windows-1253" | "cp1253" | "iso-8859-7" => Some("el"),
        "windows-1254" | "cp1254" | "iso-8859-9" | "mac-turkish" => Some("tr"),
        "windows-1255" | "cp1255" | "iso-8859-8" => Some("he"),
        "windows-1256" | "cp1256" | "iso-8859-6" | "cp720" => Some("ar"),
        "windows-1257" | "cp1257" | "iso-8859-13" => Some("et"),
        "windows-1258" | "cp1258" => Some("vi"),
        "iso-8859-2" | "mac-latin2" | "cp852" => Some("pl"),
        "iso-8859-4" | "iso-8859-13" => Some("et"),
        "iso-8859-10" | "mac-iceland" => Some("is"),
        "iso-8859-14" => Some("cy"),
        "mac-greek" | "cp737" | "cp869" => Some("el"),
        "mac-roman" => Some("en"),
        "hp-roman8" => Some("en"),
        "cp1006" => Some("ur"),
        "koi8-t" => Some("tg"),
        "kz-1048" | "ptcp154" => Some("kk"),
        "cp437" | "cp850" | "cp858" => Some("en"),
        "cp737" => Some("el"),
        "cp775" => Some("et"),
        "cp855" | "cp866" => Some("ru"),
        "cp856" | "cp862" => Some("he"),
        "cp857" => Some("tr"),
        "cp860" => Some("pt"),
        "cp861" => Some("is"),
        "cp863" => Some("fr"),
        "cp864" => Some("ar"),
        "cp865" => Some("da"),
        "cp1125" => Some("uk"),
        "cp424" => Some("he"),
        "cp500" | "cp1140" | "cp273" | "cp1026" => Some("de"),
        "cp875" => Some("el"),
        _ => None,
    }
}
