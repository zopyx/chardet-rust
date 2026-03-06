//! Stage 3: Statistical bigram scoring.

use crate::bigram_models::{models_loaded, score_best_language};
use crate::pipeline::DetectionResult;
use crate::registry::EncodingInfo;

/// Score all candidates and return results sorted by confidence descending.
pub fn score_candidates(data: &[u8], candidates: &[&EncodingInfo]) -> Vec<DetectionResult> {
    if data.is_empty() || candidates.is_empty() {
        return vec![];
    }

    let mut scores: Vec<(String, f64, Option<String>)> = Vec::new();

    // Check if we have bigram models loaded
    let use_models = models_loaded();

    for enc in candidates {
        let (score, language) = if use_models {
            // Use statistical bigram models
            score_with_models(data, enc)
        } else {
            // Fallback to simplified scoring
            score_simplified(data, enc)
        };

        if score > 0.0 {
            scores.push((enc.name.to_string(), score, language));
        }
    }

    // Sort by confidence descending
    scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

    scores
        .into_iter()
        .map(|(name, conf, lang)| DetectionResult::new(Some(&name), conf.min(1.0), lang.as_deref()))
        .collect()
}

/// Score using pre-trained bigram models
fn score_with_models(data: &[u8], enc: &EncodingInfo) -> (f64, Option<String>) {
    // For single-language encodings, use the language directly
    let single_lang = if enc.languages.len() == 1 {
        Some(enc.languages[0].to_string())
    } else {
        None
    };

    // Get score and best language from models
    let (score, best_lang) = score_best_language(data, enc.name);

    // Use detected language or fall back to single language
    let language = best_lang.or(single_lang);

    (score, language)
}

/// Simplified scoring without models (fallback)
fn score_simplified(data: &[u8], enc: &EncodingInfo) -> (f64, Option<String>) {
    // Create a simple frequency profile of the data
    let profile = create_byte_profile(data);

    let non_ascii_count: u32 = profile.frequencies[128..].iter().sum();
    let total_bytes = profile.total as f64;

    if total_bytes == 0.0 {
        return (0.0, None);
    }

    // Infer language for single-language encodings
    let language = if enc.languages.len() == 1 {
        Some(enc.languages[0].to_string())
    } else {
        None
    };

    match enc.name {
        // Encodings that are mostly ASCII-compatible
        "utf-8" | "ascii" => {
            if enc.name == "utf-8" && non_ascii_count > 0 {
                (0.95, language)
            } else if enc.name == "ascii" && non_ascii_count == 0 {
                (1.0, language)
            } else {
                (0.0, None)
            }
        }
        // Single-byte encodings typically have high non-ASCII usage
        _ if !enc.is_multibyte => {
            if non_ascii_count > 0 {
                let high_byte_entropy = calculate_entropy(&profile.frequencies[128..]);
                (0.5 + high_byte_entropy * 0.5, language)
            } else {
                (0.3, language)
            }
        }
        // Multi-byte encodings
        _ => {
            if non_ascii_count > 0 {
                let score = score_multibyte_patterns(enc.name, &profile);
                (score, language)
            } else {
                (0.0, None)
            }
        }
    }
}

/// A simple byte frequency profile.
pub struct ByteProfile {
    /// Byte frequencies (0-255)
    pub frequencies: [u32; 256],
    /// Total byte count
    pub total: usize,
}

impl Default for ByteProfile {
    fn default() -> Self {
        Self {
            frequencies: [0; 256],
            total: 0,
        }
    }
}

fn create_byte_profile(data: &[u8]) -> ByteProfile {
    let mut profile = ByteProfile::default();
    profile.total = data.len();

    for &b in data {
        profile.frequencies[b as usize] += 1;
    }

    profile
}

fn score_multibyte_patterns(name: &str, profile: &ByteProfile) -> f64 {
    match name {
        "shift_jis_2004" | "cp932" => {
            let lead_range_1: u32 = profile.frequencies[0x81..=0x9F].iter().sum();
            let lead_range_2: u32 = profile.frequencies[0xE0..=0xEF].iter().sum();
            if lead_range_1 + lead_range_2 > 0 {
                0.85
            } else {
                0.1
            }
        }
        "euc-jis-2004" | "euc-jp" => {
            let high_range: u32 = profile.frequencies[0xA1..=0xFE].iter().sum();
            if high_range > 0 {
                0.85
            } else {
                0.1
            }
        }
        "euc-kr" | "cp949" => {
            let high_range: u32 = profile.frequencies[0xA1..=0xFE].iter().sum();
            if high_range > 0 {
                0.85
            } else {
                0.1
            }
        }
        "gb18030" | "gb2312" => {
            let high_range: u32 = profile.frequencies[0x81..=0xFE].iter().sum();
            if high_range > 0 {
                0.85
            } else {
                0.1
            }
        }
        "big5hkscs" | "big5" => {
            let lead_range: u32 = profile.frequencies[0xA1..=0xF9].iter().sum();
            if lead_range > 0 {
                0.85
            } else {
                0.1
            }
        }
        _ => 0.5,
    }
}

fn calculate_entropy(frequencies: &[u32]) -> f64 {
    let total: u32 = frequencies.iter().sum();
    if total == 0 {
        return 0.0;
    }

    let total_f = total as f64;
    let mut entropy = 0.0;

    for &count in frequencies {
        if count > 0 {
            let p = count as f64 / total_f;
            entropy -= p * p.log2();
        }
    }

    // Normalize to 0-1 range (max entropy for 128 values is log2(128) = 7)
    entropy / 7.0
}
