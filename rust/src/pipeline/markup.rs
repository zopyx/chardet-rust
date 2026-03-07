//! Stage 1b: HTML/XML charset declaration extraction.
//!
//! This module extracts charset declarations from HTML and XML documents.
//! When a document explicitly declares its encoding in a meta tag or XML
//! declaration, we can use that information with high confidence.
//!
//! # Supported Declarations
//!
//! ## XML
//! ```xml
//! <?xml version="1.0" encoding="UTF-8"?>
//! ```
//!
//! ## HTML5
//! ```html
//! <meta charset="utf-8">
//! ```
//!
//! ## HTML4
//! ```html
//! <meta http-equiv="Content-Type" content="text/html; charset=utf-8">
//! ```

use super::{DetectionResult, DETERMINISTIC_CONFIDENCE};

/// Maximum bytes to scan for charset declarations.
///
/// Charset declarations typically appear early in HTML/XML documents.
/// We limit scanning to the first 4KB for performance.
const SCAN_LIMIT: usize = 4096;

/// Scan the first bytes of data for an HTML/XML charset declaration.
///
/// This function looks for charset declarations in three places:
/// 1. XML encoding declaration (<?xml ... encoding="..."?>)
/// 2. HTML5 meta charset tag
/// 3. HTML4 http-equiv Content-Type meta tag
///
/// # Arguments
///
/// * `data` - The byte sequence to analyze
///
/// # Returns
///
/// - `Some(DetectionResult)` with the declared encoding and confidence 0.95
/// - `None` if no declaration is found or if the declared encoding fails validation
///
/// # Algorithm
///
/// 1. Scan only the first SCAN_LIMIT bytes for performance
/// 2. Check for XML declaration first (must be at document start)
/// 3. Search for HTML5 `<meta charset>` tags
/// 4. Search for HTML4 `<meta http-equiv="Content-Type">` tags
/// 5. Normalize the declared encoding name
/// 6. Validate that the data is consistent with the declared encoding
///
/// # Examples
///
/// ```
/// use _chardet_rs::pipeline::markup::detect_markup_charset;
///
/// // HTML5 meta charset
/// let html = b"<html><head><meta charset=\"utf-8\">";
/// let result = detect_markup_charset(html);
/// assert!(result.is_some());
///
/// // XML declaration
/// let xml = b"<?xml version=\"1.0\" encoding=\"UTF-8\"?>";
/// let result = detect_markup_charset(xml);
/// assert!(result.is_some());
///
/// // No declaration
/// assert!(detect_markup_charset(b"Hello").is_none());
/// ```
pub fn detect_markup_charset(data: &[u8]) -> Option<DetectionResult> {
    if data.is_empty() {
        return None;
    }

    let head = &data[..data.len().min(SCAN_LIMIT)];

    // Check for XML encoding declaration first
    if let Some(raw_encoding) = detect_xml_encoding(head) {
        if let Some(encoding) = normalize_declared_encoding(&raw_encoding) {
            if validate_bytes(data, &encoding) {
                return Some(DetectionResult::new(
                    Some(&encoding),
                    DETERMINISTIC_CONFIDENCE,
                    None,
                ));
            }
        }
    }

    // Check for HTML5 charset meta tag
    if let Some(raw_encoding) = detect_html5_charset(head) {
        if let Some(encoding) = normalize_declared_encoding(&raw_encoding) {
            if validate_bytes(data, &encoding) {
                return Some(DetectionResult::new(
                    Some(&encoding),
                    DETERMINISTIC_CONFIDENCE,
                    None,
                ));
            }
        }
    }

    // Check for HTML4 content-type meta tag
    if let Some(raw_encoding) = detect_html4_charset(head) {
        if let Some(encoding) = normalize_declared_encoding(&raw_encoding) {
            if validate_bytes(data, &encoding) {
                return Some(DetectionResult::new(
                    Some(&encoding),
                    DETERMINISTIC_CONFIDENCE,
                    None,
                ));
            }
        }
    }

    None
}

/// Extract encoding from XML declaration.
///
/// Looks for: `<?xml ... encoding="..."?>`
///
/// # Arguments
///
/// * `data` - The byte sequence to analyze
///
/// # Returns
///
/// The declared encoding name, or `None` if not found.
///
/// # Parsing Rules
///
/// 1. Document must start with `<?xml`
/// 2. Find `encoding` attribute (case-insensitive)
/// 3. Extract value between matching quotes
fn detect_xml_encoding(data: &[u8]) -> Option<String> {
    // Look for <?xml ... encoding="..."?>
    let prefix = b"<?xml";
    if !data.starts_with(prefix) {
        return None;
    }

    // Find the encoding attribute
    if let Some(pos) = find_case_insensitive(data, b"encoding") {
        let after_encoding = &data[pos + 8..]; // Skip "encoding"

        // Skip whitespace and =
        let mut i = 0;
        while i < after_encoding.len() && (after_encoding[i] == b' ' || after_encoding[i] == b'=') {
            i += 1;
        }

        if i >= after_encoding.len() {
            return None;
        }

        // Get the quote character
        let quote = after_encoding[i];
        if quote != b'"' && quote != b'\'' {
            return None;
        }
        i += 1;

        // Find the closing quote
        let start = i;
        while i < after_encoding.len() && after_encoding[i] != quote {
            i += 1;
        }

        if i >= after_encoding.len() {
            return None;
        }

        let encoding = &after_encoding[start..i];
        String::from_utf8(encoding.to_vec())
            .ok()
            .map(|s| s.trim().to_lowercase())
    } else {
        None
    }
}

/// Extract charset from HTML5 meta tag.
///
/// Looks for: `<meta charset="...">`
///
/// # Arguments
///
/// * `data` - The byte sequence to analyze
///
/// # Returns
///
/// The declared charset name, or `None` if not found.
///
/// # Parsing Rules
///
/// 1. Search for `<meta` tags (case-insensitive)
/// 2. Look for `charset` attribute
/// 3. Handle quoted and unquoted values
/// 4. Stop at tag end
fn detect_html5_charset(data: &[u8]) -> Option<String> {
    // Look for <meta charset="...">
    let data_lower = data.to_ascii_lowercase();

    let mut pos = 0;
    while let Some(meta_pos) = find_subsequence(&data_lower[pos..], b"<meta") {
        let meta_start = pos + meta_pos;
        let after_meta = &data_lower[meta_start..];

        // Find charset attribute
        if let Some(cs_pos) = find_subsequence(after_meta, b"charset") {
            let after_cs = &after_meta[cs_pos + 7..];

            // Skip whitespace and =
            let mut i = 0;
            while i < after_cs.len() && (after_cs[i] == b' ' || after_cs[i] == b'=') {
                i += 1;
            }

            if i >= after_cs.len() {
                pos = meta_start + 5;
                continue;
            }

            // Handle optional quote
            let has_quote = after_cs[i] == b'"' || after_cs[i] == b'\'';
            if has_quote {
                i += 1;
            }

            let start = i;
            while i < after_cs.len() {
                let c = after_cs[i];
                if has_quote && (c == b'"' || c == b'\'') {
                    break;
                }
                if !has_quote && (c == b' ' || c == b'>' || c == b';' || c == b'"' || c == b'\'') {
                    break;
                }
                i += 1;
            }

            if start < i {
                let encoding = &after_cs[start..i];
                return String::from_utf8(encoding.to_vec())
                    .ok()
                    .map(|s| s.trim().to_lowercase());
            }
        }

        pos = meta_start + 5;
    }

    None
}

/// Extract charset from HTML4 meta http-equiv tag.
///
/// Looks for: `<meta http-equiv="Content-Type" content="...; charset=...">`
///
/// # Arguments
///
/// * `data` - The byte sequence to analyze
///
/// # Returns
///
/// The declared charset name, or `None` if not found.
///
/// # Parsing Rules
///
/// 1. Search for `<meta` tags
/// 2. Look for `http-equiv="Content-Type"` or similar
/// 3. Find `content` attribute with `charset=` parameter
/// 4. Extract charset value
fn detect_html4_charset(data: &[u8]) -> Option<String> {
    // Look for <meta http-equiv="Content-Type" content="...; charset=...">
    let data_lower = data.to_ascii_lowercase();

    let mut pos = 0;
    while let Some(meta_pos) = find_subsequence(&data_lower[pos..], b"<meta") {
        let meta_start = pos + meta_pos;
        let after_meta = &data_lower[meta_start..];

        // Look for content attribute with charset
        if let Some(content_pos) = find_subsequence(after_meta, b"content") {
            let after_content = &after_meta[content_pos + 7..];

            // Skip whitespace and =
            let mut i = 0;
            while i < after_content.len() && (after_content[i] == b' ' || after_content[i] == b'=')
            {
                i += 1;
            }

            if i >= after_content.len() {
                pos = meta_start + 5;
                continue;
            }

            // Get the quote character
            let quote = after_content[i];
            if quote != b'"' && quote != b'\'' {
                pos = meta_start + 5;
                continue;
            }
            i += 1;

            // Find charset= within the content
            let mut found = false;
            while i < after_content.len() && after_content[i] != quote {
                if after_content[i..].starts_with(b"charset=") {
                    let after_cs = &after_content[i + 8..];
                    let mut j = 0;
                    while j < after_cs.len()
                        && after_cs[j] != quote
                        && after_cs[j] != b';'
                        && after_cs[j] != b' '
                    {
                        j += 1;
                    }
                    if j > 0 {
                        let encoding = &after_cs[..j];
                        return String::from_utf8(encoding.to_vec())
                            .ok()
                            .map(|s| s.trim().to_lowercase());
                    }
                    found = true;
                    break;
                }
                i += 1;
            }

            if found {
                break;
            }
        }

        pos = meta_start + 5;
    }

    None
}

/// Find the first occurrence of a byte subsequence.
///
/// # Arguments
///
/// * `data` - The haystack to search in
/// * `pattern` - The needle to search for
///
/// # Returns
///
/// The starting index of the first match, or `None` if not found.
fn find_subsequence(data: &[u8], pattern: &[u8]) -> Option<usize> {
    if pattern.is_empty() || data.len() < pattern.len() {
        return None;
    }
    data.windows(pattern.len())
        .position(|window| window == pattern)
}

/// Case-insensitive search for a byte subsequence.
///
/// # Arguments
///
/// * `data` - The haystack to search in
/// * `pattern` - The needle to search for
///
/// # Returns
///
/// The starting index of the first case-insensitive match, or `None`.
fn find_case_insensitive(data: &[u8], pattern: &[u8]) -> Option<usize> {
    let data_lower = data.to_ascii_lowercase();
    let pattern_lower: Vec<u8> = pattern.iter().map(|&b| b.to_ascii_lowercase()).collect();
    find_subsequence(&data_lower, &pattern_lower)
}

/// Normalize a declared encoding name to a canonical form.
///
/// # Arguments
///
/// * `raw` - The raw encoding name from the document
///
/// # Returns
///
/// The normalized encoding name, or `None` if empty/invalid.
///
/// # Normalizations
///
/// | Input | Output |
/// |-------|--------|
/// | x-sjis, shift-jis | cp932 |
fn normalize_declared_encoding(raw: &str) -> Option<String> {
    let normalized = raw
        .trim()
        .trim_matches(|c| c == '"' || c == '\'')
        .to_ascii_lowercase();

    if normalized.is_empty() {
        return None;
    }

    let canonical = match normalized.as_str() {
        "x-sjis" | "shift-jis" | "shift_jis" | "sjis" => "cp932",
        _ => normalized.as_str(),
    };

    Some(canonical.to_string())
}

/// Validate that data is consistent with the declared encoding.
///
/// # Arguments
///
/// * `data` - The byte sequence to validate
/// * `encoding` - The declared encoding name
///
/// # Returns
///
/// `true` if the data appears valid for the encoding.
///
/// # Note
///
/// This is a basic check. Full validation would require attempting to
/// decode the entire content, which is expensive. We do a quick check
/// for UTF-8 validity and assume other encodings are valid.
fn validate_bytes(data: &[u8], encoding: &str) -> bool {
    // Check that data can be decoded under encoding without errors.
    // For now, we just do a basic check for common encodings.
    // In a full implementation, we'd use an encoding library.
    match encoding {
        "utf-8" | "utf8" => std::str::from_utf8(&data[..data.len().min(SCAN_LIMIT)]).is_ok(),
        _ => {
            // For other encodings, we need to use Python's codec system
            // For now, assume valid
            true
        }
    }
}
