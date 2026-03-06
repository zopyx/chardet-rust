//! Stage 2b: Multi-byte structural probing.

use crate::pipeline::PipelineContext;
use crate::registry::EncodingInfo;

/// Minimum structural score required for CJK candidates.
pub const CJK_MIN_MB_RATIO: f64 = 0.05;
pub const CJK_MIN_NON_ASCII: usize = 2;
pub const CJK_MIN_BYTE_COVERAGE: f64 = 0.35;
pub const CJK_MIN_LEAD_DIVERSITY: usize = 4;
pub const CJK_DIVERSITY_MIN_NON_ASCII: usize = 16;

/// Return 0.0--1.0 indicating how well data matches the encoding's structure.
pub fn compute_structural_score(
    data: &[u8],
    encoding_info: &EncodingInfo,
    ctx: &mut PipelineContext,
) -> f64 {
    if data.is_empty() || !encoding_info.is_multibyte {
        return 0.0;
    }

    let result = get_analysis(data, encoding_info.name, ctx);
    result.map(|(ratio, _, _)| ratio).unwrap_or(0.0)
}

/// Compute byte coverage (non-ASCII bytes in valid multi-byte sequences / total non-ASCII bytes).
pub fn compute_multibyte_byte_coverage(
    data: &[u8],
    encoding_info: &EncodingInfo,
    ctx: &mut PipelineContext,
    non_ascii_count: Option<usize>,
) -> f64 {
    if data.is_empty() || !encoding_info.is_multibyte {
        return 0.0;
    }

    let result = get_analysis(data, encoding_info.name, ctx);
    if result.is_none() {
        return 0.0;
    }

    let mb_bytes = result.unwrap().1;

    let non_ascii = non_ascii_count.unwrap_or_else(|| data.iter().filter(|&&b| b > 0x7F).count());

    if non_ascii == 0 {
        return 0.0;
    }

    mb_bytes as f64 / non_ascii as f64
}

/// Count distinct lead byte values in valid multi-byte pairs.
pub fn compute_lead_byte_diversity(
    data: &[u8],
    encoding_info: &EncodingInfo,
    ctx: &mut PipelineContext,
) -> usize {
    if data.is_empty() || !encoding_info.is_multibyte {
        return 0;
    }

    let result = get_analysis(data, encoding_info.name, ctx);
    result.map(|(_, _, diversity)| diversity).unwrap_or(256)
}

fn get_analysis(data: &[u8], name: &str, ctx: &mut PipelineContext) -> Option<(f64, usize, usize)> {
    if let Some(&cached) = ctx.analysis_cache.get(name) {
        return Some(cached);
    }

    let result = analyze_encoding(data, name);
    if let Some(r) = result {
        ctx.analysis_cache.insert(name.to_string(), r);
    }
    result
}

fn analyze_encoding(data: &[u8], name: &str) -> Option<(f64, usize, usize)> {
    match name {
        "shift_jis_2004" | "cp932" => Some(analyze_shift_jis(data)),
        "euc-jis-2004" | "euc-jp" => Some(analyze_euc_jp(data)),
        "euc-kr" | "cp949" => Some(analyze_euc_kr(data)),
        "gb18030" | "gb2312" | "gbk" => Some(analyze_gb18030(data)),
        "big5hkscs" | "big5" => Some(analyze_big5(data)),
        "johab" => Some(analyze_johab(data)),
        _ => None,
    }
}

fn analyze_shift_jis(data: &[u8]) -> (f64, usize, usize) {
    let mut lead_count = 0;
    let mut valid_count = 0;
    let mut mb_bytes = 0;
    let mut leads = std::collections::HashSet::new();

    let mut i = 0;
    while i < data.len() {
        let b = data[i];
        // Lead bytes: 0x81-0x9F, 0xE0-0xFC (includes 0xF0-0xFC for CP932 extended)
        if (0x81..=0x9F).contains(&b) || (0xE0..=0xFC).contains(&b) {
            lead_count += 1;
            if i + 1 < data.len() {
                let trail = data[i + 1];
                if (0x40..=0x7E).contains(&trail) || (0x80..=0xFC).contains(&trail) {
                    valid_count += 1;
                    leads.insert(b);
                    mb_bytes += 1;
                    if trail > 0x7F {
                        mb_bytes += 1;
                    }
                    i += 2;
                    continue;
                }
            }
            i += 1;
        } else {
            i += 1;
        }
    }

    let ratio = if lead_count > 0 {
        valid_count as f64 / lead_count as f64
    } else {
        0.0
    };
    (ratio, mb_bytes, leads.len())
}

fn analyze_euc_jp(data: &[u8]) -> (f64, usize, usize) {
    let mut lead_count = 0;
    let mut valid_count = 0;
    let mut mb_bytes = 0;
    let mut leads = std::collections::HashSet::new();

    let mut i = 0;
    while i < data.len() {
        let b = data[i];
        if b == 0x8E {
            // SS2 sequence
            lead_count += 1;
            if i + 1 < data.len() && (0xA1..=0xDF).contains(&data[i + 1]) {
                valid_count += 1;
                leads.insert(b);
                mb_bytes += 2;
                i += 2;
                continue;
            }
            i += 1;
        } else if b == 0x8F {
            // SS3 sequence
            lead_count += 1;
            if i + 2 < data.len()
                && (0xA1..=0xFE).contains(&data[i + 1])
                && (0xA1..=0xFE).contains(&data[i + 2])
            {
                valid_count += 1;
                leads.insert(b);
                mb_bytes += 3;
                i += 3;
                continue;
            }
            i += 1;
        } else if (0xA1..=0xFE).contains(&b) {
            lead_count += 1;
            if i + 1 < data.len() && (0xA1..=0xFE).contains(&data[i + 1]) {
                valid_count += 1;
                leads.insert(b);
                mb_bytes += 2;
                i += 2;
                continue;
            }
            i += 1;
        } else {
            i += 1;
        }
    }

    let ratio = if lead_count > 0 {
        valid_count as f64 / lead_count as f64
    } else {
        0.0
    };
    (ratio, mb_bytes, leads.len())
}

fn analyze_euc_kr(data: &[u8]) -> (f64, usize, usize) {
    let mut lead_count = 0;
    let mut valid_count = 0;
    let mut mb_bytes = 0;
    let mut leads = std::collections::HashSet::new();

    let mut i = 0;
    while i < data.len() {
        let b = data[i];
        if (0xA1..=0xFE).contains(&b) {
            lead_count += 1;
            if i + 1 < data.len() && (0xA1..=0xFE).contains(&data[i + 1]) {
                valid_count += 1;
                leads.insert(b);
                mb_bytes += 2;
                i += 2;
                continue;
            }
            i += 1;
        } else {
            i += 1;
        }
    }

    let ratio = if lead_count > 0 {
        valid_count as f64 / lead_count as f64
    } else {
        0.0
    };
    (ratio, mb_bytes, leads.len())
}

fn analyze_gb18030(data: &[u8]) -> (f64, usize, usize) {
    let mut lead_count = 0;
    let mut valid_count = 0;
    let mut mb_bytes = 0;
    let mut leads = std::collections::HashSet::new();

    let mut i = 0;
    while i < data.len() {
        let b = data[i];
        if (0x81..=0xFE).contains(&b) {
            lead_count += 1;
            // Try 4-byte first
            if i + 3 < data.len()
                && (0x30..=0x39).contains(&data[i + 1])
                && (0x81..=0xFE).contains(&data[i + 2])
                && (0x30..=0x39).contains(&data[i + 3])
            {
                valid_count += 1;
                leads.insert(b);
                mb_bytes += 2;
                i += 4;
                continue;
            }
            // 2-byte GB2312
            if (0xA1..=0xF7).contains(&b)
                && i + 1 < data.len()
                && (0xA1..=0xFE).contains(&data[i + 1])
            {
                valid_count += 1;
                leads.insert(b);
                mb_bytes += 2;
                i += 2;
                continue;
            }
            i += 1;
        } else {
            i += 1;
        }
    }

    let ratio = if lead_count > 0 {
        valid_count as f64 / lead_count as f64
    } else {
        0.0
    };
    (ratio, mb_bytes, leads.len())
}

fn analyze_big5(data: &[u8]) -> (f64, usize, usize) {
    let mut lead_count = 0;
    let mut valid_count = 0;
    let mut mb_bytes = 0;
    let mut leads = std::collections::HashSet::new();

    let mut i = 0;
    while i < data.len() {
        let b = data[i];
        if (0xA1..=0xF9).contains(&b) {
            lead_count += 1;
            if i + 1 < data.len() {
                let trail = data[i + 1];
                if (0x40..=0x7E).contains(&trail) || (0xA1..=0xFE).contains(&trail) {
                    valid_count += 1;
                    leads.insert(b);
                    mb_bytes += 1;
                    if trail > 0x7F {
                        mb_bytes += 1;
                    }
                    i += 2;
                    continue;
                }
            }
            i += 1;
        } else {
            i += 1;
        }
    }

    let ratio = if lead_count > 0 {
        valid_count as f64 / lead_count as f64
    } else {
        0.0
    };
    (ratio, mb_bytes, leads.len())
}

fn analyze_johab(data: &[u8]) -> (f64, usize, usize) {
    let mut lead_count = 0;
    let mut valid_count = 0;
    let mut mb_bytes = 0;
    let mut leads = std::collections::HashSet::new();

    let mut i = 0;
    while i < data.len() {
        let b = data[i];
        if (0x84..=0xD3).contains(&b) || (0xD8..=0xDE).contains(&b) || (0xE0..=0xF9).contains(&b) {
            lead_count += 1;
            if i + 1 < data.len() {
                let trail = data[i + 1];
                if (0x31..=0x7E).contains(&trail) || (0x81..=0xFE).contains(&trail) {
                    valid_count += 1;
                    leads.insert(b);
                    if b > 0x7F {
                        mb_bytes += 1;
                    }
                    if trail > 0x7F {
                        mb_bytes += 1;
                    }
                    i += 2;
                    continue;
                }
            }
            i += 1;
        } else {
            i += 1;
        }
    }

    let ratio = if lead_count > 0 {
        valid_count as f64 / lead_count as f64
    } else {
        0.0
    };
    (ratio, mb_bytes, leads.len())
}
