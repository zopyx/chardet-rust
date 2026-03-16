//! Stage 3: Statistical bigram scoring.
//!
//! This module performs statistical analysis of byte sequences to determine
//! the most likely encoding. It uses either pre-trained bigram language models
//! (when available) or simplified heuristic scoring as a fallback.
//!
//! # Bigram Model Scoring
//!
//! Bigram models capture the frequency distribution of byte pairs in text
//! of a specific language and encoding. By comparing the input data's
//! bigram profile to trained models, we can determine the best match.
//!
//! # Fallback Scoring
//!
//! When bigram models are not loaded, the system falls back to:
//! - Byte frequency analysis for single-byte encodings
//! - Multi-byte pattern detection for CJK encodings

use crate::bigram_models::{init_models, models_loaded, score_best_language};
use crate::pipeline::DetectionResult;
use crate::registry::EncodingInfo;
use std::sync::OnceLock;

static MODEL_INIT_RESULT: OnceLock<Result<(), String>> = OnceLock::new();

fn ensure_models_loaded() {
    let _ = MODEL_INIT_RESULT
        .get_or_init(|| init_models(include_bytes!("../../../src/chardet/models/models.bin")));
}

/// Score all candidates and return results sorted by confidence descending.
///
/// This is the main entry point for statistical encoding detection. It scores
/// each candidate encoding and returns them sorted by confidence.
///
/// # Arguments
///
/// * `data` - The byte sequence to analyze
/// * `candidates` - The list of candidate encodings to score
///
/// # Returns
///
/// A vector of `DetectionResult` sorted by confidence (highest first).
///
/// # Algorithm
///
/// 1. Check if bigram models are loaded
/// 2. For each candidate:
///    - If models available: use bigram model scoring
///    - Otherwise: use simplified heuristic scoring
/// 3. Sort results by confidence descending
/// 4. Clamp confidence values to [0.0, 1.0]
///
/// # Examples
///
/// ```
/// use _chardet_rs::pipeline::statistical::score_candidates;
/// use _chardet_rs::registry::{REGISTRY, get_candidates};
/// use _chardet_rs::enums::EncodingEra;
///
/// let data = b"Hello, World!";
/// let candidates = get_candidates(EncodingEra::All);
/// let results = score_candidates(data, &candidates);
/// assert!(!results.is_empty());
/// ```
pub fn score_candidates(data: &[u8], candidates: &[&EncodingInfo]) -> Vec<DetectionResult> {
    if data.is_empty() || candidates.is_empty() {
        return vec![];
    }

    ensure_models_loaded();
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

/// Score using pre-trained bigram language models.
///
/// When bigram models are loaded, this function uses them to perform
/// language-specific scoring. For single-language encodings, the language
/// is directly determined from the registry.
///
/// # Arguments
///
/// * `data` - The byte sequence to analyze
/// * `enc` - The encoding to score against
///
/// # Returns
///
/// A tuple of (confidence_score, language_code).
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

/// Simplified scoring without models (fallback).
///
/// When bigram models are not available, this function provides basic
/// heuristic scoring based on byte frequency patterns.
///
/// # Arguments
///
/// * `data` - The byte sequence to analyze
/// * `enc` - The encoding to score against
///
/// # Returns
///
/// A tuple of (confidence_score, language_code).
///
/// # Scoring Logic
///
/// - **UTF-8**: High confidence if non-ASCII bytes present, 0 otherwise
/// - **ASCII**: Maximum confidence only if no non-ASCII bytes
/// - **Single-byte**: Based on high-byte entropy
/// - **Multi-byte**: Based on characteristic byte patterns
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
///
/// Tracks the occurrence count of each byte value (0-255) in the input data.
/// This is used for entropy calculations and pattern matching.
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

/// Create a byte frequency profile from data.
///
/// # Arguments
///
/// * `data` - The byte sequence to profile
///
/// # Returns
///
/// A `ByteProfile` with frequency counts for each byte value.
fn create_byte_profile(data: &[u8]) -> ByteProfile {
    let mut profile = ByteProfile {
        total: data.len(),
        ..Default::default()
    };

    for &b in data {
        profile.frequencies[b as usize] += 1;
    }

    profile
}

/// Score multi-byte encoding based on characteristic byte patterns.
///
/// Each CJK encoding has characteristic byte ranges for lead bytes.
/// This function checks for the presence of bytes in those ranges.
///
/// # Arguments
///
/// * `name` - The encoding name
/// * `profile` - The byte frequency profile
///
/// # Returns
///
/// A confidence score based on the presence of characteristic bytes.
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

/// Calculate Shannon entropy of a frequency distribution.
///
/// Entropy measures the randomness or information content of a distribution.
/// Higher entropy indicates more uniform distribution (more "random" data).
///
/// # Arguments
///
/// * `frequencies` - The frequency distribution
///
/// # Returns
///
/// Normalized entropy value from 0.0 to 1.0.
///
/// # Formula
///
/// ```text
/// H = -sum(p(x) * log2(p(x))) / log2(n)
/// ```
///
/// Where p(x) is the probability of each symbol and n is the alphabet size.
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
