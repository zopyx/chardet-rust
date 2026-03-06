//! Stage 1b: HTML/XML charset declaration extraction.

use super::{DetectionResult, DETERMINISTIC_CONFIDENCE};

const SCAN_LIMIT: usize = 4096;

/// Scan the first bytes of data for an HTML/XML charset declaration.
pub fn detect_markup_charset(data: &[u8]) -> Option<DetectionResult> {
    if data.is_empty() {
        return None;
    }
    
    let head = &data[..data.len().min(SCAN_LIMIT)];
    
    // Check for XML encoding declaration
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
        String::from_utf8(encoding.to_vec()).ok()
            .map(|s| s.trim().to_lowercase())
    } else {
        None
    }
}

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
                if !has_quote
                    && (c == b' ' || c == b'>' || c == b';' || c == b'"' || c == b'\'')
                {
                    break;
                }
                i += 1;
            }
            
            if start < i {
                let encoding = &after_cs[start..i];
                return String::from_utf8(encoding.to_vec()).ok()
                    .map(|s| s.trim().to_lowercase());
            }
        }
        
        pos = meta_start + 5;
    }
    
    None
}

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
            while i < after_content.len() && (after_content[i] == b' ' || after_content[i] == b'=') {
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
            let content_start = i;
            let mut found = false;
            while i < after_content.len() && after_content[i] != quote {
                if after_content[i..].starts_with(b"charset=") {
                    let after_cs = &after_content[i + 8..];
                    let mut j = 0;
                    while j < after_cs.len() && after_cs[j] != quote && after_cs[j] != b';' && after_cs[j] != b' ' {
                        j += 1;
                    }
                    if j > 0 {
                        let encoding = &after_cs[..j];
                        return String::from_utf8(encoding.to_vec()).ok()
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

fn find_subsequence(data: &[u8], pattern: &[u8]) -> Option<usize> {
    if pattern.is_empty() || data.len() < pattern.len() {
        return None;
    }
    data.windows(pattern.len()).position(|window| window == pattern)
}

fn find_case_insensitive(data: &[u8], pattern: &[u8]) -> Option<usize> {
    let data_lower = data.to_ascii_lowercase();
    let pattern_lower: Vec<u8> = pattern.iter().map(|&b| b.to_ascii_lowercase()).collect();
    find_subsequence(&data_lower, &pattern_lower)
}

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

fn validate_bytes(data: &[u8], encoding: &str) -> bool {
    // Check that data can be decoded under encoding without errors.
    // For now, we just do a basic check for common encodings.
    // In a full implementation, we'd use an encoding library.
    match encoding {
        "utf-8" | "utf8" => {
            std::str::from_utf8(&data[..data.len().min(SCAN_LIMIT)]).is_ok()
        }
        _ => {
            // For other encodings, we need to use Python's codec system
            // For now, assume valid
            true
        }
    }
}
