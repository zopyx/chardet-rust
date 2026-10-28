//! Stage 3: Statistical bigram scoring.
//!
//! This module scores candidate encodings against the pre-trained bigram
//! models: it builds an IDF-weighted bigram profile of the input once, then
//! takes the cosine similarity against every language variant of every
//! candidate encoding (see `chardet/models/__init__.py`, which this mirrors).
//!
//! # Fallback Scoring
//!
//! When the model artifacts are unavailable, scoring falls back to byte
//! frequency heuristics so detection still returns a ranked answer.

use crate::bigram_models::{
    get_models, init_embedded_models, is_collapsed_whitespace, models_loaded, ModelSet,
    BIGRAM_TABLE_SIZE, LANGUAGE_SCALE_FACTOR,
};
use crate::pipeline::{
    DetectionResult, ENTROPY_NORMALIZATION_FACTOR, MAX_CONFIDENCE, MIN_CONFIDENCE,
    SIMPLIFIED_ASCII_CONFIDENCE, SIMPLIFIED_MULTIBYTE_DEFAULT_CONFIDENCE,
    SIMPLIFIED_MULTIBYTE_NO_PATTERN_CONFIDENCE, SIMPLIFIED_MULTIBYTE_PATTERN_CONFIDENCE,
    SIMPLIFIED_SINGLE_BYTE_BASE_CONFIDENCE, SIMPLIFIED_SINGLE_BYTE_ENTROPY_MULTIPLIER,
    SIMPLIFIED_SINGLE_BYTE_NO_HIGH_BYTES_CONFIDENCE, SIMPLIFIED_UTF8_CONFIDENCE,
};
use crate::registry::EncodingInfo;
use std::collections::HashMap;
use std::sync::OnceLock;

static MODEL_INIT_RESULT: OnceLock<Result<(), String>> = OnceLock::new();

fn ensure_models_loaded() {
    let _ = MODEL_INIT_RESULT.get_or_init(init_embedded_models);
}

/// An IDF-weighted bigram profile of one input sample.
///
/// ``freq`` holds the weighted count per bigram index (``(b1 << 8) | b2``),
/// ``nonzero`` lists the indices carrying weight in first-encounter order,
/// and ``row_freq`` aggregates the weights by lead byte so a model's row
/// maxima can upper-bound its score.
pub struct BigramProfile {
    freq: Vec<u32>,
    /// Bigram indices carrying weight, in first-encounter order.
    pub nonzero: Vec<usize>,
    /// L2 norm of the weighted frequencies.
    pub input_norm: f64,
    /// Weighted totals per lead byte.
    pub row_freq: [u64; 256],
    /// Lead bytes with a non-zero total.
    pub nonzero_rows: Vec<usize>,
}

impl BigramProfile {
    /// Build the profile the encoding ranking scores with (IDF weights).
    pub fn new(data: &[u8], idf: &[u8]) -> Self {
        if data.len() < 2 {
            return Self::empty();
        }

        let mut freq = vec![0u32; BIGRAM_TABLE_SIZE];
        let mut nonzero: Vec<usize> = Vec::new();

        for pair in data.windows(2) {
            let (b1, b2) = (pair[0], pair[1]);
            if b1 == b2 && is_collapsed_whitespace(b1) {
                continue;
            }
            let idx = ((b1 as usize) << 8) | b2 as usize;
            if freq[idx] == 0 {
                nonzero.push(idx);
            }
            freq[idx] += idf[idx] as u32;
        }

        Self::finish(freq, nonzero)
    }

    /// Build the profile the language tiers score with.
    ///
    /// Each occurrence carries IDF; the root-IDF factor is then folded in once
    /// per distinct bigram, and the profile's own norm carries the same scale
    /// factor so the two cancel in the quotient.
    pub fn for_language(data: &[u8], idf: &[u8], language_scale: &[u8]) -> Self {
        if data.len() < 2 {
            return Self::empty();
        }

        let mut freq = vec![0u32; BIGRAM_TABLE_SIZE];
        let mut nonzero: Vec<usize> = Vec::new();

        for pair in data.windows(2) {
            let (b1, b2) = (pair[0], pair[1]);
            if b1 == b2 && is_collapsed_whitespace(b1) {
                continue;
            }
            let idx = ((b1 as usize) << 8) | b2 as usize;
            if freq[idx] == 0 {
                nonzero.push(idx);
            }
            freq[idx] += idf[idx] as u32;
        }

        let mut norm_sq: i64 = 0;
        for &idx in &nonzero {
            let v = freq[idx] as i64;
            norm_sq += v * v;
            freq[idx] = (v as u64 * language_scale[idx] as u64) as u32;
        }

        let mut profile = Self::finish(freq, nonzero);
        profile.input_norm = LANGUAGE_SCALE_FACTOR * (norm_sq as f64).sqrt();
        profile
    }

    fn empty() -> Self {
        Self {
            freq: Vec::new(),
            nonzero: Vec::new(),
            input_norm: 0.0,
            row_freq: [0; 256],
            nonzero_rows: Vec::new(),
        }
    }

    /// Build a profile from pre-computed weighted frequencies.
    ///
    /// Used by confusion resolution's focused profiles, which hold only the
    /// bigrams where the two encodings differ.  Zero counts are dropped and
    /// the norm is derived from the sparse set, so no dense table is built.
    pub fn from_weighted_freq(weighted: &HashMap<usize, u32>) -> Self {
        let mut nonzero: Vec<usize> = Vec::with_capacity(weighted.len());
        let mut freq = vec![0u32; BIGRAM_TABLE_SIZE];
        for (&idx, &count) in weighted {
            if count != 0 {
                nonzero.push(idx);
                freq[idx] = count;
            }
        }
        Self::finish(freq, nonzero)
    }

    fn finish(freq: Vec<u32>, nonzero: Vec<usize>) -> Self {
        let mut norm_sq: i64 = 0;
        let mut row_freq = [0u64; 256];
        for &idx in &nonzero {
            let v = freq[idx] as i64;
            norm_sq += v * v;
            row_freq[idx >> 8] += v as u64;
        }

        let nonzero_rows = (0..256).filter(|&b1| row_freq[b1] != 0).collect();

        Self {
            freq,
            nonzero,
            input_norm: (norm_sq as f64).sqrt(),
            row_freq,
            nonzero_rows,
        }
    }
}

/// Score a profile against one model table using cosine similarity.
pub fn score_with_profile(profile: &BigramProfile, model: &[u8], model_norm: f64) -> f64 {
    if profile.input_norm == 0.0 || model_norm == 0.0 {
        return 0.0;
    }

    let mut dot = 0i64;
    for &idx in &profile.nonzero {
        dot += model[idx] as i64 * profile.freq[idx] as i64;
    }

    dot as f64 / (model_norm * profile.input_norm)
}

/// Best score and its language across all language variants of *encoding*.
pub fn score_best_language(
    models: &ModelSet,
    profile: &BigramProfile,
    encoding: &str,
) -> (f64, Option<String>) {
    let variants = match models.variants(encoding) {
        Some(variants) => variants,
        None => return (0.0, None),
    };

    let mut best_score = 0.0f64;
    let mut best_lang: Option<String> = None;

    for variant in variants {
        let model = match models.tables.get(&variant.key) {
            Some(table) => table,
            None => continue,
        };
        let norm = models.norms.get(&variant.key).copied().unwrap_or(0.0);
        let score = score_with_profile(profile, model, norm);
        if score > best_score {
            best_score = score;
            best_lang = variant.language.clone();
        }
    }

    (best_score, best_lang)
}

/// ADR-0005's hand-audited rare-language set, shared with the encoding-side
/// arbitration gate in the postprocess stage.
pub const RARE_LANGUAGES: [&str; 4] = ["gd", "cy", "ga", "br"];

/// The ANSI/ASCII-art pseudo-language; never a valid demotion target.
pub const ART_LANGUAGE: &str = "zxx";

/// Maximum lead a rare-language variant may hold over the best prevalent one
/// on a thin input before the label counts as noise, in weighted cosine.
const THIN_RARE_MARGIN: f64 = 0.06;

/// Score in the language tiers' IDF-weighted space across all variants of
/// *encoding*, optionally demoting a rare winner on a thin input.
///
/// Mirrors `chardet.models.score_best_language(language_weighting=True)`.
pub fn score_best_language_weighted(
    models: &ModelSet,
    profile: &BigramProfile,
    encoding: &str,
    demote_thin_rare: bool,
) -> (f64, Option<String>) {
    let variants = match models.variants(encoding) {
        Some(variants) => variants,
        None => return (0.0, None),
    };

    let mut best_score = 0.0f64;
    let mut best_lang: Option<String> = None;
    let mut best_prevalent = 0.0f64;
    let mut best_prevalent_lang: Option<String> = None;

    for variant in variants {
        let model = match models.tables.get(&variant.key) {
            Some(table) => table,
            None => continue,
        };
        let norm = models.langnorm.get(&variant.key).copied().unwrap_or(0.0);
        let score = score_with_profile(profile, model, norm);

        if score > best_score {
            best_score = score;
            best_lang = variant.language.clone();
        }

        if demote_thin_rare {
            let lang = variant.language.as_deref();
            let prevalent = lang.is_some_and(|l| !RARE_LANGUAGES.contains(&l) && l != ART_LANGUAGE);
            if prevalent && score > best_prevalent {
                best_prevalent = score;
                best_prevalent_lang = variant.language.clone();
            }
        }
    }

    if demote_thin_rare
        && best_lang
            .as_deref()
            .is_some_and(|l| RARE_LANGUAGES.contains(&l))
        && best_prevalent_lang.is_some()
        && best_score - best_prevalent < THIN_RARE_MARGIN
    {
        best_lang = best_prevalent_lang;
    }

    (best_score, best_lang)
}

/// Score all candidates and return results sorted by confidence descending.
///
/// * `full_ranking` — when `true`, every candidate is scored so the returned
///   list is complete (`detect_all` needs this). Rust currently scores every
///   candidate in both modes: the pruned path upstream is a performance
///   optimization whose contract keeps the top of the ranking identical.
pub fn score_candidates(
    data: &[u8],
    candidates: &[&EncodingInfo],
    full_ranking: bool,
) -> Vec<DetectionResult> {
    let _ = full_ranking;

    if data.is_empty() || candidates.is_empty() {
        return vec![];
    }

    ensure_models_loaded();

    let models = match get_models() {
        Some(models) => models,
        None => return score_with_simplified(data, candidates),
    };

    let profile = BigramProfile::new(data, &models.idf);
    if profile.input_norm == 0.0 {
        return vec![];
    }

    let mut scores: Vec<(String, f64, Option<String>)> = Vec::new();
    for enc in candidates {
        let (score, language) = score_best_language(&models, &profile, enc.name);
        if score > 0.0 {
            scores.push((enc.name.to_string(), score, language));
        }
    }

    scores.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .expect("confidence scores are never NaN")
    });

    scores
        .into_iter()
        .map(|(name, confidence, language)| {
            DetectionResult::new(
                Some(&name),
                confidence.clamp(MIN_CONFIDENCE, MAX_CONFIDENCE),
                language.as_deref(),
            )
        })
        .collect()
}

/// Score with the heuristic fallback used when no models are available.
fn score_with_simplified(data: &[u8], candidates: &[&EncodingInfo]) -> Vec<DetectionResult> {
    let mut scores: Vec<(String, f64, Option<String>)> = Vec::new();

    for enc in candidates {
        let (score, language) = score_simplified(data, enc);
        if score > MIN_CONFIDENCE {
            scores.push((enc.name.to_string(), score, language));
        }
    }

    scores.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .expect("confidence scores are never NaN")
    });

    scores
        .into_iter()
        .map(|(name, confidence, language)| {
            DetectionResult::new(
                Some(&name),
                confidence.clamp(MIN_CONFIDENCE, MAX_CONFIDENCE),
                language.as_deref(),
            )
        })
        .collect()
}

/// Report whether the statistical stage has models to score with.
pub fn models_available() -> bool {
    ensure_models_loaded();
    models_loaded()
}

/// Simplified scoring without models (fallback).
///
/// - **UTF-8**: high confidence when non-ASCII bytes are present
/// - **ASCII**: maximum confidence only when no non-ASCII bytes are present
/// - **Single-byte**: derived from high-byte entropy
/// - **Multi-byte**: derived from characteristic lead-byte ranges
fn score_simplified(data: &[u8], enc: &EncodingInfo) -> (f64, Option<String>) {
    let profile = create_byte_profile(data);

    let non_ascii_count: u32 = profile.frequencies[128..].iter().sum();
    let total_bytes = profile.total as f64;

    if total_bytes == 0.0 {
        return (MIN_CONFIDENCE, None);
    }

    let language = if enc.languages.len() == 1 {
        Some(enc.languages[0].to_string())
    } else {
        None
    };

    match enc.name {
        "utf-8" | "ascii" => {
            if enc.name == "utf-8" && non_ascii_count > 0 {
                (SIMPLIFIED_UTF8_CONFIDENCE, language)
            } else if enc.name == "ascii" && non_ascii_count == 0 {
                (SIMPLIFIED_ASCII_CONFIDENCE, language)
            } else {
                (MIN_CONFIDENCE, None)
            }
        }
        _ if !enc.is_multibyte_cjk => {
            if non_ascii_count > 0 {
                let high_byte_entropy = calculate_entropy(&profile.frequencies[128..]);
                (
                    SIMPLIFIED_SINGLE_BYTE_BASE_CONFIDENCE
                        + high_byte_entropy * SIMPLIFIED_SINGLE_BYTE_ENTROPY_MULTIPLIER,
                    language,
                )
            } else {
                (SIMPLIFIED_SINGLE_BYTE_NO_HIGH_BYTES_CONFIDENCE, language)
            }
        }
        _ => {
            if non_ascii_count > 0 {
                (score_multibyte_patterns(enc.name, &profile), language)
            } else {
                (MIN_CONFIDENCE, None)
            }
        }
    }
}

/// A simple byte frequency profile.
pub struct ByteProfile {
    /// Byte frequencies (0-255).
    pub frequencies: [u32; 256],
    /// Total byte count.
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
    let mut profile = ByteProfile {
        total: data.len(),
        ..Default::default()
    };

    for &b in data {
        profile.frequencies[b as usize] += 1;
    }

    profile
}

fn score_multibyte_patterns(name: &str, profile: &ByteProfile) -> f64 {
    let hit = |range: std::ops::RangeInclusive<usize>| -> bool {
        profile.frequencies[*range.start()..=*range.end()]
            .iter()
            .sum::<u32>()
            > 0
    };

    match name {
        "shift_jis_2004" | "cp932" => {
            if hit(0x81..=0x9F) || hit(0xE0..=0xEF) {
                SIMPLIFIED_MULTIBYTE_PATTERN_CONFIDENCE
            } else {
                SIMPLIFIED_MULTIBYTE_NO_PATTERN_CONFIDENCE
            }
        }
        "euc_jis_2004" | "euc-jp" | "euc_kr" | "cp949" => {
            if hit(0xA1..=0xFE) {
                SIMPLIFIED_MULTIBYTE_PATTERN_CONFIDENCE
            } else {
                SIMPLIFIED_MULTIBYTE_NO_PATTERN_CONFIDENCE
            }
        }
        "gb18030" | "gb2312" => {
            if hit(0x81..=0xFE) {
                SIMPLIFIED_MULTIBYTE_PATTERN_CONFIDENCE
            } else {
                SIMPLIFIED_MULTIBYTE_NO_PATTERN_CONFIDENCE
            }
        }
        "big5hkscs" | "big5" => {
            if hit(0xA1..=0xF9) {
                SIMPLIFIED_MULTIBYTE_PATTERN_CONFIDENCE
            } else {
                SIMPLIFIED_MULTIBYTE_NO_PATTERN_CONFIDENCE
            }
        }
        _ => SIMPLIFIED_MULTIBYTE_DEFAULT_CONFIDENCE,
    }
}

fn calculate_entropy(frequencies: &[u32]) -> f64 {
    let total: u32 = frequencies.iter().sum();
    if total == 0 {
        return MIN_CONFIDENCE;
    }

    let total_f = total as f64;
    let mut entropy = 0.0;

    for &count in frequencies {
        if count > 0 {
            let p = count as f64 / total_f;
            entropy -= p * p.log2();
        }
    }

    entropy / ENTROPY_NORMALIZATION_FACTOR
}

/// Load the model set if it is not loaded yet (debug helper support).
pub fn ensure_models_public() {
    ensure_models_loaded();
}

/// Every encoding name the loaded model set can score.
pub fn model_encoding_names() -> Vec<String> {
    ensure_models_loaded();
    match get_models() {
        Some(models) => {
            let mut names: Vec<String> = models.index.keys().cloned().collect();
            names.sort();
            names
        }
        None => Vec::new(),
    }
}

/// Statistical score for one encoding on *data* (debug helper support).
pub fn debug_score(data: &[u8], encoding: &str) -> (f64, Option<String>) {
    ensure_models_loaded();
    let Some(models) = get_models() else {
        return (0.0, None);
    };
    let profile = BigramProfile::new(data, &models.idf);
    let variants = match models.variants(encoding) {
        Some(variants) => variants.len(),
        None => return (0.0, None),
    };
    let (score, lang) = score_best_language(&models, &profile, encoding);
    eprintln!(
        "debug_score {} variants={} score={} input_norm={}",
        encoding, variants, score, profile.input_norm
    );
    (score, lang)
}
