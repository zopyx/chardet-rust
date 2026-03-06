//! Stage 2a: Byte sequence validity filtering.

use crate::registry::EncodingInfo;

/// Filter candidates to only those where data decodes without errors.
pub fn filter_by_validity<'a>(
    data: &[u8],
    candidates: &[&'a EncodingInfo],
) -> Vec<&'a EncodingInfo> {
    if data.is_empty() {
        return candidates.to_vec();
    }
    
    candidates
        .iter()
        .filter(|enc| is_valid_for_encoding(data, enc))
        .copied()
        .collect()
}

/// Check if data is valid for a given encoding.
fn is_valid_for_encoding(data: &[u8], enc: &EncodingInfo) -> bool {
    // For most encodings, we rely on structural validation.
    // For UTF-8, we've already validated it earlier in the pipeline.
    
    match enc.name {
        "utf-8" | "utf-8-sig" => {
            // UTF-8 validation is done separately
            true
        }
        "ascii" => {
            data.iter().all(|&b| b < 0x80)
        }
        _ if enc.is_multibyte => {
            // For CJK encodings, we do basic structural checks
            is_valid_multibyte(data, enc.name)
        }
        _ => {
            // For single-byte encodings, most byte sequences are technically valid
            // We rely on statistical analysis to determine the correct one
            true
        }
    }
}

fn is_valid_multibyte(data: &[u8], encoding: &str) -> bool {
    match encoding {
        "shift_jis_2004" | "cp932" => is_valid_shift_jis(data),
        "euc-jis-2004" | "euc-jp" => is_valid_euc_jp(data),
        "euc-kr" | "cp949" => is_valid_euc_kr(data),
        "gb18030" | "gb2312" | "gbk" => is_valid_gb18030(data),
        "big5hkscs" | "big5" => is_valid_big5(data),
        "johab" => is_valid_johab(data),
        "hz-gb-2312" => is_valid_hz(data),
        _ => true, // Unknown multibyte - let it through for statistical scoring
    }
}

fn is_valid_shift_jis(data: &[u8]) -> bool {
    let mut i = 0;
    while i < data.len() {
        let b = data[i];
        if b < 0x80 {
            i += 1;
            continue;
        }
        // Lead bytes: 0x81-0x9F, 0xE0-0xEF
        if (0x81..=0x9F).contains(&b) || (0xE0..=0xEF).contains(&b) {
            if i + 1 >= data.len() {
                return true; // Truncated at end is OK
            }
            let trail = data[i + 1];
            // Trail bytes: 0x40-0x7E, 0x80-0xFC
            if !((0x40..=0x7E).contains(&trail) || (0x80..=0xFC).contains(&trail)) {
                return false;
            }
            i += 2;
        } else {
            // Invalid lead byte
            return false;
        }
    }
    true
}

fn is_valid_euc_jp(data: &[u8]) -> bool {
    let mut i = 0;
    while i < data.len() {
        let b = data[i];
        if b < 0x80 {
            i += 1;
            continue;
        }
        if b == 0x8E {
            // SS2: half-width katakana
            if i + 1 >= data.len() {
                return true;
            }
            if !(0xA1..=0xDF).contains(&data[i + 1]) {
                return false;
            }
            i += 2;
        } else if b == 0x8F {
            // SS3: JIS X 0212
            if i + 2 >= data.len() {
                return true;
            }
            if !(0xA1..=0xFE).contains(&data[i + 1]) || !(0xA1..=0xFE).contains(&data[i + 2]) {
                return false;
            }
            i += 3;
        } else if (0xA1..=0xFE).contains(&b) {
            // Two-byte sequence
            if i + 1 >= data.len() {
                return true;
            }
            if !(0xA1..=0xFE).contains(&data[i + 1]) {
                return false;
            }
            i += 2;
        } else {
            return false;
        }
    }
    true
}

fn is_valid_euc_kr(data: &[u8]) -> bool {
    let mut i = 0;
    while i < data.len() {
        let b = data[i];
        if b < 0x80 {
            i += 1;
            continue;
        }
        if (0xA1..=0xFE).contains(&b) {
            if i + 1 >= data.len() {
                return true;
            }
            if !(0xA1..=0xFE).contains(&data[i + 1]) {
                return false;
            }
            i += 2;
        } else {
            return false;
        }
    }
    true
}

fn is_valid_gb18030(data: &[u8]) -> bool {
    let mut i = 0;
    while i < data.len() {
        let b = data[i];
        if b < 0x80 {
            i += 1;
            continue;
        }
        if (0x81..=0xFE).contains(&b) {
            if i + 1 >= data.len() {
                return true;
            }
            let b2 = data[i + 1];
            if (0x30..=0x39).contains(&b2) {
                // 4-byte sequence
                if i + 3 >= data.len() {
                    return true;
                }
                if !(0x81..=0xFE).contains(&data[i + 2]) || !(0x30..=0x39).contains(&data[i + 3]) {
                    return false;
                }
                i += 4;
            } else if (0x40..=0xFE).contains(&b2) {
                // 2-byte sequence
                i += 2;
            } else {
                return false;
            }
        } else {
            return false;
        }
    }
    true
}

fn is_valid_big5(data: &[u8]) -> bool {
    let mut i = 0;
    while i < data.len() {
        let b = data[i];
        if b < 0x80 {
            i += 1;
            continue;
        }
        if (0xA1..=0xF9).contains(&b) {
            if i + 1 >= data.len() {
                return true;
            }
            let trail = data[i + 1];
            if !((0x40..=0x7E).contains(&trail) || (0xA1..=0xFE).contains(&trail)) {
                return false;
            }
            i += 2;
        } else {
            return false;
        }
    }
    true
}

fn is_valid_johab(data: &[u8]) -> bool {
    let mut i = 0;
    while i < data.len() {
        let b = data[i];
        if b < 0x80 {
            i += 1;
            continue;
        }
        // Lead: 0x84-0xD3, 0xD8-0xDE, 0xE0-0xF9
        if (0x84..=0xD3).contains(&b) || 
           (0xD8..=0xDE).contains(&b) || 
           (0xE0..=0xF9).contains(&b) {
            if i + 1 >= data.len() {
                return true;
            }
            let trail = data[i + 1];
            // Trail: 0x31-0x7E, 0x81-0xFE
            if !((0x31..=0x7E).contains(&trail) || (0x81..=0xFE).contains(&trail)) {
                return false;
            }
            i += 2;
        } else {
            return false;
        }
    }
    true
}

fn is_valid_hz(data: &[u8]) -> bool {
    // HZ-GB-2312 uses ~{ and ~} to shift in/out of GB mode
    // In GB mode, characters are pairs of bytes in 0x21-0x7E
    let mut in_gb_mode = false;
    let mut i = 0;
    
    while i < data.len() {
        if data[i] == b'~' {
            if i + 1 < data.len() {
                if data[i + 1] == b'{' {
                    in_gb_mode = true;
                    i += 2;
                    continue;
                } else if data[i + 1] == b'}' {
                    in_gb_mode = false;
                    i += 2;
                    continue;
                } else if data[i + 1] == b'~' {
                    // Escaped tilde
                    i += 2;
                    continue;
                }
            }
        }
        
        if in_gb_mode {
            // In GB mode, expect pairs in 0x21-0x7E
            if !(0x21..=0x7E).contains(&data[i]) {
                return false;
            }
        } else if data[i] > 0x7F {
            return false;
        }
        
        i += 1;
    }
    
    true
}
