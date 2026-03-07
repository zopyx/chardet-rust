//! Stage 2b: Multi-byte structural probing.
//!
//! This module analyzes the structural patterns of CJK multi-byte encodings.
//! It validates that high bytes appear in proper sequences and measures
//! how well the data conforms to each encoding's structural rules.
//!
//! # Gating Strategy
//!
//! Before expensive statistical analysis, we validate that CJK candidates
//! actually have the expected multi-byte structure. This eliminates
//! false positives where random binary data happens to have bytes that
//! look like CJK lead bytes.
//!
//! # Analysis Metrics
//!
//! 1. **Valid sequence ratio**: Percentage of lead bytes followed by valid trails
//! 2. **Byte coverage**: Percentage of high bytes in valid multi-byte sequences
//! 3. **Lead diversity**: Number of distinct lead byte values used

use crate::pipeline::PipelineContext;
use crate::registry::EncodingInfo;

/// Minimum structural score for CJK candidates to proceed.
///
/// Candidates with scores below this threshold are filtered out.
pub const CJK_MIN_MB_RATIO: f64 = 0.12;

/// Minimum number of non-ASCII bytes required for CJK analysis.
///
/// Too few high bytes make structural analysis unreliable.
pub const CJK_MIN_NON_ASCII: usize = 4;

/// Minimum percentage of high bytes that must be in valid sequences.
///
/// Filters out data with many orphaned high bytes.
pub const CJK_MIN_BYTE_COVERAGE: f64 = 0.55;

/// Minimum number of distinct lead bytes for diversity check.
///
/// Real CJK text typically uses many different lead bytes.
pub const CJK_MIN_LEAD_DIVERSITY: usize = 6;

/// Threshold for applying the lead diversity check.
///
/// Only check diversity when there are enough high bytes.
pub const CJK_DIVERSITY_MIN_NON_ASCII: usize = 16;

/// Compute structural score for a multi-byte encoding.
///
/// Returns a value from 0.0 to 1.0 indicating how well the data matches
/// the encoding's structural rules. Higher scores indicate better matches.
///
/// # Arguments
///
/// * `data` - The byte sequence to analyze
/// * `encoding_info` - The encoding to score against
/// * `ctx` - Pipeline context for caching results
///
/// # Returns
///
/// A structural score from 0.0 (no match) to 1.0 (perfect match).
///
/// # Examples
///
/// ```
/// use _chardet_rs::pipeline::structural::compute_structural_score;
/// use _chardet_rs::registry::REGISTRY;
/// use _chardet_rs::pipeline::PipelineContext;
///
/// let data: Vec<u8> = vec![0x82, 0xA0, 0x82, 0xA2]; // Valid Shift_JIS
/// let enc_info = REGISTRY.get("shift_jis").unwrap();
/// let mut ctx = PipelineContext::new();
/// let score = compute_structural_score(&data, enc_info, &mut ctx);
/// // Score will be high for valid Shift_JIS data
/// ```
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

/// Compute byte coverage for a multi-byte encoding.
///
/// Returns the percentage of non-ASCII bytes that appear in valid
/// multi-byte sequences. Low coverage indicates many orphaned high bytes.
///
/// # Arguments
///
/// * `data` - The byte sequence to analyze
/// * `encoding_info` - The encoding to check
/// * `ctx` - Pipeline context for caching
/// * `non_ascii_count` - Optional pre-computed non-ASCII count
///
/// # Returns
///
/// A ratio from 0.0 to 1.0 representing byte coverage.
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
///
/// Real CJK text typically uses many different lead bytes. Data with
/// very few distinct lead bytes may be random or misidentified.
///
/// # Arguments
///
/// * `data` - The byte sequence to analyze
/// * `encoding_info` - The encoding to check
/// * `ctx` - Pipeline context for caching
///
/// # Returns
///
/// The number of distinct lead byte values used in valid sequences.
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

/// Get cached analysis or compute it.
///
/// Uses the pipeline context to cache analysis results, avoiding
/// redundant computation when the same encoding is analyzed multiple times.
///
/// # Arguments
///
/// * `data` - The byte sequence to analyze
/// * `name` - The encoding name
/// * `ctx` - Pipeline context for caching
///
/// # Returns
///
/// The cached or computed analysis result.
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

/// Analyze data against a specific encoding's structural rules.
///
/// # Arguments
///
/// * `data` - The byte sequence to analyze
/// * `name` - The encoding name
///
/// # Returns
///
/// A tuple of (valid_ratio, mb_byte_count, lead_diversity) or None.
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

/// Analyze data against Shift_JIS structural rules.
///
/// # Shift_JIS Structure
///
/// - Lead bytes: 0x81-0x9F, 0xE0-0xFC
/// - Trail bytes: 0x40-0x7E, 0x80-0xFC
/// - Single-byte katakana: 0xA0-0xDF
///
/// # Analysis
///
/// Counts valid lead/trail pairs and tracks lead byte diversity.
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

/// Analyze data against EUC-JP structural rules.
///
/// # EUC-JP Structure
///
/// - Two-byte JIS X 0208: 0xA1-0xFE + 0xA1-0xFE
/// - SS2 (half-width katakana): 0x8E + 0xA1-0xDF
/// - SS3 (JIS X 0212): 0x8F + 0xA1-0xFE + 0xA1-0xFE
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

/// Analyze data against EUC-KR structural rules.
///
/// # EUC-KR Structure
///
/// - Two-byte KS X 1001: 0xA1-0xFE + 0xA1-0xFE
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

/// Analyze data against GB18030 structural rules.
///
/// # GB18030 Structure
///
/// - Two-byte GBK: 0x81-0xFE + 0x40-0xFE
/// - Four-byte: 0x81-0xFE + 0x30-0x39 + 0x81-0xFE + 0x30-0x39
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

/// Analyze data against Big5 structural rules.
///
/// # Big5 Structure
///
/// - Lead bytes: 0xA1-0xF9
/// - Trail bytes: 0x40-0x7E, 0xA1-0xFE
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

/// Analyze data against Johab structural rules.
///
/// # Johab Structure
///
/// - Lead bytes: 0x84-0xD3, 0xD8-0xDE, 0xE0-0xF9
/// - Trail bytes: 0x31-0x7E, 0x81-0xFE
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
